//! Newline-delimited message framing for the shim, bounded like the MCP
//! server's.

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::agent::mcp::server::MAX_MESSAGE_BYTES;

pub(super) enum Line {
    Data,
    /// Longer than any message may be; skipped up to its end.
    TooLong,
    Eof,
}

/// Read one newline-terminated line (the newline included, when present).
pub(super) async fn read_line<R>(reader: &mut R, line: &mut Vec<u8>) -> std::io::Result<Line>
where
    R: AsyncBufRead + Unpin,
{
    let read = (&mut *reader)
        .take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_until(b'\n', line)
        .await?;
    if read == 0 {
        return Ok(Line::Eof);
    }
    if line.len() <= MAX_MESSAGE_BYTES {
        return Ok(Line::Data);
    }
    // Drop the rest of the oversized line so the next read starts clean.
    if line.last() != Some(&b'\n') {
        let mut rest = Vec::new();
        loop {
            rest.clear();
            let read = (&mut *reader)
                .take(64 * 1024)
                .read_until(b'\n', &mut rest)
                .await?;
            if read == 0 || rest.last() == Some(&b'\n') {
                break;
            }
        }
    }
    Ok(Line::TooLong)
}

pub(super) async fn write_line<W: AsyncWrite + Unpin>(
    writer: &mut W,
    line: &[u8],
) -> std::io::Result<()> {
    writer.write_all(line).await?;
    if line.last() != Some(&b'\n') {
        writer.write_all(b"\n").await?;
    }
    writer.flush().await
}
