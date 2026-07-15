//! S3 upload-ACL application use case.

use std::sync::{Arc, Mutex};

use super::DesktopApplication;
use crate::error::{AppError, AppResult};
use crate::session::s3::S3Fs;
use crate::session::SessionEntry;
use crate::vault::model::{PublicVault, S3UploadAcl};
use crate::vault::VaultManager;

impl DesktopApplication {
    pub(crate) async fn set_s3_upload_acl(
        &self,
        session_id: String,
        mode: S3UploadAcl,
        persist: bool,
    ) -> AppResult<Option<PublicVault>> {
        if !persist {
            return self
                .run_session_operation(&session_id, move |entry, _lease| async move {
                    s3_of(&entry)?.set_upload_acl(mode);
                    Ok(None)
                })
                .await;
        }

        let access_lease = self.require_unlocked().map_err(AppError::from)?;
        let expected_vault = access_lease.vault().as_str().to_owned();
        let application = self.clone();
        let vault = self.vault.clone();
        let sessions = self.sessions.clone();
        self.run_owned_operation(async move {
            let _lifecycle = application.lock_lifecycle().await;
            access_lease
                .validate(&application)
                .map_err(AppError::from)?;
            let runtime = application.clone();
            let owner_sessions = sessions.clone();
            application
                .run_session_blocking_operation(&session_id, move |entry, session_lease| {
                    let fs = s3_of(&entry)?;
                    let connection_id = entry.connection_id.clone();
                    persist_s3_upload_acl(
                        vault.as_ref(),
                        &connection_id,
                        mode,
                        |manager| {
                            access_lease.validate(&runtime).map_err(AppError::from)?;
                            session_lease.validate(&runtime).map_err(AppError::from)?;
                            if manager.vault_id() != expected_vault {
                                return Err(AppError::WrongRuntimeContext);
                            }
                            if !owner_sessions.owns_entry(&entry) {
                                return Err(AppError::SessionNotFound);
                            }
                            Ok(())
                        },
                        |committed| fs.set_upload_acl(committed),
                    )
                })
                .await
        })
        .await
    }
}

fn s3_of(entry: &SessionEntry) -> AppResult<Arc<S3Fs>> {
    entry
        .s3
        .clone()
        .ok_or_else(|| AppError::Other("not an S3 session".into()))
}

fn persist_s3_upload_acl(
    vault: &Mutex<VaultManager>,
    connection_id: &str,
    mode: S3UploadAcl,
    validate_context: impl FnOnce(&VaultManager) -> AppResult<()>,
    apply_live: impl FnOnce(S3UploadAcl),
) -> AppResult<Option<PublicVault>> {
    let mut manager = vault.lock().unwrap();
    validate_context(&manager)?;
    let updated = manager.with_payload(|payload| {
        let connection = payload
            .connections
            .get_mut(connection_id)
            .ok_or(AppError::ConnectionNotFound)?;
        connection
            .s3
            .get_or_insert_with(Default::default)
            .upload_acl = mode;
        Ok(Some(payload.to_public()))
    })?;
    // Keep the vault mutex through the live update. Concurrent persisted
    // changes therefore commit and take effect in one consistent order.
    apply_live(mode);
    Ok(updated)
}

#[cfg(test)]
#[path = "s3_upload_acl_tests.rs"]
mod tests;
