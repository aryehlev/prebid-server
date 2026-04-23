//! QUIC / HTTP/3 auction server built on `quinn` and `h3`.

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use http::{Method, Response, StatusCode};
use tracing::{debug, error, info, warn};

use crate::error::QuicError;
use crate::handler::AuctionHandler;

/// Configuration required to bind a [`QuicAuctionServer`].
#[derive(Debug, Clone)]
pub struct QuicServerConfig {
    /// Socket address to bind the UDP listener on.
    pub bind_addr: SocketAddr,
    /// Server certificate in DER format.
    pub cert_der: Vec<u8>,
    /// Server private key in DER format.
    pub key_der: Vec<u8>,
    /// ALPN protocols to advertise. Defaults to `h3` in [`Self::new`].
    pub alpn_protocols: Vec<Vec<u8>>,
}

impl QuicServerConfig {
    /// Construct a new configuration. If `alpn_protocols` is empty the
    /// standard HTTP/3 ALPN value (`h3`) is used.
    pub fn new(
        bind_addr: SocketAddr,
        cert_der: Vec<u8>,
        key_der: Vec<u8>,
        alpn_protocols: Vec<Vec<u8>>,
    ) -> Self {
        let alpn_protocols = if alpn_protocols.is_empty() {
            vec![b"h3".to_vec()]
        } else {
            alpn_protocols
        };
        Self {
            bind_addr,
            cert_der,
            key_der,
            alpn_protocols,
        }
    }

    /// Build a `rustls::ServerConfig` from this configuration.
    pub fn build_rustls_config(&self) -> Result<rustls::ServerConfig, QuicError> {
        let cert = rustls::pki_types::CertificateDer::from(self.cert_der.clone());
        let key = rustls::pki_types::PrivateKeyDer::try_from(self.key_der.clone())
            .map_err(|e| QuicError::Tls(format!("invalid private key: {e}")))?;

        let mut cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| QuicError::Rustls(e.to_string()))?
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| QuicError::Rustls(e.to_string()))?;

        cfg.alpn_protocols = self.alpn_protocols.clone();
        cfg.max_early_data_size = u32::MAX;
        Ok(cfg)
    }

    /// Build a `quinn::ServerConfig` suitable for QUIC + HTTP/3.
    pub fn build_quinn_config(&self) -> Result<quinn::ServerConfig, QuicError> {
        let rustls_cfg = self.build_rustls_config()?;
        let quic_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(rustls_cfg)
            .map_err(|e| QuicError::Rustls(format!("quinn crypto: {e}")))?;
        Ok(quinn::ServerConfig::with_crypto(Arc::new(quic_crypto)))
    }
}

/// A QUIC auction server.
///
/// Thin wrapper around a [`quinn::Endpoint`]. Construct one with
/// [`QuicAuctionServer::bind`] and then drive it via [`QuicAuctionServer::run`]
/// with an implementation of [`AuctionHandler`].
pub struct QuicAuctionServer {
    endpoint: quinn::Endpoint,
    local_addr: SocketAddr,
}

impl QuicAuctionServer {
    /// Bind the UDP socket and construct the underlying `quinn::Endpoint`.
    pub fn bind(config: QuicServerConfig) -> Result<Self, QuicError> {
        let quinn_cfg = config.build_quinn_config()?;
        let endpoint = quinn::Endpoint::server(quinn_cfg, config.bind_addr)?;
        let local_addr = endpoint.local_addr()?;
        info!(addr = %local_addr, "experimental-quic server bound");
        Ok(Self {
            endpoint,
            local_addr,
        })
    }

    /// Returns the actual local address the endpoint bound to.
    ///
    /// This is particularly useful when `bind_addr` uses port `0` to request
    /// an ephemeral port – the real port can be read back here.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Run the accept loop until the endpoint is closed.
    ///
    /// Each accepted QUIC connection is driven on its own Tokio task. Within
    /// a connection we accept HTTP/3 requests, dispatch POST
    /// `/openrtb2/auction` to `handler`, and reply with either the handler's
    /// body on success or an appropriate status on failure. All other
    /// method/path combinations respond `404 Not Found`.
    pub async fn run<H: AuctionHandler>(self, handler: Arc<H>) {
        while let Some(incoming) = self.endpoint.accept().await {
            let handler = handler.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        if let Err(e) = handle_connection(connection, handler).await {
                            warn!(error = %e, "connection terminated with error");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to accept quic connection");
                    }
                }
            });
        }
        debug!("experimental-quic accept loop exited");
    }

    /// Close the underlying endpoint, ending the accept loop.
    pub fn close(&self) {
        self.endpoint
            .close(0u32.into(), b"experimental-quic server shutting down");
    }
}

async fn handle_connection<H: AuctionHandler>(
    connection: quinn::Connection,
    handler: Arc<H>,
) -> Result<(), QuicError> {
    let h3_conn = h3::server::Connection::new(h3_quinn::Connection::new(connection))
        .await
        .map_err(|e| QuicError::H3(e.to_string()))?;
    let mut h3_conn = h3_conn;

    loop {
        match h3_conn.accept().await {
            Ok(Some((req, stream))) => {
                let handler = handler.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_request(req, stream, handler).await {
                        warn!(error = %e, "request handler error");
                    }
                });
            }
            Ok(None) => {
                // Graceful close from the peer.
                break;
            }
            Err(e) => {
                // h3 errors are already descriptive; only log non-graceful.
                error!(error = %e, "h3 accept error");
                break;
            }
        }
    }
    Ok(())
}

async fn handle_request<H, S>(
    req: http::Request<()>,
    mut stream: h3::server::RequestStream<S, Bytes>,
    handler: Arc<H>,
) -> Result<(), QuicError>
where
    H: AuctionHandler,
    S: h3::quic::BidiStream<Bytes>,
{
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    debug!(%method, %path, "experimental-quic request");

    if method != Method::POST || path != "/openrtb2/auction" {
        let resp = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(())
            .expect("building 404 response");
        stream
            .send_response(resp)
            .await
            .map_err(|e| QuicError::H3(e.to_string()))?;
        stream
            .finish()
            .await
            .map_err(|e| QuicError::H3(e.to_string()))?;
        return Ok(());
    }

    // Drain the request body.
    let mut body = BytesMut::new();
    loop {
        match stream
            .recv_data()
            .await
            .map_err(|e| QuicError::H3(e.to_string()))?
        {
            Some(mut chunk) => {
                while chunk.has_remaining() {
                    let bytes = chunk.chunk();
                    body.extend_from_slice(bytes);
                    let len = bytes.len();
                    chunk.advance(len);
                }
            }
            None => break,
        }
    }

    let response_bytes = match handler.handle(body.freeze()).await {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "auction handler returned error");
            let status = match e {
                crate::handler::HandlerError::BadRequest(_) => StatusCode::BAD_REQUEST,
                crate::handler::HandlerError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let resp = Response::builder()
                .status(status)
                .body(())
                .expect("building error response");
            stream
                .send_response(resp)
                .await
                .map_err(|e| QuicError::H3(e.to_string()))?;
            stream
                .finish()
                .await
                .map_err(|e| QuicError::H3(e.to_string()))?;
            return Ok(());
        }
    };

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(())
        .expect("building 200 response");
    stream
        .send_response(resp)
        .await
        .map_err(|e| QuicError::H3(e.to_string()))?;
    stream
        .send_data(response_bytes)
        .await
        .map_err(|e| QuicError::H3(e.to_string()))?;
    stream
        .finish()
        .await
        .map_err(|e| QuicError::H3(e.to_string()))?;
    Ok(())
}

// `Buf` is needed for `.has_remaining()` / `.chunk()` / `.advance()` above.
use bytes::Buf;
