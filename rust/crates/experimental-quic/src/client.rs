//! A minimal HTTP/3 client used by integration tests.
//!
//! This client is **not** production-grade – it skips certificate verification
//! and exists purely so we can round-trip a request against
//! [`crate::server::QuicAuctionServer`] inside a single test process.

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::{Buf, Bytes, BytesMut};
use http::Request;

use crate::error::QuicError;

/// A minimal HTTP/3 client used to drive integration tests.
pub struct QuicAuctionClient;

impl QuicAuctionClient {
    /// POST `body` to `/openrtb2/auction` on the server at `addr` and return
    /// the raw response body.
    ///
    /// Uses a `NoVerify` TLS verifier – only suitable for tests.
    pub async fn post(addr: SocketAddr, body: Bytes) -> Result<Bytes, QuicError> {
        let crypto = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| QuicError::Rustls(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(danger::NoVerify))
        .with_no_client_auth();

        let mut crypto = crypto;
        crypto.alpn_protocols = vec![b"h3".to_vec()];

        let quic_client_cfg = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(|e| QuicError::Rustls(format!("quinn client crypto: {e}")))?;
        let client_cfg = quinn::ClientConfig::new(Arc::new(quic_client_cfg));

        let bind: SocketAddr = if addr.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        let mut endpoint = quinn::Endpoint::client(bind)?;
        endpoint.set_default_client_config(client_cfg);

        let connecting = endpoint
            .connect(addr, "localhost")
            .map_err(|e| QuicError::Connection(e.to_string()))?;
        let connection = connecting
            .await
            .map_err(|e| QuicError::Connection(e.to_string()))?;

        let quinn_conn = h3_quinn::Connection::new(connection);
        let (mut driver, mut send_request) =
            h3::client::new(quinn_conn)
                .await
                .map_err(|e| QuicError::H3(e.to_string()))?;

        let drive = tokio::spawn(async move {
            let _ = futures::future::poll_fn(|cx| driver.poll_close(cx)).await;
        });

        let req = Request::builder()
            .method("POST")
            .uri(format!("https://localhost{}/openrtb2/auction", ""))
            .header("content-type", "application/json")
            .body(())
            .expect("build request");

        let mut stream = send_request
            .send_request(req)
            .await
            .map_err(|e| QuicError::H3(e.to_string()))?;
        stream
            .send_data(body)
            .await
            .map_err(|e| QuicError::H3(e.to_string()))?;
        stream
            .finish()
            .await
            .map_err(|e| QuicError::H3(e.to_string()))?;

        let _resp = stream
            .recv_response()
            .await
            .map_err(|e| QuicError::H3(e.to_string()))?;

        let mut body = BytesMut::new();
        while let Some(mut chunk) = stream
            .recv_data()
            .await
            .map_err(|e| QuicError::H3(e.to_string()))?
        {
            while chunk.has_remaining() {
                let slice = chunk.chunk();
                body.extend_from_slice(slice);
                let n = slice.len();
                chunk.advance(n);
            }
        }

        drop(send_request);
        endpoint.wait_idle().await;
        let _ = drive.await;

        Ok(body.freeze())
    }
}

mod danger {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, SignatureScheme};

    /// Accepts any server certificate. **Tests only.**
    #[derive(Debug)]
    pub struct NoVerify;

    impl ServerCertVerifier for NoVerify {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::RSA_PKCS1_SHA384,
                SignatureScheme::RSA_PKCS1_SHA512,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ECDSA_NISTP384_SHA384,
                SignatureScheme::ECDSA_NISTP521_SHA512,
                SignatureScheme::RSA_PSS_SHA256,
                SignatureScheme::RSA_PSS_SHA384,
                SignatureScheme::RSA_PSS_SHA512,
                SignatureScheme::ED25519,
                SignatureScheme::ED448,
            ]
        }
    }
}
