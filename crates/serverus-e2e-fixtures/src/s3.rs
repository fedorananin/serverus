use std::path::Path;

use anyhow::{Context, Result};
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use s3s::auth::SimpleAuth;
use s3s::service::S3ServiceBuilder;
use tokio::task::JoinHandle;

mod acl_backend;
mod probe;

pub use acl_backend::AclFs;
pub use probe::MultipartProbe;

#[cfg(test)]
mod tests;

/// Stable credentials owned by the E2E harness. Mirror them in the JS test
/// constants module; never add them to the runtime JSON manifest or logs.
pub const ACCESS_KEY: &str = "serverus-e2e-access";
pub const SECRET_KEY: &str = "serverus-e2e-secret";

pub struct S3Server {
    port: u16,
    task: JoinHandle<()>,
}

impl S3Server {
    pub async fn start(root: &Path) -> Result<Self> {
        let filesystem = s3s_fs::FileSystem::new(root)
            .map_err(|error| anyhow::anyhow!("open S3 fixture root: {error:?}"))?;
        let service = {
            let mut builder = S3ServiceBuilder::new(AclFs::new(filesystem, None));
            builder.set_auth(SimpleAuth::from_single(ACCESS_KEY, SECRET_KEY));
            builder.build()
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind S3 fixture")?;
        let port = listener
            .local_addr()
            .context("read S3 fixture address")?
            .port();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let service = service.clone();
                tokio::spawn(async move {
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        Ok(Self { port, task })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for S3Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
