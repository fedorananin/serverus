use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use s3s::dto;
use s3s::{S3Request, S3Response, S3Result, S3};

use super::MultipartProbe;

const ALL_USERS: &str = "http://acs.amazonaws.com/groups/global/AllUsers";

/// Filesystem-backed S3 fixture with deterministic in-memory object ACLs.
///
/// `s3s-fs` deliberately does not implement ACL operations. Wrapping it here
/// lets browser scenarios exercise the same public/private contract as real
/// ACL-capable providers while all object bytes still travel over real HTTP.
pub struct AclFs {
    inner: s3s_fs::FileSystem,
    public: Mutex<HashSet<(String, String)>>,
    pending: Mutex<HashMap<String, (String, String, bool)>>,
    multipart_probe: Option<Arc<MultipartProbe>>,
}

impl AclFs {
    pub fn new(inner: s3s_fs::FileSystem, multipart_probe: Option<Arc<MultipartProbe>>) -> Self {
        Self {
            inner,
            public: Mutex::new(HashSet::new()),
            pending: Mutex::new(HashMap::new()),
            multipart_probe,
        }
    }

    fn set_public(&self, bucket: &str, key: &str, public: bool) {
        let mut set = self.public.lock().unwrap();
        if public {
            set.insert((bucket.to_string(), key.to_string()));
        } else {
            set.remove(&(bucket.to_string(), key.to_string()));
        }
    }

    fn is_public_acl(acl: Option<&dto::ObjectCannedACL>) -> bool {
        acl.map(|acl| acl.as_str() == dto::ObjectCannedACL::PUBLIC_READ)
            .unwrap_or(false)
    }
}

#[async_trait::async_trait]
impl S3 for AclFs {
    async fn list_buckets(
        &self,
        req: S3Request<dto::ListBucketsInput>,
    ) -> S3Result<S3Response<dto::ListBucketsOutput>> {
        self.inner.list_buckets(req).await
    }

    async fn create_bucket(
        &self,
        req: S3Request<dto::CreateBucketInput>,
    ) -> S3Result<S3Response<dto::CreateBucketOutput>> {
        self.inner.create_bucket(req).await
    }

    async fn delete_bucket(
        &self,
        req: S3Request<dto::DeleteBucketInput>,
    ) -> S3Result<S3Response<dto::DeleteBucketOutput>> {
        self.inner.delete_bucket(req).await
    }

    async fn head_bucket(
        &self,
        req: S3Request<dto::HeadBucketInput>,
    ) -> S3Result<S3Response<dto::HeadBucketOutput>> {
        self.inner.head_bucket(req).await
    }

    async fn list_objects_v2(
        &self,
        req: S3Request<dto::ListObjectsV2Input>,
    ) -> S3Result<S3Response<dto::ListObjectsV2Output>> {
        self.inner.list_objects_v2(req).await
    }

    async fn head_object(
        &self,
        req: S3Request<dto::HeadObjectInput>,
    ) -> S3Result<S3Response<dto::HeadObjectOutput>> {
        self.inner.head_object(req).await
    }

    async fn get_object(
        &self,
        req: S3Request<dto::GetObjectInput>,
    ) -> S3Result<S3Response<dto::GetObjectOutput>> {
        self.inner.get_object(req).await
    }

    async fn put_object(
        &self,
        req: S3Request<dto::PutObjectInput>,
    ) -> S3Result<S3Response<dto::PutObjectOutput>> {
        if let Some(probe) = &self.multipart_probe {
            probe.put_object_calls.fetch_add(1, Ordering::SeqCst);
        }
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let public = Self::is_public_acl(req.input.acl.as_ref());
        let response = self.inner.put_object(req).await?;
        self.set_public(&bucket, &key, public);
        Ok(response)
    }

    async fn delete_object(
        &self,
        req: S3Request<dto::DeleteObjectInput>,
    ) -> S3Result<S3Response<dto::DeleteObjectOutput>> {
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let response = self.inner.delete_object(req).await?;
        self.set_public(&bucket, &key, false);
        Ok(response)
    }

    async fn copy_object(
        &self,
        req: S3Request<dto::CopyObjectInput>,
    ) -> S3Result<S3Response<dto::CopyObjectOutput>> {
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let public = Self::is_public_acl(req.input.acl.as_ref());
        let response = self.inner.copy_object(req).await?;
        self.set_public(&bucket, &key, public);
        Ok(response)
    }

    async fn create_multipart_upload(
        &self,
        req: S3Request<dto::CreateMultipartUploadInput>,
    ) -> S3Result<S3Response<dto::CreateMultipartUploadOutput>> {
        if let Some(probe) = &self.multipart_probe {
            probe.create_calls.fetch_add(1, Ordering::SeqCst);
        }
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let public = Self::is_public_acl(req.input.acl.as_ref());
        let response = self.inner.create_multipart_upload(req).await?;
        if let Some(upload_id) = &response.output.upload_id {
            self.pending
                .lock()
                .unwrap()
                .insert(upload_id.clone(), (bucket, key, public));
        }
        Ok(response)
    }

    async fn upload_part(
        &self,
        req: S3Request<dto::UploadPartInput>,
    ) -> S3Result<S3Response<dto::UploadPartOutput>> {
        if let Some(probe) = &self.multipart_probe {
            probe.upload_part_calls.fetch_add(1, Ordering::SeqCst);
            probe.upload_part_started.notify_one();
            if probe.block_upload_part.load(Ordering::SeqCst) {
                probe.release_upload_part.notified().await;
            }
        }
        self.inner.upload_part(req).await
    }

    async fn complete_multipart_upload(
        &self,
        req: S3Request<dto::CompleteMultipartUploadInput>,
    ) -> S3Result<S3Response<dto::CompleteMultipartUploadOutput>> {
        if let Some(probe) = &self.multipart_probe {
            probe.complete_calls.fetch_add(1, Ordering::SeqCst);
        }
        let upload_id = req.input.upload_id.clone();
        let response = self.inner.complete_multipart_upload(req).await?;
        if let Some((bucket, key, public)) = self.pending.lock().unwrap().remove(&upload_id) {
            self.set_public(&bucket, &key, public);
        }
        Ok(response)
    }

    async fn abort_multipart_upload(
        &self,
        req: S3Request<dto::AbortMultipartUploadInput>,
    ) -> S3Result<S3Response<dto::AbortMultipartUploadOutput>> {
        if let Some(probe) = &self.multipart_probe {
            probe.abort_calls.fetch_add(1, Ordering::SeqCst);
            probe.abort_seen.notify_one();
        }
        self.pending.lock().unwrap().remove(&req.input.upload_id);
        self.inner.abort_multipart_upload(req).await
    }

    async fn get_object_acl(
        &self,
        req: S3Request<dto::GetObjectAclInput>,
    ) -> S3Result<S3Response<dto::GetObjectAclOutput>> {
        let public = self
            .public
            .lock()
            .unwrap()
            .contains(&(req.input.bucket.clone(), req.input.key.clone()));
        let grants = if public {
            vec![dto::Grant {
                grantee: Some(dto::Grantee {
                    display_name: None,
                    email_address: None,
                    id: None,
                    type_: dto::Type::from_static(dto::Type::GROUP),
                    uri: Some(ALL_USERS.to_string()),
                }),
                permission: Some(dto::Permission::from_static(dto::Permission::READ)),
            }]
        } else {
            Vec::new()
        };
        Ok(S3Response::new(dto::GetObjectAclOutput {
            grants: Some(grants),
            owner: None,
            request_charged: None,
        }))
    }

    async fn put_object_acl(
        &self,
        req: S3Request<dto::PutObjectAclInput>,
    ) -> S3Result<S3Response<dto::PutObjectAclOutput>> {
        let public = Self::is_public_acl(req.input.acl.as_ref());
        self.set_public(&req.input.bucket, &req.input.key, public);
        Ok(S3Response::new(dto::PutObjectAclOutput {
            request_charged: None,
        }))
    }
}
