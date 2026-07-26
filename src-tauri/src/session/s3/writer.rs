//! Streaming upload: buffered multipart with a plain `PutObject` fast path.
//!
//! The writer buffers up to [`PART_SIZE`] bytes before it commits to a
//! multipart upload, so small objects cost a single request. The content type
//! is applied to whichever path the object ends up taking — `PutObject` and
//! `CreateMultipartUpload` are the only two requests that can carry it.
//!
//! The type the name implies is fixed when the writer is created. When the
//! name implies nothing, the bytes are asked instead: both requests are built
//! while the head of the object is still buffered, so sniffing costs no extra
//! read.
//!
//! This module is the [`AsyncWrite`] state machine; [`inner`] holds the
//! requests it drives.

mod inner;

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use aws_sdk_s3::types::ObjectCannedAcl;
use aws_sdk_s3::Client;
use futures::future::BoxFuture;
use futures::FutureExt;
use tokio::io::AsyncWrite;

use inner::{AbortSlot, WriterInner};

/// Multipart part size. Must be ≥ 5 MiB (S3 minimum for non-final parts).
pub(super) const PART_SIZE: usize = 8 * 1024 * 1024;

enum WriterState {
    Ready(Box<WriterInner>),
    /// A part upload in flight; the inner state comes back when it lands.
    Busy(BoxFuture<'static, std::io::Result<Box<WriterInner>>>),
    Finishing(BoxFuture<'static, std::io::Result<()>>),
    Done,
    Failed,
}

pub(super) struct S3Writer {
    state: WriterState,
    abort: Arc<AbortSlot>,
}

impl S3Writer {
    pub(super) fn new(
        client: Client,
        bucket: String,
        key: String,
        acl: Option<ObjectCannedAcl>,
        content_type: Option<&'static str>,
    ) -> S3Writer {
        let (inner, abort) = WriterInner::new(client, bucket, key, acl, content_type);
        S3Writer {
            state: WriterState::Ready(inner),
            abort,
        }
    }

    /// Drive a Busy state to completion; returns Pending while in flight.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        loop {
            match &mut self.state {
                WriterState::Ready(_) | WriterState::Done => return Poll::Ready(Ok(())),
                WriterState::Busy(fut) => match futures::ready!(fut.as_mut().poll(cx)) {
                    Ok(inner) => {
                        self.abort.set_upload_id(inner.upload_id());
                        self.state = WriterState::Ready(inner);
                    }
                    Err(e) => {
                        self.state = WriterState::Failed;
                        return Poll::Ready(Err(e));
                    }
                },
                WriterState::Finishing(_) => {
                    return Poll::Ready(Err(std::io::Error::other("write after shutdown")))
                }
                WriterState::Failed => {
                    return Poll::Ready(Err(std::io::Error::other("upload already failed")))
                }
            }
        }
    }
}

impl AsyncWrite for S3Writer {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        futures::ready!(self.poll_ready(cx))?;
        let WriterState::Ready(inner) = &mut self.state else {
            return Poll::Ready(Err(std::io::Error::other("write after shutdown")));
        };
        inner.buffer(buf);
        if inner.buffered() >= PART_SIZE {
            let WriterState::Ready(inner) = std::mem::replace(&mut self.state, WriterState::Done)
            else {
                unreachable!()
            };
            self.state = WriterState::Busy(inner.flush_part().boxed());
            // Kick the upload off; the bytes are accepted either way and the
            // next poll_* call continues driving it.
            let _ = self.poll_ready(cx)?;
        }
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // Buffered bytes below the part threshold can only go out on
        // shutdown; "flush" just drains any in-flight part.
        self.poll_ready(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        loop {
            match &mut self.state {
                WriterState::Ready(_) => {
                    let WriterState::Ready(inner) =
                        std::mem::replace(&mut self.state, WriterState::Done)
                    else {
                        unreachable!()
                    };
                    self.state = WriterState::Finishing(inner.finish().boxed());
                }
                WriterState::Busy(_) => futures::ready!(self.poll_ready(cx))?,
                WriterState::Finishing(fut) => {
                    let result = futures::ready!(fut.as_mut().poll(cx));
                    self.state = if result.is_ok() {
                        self.abort.set_upload_id(None);
                        WriterState::Done
                    } else {
                        WriterState::Failed
                    };
                    return Poll::Ready(result);
                }
                WriterState::Done => return Poll::Ready(Ok(())),
                WriterState::Failed => {
                    return Poll::Ready(Err(std::io::Error::other("upload already failed")))
                }
            }
        }
    }
}

impl Drop for S3Writer {
    fn drop(&mut self) {
        // A multipart upload that never completed leaves billable orphaned
        // parts behind.
        self.abort.abort_pending();
    }
}
