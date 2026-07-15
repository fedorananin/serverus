use std::sync::Arc;

use tauri::AppHandle;

use crate::error::AppResult;
use crate::vault::model::Protocol;

use super::super::{ftp, s3, SessionEntry, SessionManager};
use super::emit_session_state;
use super::plan::ConnectionPlan;

impl SessionManager {
    pub(super) async fn connect_storage(
        &self,
        app: &AppHandle,
        connection_id: &str,
        plan: ConnectionPlan,
    ) -> AppResult<Arc<SessionEntry>> {
        let (protocol, ftp, s3) = match plan {
            ConnectionPlan::Ftp {
                config,
                max_parallel,
            } => (
                Protocol::Ftp,
                Some(ftp::FtpPool::new(config, max_parallel + 1)),
                None,
            ),
            ConnectionPlan::S3 { config } => (Protocol::S3, None, Some(s3::S3Fs::new(config))),
            ConnectionPlan::Ssh { .. } => unreachable!("SSH plans use the SSH connector"),
        };

        let session_id = uuid::Uuid::new_v4().to_string();
        emit_session_state(app, &session_id, connection_id, "connecting", None);

        let probe = match (&ftp, &s3) {
            (Some(pool), _) => pool.probe().await,
            (_, Some(file_system)) => file_system.probe().await,
            _ => unreachable!("storage connection requires an FTP or S3 configuration"),
        };

        match probe {
            Ok(()) => {
                let entry = Arc::new(SessionEntry::storage(
                    session_id.clone(),
                    connection_id.to_string(),
                    protocol,
                    ftp,
                    s3,
                ));
                self.sessions
                    .lock()
                    .unwrap()
                    .insert(session_id.clone(), entry.clone());
                emit_session_state(app, &session_id, connection_id, "connected", None);
                Ok(entry)
            }
            Err(error) => {
                emit_session_state(
                    app,
                    &session_id,
                    connection_id,
                    "error",
                    Some(error.to_string()),
                );
                Err(error)
            }
        }
    }
}
