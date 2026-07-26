//! The upload requests behind [`super::S3Writer`], and the abort slot that
//! cleans up after one that never completes.
//!
//! Everything that talks to S3 lives here; the parent module is the
//! [`tokio::io::AsyncWrite`] state machine that drives it.

use std::sync::{Arc, Mutex};

use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart, ObjectCannedAcl};
use aws_sdk_s3::Client;

use crate::session::s3::{content_type, sdk_err_msg};

/// Identify the object from its opening bytes. A short object is sniffed on
/// whatever it has; the signatures simply fail to match past its end.
fn sniff_head(buf: &[u8]) -> Option<&'static str> {
    content_type::sniff(&buf[..buf.len().min(content_type::HEAD_LEN)])
}

pub(super) struct WriterInner {
    client: Client,
    bucket: String,
    key: String,
    acl: Option<ObjectCannedAcl>,
    /// The media type the object's name implies, or `None` when the name says
    /// nothing and the bytes get asked instead.
    content_type: Option<&'static str>,
    abort: Arc<AbortSlot>,
    upload_id: Option<String>,
    parts: Vec<CompletedPart>,
    next_part: i32,
    buf: Vec<u8>,
}

impl WriterInner {
    /// Build the upload state together with the abort slot that shadows it.
    pub(super) fn new(
        client: Client,
        bucket: String,
        key: String,
        acl: Option<ObjectCannedAcl>,
        content_type: Option<&'static str>,
    ) -> (Box<WriterInner>, Arc<AbortSlot>) {
        let abort = Arc::new(AbortSlot {
            client: client.clone(),
            bucket: bucket.clone(),
            key: key.clone(),
            upload_id: Mutex::new(None),
        });
        let inner = Box::new(WriterInner {
            client,
            bucket,
            key,
            acl,
            content_type,
            abort: abort.clone(),
            upload_id: None,
            parts: Vec::new(),
            next_part: 1,
            buf: Vec::new(),
        });
        (inner, abort)
    }

    pub(super) fn buffer(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub(super) fn buffered(&self) -> usize {
        self.buf.len()
    }

    pub(super) fn upload_id(&self) -> Option<String> {
        self.upload_id.clone()
    }

    /// The type to declare on the request being built. Must be called while
    /// the head of the object is still buffered — that is, before `buf` is
    /// taken for the body.
    fn declared_content_type(&self) -> Option<String> {
        self.content_type
            .or_else(|| sniff_head(&self.buf))
            .map(str::to_string)
    }

    /// Upload the buffered bytes as the next part (creates the multipart
    /// upload lazily on the first call).
    pub(super) async fn flush_part(mut self: Box<Self>) -> std::io::Result<Box<Self>> {
        if self.upload_id.is_none() {
            // CreateMultipartUpload is the only place a multipart object can
            // declare its type; the parts and the completion carry no headers
            // of their own. `buf` still holds this part, so the object's head
            // is available to sniff.
            let content_type = self.declared_content_type();
            let mut req = self
                .client
                .create_multipart_upload()
                .bucket(&self.bucket)
                .key(&self.key)
                .set_content_type(content_type);
            if let Some(acl) = &self.acl {
                req = req.acl(acl.clone());
            }
            let out = req
                .send()
                .await
                .map_err(|e| std::io::Error::other(sdk_err_msg(&e)))?;
            let upload_id = out
                .upload_id()
                .ok_or_else(|| std::io::Error::other("no upload id"))?
                .to_string();
            self.abort.set_upload_id(Some(upload_id.clone()));
            self.upload_id = Some(upload_id);
        }
        let body = std::mem::take(&mut self.buf);
        let part_number = self.next_part;
        self.next_part += 1;
        let out = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(&self.key)
            .upload_id(self.upload_id.as_deref().unwrap_or_default())
            .part_number(part_number)
            .body(body.into())
            .send()
            .await
            .map_err(|e| std::io::Error::other(sdk_err_msg(&e)))?;
        self.parts.push(
            CompletedPart::builder()
                .part_number(part_number)
                .set_e_tag(out.e_tag().map(str::to_string))
                .build(),
        );
        Ok(self)
    }

    pub(super) async fn finish(mut self: Box<Self>) -> std::io::Result<()> {
        match self.upload_id.clone() {
            // Small object: everything still buffered — one PutObject.
            None => {
                // Resolved before the body is taken, while `buf` still holds
                // the bytes to sniff.
                let content_type = self.declared_content_type();
                let mut req = self
                    .client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&self.key)
                    .body(std::mem::take(&mut self.buf).into())
                    .set_content_type(content_type);
                if let Some(acl) = &self.acl {
                    req = req.acl(acl.clone());
                }
                req.send()
                    .await
                    .map(|_| ())
                    .map_err(|e| std::io::Error::other(sdk_err_msg(&e)))
            }
            Some(upload_id) => {
                if !self.buf.is_empty() {
                    self = self.flush_part().await?;
                }
                self.client
                    .complete_multipart_upload()
                    .bucket(&self.bucket)
                    .key(&self.key)
                    .upload_id(&upload_id)
                    .multipart_upload(
                        CompletedMultipartUpload::builder()
                            .set_parts(Some(std::mem::take(&mut self.parts)))
                            .build(),
                    )
                    .send()
                    .await
                    .map(|_| ())
                    .map_err(|e| std::io::Error::other(sdk_err_msg(&e)))
            }
        }
    }
}

/// What is needed to abort an incomplete multipart upload if the writer is
/// dropped mid-transfer (cancel / error) — otherwise orphaned parts linger
/// (and bill) on the provider.
pub(super) struct AbortSlot {
    client: Client,
    bucket: String,
    key: String,
    upload_id: Mutex<Option<String>>,
}

impl AbortSlot {
    /// Record the upload that would need aborting, or clear it once the
    /// object is safely complete.
    pub(super) fn set_upload_id(&self, upload_id: Option<String>) {
        *self.upload_id.lock().unwrap() = upload_id;
    }

    /// Abort a multipart upload that never completed. Fire-and-forget: the
    /// writer is being dropped, so there is nobody left to report to.
    pub(super) fn abort_pending(self: &Arc<Self>) {
        let Some(upload_id) = self.upload_id.lock().unwrap().take() else {
            return;
        };
        let slot = self.clone();
        tokio::spawn(async move {
            let _ = slot
                .client
                .abort_multipart_upload()
                .bucket(&slot.bucket)
                .key(&slot.key)
                .upload_id(upload_id)
                .send()
                .await;
        });
    }
}
