//! Running a short non-interactive command on an SSH session and capturing
//! its output (e.g. `echo "$SHELL"` to learn the login shell's syntax).

use std::time::Duration;

use crate::error::{AppError, AppResult};

use super::ssh::SshSession;

/// Output of a captured exec.
pub struct ExecCapture {
    pub exit_status: Option<u32>,
    /// stdout and stderr interleaved, lossily decoded, capped.
    pub output: String,
}

impl SshSession {
    /// Run `cmd` on a fresh exec channel. Output beyond `max_bytes` is
    /// dropped; the whole exchange is bounded by `timeout`.
    pub async fn exec_capture(
        &self,
        cmd: &str,
        max_bytes: usize,
        timeout: Duration,
    ) -> AppResult<ExecCapture> {
        let channel = {
            let handle = self.handle.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|e| AppError::Connect(format!("exec channel: {e}")))?
        };
        channel
            .exec(true, cmd)
            .await
            .map_err(|e| AppError::Connect(format!("exec: {e}")))?;
        let (mut read, write) = channel.split();
        let mut output = Vec::new();
        let mut exit_status = None;
        let collect = async {
            while let Some(msg) = read.wait().await {
                match msg {
                    russh::ChannelMsg::Data { data }
                    | russh::ChannelMsg::ExtendedData { data, .. } => {
                        let room = max_bytes.saturating_sub(output.len());
                        output.extend_from_slice(&data[..data.len().min(room)]);
                    }
                    russh::ChannelMsg::ExitStatus {
                        exit_status: status,
                    } => exit_status = Some(status),
                    russh::ChannelMsg::Close => break,
                    _ => {}
                }
            }
        };
        let finished = tokio::time::timeout(timeout, collect).await.is_ok();
        let _ = write.close().await;
        if !finished {
            return Err(AppError::Other(format!(
                "`{cmd}` did not finish within {}s",
                timeout.as_secs()
            )));
        }
        Ok(ExecCapture {
            exit_status,
            output: String::from_utf8_lossy(&output).into_owned(),
        })
    }
}
