//! Shared transport-level error classification for provider runtimes.
//!
//! Every provider (Anthropic, OpenAI, Gemini, Cursor, ...) needs to decide
//! whether a request failure is a transient transport fault worth retrying on
//! a fresh connection, or a real error to surface. Keeping the classifier here
//! ensures all providers recognize the same fault vocabulary.

use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Result;
use memchr::memmem;

/// Send a request while bounding the wait for response headers.
///
/// Reqwest's `send` future covers connection establishment and the wait for
/// response headers, but streaming body reads happen after it resolves. Provider
/// stream loops apply their own idle timeout to those body reads, so using the
/// same budget here closes the zero-response-bytes gap without imposing an
/// overall deadline on a legitimately long stream.
pub async fn send_with_initial_response_timeout(
    request: reqwest::RequestBuilder,
    timeout: Duration,
) -> Result<reqwest::Response> {
    match tokio::time::timeout(timeout, request.send()).await {
        Ok(response) => Ok(response?),
        Err(_) => anyhow::bail!(
            "Initial response timeout: no response headers received within {timeout:?}"
        ),
    }
}

/// Whether an error message describes a transient transport-level fault
/// (connection reset, DNS hiccup, TLS teardown, HTTP/2 stream error, ...)
/// that is likely to succeed on retry with a fresh connection.
///
/// Implementation notes:
///   - Patterns are stored in lowercase and matched with `memmem::Finder`,
///     which is SIMD-accelerated (typically 10-50x faster than `str::contains`
///     for short needles on long haystacks).
///   - The ASCII-lowercased view of the input is built into a 512-byte stack
///     buffer; only inputs longer than that fall back to a heap allocation
///     (rare for error messages).
pub fn is_transient_transport_error(error_str: &str) -> bool {
    let mut stack_buf = [0u8; 512];
    let lowered: Cow<[u8]> = ascii_lower_into(error_str, &mut stack_buf);
    for finder in TRANSIENT_FINDERS.get_or_init(build_finders) {
        if finder.find(lowered.as_ref()).is_some() {
            return true;
        }
    }
    false
}

/// Lowercases ASCII bytes of `input` into `buf` and returns the populated
/// prefix. If the input is longer than the buffer, allocates once on the
/// heap for the same purpose. The returned slice never aliases `input`.
fn ascii_lower_into<'a>(input: &str, buf: &'a mut [u8]) -> Cow<'a, [u8]> {
    let bytes = input.as_bytes();
    if bytes.len() > buf.len() {
        let mut owned = Vec::with_capacity(bytes.len());
        for &b in bytes {
            owned.push(b.to_ascii_lowercase());
        }
        Cow::Owned(owned)
    } else {
        for (dst, &src) in buf.iter_mut().zip(bytes.iter()) {
            *dst = src.to_ascii_lowercase();
        }
        Cow::Borrowed(&buf[..bytes.len()])
    }
}

fn build_finders() -> Vec<memchr::memmem::Finder<'static>> {
    TRANSIENT_PATTERNS
        .iter()
        .map(|p| memchr::memmem::Finder::new(p.as_bytes()))
        .collect()
}

/// All transient transport patterns, lowercase, longest-first.
static TRANSIENT_PATTERNS: &[&str] = &[
    // TLS / crypto failures
    "decryption failed or bad record mac",
    "fatal alert: badrecordmac",
    "fatal alert: bad_record_mac",
    "received fatal alert: badrecordmac",
    "received fatal alert: bad_record_mac",
    "tls handshake eof",
    "tls alert",
    "ssl error",
    "ssl_error",
    "certificate verify failed",
    "cert has expired",
    // DNS / network
    "temporary failure in name resolution",
    "failed to lookup address information",
    "name or service not known",
    "no route to host",
    "network is unreachable",
    "host is unreachable",
    "dns error",
    "dns probe",
    // Connection lifecycle
    "client error (connect)",
    "connection reset",
    "connection closed",
    "connection refused",
    "connection aborted",
    "connection error",
    "connection abort",
    "connection pool closed",
    "broken connection",
    "broken pipe",
    "peer closed connection",
    "eof before message",
    "channel closed",
    "channel receive error",
    "socket closed",
    // HTTP/2
    "http2 error",
    "go away",
    "goaway",
    "stream error",
    "refused_stream",
    "refused stream",
    "unspecific protocol error",
    "protocol error",
    "enhance_your_calm",
    // Body / stream errors
    "request or response body error",
    "incomplete message",
    "unexpected eof",
    "close_notify",
    "error decoding",
    "error reading",
    // Timeout
    "operation timed out",
    "timed out",
    "timeout",
    "io error",
    // Server overload signals
    "server is overloaded",
    "server overload",
    "server too busy",
    "capacity exceeded",
    "try again later",
    "service temporarily unavailable",
];

static TRANSIENT_FINDERS: OnceLock<Vec<memmem::Finder<'static>>> = OnceLock::new();

#[cfg(test)]
mod tests {
    use super::{is_transient_transport_error, send_with_initial_response_timeout};
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn accepted_request_without_response_headers_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind blackhole server");
        let address = listener.local_addr().expect("read blackhole address");
        let (request_seen_tx, request_seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set request read timeout");
            let mut request = [0_u8; 4096];
            let bytes_read = stream.read(&mut request).expect("read request");
            assert!(bytes_read > 0, "client should send request bytes");
            request_seen_tx.send(()).expect("report request received");

            // Keep the socket open without sending status or response headers.
            let _ = release_rx.recv_timeout(Duration::from_secs(2));
        });

        let request = reqwest::Client::new()
            .post(format!("http://{address}/chat/completions"))
            .body("{}");
        let timeout = Duration::from_millis(250);
        let started = Instant::now();
        let error = send_with_initial_response_timeout(request, timeout)
            .await
            .expect_err("blackholed response should time out");

        assert!(
            error.to_string().contains("Initial response timeout"),
            "unexpected error: {error:#}"
        );
        assert!(
            is_transient_transport_error(&error.to_string()),
            "initial response timeouts must enter provider retry machinery"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "initial response wait exceeded its timeout"
        );
        request_seen_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("server should receive the request before the timeout");

        let _ = release_tx.send(());
        server.join().expect("blackhole server should exit");
    }

    #[test]
    fn http2_stream_protocol_error_is_transient() {
        // Exact shape reqwest/h2 surfaces for a reset on a reused HTTP/2 connection.
        let msg = "error sending request for url (https://api.anthropic.com/v1/messages): \
                   client error (SendRequest): http2 error: stream error received: \
                   unspecific protocol error detected";
        assert!(is_transient_transport_error(msg));
    }

    #[test]
    fn http2_goaway_and_refused_stream_are_transient() {
        assert!(is_transient_transport_error("http2 error: GOAWAY received"));
        assert!(is_transient_transport_error("stream error: REFUSED_STREAM"));
    }

    #[test]
    fn auth_errors_are_not_transient() {
        assert!(!is_transient_transport_error("401 unauthorized"));
        assert!(!is_transient_transport_error("invalid x-api-key"));
    }

    /// Real transport-error shapes harvested from ~/.alphacode/logs.
    #[test]
    fn real_world_transport_errors_are_transient() {
        let real_errors = [
            "client error (Connect): dns error: failed to lookup address information: \
             Name or service not known",
            "client error (SendRequest): http2 error: keep-alive timed out: operation timed out",
            "client error (SendRequest): connection error: peer closed connection without \
             sending TLS close_notify: https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof",
            "client error (Connect): operation timed out",
            "client error (SendRequest): connection error: timed out",
            "client error (Connect): tls handshake eof",
            "error decoding response body: request or response body error: operation timed out",
        ];
        for error in real_errors {
            assert!(
                is_transient_transport_error(error),
                "should be transient: {error}"
            );
        }
    }
}
