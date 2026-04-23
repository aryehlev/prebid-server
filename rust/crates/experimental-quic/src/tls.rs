//! TLS helpers: PEM loading and self-signed cert generation for tests.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::error::QuicError;

/// Load a chain of certificates from a PEM file on disk.
///
/// Returns the raw DER bytes of each certificate in the chain.
pub fn load_certs<P: AsRef<Path>>(path: P) -> Result<Vec<Vec<u8>>, QuicError> {
    let file = File::open(path.as_ref())
        .map_err(|e| QuicError::Tls(format!("opening cert file: {e}")))?;
    let mut reader = BufReader::new(file);
    let mut certs = Vec::new();
    for item in rustls_pemfile::certs(&mut reader) {
        let cert = item.map_err(|e| QuicError::Tls(format!("parsing cert: {e}")))?;
        certs.push(cert.to_vec());
    }
    if certs.is_empty() {
        return Err(QuicError::Tls(format!(
            "no certificates found in {}",
            path.as_ref().display()
        )));
    }
    Ok(certs)
}

/// Load a PKCS#8 or PKCS#1 private key from a PEM file on disk.
///
/// Returns the raw DER bytes of the first key encountered.
pub fn load_key<P: AsRef<Path>>(path: P) -> Result<Vec<u8>, QuicError> {
    let file = File::open(path.as_ref())
        .map_err(|e| QuicError::Tls(format!("opening key file: {e}")))?;
    let mut reader = BufReader::new(file);

    for item in rustls_pemfile::read_all(&mut reader) {
        let item = item.map_err(|e| QuicError::Tls(format!("parsing key: {e}")))?;
        match item {
            rustls_pemfile::Item::Pkcs1Key(key) => return Ok(key.secret_pkcs1_der().to_vec()),
            rustls_pemfile::Item::Pkcs8Key(key) => return Ok(key.secret_pkcs8_der().to_vec()),
            rustls_pemfile::Item::Sec1Key(key) => return Ok(key.secret_sec1_der().to_vec()),
            _ => continue,
        }
    }
    Err(QuicError::Tls(format!(
        "no private key found in {}",
        path.as_ref().display()
    )))
}

/// Generate a self-signed certificate and private key for the given host.
///
/// This is intended for tests and local experiments only. Returns
/// `(cert_der, key_der)` where both are raw DER bytes suitable for direct use
/// with `rustls` / `quinn`.
pub fn generate_self_signed(host: &str) -> (Vec<u8>, Vec<u8>) {
    let certified = rcgen::generate_simple_self_signed(vec![host.to_string()])
        .expect("rcgen self-signed generation should succeed for a single hostname");
    let cert_der = certified.cert.der().to_vec();
    let key_der = certified.key_pair.serialize_der();
    (cert_der, key_der)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_self_signed_produces_nonempty_der() {
        let (cert, key) = generate_self_signed("localhost");
        assert!(!cert.is_empty(), "cert DER should be non-empty");
        assert!(!key.is_empty(), "key DER should be non-empty");
        // A DER-encoded X.509 certificate always begins with the SEQUENCE tag
        // (0x30) followed by a length octet. Sanity check that.
        assert_eq!(cert[0], 0x30, "cert should start with ASN.1 SEQUENCE tag");
        assert_eq!(key[0], 0x30, "key should start with ASN.1 SEQUENCE tag");
    }

    #[test]
    fn generate_self_signed_honours_host() {
        let (cert, _) = generate_self_signed("example.test");
        // Cheapest possible check: the hostname bytes appear in the DER.
        assert!(cert.windows(12).any(|w| w == b"example.test"));
    }
}
