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
        let payload = self.payload.as_ref().ok_or(AppError::VaultLocked)?;
        let kdf = header.kdf;
        let next_header = format::wrap_new(new, dek, kdf)?;
        let bytes = seal_payload(&next_header, dek, payload)?;

        write_password_change_atomic(&self.path, &bytes)?;
        self.header = Some(next_header);
        Ok(())
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
        let bytes = seal_payload(header, dek, payload)?;
        write_atomic(&self.path, &bytes)?;
        Ok(())
    }
}

fn seal_payload(
    header: &VaultHeader,
    dek: &[u8; 32],
    payload: &VaultPayload,
) -> AppResult<Vec<u8>> {
    let json = Zeroizing::new(
        serde_json::to_vec(payload)
            .map_err(|e| AppError::Other(format!("payload serialization failed: {e}")))?,
    );
    format::seal(header, dek, &json)
}

fn bak_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    // One overwritable .bak of the previous version.
    if path.exists() {
        let previous = fs::read(path)?;
        replace_atomic(&bak_path(path), &previous)?;
    }

    replace_atomic(path, bytes)
}

/// Rekey both recoverable copies without ever leaving a partially-written
/// vault. If interrupted between the two renames, the primary still opens
/// with the old password and the backup already opens with the new one.
fn write_password_change_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    replace_atomic(&bak_path(path), bytes)?;
    replace_atomic(path, bytes)
}

fn replace_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&dir)?;

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
mod tests;
