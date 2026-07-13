//! Quick unlock: keep the DEK behind device biometrics so the vault can be
//! reopened with Touch ID / Windows Hello (SPEC §2.3).
//!
//! Platform-specific code stays behind [`QuickUnlock`]: macOS uses the
//! Keychain + LocalAuthentication, Windows uses KeyCredentialManager
//! (Windows Hello), Linux falls back to [`NoQuickUnlock`] (master password
//! only) until a Secret Service / fprintd backend lands.

use crate::error::{AppError, AppResult};
use zeroize::Zeroizing;

pub trait QuickUnlock: Send + Sync {
    /// Human-readable name of the mechanism, for UI labels.
    fn method_name(&self) -> &'static str {
        "Biometric unlock"
    }
    /// Whether the device can do biometric quick unlock at all.
    fn is_available(&self) -> bool;
    /// Whether a DEK is currently stored for this vault.
    fn has_dek(&self, vault_id: &str) -> bool;
    /// Store the DEK protected by biometrics.
    fn store_dek(&self, vault_id: &str, dek: &[u8]) -> AppResult<()>;
    /// Retrieve the DEK; triggers the system biometric prompt.
    fn retrieve_dek(&self, vault_id: &str) -> AppResult<Zeroizing<Vec<u8>>>;
    /// Remove the stored DEK (e.g. Touch ID disabled in settings).
    fn clear(&self, vault_id: &str);
}

/// Used on platforms without an implementation and in tests.
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
pub struct NoQuickUnlock;

impl QuickUnlock for NoQuickUnlock {
    fn is_available(&self) -> bool {
        false
    }
    fn has_dek(&self, _vault_id: &str) -> bool {
        false
    }
    fn store_dek(&self, _vault_id: &str, _dek: &[u8]) -> AppResult<()> {
        Err(AppError::QuickUnlockUnavailable("not supported".into()))
    }
    fn retrieve_dek(&self, _vault_id: &str) -> AppResult<Zeroizing<Vec<u8>>> {
        Err(AppError::QuickUnlockUnavailable("not supported".into()))
    }
    fn clear(&self, _vault_id: &str) {}
}

#[cfg(target_os = "macos")]
pub use macos::MacQuickUnlock;

#[cfg(target_os = "macos")]
mod macos {
    use super::QuickUnlock;
    use crate::error::{AppError, AppResult};
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::passwords::{
        delete_generic_password, generic_password, get_generic_password,
        set_generic_password_options,
    };
    use security_framework::passwords_options::{AccessControlOptions, PasswordOptions};
    use zeroize::Zeroizing;

    /// Keychain service for items in the data-protection keychain guarded by
    /// a `biometryCurrentSet` access control (the preferred mode).
    const SERVICE_ACL: &str = "me.fedorananin.serverus.dek";
    /// Fallback service: plain keychain item, access gated in-process through
    /// `LAContext` evaluation. Used when the data-protection keychain is not
    /// available (unsigned dev builds lack the required entitlement). Note the
    /// trade-off: this mode does not auto-invalidate when the fingerprint set
    /// changes — the hardware-backed ACL mode does.
    const SERVICE_LA: &str = "me.fedorananin.serverus.dek.la";

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
    const ERR_SEC_USER_CANCELED: i32 = -128;
    const LA_ERROR_USER_CANCEL: isize = -2;

    pub struct MacQuickUnlock;

    fn biometry_available() -> bool {
        let ctx = unsafe { LAContext::new() };
        unsafe {
            ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
                .is_ok()
        }
    }

    /// Blocking Touch ID prompt via LocalAuthentication. Must not run on the
    /// main thread (callers go through `spawn_blocking`).
    fn evaluate_biometry(reason: &str) -> AppResult<()> {
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), (isize, String)>>();
        let ctx = unsafe { LAContext::new() };
        let block = RcBlock::new(move |success: Bool, error: *mut NSError| {
            let outcome = if success.as_bool() {
                Ok(())
            } else {
                let (code, msg) = unsafe { error.as_ref() }
                    .map(|e| (e.code(), e.localizedDescription().to_string()))
                    .unwrap_or((0, "authentication failed".into()));
                Err((code, msg))
            };
            let _ = tx.send(outcome);
        });
        unsafe {
            ctx.evaluatePolicy_localizedReason_reply(
                LAPolicy::DeviceOwnerAuthenticationWithBiometrics,
                &NSString::from_str(reason),
                &block,
            );
        }
        match rx.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err((LA_ERROR_USER_CANCEL, _))) => Err(AppError::QuickUnlockCancelled),
            Ok(Err((_, msg))) => Err(AppError::QuickUnlockUnavailable(msg)),
            Err(_) => Err(AppError::QuickUnlockUnavailable(
                "authentication interrupted".into(),
            )),
        }
    }

    fn acl_options(vault_id: &str, for_write: bool) -> AppResult<PasswordOptions> {
        let mut options = PasswordOptions::new_generic_password(SERVICE_ACL, vault_id);
        options.use_protected_keychain();
        if for_write {
            let access = SecAccessControl::create_with_protection(
                Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
                AccessControlOptions::BIOMETRY_CURRENT_SET.bits(),
            )
            .map_err(|e| AppError::QuickUnlockUnavailable(format!("access control: {e}")))?;
            options.set_access_control(access);
        }
        Ok(options)
    }

    impl QuickUnlock for MacQuickUnlock {
        fn method_name(&self) -> &'static str {
            "Touch ID"
        }

        fn is_available(&self) -> bool {
            biometry_available()
        }

        fn has_dek(&self, vault_id: &str) -> bool {
            // Item presence checks must not trigger a biometric prompt, so we
            // only probe the LA-gated item directly; for the ACL item we rely
            // on a marker stored alongside it in the regular keychain.
            get_generic_password(SERVICE_LA, vault_id).is_ok()
                || get_generic_password(SERVICE_ACL_MARKER, vault_id).is_ok()
        }

        fn store_dek(&self, vault_id: &str, dek: &[u8]) -> AppResult<()> {
            if !biometry_available() {
                return Err(AppError::QuickUnlockUnavailable(
                    "biometry not available".into(),
                ));
            }
            self.clear(vault_id);

            // Preferred: data-protection keychain item with biometryCurrentSet ACL.
            let stored_acl = acl_options(vault_id, true).and_then(|options| {
                set_generic_password_options(dek, options)
                    .map_err(|e| AppError::QuickUnlockUnavailable(format!("keychain: {e}")))
            });
            match stored_acl {
                Ok(()) => {
                    // Marker so has_dek() can answer without prompting.
                    let _ = security_framework::passwords::set_generic_password(
                        SERVICE_ACL_MARKER,
                        vault_id,
                        b"1",
                    );
                    Ok(())
                }
                Err(_) => {
                    // Fallback for unsigned dev builds: LA-gated plain item.
                    security_framework::passwords::set_generic_password(SERVICE_LA, vault_id, dek)
                        .map_err(|e| AppError::QuickUnlockUnavailable(format!("keychain: {e}")))
                }
            }
        }

        fn retrieve_dek(&self, vault_id: &str) -> AppResult<Zeroizing<Vec<u8>>> {
            // ACL mode first: the system shows the Touch ID prompt itself.
            if get_generic_password(SERVICE_ACL_MARKER, vault_id).is_ok() {
                let options = acl_options(vault_id, false)?;
                return match generic_password(options) {
                    Ok(dek) => Ok(Zeroizing::new(dek)),
                    Err(e) if e.code() == ERR_SEC_USER_CANCELED => {
                        Err(AppError::QuickUnlockCancelled)
                    }
                    Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => {
                        // Biometry set changed → item invalidated. Ask for the
                        // master password again (caller handles this).
                        Err(AppError::QuickUnlockUnavailable(
                            "stored key was invalidated".into(),
                        ))
                    }
                    Err(e) => Err(AppError::QuickUnlockUnavailable(format!("keychain: {e}"))),
                };
            }

            // LA-gated mode: prompt first, then read the item.
            match get_generic_password(SERVICE_LA, vault_id) {
                Ok(dek) => {
                    evaluate_biometry("unlock the vault")?;
                    Ok(Zeroizing::new(dek))
                }
                Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Err(
                    AppError::QuickUnlockUnavailable("no stored key for this vault".into()),
                ),
                Err(e) => Err(AppError::QuickUnlockUnavailable(format!("keychain: {e}"))),
            }
        }

        fn clear(&self, vault_id: &str) {
            let _ = delete_generic_password(SERVICE_LA, vault_id);
            let _ = delete_generic_password(SERVICE_ACL_MARKER, vault_id);
            if let Ok(options) = acl_options(vault_id, false) {
                let _ = security_framework::passwords::delete_generic_password_options(options);
            }
        }
    }

    /// Marker item (regular keychain, no ACL, no secret content) recording
    /// that an ACL-protected DEK exists for the vault.
    const SERVICE_ACL_MARKER: &str = "me.fedorananin.serverus.dek.marker";
}

#[cfg(target_os = "windows")]
pub use windows_hello::WindowsQuickUnlock;

#[cfg(target_os = "windows")]
mod windows_hello {
    //! Windows Hello quick unlock (the KeePassXC scheme): a per-vault random
    //! challenge is signed with a KeyCredentialManager key — the operation
    //! that shows the Hello prompt (fingerprint / face / PIN). The signature
    //! is RSA PKCS#1 v1.5, i.e. deterministic, so HKDF-SHA256 over it yields
    //! a stable AES-256-GCM key that wraps the DEK. The wrapped blob sits in
    //! the app config dir and is useless without the Hello-gated signature;
    //! resetting Windows Hello regenerates the key and the blob stops
    //! decrypting (GCM authentication fails) — same "invalidated, fall back
    //! to the master password" behavior as the macOS biometryCurrentSet ACL.

    use std::path::PathBuf;

    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use hkdf::Hkdf;
    use rand::RngCore;
    use serde::{Deserialize, Serialize};
    use sha2::Sha256;
    use windows::core::HSTRING;
    use windows::Security::Credentials::{
        KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
    };
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Storage::Streams::{DataReader, IBuffer};
    use zeroize::Zeroizing;

    use super::QuickUnlock;
    use crate::error::{AppError, AppResult};

    pub struct WindowsQuickUnlock;

    /// Wrapped-DEK blob on disk (all fields base64). Not secret by itself —
    /// the wrapping key only exists as a Hello-gated signing operation.
    #[derive(Serialize, Deserialize)]
    struct Blob {
        challenge: String,
        nonce: String,
        ciphertext: String,
    }

    fn unavailable(e: impl std::fmt::Display) -> AppError {
        AppError::QuickUnlockUnavailable(format!("Windows Hello: {e}"))
    }

    fn key_name(vault_id: &str) -> HSTRING {
        HSTRING::from(format!(
            "me.fedorananin.serverus.{}",
            URL_SAFE_NO_PAD.encode(vault_id)
        ))
    }

    fn blob_path(vault_id: &str) -> PathBuf {
        crate::app_config::config_dir()
            .join("quick_unlock")
            .join(format!("{}.json", URL_SAFE_NO_PAD.encode(vault_id)))
    }

    fn buffer_to_vec(buf: &IBuffer) -> AppResult<Zeroizing<Vec<u8>>> {
        let len = buf.Length().map_err(unavailable)? as usize;
        let mut out = Zeroizing::new(vec![0u8; len]);
        let reader = DataReader::FromBuffer(buf).map_err(unavailable)?;
        reader.ReadBytes(&mut out).map_err(unavailable)?;
        Ok(out)
    }

    /// Sign `challenge` with the vault's Hello-protected key. This is the
    /// call that shows the Windows Hello prompt. Blocking (`.join()`), so
    /// callers must be off the main thread — they go through
    /// `spawn_blocking`, same as the macOS Touch ID path.
    fn hello_sign(vault_id: &str, challenge: &[u8], create: bool) -> AppResult<Zeroizing<Vec<u8>>> {
        let name = key_name(vault_id);
        let result = if create {
            KeyCredentialManager::RequestCreateAsync(
                &name,
                KeyCredentialCreationOption::ReplaceExisting,
            )
        } else {
            KeyCredentialManager::OpenAsync(&name)
        }
        .and_then(|op| op.join())
        .map_err(unavailable)?;

        let status = result.Status().map_err(unavailable)?;
        if status == KeyCredentialStatus::UserCanceled {
            return Err(AppError::QuickUnlockCancelled);
        }
        if status == KeyCredentialStatus::NotFound {
            return Err(AppError::QuickUnlockUnavailable(
                "stored key was invalidated".into(),
            ));
        }
        if status != KeyCredentialStatus::Success {
            return Err(unavailable(format!("credential status {}", status.0)));
        }

        let credential = result.Credential().map_err(unavailable)?;
        let buf = CryptographicBuffer::CreateFromByteArray(challenge).map_err(unavailable)?;
        let signed = credential
            .RequestSignAsync(&buf)
            .and_then(|op| op.join())
            .map_err(unavailable)?;
        let status = signed.Status().map_err(unavailable)?;
        if status == KeyCredentialStatus::UserCanceled {
            return Err(AppError::QuickUnlockCancelled);
        }
        if status != KeyCredentialStatus::Success {
            return Err(unavailable(format!("sign status {}", status.0)));
        }
        buffer_to_vec(&signed.Result().map_err(unavailable)?)
    }

    /// Signature → wrapping key. HKDF-SHA256 with the challenge as salt.
    fn derive_key(signature: &[u8], challenge: &[u8]) -> Zeroizing<[u8; 32]> {
        let hk = Hkdf::<Sha256>::new(Some(challenge), signature);
        let mut key = Zeroizing::new([0u8; 32]);
        hk.expand(b"serverus quick unlock v1", key.as_mut_slice())
            .expect("32 bytes is a valid HKDF output length");
        key
    }

    impl QuickUnlock for WindowsQuickUnlock {
        fn method_name(&self) -> &'static str {
            "Windows Hello"
        }

        fn is_available(&self) -> bool {
            KeyCredentialManager::IsSupportedAsync()
                .and_then(|op| op.join())
                .unwrap_or(false)
        }

        fn has_dek(&self, vault_id: &str) -> bool {
            blob_path(vault_id).exists()
        }

        fn store_dek(&self, vault_id: &str, dek: &[u8]) -> AppResult<()> {
            let mut challenge = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut challenge);
            // ReplaceExisting: re-enrolling always starts from a fresh key.
            let signature = hello_sign(vault_id, &challenge, true)?;
            let key = derive_key(&signature, &challenge);

            let mut nonce = [0u8; 12];
            rand::thread_rng().fill_bytes(&mut nonce);
            let ciphertext = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_ref()))
                .encrypt(Nonce::from_slice(&nonce), dek)
                .map_err(|_| unavailable("encryption failed"))?;

            let blob = Blob {
                challenge: URL_SAFE_NO_PAD.encode(challenge),
                nonce: URL_SAFE_NO_PAD.encode(nonce),
                ciphertext: URL_SAFE_NO_PAD.encode(&ciphertext),
            };
            let path = blob_path(vault_id);
            std::fs::create_dir_all(path.parent().expect("blob path has a parent"))?;
            std::fs::write(&path, serde_json::to_vec(&blob).expect("blob serializes"))?;
            Ok(())
        }

        fn retrieve_dek(&self, vault_id: &str) -> AppResult<Zeroizing<Vec<u8>>> {
            let raw = std::fs::read(blob_path(vault_id)).map_err(|_| {
                AppError::QuickUnlockUnavailable("no stored key for this vault".into())
            })?;
            let blob: Blob = serde_json::from_slice(&raw)
                .map_err(|_| unavailable("corrupted quick-unlock data"))?;
            let decode = |s: &str| {
                URL_SAFE_NO_PAD
                    .decode(s)
                    .map_err(|_| unavailable("corrupted quick-unlock data"))
            };
            let challenge = decode(&blob.challenge)?;
            let nonce = decode(&blob.nonce)?;
            let ciphertext = decode(&blob.ciphertext)?;

            let signature = hello_sign(vault_id, &challenge, false)?;
            let key = derive_key(&signature, &challenge);
            Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_ref()))
                .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
                .map(Zeroizing::new)
                // Hello reset → new key → different signature → GCM fails.
                .map_err(|_| AppError::QuickUnlockUnavailable("stored key was invalidated".into()))
        }

        fn clear(&self, vault_id: &str) {
            let _ = std::fs::remove_file(blob_path(vault_id));
            if let Ok(op) = KeyCredentialManager::DeleteAsync(&key_name(vault_id)) {
                let _ = op.join();
            }
        }
    }
}
