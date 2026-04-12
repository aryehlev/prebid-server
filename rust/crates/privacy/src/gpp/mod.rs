//! GPP (Global Privacy Platform) support.
//!
//! Mirrors Go `privacy/gpp/` and the upstream IAB GPP spec.
//! Provides a section registry, header parser, bit reader, policy
//! container, and minimal section decoders for US-P and US-NAT.

pub mod bitfield;
pub mod header;
pub mod policy;
pub mod section;
pub mod sections;

use thiserror::Error;

/// Errors produced when parsing GPP payloads.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GppError {
    #[error("invalid GPP header: {0}")]
    InvalidHeader(String),
    #[error("base64 decode failed: {0}")]
    Base64(String),
    #[error("truncated GPP payload")]
    Truncated,
    #[error("unknown GPP section id: {0}")]
    UnknownSection(i32),
    #[error("bad bitfield: {0}")]
    BadBitfield(String),
}
