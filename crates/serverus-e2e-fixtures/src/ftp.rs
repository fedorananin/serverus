use std::path::Path;

use anyhow::{ensure, Context, Result};
use tokio::task::JoinHandle;

use crate::net::{reserve_local_port, wait_for_listener};

mod faults;

use faults::FaultyFilesystem;

#[cfg(test)]
use faults::{FailAfter, RetrievalFaults, FAULT_AFTER_BYTES};

#[cfg(test)]
mod tests;

pub struct FtpServer {
    port: u16,
    task: JoinHandle<()>,
}

impl FtpServer {
    pub async fn start(root: &Path) -> Result<Self> {
        ensure!(root.is_dir(), "FTP fixture root is not a directory");
        let port = reserve_local_port()?;
        let root = root.to_path_buf();
        let telemetry = root
            .parent()
            .context("FTP fixture root has no workspace parent")?
            .join("ftp-retrievals.jsonl");
        let faults = faults::RetrievalFaults::new(telemetry);
        let server = libunftp::ServerBuilder::new(Box::new(move || {
            FaultyFilesystem::new(root.clone(), faults.clone()).expect("validated FTP fixture root")
        }))
        .passive_ports(40_000..=49_999)
        .build()
        .context("build FTP fixture")?;
        let address = format!("127.0.0.1:{port}");
        let task = tokio::spawn(async move {
            let _ = server.listen(address).await;
        });

        if let Err(error) = wait_for_listener(port).await {
            task.abort();
            return Err(error).context("start FTP fixture");
        }
        Ok(Self { port, task })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for FtpServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
