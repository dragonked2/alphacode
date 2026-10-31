//! Bounded reads for newline-delimited client frames.
//!
//! The server protocol is newline-delimited JSON over a local socket (Unix
//! domain socket on Unix, named pipe on Windows). `AsyncBufReadExt::read_line`
//! appends until it sees a `\n`, so a client that never sends one can grow the
//! destination `String` without bound — a single missing newline turns into
//! unbounded allocation until the process is killed. Reading a whole frame into
//! memory before parsing it means the allocation happens on the server's side,
//! so a local client can OOM the server rather than itself.
//!
//! [`FrameReader`] caps the read. Once the cap is exceeded the reader stops
//! appending and drains the remainder of the oversized frame before reporting
//! it, which keeps the stream in sync: the next call returns the first frame
//! after the oversized one instead of a fragment of a torn line.

use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Maximum size of a single client request frame, in bytes.
///
/// Requests are JSON envelopes dispatched to tool and session handlers. Real
/// requests are small — the largest legitimate payloads are long stdin batches
/// and file writes, which land in the low hundreds of KB. 8 MiB leaves a wide
/// margin over any real client while still bounding a hostile or wedged client
/// to a fixed allocation instead of an OOM.
pub(crate) const MAX_CLIENT_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Outcome of a bounded frame read.
#[derive(Debug)]
pub(crate) enum FrameRead {
    /// A complete frame (newline seen) that is within the size cap.
    Line(String),
    /// The frame exceeded [`MAX_CLIENT_FRAME_BYTES`]. The oversized frame has
    /// been fully drained, so the stream is positioned at the next frame.
    TooLarge { limit: usize },
    /// The peer closed the connection cleanly on a frame boundary.
    Eof,
}

/// State survives cancellation of a read by `select!`, including while an
/// oversized frame is being drained. Each connection owns one instance.
#[derive(Default)]
pub(crate) struct FrameReader {
    buffer: Vec<u8>,
    oversized: bool,
}

impl FrameReader {
    /// Read a frame while storing at most `limit` bytes. Oversized frames are
    /// drained through their newline so the next request remains aligned.
    pub(crate) async fn read<R>(
        &mut self,
        reader: &mut R,
        limit: usize,
    ) -> std::io::Result<FrameRead>
    where
        R: AsyncBufRead + Unpin,
    {
        loop {
            // fill_buf is cancellation safe. Update our persistent state and
            // consume the bytes without an intervening await.
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                let oversized = std::mem::take(&mut self.oversized);
                self.buffer.clear();
                return Ok(if oversized {
                    FrameRead::TooLarge { limit }
                } else {
                    FrameRead::Eof
                });
            }
            let end = available.iter().position(|&byte| byte == b'\n');
            let count = end.map_or(available.len(), |index| index + 1);
            if !self.oversized {
                if count > limit.saturating_sub(self.buffer.len()) {
                    self.oversized = true;
                    self.buffer.clear();
                } else {
                    self.buffer.extend_from_slice(&available[..count]);
                }
            }
            reader.consume(count);
            if end.is_some() {
                if std::mem::take(&mut self.oversized) {
                    return Ok(FrameRead::TooLarge { limit });
                }
                let bytes = std::mem::take(&mut self.buffer);
                let decoded = String::from_utf8(bytes)
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                return Ok(FrameRead::Line(decoded));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn cancelled_reads_preserve_partial_frames() {
        let (mut writer, stream) = tokio::io::duplex(64);
        let mut reader = tokio::io::BufReader::new(stream);
        let mut frames = FrameReader::default();
        writer.write_all(b"{\"id\":").await.unwrap();
        assert!(
            timeout(Duration::from_millis(20), frames.read(&mut reader, 64))
                .await
                .is_err()
        );
        writer.write_all(b"1}\n").await.unwrap();
        assert!(
            matches!(frames.read(&mut reader, 64).await.unwrap(), FrameRead::Line(line) if line == "{\"id\":1}\n")
        );
    }

    #[tokio::test]
    async fn oversized_unterminated_frame_remains_bounded_across_cancellation() {
        let (mut writer, stream) = tokio::io::duplex(1024);
        let mut reader = tokio::io::BufReader::with_capacity(32, stream);
        let mut frames = FrameReader::default();
        writer.write_all(&[b'x'; 512]).await.unwrap();
        assert!(
            timeout(Duration::from_millis(20), frames.read(&mut reader, 16))
                .await
                .is_err()
        );
        assert!(frames.oversized);
        assert!(frames.buffer.len() <= 16);
        writer.write_all(b"\n{}\n").await.unwrap();
        assert!(matches!(
            frames.read(&mut reader, 16).await.unwrap(),
            FrameRead::TooLarge { limit: 16 }
        ));
        assert!(
            matches!(frames.read(&mut reader, 16).await.unwrap(), FrameRead::Line(line) if line == "{}\n")
        );
    }

    #[tokio::test]
    async fn truncated_frame_is_not_dispatched() {
        let mut reader = tokio::io::BufReader::new(&b"{\"id\":1}"[..]);
        assert!(matches!(
            FrameReader::default().read(&mut reader, 64).await.unwrap(),
            FrameRead::Eof
        ));
    }

    #[tokio::test]
    async fn invalid_utf8_is_rejected_instead_of_changing_request_content() {
        let mut reader = tokio::io::BufReader::new(&b"{\"text\":\"\xff\"}\n"[..]);
        let error = FrameReader::default()
            .read(&mut reader, 64)
            .await
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn reads_a_normal_frame() {
        let mut reader = tokio::io::BufReader::new(&b"{\"id\":1}\n"[..]);
        match FrameReader::default()
            .read(&mut reader, MAX_CLIENT_FRAME_BYTES)
            .await
            .expect("read ok")
        {
            FrameRead::Line(line) => assert_eq!(line, "{\"id\":1}\n"),
            other => panic!("expected a line, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reads_a_frame_at_exactly_the_limit() {
        let payload = "a".repeat(8);
        let frame = format!("{payload}\n");
        let mut reader = tokio::io::BufReader::new(frame.as_bytes());
        match FrameReader::default()
            .read(&mut reader, 9)
            .await
            .expect("read ok")
        {
            FrameRead::Line(line) => assert_eq!(line, frame),
            other => panic!("expected a line at the limit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_an_oversized_frame_and_resyncs() {
        // An oversized frame followed by a valid one. The oversized frame must
        // be reported as TooLarge and the valid frame must still be readable —
        // this is the property that keeps one hostile frame from poisoning the
        // whole connection.
        let mut stream = Vec::new();
        stream.extend_from_slice(&[b'x'; 64]);
        stream.push(b'\n');
        stream.extend_from_slice(b"{\"id\":2}\n");
        let mut reader = tokio::io::BufReader::new(&stream[..]);

        match FrameReader::default()
            .read(&mut reader, 16)
            .await
            .expect("read ok")
        {
            FrameRead::TooLarge { limit } => assert_eq!(limit, 16),
            other => panic!("expected TooLarge, got {other:?}"),
        }
        match FrameReader::default()
            .read(&mut reader, MAX_CLIENT_FRAME_BYTES)
            .await
            .expect("read ok")
        {
            FrameRead::Line(line) => assert_eq!(line, "{\"id\":2}\n"),
            other => panic!("expected the following frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unterminated_oversized_frame_does_not_hang() {
        // No newline anywhere: the drain loop must terminate on EOF rather than
        // waiting forever for a terminator that will never come.
        let stream = [b'x'; 128];
        let mut reader = tokio::io::BufReader::new(&stream[..]);
        match FrameReader::default()
            .read(&mut reader, 16)
            .await
            .expect("read ok")
        {
            FrameRead::TooLarge { limit } => assert_eq!(limit, 16),
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reports_eof_on_a_clean_close() {
        let mut reader = tokio::io::BufReader::new(&b""[..]);
        assert!(matches!(
            FrameReader::default()
                .read(&mut reader, MAX_CLIENT_FRAME_BYTES)
                .await
                .expect("read ok"),
            FrameRead::Eof
        ));
    }

    #[tokio::test]
    async fn handles_multibyte_content_at_the_limit() {
        // A frame of multi-byte characters must not panic or corrupt when the
        // limit lands mid-character, which is the failure mode the raw byte
        // slicing elsewhere in the codebase had.
        let frame = "é".repeat(8) + "\n";
        let mut reader = tokio::io::BufReader::new(frame.as_bytes());
        match FrameReader::default()
            .read(&mut reader, MAX_CLIENT_FRAME_BYTES)
            .await
            .expect("read ok")
        {
            FrameRead::Line(line) => assert_eq!(line, frame),
            other => panic!("expected a line, got {other:?}"),
        }
    }
}
