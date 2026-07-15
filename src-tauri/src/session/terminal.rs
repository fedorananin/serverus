use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use russh::{ChannelMsg, ChannelWriteHalf};
use tauri::ipc::Channel;

use crate::error::{AppError, AppResult};

use super::{SessionEntry, SessionManager, TerminalStreamEvent};

pub(super) struct TerminalEntry {
    session_id: String,
    write: ChannelWriteHalf<russh::client::Msg>,
}

impl SessionManager {
    // -- Terminals (SPEC §5.5) --

    pub async fn term_open(
        &self,
        entry: Arc<SessionEntry>,
        cols: u16,
        rows: u16,
        output: Channel<TerminalStreamEvent>,
    ) -> AppResult<String> {
        let ssh = entry
            .ssh
            .clone()
            .ok_or_else(|| AppError::Other("not an SSH session".into()))?;
        let channel = {
            let handle = ssh.handle.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|e| AppError::Connect(format!("terminal channel: {e}")))?
        };
        channel
            .request_pty(true, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
            .await
            .map_err(|e| AppError::Connect(format!("pty: {e}")))?;
        channel
            .request_shell(true)
            .await
            .map_err(|e| AppError::Connect(format!("shell: {e}")))?;

        let (mut read, write) = channel.split();
        let term_id = uuid::Uuid::new_v4().to_string();
        self.terminals.lock().await.insert(
            term_id.clone(),
            TerminalEntry {
                session_id: entry.id.clone(),
                write,
            },
        );

        if !self.owns_entry(&entry) {
            self.term_close(&term_id).await;
            return Err(AppError::SessionNotFound);
        }

        // The frontend creates this IPC channel before invoking term_open, so
        // output is ordered even while the command is still in flight. Batch
        // at ~16 ms so floods don't hang the UI (SPEC §5.5).
        tokio::spawn(async move {
            let b64 = base64::engine::general_purpose::STANDARD;
            let mut buf: Vec<u8> = Vec::new();
            let mut ticker = tokio::time::interval(Duration::from_millis(16));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let flush = |buf: &mut Vec<u8>| {
                if !buf.is_empty() {
                    let _ = output.send(TerminalStreamEvent::Data {
                        data: b64.encode(&buf),
                    });
                    buf.clear();
                }
            };
            loop {
                tokio::select! {
                    msg = read.wait() => match msg {
                        Some(ChannelMsg::Data { data }) => {
                            buf.extend_from_slice(&data);
                            if buf.len() > 256 * 1024 {
                                flush(&mut buf);
                            }
                        }
                        Some(ChannelMsg::ExtendedData { data, .. }) => {
                            buf.extend_from_slice(&data);
                        }
                        Some(ChannelMsg::ExitStatus { .. })
                        | Some(ChannelMsg::Eof)
                        | Some(ChannelMsg::Close)
                        | None => {
                            flush(&mut buf);
                            let _ = output.send(TerminalStreamEvent::Exit);
                            break;
                        }
                        Some(_) => {}
                    },
                    _ = ticker.tick() => flush(&mut buf),
                }
            }
        });

        Ok(term_id)
    }

    pub async fn term_write(&self, term_id: &str, data: &[u8]) -> AppResult<()> {
        let terminals = self.terminals.lock().await;
        let term = terminals.get(term_id).ok_or(AppError::SessionNotFound)?;
        term.write
            .data(data)
            .await
            .map_err(|e| AppError::Other(format!("terminal write: {e}")))
    }

    pub async fn term_resize(&self, term_id: &str, cols: u16, rows: u16) -> AppResult<()> {
        let terminals = self.terminals.lock().await;
        let term = terminals.get(term_id).ok_or(AppError::SessionNotFound)?;
        term.write
            .window_change(cols as u32, rows as u32, 0, 0)
            .await
            .map_err(|e| AppError::Other(format!("terminal resize: {e}")))
    }

    pub async fn term_close(&self, term_id: &str) {
        if let Some(term) = self.terminals.lock().await.remove(term_id) {
            let _ = term.write.close().await;
        }
    }

    pub(super) async fn close_session_terminals(&self, session_id: &str) {
        let mut terminals = self.terminals.lock().await;
        let ids: Vec<String> = terminals
            .iter()
            .filter(|(_, terminal)| terminal.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(terminal) = terminals.remove(&id) {
                let _ = terminal.write.close().await;
            }
        }
    }
}
