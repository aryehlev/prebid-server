//! Integration test: start the QUIC auction server on an ephemeral port with
//! a self-signed certificate and a [`StubAuctionHandler`], then POST `{}` to
//! `/openrtb2/auction` using [`QuicAuctionClient`] and assert the response
//! body matches.
//!
//! This round-trip exercises a non-trivial amount of `quinn` + `h3`
//! plumbing. It is hermetic: the server binds an ephemeral port on
//! `127.0.0.1`, a self-signed cert with `localhost` as SAN is generated in
//! process, and the client connects back via UDP loopback. The rustls ring
//! crypto provider is installed explicitly so the test is robust to other
//! tests in the process having (or not having) already installed one.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use experimental_quic::{
    client::QuicAuctionClient, tls::generate_self_signed, QuicAuctionServer, QuicServerConfig,
    StubAuctionHandler,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quic_post_auction_roundtrip() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (cert_der, key_der) = generate_self_signed("localhost");
    let bind_addr = "127.0.0.1:0".parse().unwrap();
    let config = QuicServerConfig::new(bind_addr, cert_der, key_der, vec![]);

    let server = QuicAuctionServer::bind(config).expect("bind server");
    let addr = server.local_addr();

    let server_task = tokio::spawn(async move {
        server.run(Arc::new(StubAuctionHandler::new())).await;
    });

    // Give the server a moment to settle before the client reaches out.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let body = Bytes::from_static(b"{}");
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        QuicAuctionClient::post(addr, body.clone()),
    )
    .await
    .expect("client did not time out")
    .expect("client round-trip");

    assert_eq!(response, body);

    server_task.abort();
    let _ = server_task.await;
}
