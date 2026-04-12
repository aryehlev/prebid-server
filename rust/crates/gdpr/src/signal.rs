//! GDPR signal type, ported from `gdpr/signal.go`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The GDPR signal accompanying an incoming auction request.
///
/// Mirrors the Go enum:
/// - `SignalAmbiguous = -1`
/// - `SignalNo        =  0`
/// - `SignalYes       =  1`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i8)]
pub enum Signal {
    Ambiguous = -1,
    No = 0,
    Yes = 1,
}

impl Default for Signal {
    fn default() -> Self {
        Signal::Ambiguous
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("GDPR signal should be integer 0 or 1")]
pub struct SignalError;

impl Signal {
    /// Parses a `Signal` from its string representation.
    ///
    /// An empty string yields `Signal::Ambiguous`, matching Go's
    /// `StrSignalParse`.
    pub fn parse_str(signal: &str) -> Result<Signal, SignalError> {
        if signal.is_empty() {
            return Ok(Signal::Ambiguous);
        }
        match signal.parse::<i32>() {
            Ok(v) => Signal::from_int(v),
            Err(_) => Err(SignalError),
        }
    }

    /// Parses an integer into a `Signal`. Only 0 and 1 are valid.
    pub fn from_int(i: i32) -> Result<Signal, SignalError> {
        match i {
            0 => Ok(Signal::No),
            1 => Ok(Signal::Yes),
            _ => Err(SignalError),
        }
    }

    /// Normalizes an ambiguous signal using the given default (`"0"` or `"1"`).
    pub fn normalize(self, default_value: &str) -> Signal {
        if self != Signal::Ambiguous {
            return self;
        }
        if default_value == "0" {
            Signal::No
        } else {
            Signal::Yes
        }
    }

    pub fn as_i8(self) -> i8 {
        self as i8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_is_ambiguous() {
        assert_eq!(Signal::parse_str("").unwrap(), Signal::Ambiguous);
    }

    #[test]
    fn parse_zero_and_one() {
        assert_eq!(Signal::parse_str("0").unwrap(), Signal::No);
        assert_eq!(Signal::parse_str("1").unwrap(), Signal::Yes);
    }

    #[test]
    fn parse_invalid() {
        assert!(Signal::parse_str("foo").is_err());
        assert!(Signal::parse_str("2").is_err());
        assert!(Signal::parse_str("-1").is_err());
    }

    #[test]
    fn normalize_rules() {
        assert_eq!(Signal::Yes.normalize("0"), Signal::Yes);
        assert_eq!(Signal::No.normalize("1"), Signal::No);
        assert_eq!(Signal::Ambiguous.normalize("0"), Signal::No);
        assert_eq!(Signal::Ambiguous.normalize("1"), Signal::Yes);
        assert_eq!(Signal::Ambiguous.normalize(""), Signal::Yes);
    }

    #[test]
    fn as_i8_values() {
        assert_eq!(Signal::Ambiguous.as_i8(), -1);
        assert_eq!(Signal::No.as_i8(), 0);
        assert_eq!(Signal::Yes.as_i8(), 1);
    }
}
