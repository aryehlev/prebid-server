//! Remote ads-cert signer.
//!
//! A thin HTTP client that POSTs a [`SignatureRequest`] to an ads-cert
//! signing server and parses the returned [`SignatureResponse`]. This maps
//! directly onto the Go `experiment/adscert/remotesigner.go` transport
//! layer, albeit over HTTP/JSON rather than gRPC to stay within the
//! workspace's existing dependency footprint.
//!
//! The semantics intentionally mirror the Go side:
//!
//! * `SignatureStatus::Success` means the remote produced a usable
//!   signature string.
//! * `InProgress` / `Unknown` / `SignatureUnavailable` are returned as-is
//!   so the caller can decide whether to retry or fall back.
//!
//! The signer is "near-real" in the sense that the wire protocol is
//! complete (HTTP, JSON body, status codes, timeouts) but the server side
//! is assumed to exist out-of-process.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Status codes returned by the ads-cert signing server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignatureStatus {
    /// A signature was successfully produced.
    Success,
    /// The server did not recognise the request.
    Unknown,
    /// Signing is still in progress; the caller should retry.
    InProgress,
    /// Signing is permanently unavailable for this request.
    SignatureUnavailable,
}

/// The body POSTed to the remote signer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureRequest {
    /// The destination URL that the payload is about to be sent to.
    pub destination_url: String,
    /// The raw request body that will be sent.
    #[serde(with = "base64_bytes")]
    pub body: Vec<u8>,
    /// Client-side wall-clock timestamp in milliseconds since the Unix
    /// epoch. Sent to the server for freshness checks.
    pub timestamp_ms: i64,
}

/// The body returned by the remote signer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureResponse {
    /// Outcome of the signing attempt.
    pub status: SignatureStatus,
    /// The signed header value, when `status == Success`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_message: Option<String>,
    /// Optional debug / request-info string returned by the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_info: Option<String>,
}

/// Errors that can be raised by [`RemoteSigner::sign`].
#[derive(Debug, Error)]
pub enum SignError {
    /// Transport-level HTTP failure (connection refused, TLS, ...).
    #[error("http transport error: {0}")]
    Http(String),
    /// The server replied with a non-success status code.
    #[error("remote signer returned status {0}")]
    BadStatus(u16),
    /// The response body could not be decoded as [`SignatureResponse`].
    #[error("failed to decode response body: {0}")]
    Decode(String),
    /// The request did not complete within the configured timeout.
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
}

/// A blocking-free remote signer using `reqwest::Client`.
#[derive(Debug, Clone)]
pub struct RemoteSigner {
    /// Underlying HTTP client.
    pub client: reqwest::Client,
    /// Base endpoint (e.g. `http://localhost:3000`). `signRequest` is
    /// appended when a call is made.
    pub endpoint: String,
    /// Per-request timeout.
    pub timeout: Duration,
}

impl RemoteSigner {
    /// Construct a new remote signer targeting `endpoint`, enforcing the
    /// supplied per-request timeout. The underlying `reqwest::Client` is
    /// configured with the same timeout.
    pub fn new(endpoint: impl Into<String>, timeout: Duration) -> Result<Self, SignError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| SignError::Http(e.to_string()))?;
        Ok(Self {
            client,
            endpoint: endpoint.into(),
            timeout,
        })
    }

    /// Send a signing request to `{endpoint}/signRequest` and parse the
    /// reply.
    pub async fn sign(&self, req: &SignatureRequest) -> Result<SignatureResponse, SignError> {
        let url = format!("{}/signRequest", self.endpoint.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    SignError::Timeout(self.timeout)
                } else {
                    SignError::Http(e.to_string())
                }
            })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(SignError::BadStatus(status.as_u16()));
        }

        // Pull the body as text first so that parse errors carry the raw
        // payload in their message, which helps debugging.
        let text = resp
            .text()
            .await
            .map_err(|e| SignError::Http(e.to_string()))?;
        serde_json::from_str::<SignatureResponse>(&text)
            .map_err(|e| SignError::Decode(format!("{e}: {text}")))
    }
}

/// Serde adapter that encodes `Vec<u8>` as base64 on the wire, so the
/// signing request body stays JSON-safe regardless of its contents. Uses
/// the `base64` crate via `serde_json::Value` to avoid an extra direct
/// dependency.
mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Alphabet matching RFC 4648 without padding; implemented inline to
    /// avoid pulling in a new crate dependency.
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
        let mut i = 0;
        while i + 3 <= bytes.len() {
            let b = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
            out.push(ALPHABET[((b >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((b >> 12) & 0x3f) as usize] as char);
            out.push(ALPHABET[((b >> 6) & 0x3f) as usize] as char);
            out.push(ALPHABET[(b & 0x3f) as usize] as char);
            i += 3;
        }
        match bytes.len() - i {
            0 => {}
            1 => {
                let b = (bytes[i] as u32) << 16;
                out.push(ALPHABET[((b >> 18) & 0x3f) as usize] as char);
                out.push(ALPHABET[((b >> 12) & 0x3f) as usize] as char);
                out.push('=');
                out.push('=');
            }
            2 => {
                let b = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
                out.push(ALPHABET[((b >> 18) & 0x3f) as usize] as char);
                out.push(ALPHABET[((b >> 12) & 0x3f) as usize] as char);
                out.push(ALPHABET[((b >> 6) & 0x3f) as usize] as char);
                out.push('=');
            }
            _ => unreachable!(),
        }
        out
    }

    fn decode(s: &str) -> Result<Vec<u8>, String> {
        let s = s.trim();
        let mut table = [255u8; 256];
        for (i, &c) in ALPHABET.iter().enumerate() {
            table[c as usize] = i as u8;
        }
        let mut out = Vec::with_capacity(s.len() / 4 * 3);
        let bytes = s.as_bytes();
        let mut i = 0;
        while i + 4 <= bytes.len() {
            let mut buf = [0u8; 4];
            let mut pad = 0usize;
            for (k, b) in bytes[i..i + 4].iter().enumerate() {
                if *b == b'=' {
                    pad += 1;
                    buf[k] = 0;
                } else {
                    let v = table[*b as usize];
                    if v == 255 {
                        return Err(format!("invalid base64 char '{}'", *b as char));
                    }
                    buf[k] = v;
                }
            }
            let n = ((buf[0] as u32) << 18)
                | ((buf[1] as u32) << 12)
                | ((buf[2] as u32) << 6)
                | (buf[3] as u32);
            out.push((n >> 16) as u8);
            if pad < 2 {
                out.push((n >> 8) as u8);
            }
            if pad < 1 {
                out.push(n as u8);
            }
            i += 4;
        }
        if i != bytes.len() {
            return Err("invalid base64 length".to_string());
        }
        Ok(out)
    }

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(de)?;
        decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// Spin up a one-shot HTTP server on an ephemeral port. The server
    /// accepts a single connection, reads until it sees `\r\n\r\n` plus any
    /// body implied by `Content-Length`, and then writes a canned
    /// `SignatureResponse` back.
    async fn spawn_stub_server() -> (SocketAddr, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");

        let handle = tokio::spawn(async move {
            let (mut sock, _peer) = listener.accept().await.expect("accept");
            let mut buf = Vec::with_capacity(4096);
            let mut tmp = [0u8; 1024];

            // Read until we find the header/body boundary.
            let header_end;
            loop {
                let n = sock.read(&mut tmp).await.expect("read");
                if n == 0 {
                    panic!("unexpected EOF while reading request headers");
                }
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = pos + 4;
                    break;
                }
            }

            // Parse Content-Length out of the headers.
            let header_text =
                std::str::from_utf8(&buf[..header_end]).expect("ascii headers");
            let mut content_length = 0usize;
            for line in header_text.split("\r\n") {
                if let Some(rest) = line.strip_prefix("Content-Length:") {
                    content_length = rest.trim().parse().unwrap_or(0);
                } else if let Some(rest) = line.strip_prefix("content-length:") {
                    content_length = rest.trim().parse().unwrap_or(0);
                }
            }
            // Read the rest of the body.
            while buf.len() < header_end + content_length {
                let n = sock.read(&mut tmp).await.expect("read body");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
            }

            // Write a fake signature response.
            let body = r#"{"status":"SUCCESS","signature_message":"sig=deadbeef","request_info":"ok"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            sock.write_all(response.as_bytes()).await.expect("write");
            sock.flush().await.ok();
            sock.shutdown().await.ok();

            buf
        });

        (addr, handle)
    }

    #[tokio::test]
    async fn remote_signer_parses_success_response() {
        let (addr, srv) = spawn_stub_server().await;
        let endpoint = format!("http://{}", addr);

        let signer = RemoteSigner::new(endpoint, Duration::from_secs(2)).expect("build signer");
        let req = SignatureRequest {
            destination_url: "https://example.com/bid".to_string(),
            body: b"{\"ping\":true}".to_vec(),
            timestamp_ms: 1_700_000_000_000,
        };

        let resp = signer.sign(&req).await.expect("sign ok");
        assert_eq!(resp.status, SignatureStatus::Success);
        assert_eq!(resp.signature_message.as_deref(), Some("sig=deadbeef"));
        assert_eq!(resp.request_info.as_deref(), Some("ok"));

        // Inspect what the server received: must be a POST to /signRequest
        // with a JSON body containing our destination URL.
        let received = srv.await.expect("server task");
        let text = String::from_utf8(received).expect("utf8");
        assert!(text.starts_with("POST /signRequest"), "request line was: {}", text);
        assert!(text.contains("example.com/bid"), "body missing: {}", text);
    }

    #[test]
    fn signature_status_round_trips() {
        let json = serde_json::to_string(&SignatureStatus::InProgress).unwrap();
        assert_eq!(json, "\"IN_PROGRESS\"");
        let back: SignatureStatus = serde_json::from_str("\"SIGNATURE_UNAVAILABLE\"").unwrap();
        assert_eq!(back, SignatureStatus::SignatureUnavailable);
    }

    #[test]
    fn base64_round_trip() {
        let req = SignatureRequest {
            destination_url: "https://dest".to_string(),
            body: vec![0u8, 1, 2, 3, 255, 254],
            timestamp_ms: 1,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: SignatureRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.body, req.body);
    }
}
