//! Vault: the single encrypted `*.serverus` file holding everything
//! (SPEC §2). This module owns the file format, crypto, atomic persistence
//! and the in-memory unlocked state.

pub mod format;
pub mod import;
pub mod model;
pub mod quick_unlock;
pub mod tree;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use format::{KdfParams, VaultHeader};
use model::VaultPayload;

pub struct VaultManager {
    path: PathBuf,
    header: Option<VaultHeader>,
    dek: Option<Zeroizing<[u8; 32]>>,
    payload: Option<VaultPayload>,
}

impl VaultManager {
    pub fn new(path: PathBuf) -> Self {
        VaultManager {
            path,
            header: None,
            dek: None,
            payload: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Move the vault to a new location: write there atomically, then switch.
    /// The previous file is left in place as a manual backup.
    /// Move the vault file. A directory target (an existing folder, or a
    /// path spelled with a trailing slash) means "move into that folder" —
    /// the current file name is kept. Missing parent folders are created.
    /// The old file stays at the previous location as a manual backup.
    pub fn set_path(&mut self, path: PathBuf) -> AppResult<()> {
        let spelled_as_dir = {
            let s = path.to_string_lossy();
            s.ends_with('/') || (cfg!(windows) && s.ends_with('\\'))
        };
        let path = if path.is_dir() || spelled_as_dir {
            match self.path.file_name() {
                Some(name) => path.join(name),
                None => path,
            }
        } else {
            path
        };
        if path == self.path {
            return Ok(());
        }
        if path.exists() {
            return Err(AppError::Other(format!(
                "{} already exists — pick another name or remove that file first",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let old = std::mem::replace(&mut self.path, path);
        if let Err(e) = self.save() {
            self.path = old;
            return Err(e);
        }
        Ok(())
    }

    /// Move the file, then commit an external pointer to the resulting path.
    /// If that pointer cannot be persisted, keep using the original path in
    /// memory; the newly written file remains only as a recoverable backup.
    pub fn set_path_transactional(
        &mut self,
        path: PathBuf,
        persist: impl FnOnce(&Path) -> AppResult<()>,
    ) -> AppResult<()> {
        let previous = self.path.clone();
        self.set_path(path)?;
        if let Err(error) = persist(&self.path) {
            self.path = previous;
            return Err(error);
        }
        Ok(())
    }

    /// Stable identifier for keychain storage — the canonical vault path.
    pub fn vault_id(&self) -> String {
        self.path.to_string_lossy().to_string()
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn is_unlocked(&self) -> bool {
        self.dek.is_some() && self.payload.is_some()
    }

    /// Create a brand-new vault file with a fresh DEK. Fails if one exists.
    pub fn create(&mut self, password: &str, kdf: KdfParams) -> AppResult<()> {
        if self.exists() {
            return Err(AppError::VaultExists);
        }
        let dek = Zeroizing::new(format::random_bytes::<32>());
        let header = format::wrap_new(password, &dek, kdf)?;
        let payload = VaultPayload::default();

        self.header = Some(header);
        self.dek = Some(dek);
        self.payload = Some(payload);
        self.save()?;
        Ok(())
    }

    fn load_file(&self) -> AppResult<(VaultHeader, [u8; 12], Vec<u8>)> {
        if !self.exists() {
            return Err(AppError::VaultNotFound);
        }
        let bytes = fs::read(&self.path)?;
        format::parse(&bytes)
    }

    pub fn unlock_with_password(&mut self, password: &str) -> AppResult<()> {
        let (header, nonce, ct) = self.load_file()?;
        let dek = format::unwrap_dek(&header, password)?;
        let json = format::open_payload(&header, &dek, &nonce, &ct)?;
        let payload: VaultPayload = serde_json::from_slice(&json)
            .map_err(|e| AppError::Corrupted(format!("payload parse: {e}")))?;
        self.header = Some(header);
        self.dek = Some(dek);
        self.payload = Some(payload);
        Ok(())
    }

    /// Unlock with a DEK retrieved from quick unlock. The GCM tag check on the
    /// payload validates the key.
    pub fn unlock_with_dek(&mut self, dek_bytes: &[u8]) -> AppResult<()> {
        if dek_bytes.len() != 32 {
            return Err(AppError::QuickUnlockUnavailable("bad stored key".into()));
        }
        let (header, nonce, ct) = self.load_file()?;
        let mut dek = Zeroizing::new([0u8; 32]);
        dek.copy_from_slice(dek_bytes);
        let json = format::open_payload(&header, &dek, &nonce, &ct)
            .map_err(|_| AppError::QuickUnlockUnavailable("stored key does not match".into()))?;
        let payload: VaultPayload = serde_json::from_slice(&json)
            .map_err(|e| AppError::Corrupted(format!("payload parse: {e}")))?;
        self.header = Some(header);
        self.dek = Some(dek);
        self.payload = Some(payload);
        Ok(())
    }

    /// Zeroize the DEK and drop decrypted data (SPEC §2.4). Open network
    /// sessions are managed elsewhere and intentionally survive a lock.
    pub fn lock(&mut self) {
        self.dek = None; // Zeroizing zeroes on drop
        self.payload = None;
        self.header = None;
    }

    pub fn dek(&self) -> AppResult<&[u8; 32]> {
        self.dek.as_deref().ok_or(AppError::VaultLocked)
    }

    pub fn payload(&self) -> AppResult<&VaultPayload> {
        self.payload.as_ref().ok_or(AppError::VaultLocked)
    }

    /// Mutate the payload and persist atomically in one step.
    pub fn with_payload<T>(
        &mut self,
        f: impl FnOnce(&mut VaultPayload) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut next = self.payload()?.clone();
        let result = f(&mut next)?;
        self.save_payload(&next)?;
        self.payload = Some(next);
        Ok(result)
    }

    /// Change the master password: re-wrap the DEK, leave data untouched
    /// (SPEC §2.2). Verifies the current password first.
    pub fn change_password(&mut self, current: &str, new: &str) -> AppResult<()> {
        let header = self.header.as_ref().ok_or(AppError::VaultLocked)?;
        format::unwrap_dek(header, current)?;
        let dek = self.dek.as_ref().ok_or(AppError::VaultLocked)?;
        let kdf = header.kdf;
        self.header = Some(format::wrap_new(new, dek, kdf)?);
        self.save()
    }

    /// Serialize, encrypt and write the vault atomically: temp file + rename,
    /// with a `.bak` copy of the previous version (SPEC §2.2). The plaintext
    /// payload never touches the disk.
    pub fn save(&self) -> AppResult<()> {
        let payload = self.payload.as_ref().ok_or(AppError::VaultLocked)?;
        self.save_payload(payload)
    }

    fn save_payload(&self, payload: &VaultPayload) -> AppResult<()> {
        let header = self.header.as_ref().ok_or(AppError::VaultLocked)?;
        let dek = self.dek.as_ref().ok_or(AppError::VaultLocked)?;
        let json = Zeroizing::new(
            serde_json::to_vec(payload)
                .map_err(|e| AppError::Other(format!("payload serialization failed: {e}")))?,
        );
        let bytes = format::seal(header, dek, &json)?;
        write_atomic(&self.path, &bytes)?;
        Ok(())
    }
}

fn bak_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&dir)?;

    // One overwritable .bak of the previous version.
    if path.exists() {
        fs::copy(path, bak_path(path))?;
    }

    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    // Persist the rename itself.
    if let Ok(d) = fs::File::open(&dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::model::{AuthConfig, AuthMethod, Connection, Protocol};

    fn test_kdf() -> KdfParams {
        KdfParams {
            m_cost_kib: 8 * 1024,
            t_cost: 1,
            p_cost: 1,
        }
    }

    fn temp_vault() -> (tempfile::TempDir, VaultManager) {
        let dir = tempfile::tempdir().unwrap();
        let mgr = VaultManager::new(dir.path().join("test.serverus"));
        (dir, mgr)
    }

    fn sample_connection() -> Connection {
        Connection {
            name: "test".into(),
            badge: None,
            protocol: Protocol::Ssh,
            host: "example.com".into(),
            port: 22,
            auth: AuthConfig {
                method: AuthMethod::Password,
                username: "root".into(),
                password: Some("secret".into()),
                key_path: None,
                key_inline: None,
                key_passphrase: None,
            },
            jump_host: None,
            ftp: None,
            s3: None,
            remote_dir: None,
            local_dir: None,
            tunnels: vec![],
            disable_terminal: false,
            notes: String::new(),
        }
    }

    #[test]
    fn create_lock_unlock_roundtrip() {
        let (_dir, mut mgr) = temp_vault();
        mgr.create("master", test_kdf()).unwrap();
        mgr.with_payload(|p| {
            p.connections.insert("c1".into(), sample_connection());
            Ok(())
        })
        .unwrap();

        mgr.lock();
        assert!(!mgr.is_unlocked());
        assert!(matches!(mgr.payload(), Err(AppError::VaultLocked)));

        mgr.unlock_with_password("master").unwrap();
        let payload = mgr.payload().unwrap();
        assert_eq!(
            payload.connections["c1"].auth.password.as_deref(),
            Some("secret")
        );
    }

    #[test]
    fn wrong_password_fails() {
        let (_dir, mut mgr) = temp_vault();
        mgr.create("master", test_kdf()).unwrap();
        mgr.lock();
        assert!(matches!(
            mgr.unlock_with_password("nope"),
            Err(AppError::InvalidPassword)
        ));
    }

    #[test]
    fn change_password_keeps_data() {
        let (_dir, mut mgr) = temp_vault();
        mgr.create("old", test_kdf()).unwrap();
        mgr.with_payload(|p| {
            p.connections.insert("c1".into(), sample_connection());
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            mgr.change_password("wrong", "new"),
            Err(AppError::InvalidPassword)
        ));
        mgr.change_password("old", "new").unwrap();
        mgr.lock();
        assert!(mgr.unlock_with_password("old").is_err());
        mgr.unlock_with_password("new").unwrap();
        assert!(mgr.payload().unwrap().connections.contains_key("c1"));
    }

    #[test]
    fn with_payload_rolls_back_when_mutation_fails() {
        let (_dir, mut mgr) = temp_vault();
        mgr.create("pw", test_kdf()).unwrap();

        let result: AppResult<()> = mgr.with_payload(|p| {
            p.connections.insert("c1".into(), sample_connection());
            Err(AppError::Other("mutation failed".into()))
        });

        assert!(result.is_err());
        assert!(!mgr.payload().unwrap().connections.contains_key("c1"));
    }

    #[test]
    fn with_payload_rolls_back_when_save_fails() {
        let (dir, mut mgr) = temp_vault();
        mgr.create("pw", test_kdf()).unwrap();
        let original_path = mgr.path().to_path_buf();

        let non_directory = dir.path().join("not-a-directory");
        fs::write(&non_directory, b"block parent directory creation").unwrap();
        mgr.path = non_directory.join("test.serverus");

        let result = mgr.with_payload(|p| {
            p.connections.insert("c1".into(), sample_connection());
            Ok(())
        });

        assert!(result.is_err());
        assert!(!mgr.payload().unwrap().connections.contains_key("c1"));

        mgr.path = original_path;
        mgr.lock();
        mgr.unlock_with_password("pw").unwrap();
        assert!(!mgr.payload().unwrap().connections.contains_key("c1"));
    }

    #[test]
    fn bak_file_holds_previous_version() {
        let (_dir, mut mgr) = temp_vault();
        mgr.create("pw", test_kdf()).unwrap();
        let v1 = fs::read(mgr.path()).unwrap();

        mgr.with_payload(|p| {
            p.connections.insert("c1".into(), sample_connection());
            Ok(())
        })
        .unwrap();

        let bak = bak_path(mgr.path());
        assert!(bak.is_file());
        assert_eq!(fs::read(&bak).unwrap(), v1);

        // The .bak decrypts fine with the same password.
        let mut bak_mgr = VaultManager::new(bak);
        bak_mgr.unlock_with_password("pw").unwrap();
        assert!(bak_mgr.payload().unwrap().connections.is_empty());
    }

    #[test]
    fn corrupted_file_reports_corruption() {
        let (_dir, mut mgr) = temp_vault();
        mgr.create("pw", test_kdf()).unwrap();
        mgr.lock();

        let mut bytes = fs::read(mgr.path()).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(mgr.path(), &bytes).unwrap();

        assert!(matches!(
            mgr.unlock_with_password("pw"),
            Err(AppError::Corrupted(_))
        ));
    }

    #[test]
    fn unlock_with_dek_roundtrip() {
        let (_dir, mut mgr) = temp_vault();
        mgr.create("pw", test_kdf()).unwrap();
        let dek = *mgr.dek().unwrap();
        mgr.lock();

        mgr.unlock_with_dek(&dek).unwrap();
        assert!(mgr.is_unlocked());

        mgr.lock();
        let wrong = [0u8; 32];
        assert!(mgr.unlock_with_dek(&wrong).is_err());
    }

    #[test]
    fn set_path_handles_folders_conflicts_and_missing_parents() {
        let (dir, mut mgr) = temp_vault();
        mgr.create("pw", test_kdf()).unwrap();

        // A directory target keeps the current file name.
        let sub = dir.path().join("backups");
        fs::create_dir(&sub).unwrap();
        mgr.set_path(sub.clone()).unwrap();
        assert_eq!(mgr.path(), sub.join("test.serverus"));
        assert!(sub.join("test.serverus").is_file());

        // An occupied target is a clear error and the vault stays put.
        let occupied = dir.path().join("other.serverus");
        fs::write(&occupied, b"x").unwrap();
        let err = mgr.set_path(occupied).unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert_eq!(mgr.path(), sub.join("test.serverus"));

        // Missing parent folders are created for a full file path.
        let deep = dir.path().join("a").join("b").join("moved.serverus");
        mgr.set_path(deep.clone()).unwrap();
        assert!(deep.is_file());

        // Trailing slash = "into this folder", even if it doesn't exist yet.
        let slashed = format!("{}/", dir.path().join("slashdir").display());
        mgr.set_path(PathBuf::from(slashed)).unwrap();
        assert!(dir.path().join("slashdir").join("moved.serverus").is_file());
    }

    #[test]
    fn set_path_rolls_runtime_pointer_back_when_config_persist_fails() {
        let (dir, mut mgr) = temp_vault();
        mgr.create("pw", test_kdf()).unwrap();
        let original = mgr.path().to_path_buf();
        let target = dir.path().join("new.serverus");

        let result = mgr.set_path_transactional(target.clone(), |_| {
            Err(AppError::Other("config write failed".into()))
        });

        assert!(result.is_err());
        assert_eq!(mgr.path(), original);
        assert!(target.is_file(), "the staged copy remains recoverable");
    }
}
