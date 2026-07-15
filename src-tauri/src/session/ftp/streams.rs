use std::pin::Pin;
use std::task::{Context, Poll};

use futures::future::BoxFuture;
use futures::FutureExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::session::remote_fs::{BoxRead, BoxWrite};

use super::pool::PooledConn;

type DataStream = suppaftp::tokio::AsyncDataStream<suppaftp::tokio::AsyncRustlsStream>;

pub(super) fn reader(pooled: PooledConn, stream: DataStream) -> BoxRead {
    Box::new(FtpReader {
        state: ReaderState::Reading(Box::new((pooled, stream))),
    })
}

pub(super) fn writer(pooled: PooledConn, stream: DataStream) -> BoxWrite {
    Box::new(FtpWriter {
        state: WriterState::Writing(Box::new((pooled, stream))),
    })
}

enum ReaderState {
    Reading(Box<(PooledConn, DataStream)>),
    Finalizing(BoxFuture<'static, std::io::Result<()>>),
    Done,
}

/// Read stream owning its pooled connection. EOF is returned only after the
/// server's final transfer reply has been read. A mid-stream drop simply
/// closes the data and control connections (the pool slot frees via the
/// permit).
struct FtpReader {
    state: ReaderState,
}

impl AsyncRead for FtpReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            match &mut self.state {
                ReaderState::Reading(inner) => {
                    let before = buf.filled().len();
                    match Pin::new(&mut inner.1).poll_read(cx, buf) {
                        Poll::Ready(Ok(())) if buf.filled().len() == before => {
                            let ReaderState::Reading(inner) =
                                std::mem::replace(&mut self.state, ReaderState::Done)
                            else {
                                unreachable!()
                            };
                            let (mut pooled, stream) = *inner;
                            self.state = ReaderState::Finalizing(
                                async move {
                                    let conn = pooled
                                        .conn
                                        .as_mut()
                                        .ok_or_else(|| std::io::Error::other("connection gone"))?;
                                    conn.finalize_retr_stream(stream)
                                        .await
                                        .map_err(std::io::Error::other)?;
                                    drop(pooled);
                                    Ok(())
                                }
                                .boxed(),
                            );
                        }
                        other => return other,
                    }
                }
                ReaderState::Finalizing(future) => {
                    let result = futures::ready!(future.as_mut().poll(cx));
                    self.state = ReaderState::Done;
                    return Poll::Ready(result);
                }
                ReaderState::Done => return Poll::Ready(Ok(())),
            }
        }
    }
}

enum WriterState {
    Writing(Box<(PooledConn, DataStream)>),
    Finalizing(BoxFuture<'static, std::io::Result<()>>),
    Done,
}

/// Write stream owning its pooled connection; `shutdown()` finalizes the
/// transfer (waits for the server's 226).
struct FtpWriter {
    state: WriterState,
}

impl AsyncWrite for FtpWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match &mut self.state {
            WriterState::Writing(inner) => Pin::new(&mut inner.1).poll_write(cx, buf),
            _ => Poll::Ready(Err(std::io::Error::other("write after shutdown"))),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match &mut self.state {
            WriterState::Writing(inner) => Pin::new(&mut inner.1).poll_flush(cx),
            _ => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        loop {
            match &mut self.state {
                WriterState::Writing(_) => {
                    let WriterState::Writing(inner) =
                        std::mem::replace(&mut self.state, WriterState::Done)
                    else {
                        unreachable!()
                    };
                    let (mut pooled, stream) = *inner;
                    self.state = WriterState::Finalizing(
                        async move {
                            let conn = pooled
                                .conn
                                .as_mut()
                                .ok_or_else(|| std::io::Error::other("connection gone"))?;
                            conn.finalize_put_stream(stream)
                                .await
                                .map_err(std::io::Error::other)?;
                            drop(pooled);
                            Ok(())
                        }
                        .boxed(),
                    );
                }
                WriterState::Finalizing(future) => {
                    let result = futures::ready!(future.as_mut().poll(cx));
                    self.state = WriterState::Done;
                    return Poll::Ready(result);
                }
                WriterState::Done => return Poll::Ready(Ok(())),
            }
        }
    }
}
