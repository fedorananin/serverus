use s3s::dto;
use s3s::{S3Request, S3};

use super::AclFs;

const ALL_USERS: &str = "http://acs.amazonaws.com/groups/global/AllUsers";

fn request<T>(input: T) -> S3Request<T> {
    S3Request {
        input,
        method: hyper::Method::GET,
        uri: hyper::Uri::default(),
        headers: hyper::HeaderMap::new(),
        extensions: hyper::http::Extensions::new(),
        credentials: None,
        region: None,
        service: None,
        trailing_headers: None,
    }
}

#[tokio::test]
async fn fixture_tracks_public_and_private_object_acl() {
    let root = tempfile::tempdir().unwrap();
    let filesystem = s3s_fs::FileSystem::new(root.path()).unwrap();
    let backend = AclFs::new(filesystem, None);

    backend
        .put_object_acl(request(dto::PutObjectAclInput {
            acl: Some(dto::ObjectCannedACL::from_static(
                dto::ObjectCannedACL::PUBLIC_READ,
            )),
            bucket: "bucket".into(),
            key: "object.txt".into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    let public = backend
        .get_object_acl(request(dto::GetObjectAclInput {
            bucket: "bucket".into(),
            key: "object.txt".into(),
            ..Default::default()
        }))
        .await
        .unwrap();

    assert_eq!(
        public.output.grants.unwrap()[0]
            .grantee
            .as_ref()
            .and_then(|grantee| grantee.uri.as_deref()),
        Some(ALL_USERS),
    );

    backend
        .put_object_acl(request(dto::PutObjectAclInput {
            acl: Some(dto::ObjectCannedACL::from_static(
                dto::ObjectCannedACL::PRIVATE,
            )),
            bucket: "bucket".into(),
            key: "object.txt".into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    let private = backend
        .get_object_acl(request(dto::GetObjectAclInput {
            bucket: "bucket".into(),
            key: "object.txt".into(),
            ..Default::default()
        }))
        .await
        .unwrap();

    assert!(private.output.grants.unwrap().is_empty());
}
