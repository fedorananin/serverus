use std::path::Path;
use std::sync::Arc;

use s3s::auth::SimpleAuth;
use s3s::service::S3ServiceBuilder;
pub(crate) use serverus_e2e_fixtures::s3::MultipartProbe;

use super::acl_backend::AclFs;
use super::common::{ACCESS_KEY, SECRET_KEY};

/// Serve an S3 API over real TCP and return its assigned port.
pub(crate) async fn spawn_s3(root: &Path) -> u16 {
    spawn_s3_with_probe(root, None).await
}

pub(crate) async fn spawn_s3_with_probe(
    root: &Path,
    multipart_probe: Option<Arc<MultipartProbe>>,
) -> u16 {
    let fs = s3s_fs::FileSystem::new(root).unwrap();
    let service = {
        let mut builder = S3ServiceBuilder::new(AclFs::new(fs, multipart_probe));
        builder.set_auth(SimpleAuth::from_single(ACCESS_KEY, SECRET_KEY));
        builder.build()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let service = service.clone();
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                    .await;
            });
        }
    });
    port
}
