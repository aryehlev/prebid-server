//! In-process ads-cert signer.
//!
//! This is a **stub** signer useful for local development and unit tests.
//! It produces a deterministic, opaque token derived from the request and
//! the signer's key material via Rust's `std::collections::hash_map::DefaultHasher`.
//!
//! NOTE: the output is **NOT** a cryptographic signature. It must never be
//! trusted by any production verifier. Use [`crate::adscert_remote::RemoteSigner`]
//! (or a future Rust crypto-backed implementation) for real traffic.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::adscert_remote::{SignatureRequest, SignatureResponse, SignatureStatus};

/// A fake ads-cert signer that runs entirely in this process.
///
/// The `key_id` is surfaced verbatim as part of the emitted signature
/// string so tests can distinguish between signer instances.
#[derive(Debug, Clone)]
pub struct InProcessSigner {
    /// Identifier for the "key" used to sign requests. Included verbatim
    /// in the output.
    pub key_id: String,
    /// 32-byte opaque key material. Hashed together with the request to
    /// produce the fake signature.
    pub secret: [u8; 32],
}

impl InProcessSigner {
    /// Construct an in-process signer with explicit key material.
    pub fn new(key_id: impl Into<String>, secret: [u8; 32]) -> Self {
        Self {
            key_id: key_id.into(),
            secret,
        }
    }

    /// Construct an in-process signer with a pseudo-random key based on
    /// the current process's allocation address and the wall clock. This
    /// is sufficient for tests; it is **not** cryptographically secure.
    pub fn new_random() -> Self {
        let mut secret = [0u8; 32];
        // Seed from address + time, then splat into the buffer.
        let base_ptr: Box<u8> = Box::new(0);
        let seed_addr = (&*base_ptr as *const u8) as u64;
        let seed_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let mut state: u64 = seed_addr ^ seed_time.wrapping_mul(0x9E3779B97F4A7C15);
        for chunk in secret.chunks_mut(8) {
            // Tiny LCG-ish mixer; purely for variation.
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let bytes = state.to_le_bytes();
            for (i, slot) in chunk.iter_mut().enumerate() {
                *slot = bytes[i];
            }
        }
        Self::new(format!("stub-{:016x}", seed_time), secret)
    }

    /// Produce a signature for the given request. Always returns
    /// `SignatureStatus::Success` with a deterministic signature string of
    /// the form `key=<key_id>; mac=<hex>; ts=<timestamp>`.
    pub fn sign(&self, req: &SignatureRequest) -> SignatureResponse {
        let mut hasher = DefaultHasher::new();
        self.key_id.hash(&mut hasher);
        self.secret.hash(&mut hasher);
        req.destination_url.hash(&mut hasher);
        req.body.hash(&mut hasher);
        req.timestamp_ms.hash(&mut hasher);
        let digest = hasher.finish();

        let signature_message = format!(
            "key={}; mac={:016x}; ts={}",
            self.key_id, digest, req.timestamp_ms
        );

        SignatureResponse {
            status: SignatureStatus::Success,
            signature_message: Some(signature_message),
            request_info: Some("in-process stub (not cryptographically secure)".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> SignatureRequest {
        SignatureRequest {
            destination_url: "https://example.com/bid".to_string(),
            body: b"hello".to_vec(),
            timestamp_ms: 42,
        }
    }

    #[test]
    fn deterministic_signature_for_same_request() {
        let signer = InProcessSigner::new("key-a", [7u8; 32]);
        let a = signer.sign(&sample_request());
        let b = signer.sign(&sample_request());
        assert_eq!(a.status, SignatureStatus::Success);
        assert_eq!(a.signature_message, b.signature_message);
        assert!(a
            .signature_message
            .as_deref()
            .unwrap()
            .starts_with("key=key-a;"));
    }

    #[test]
    fn different_keys_produce_different_output() {
        let s1 = InProcessSigner::new("k1", [1u8; 32]);
        let s2 = InProcessSigner::new("k2", [2u8; 32]);
        let r = sample_request();
        assert_ne!(s1.sign(&r).signature_message, s2.sign(&r).signature_message);
    }

    #[test]
    fn new_random_smoke() {
        let s = InProcessSigner::new_random();
        assert_eq!(s.secret.len(), 32);
        assert!(s.key_id.starts_with("stub-"));
        let resp = s.sign(&sample_request());
        assert_eq!(resp.status, SignatureStatus::Success);
    }
}
