//! A minimal bit reader for TCF 2.2 consent segments.
//!
//! TCF segments are bit-packed, big-endian streams with mixed field widths
//! (6, 12, 16, 24, 36 bits, etc). This module provides a cursor that reads
//! unsigned integers of arbitrary width (up to 64 bits) from a byte slice.

use chrono::{DateTime, TimeZone, Utc};

/// Error returned by [`BitReader`] when the requested read would go past
/// the end of the underlying byte slice, or when the requested read is
/// otherwise impossible (e.g. more than 64 bits at once).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitReaderError(pub String);

impl std::fmt::Display for BitReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bit reader error: {}", self.0)
    }
}

impl std::error::Error for BitReaderError {}

/// Cursor over a byte slice that reads big-endian bit fields.
#[derive(Debug, Clone)]
pub struct BitReader<'a> {
    bytes: &'a [u8],
    /// Current bit offset from the start of `bytes`.
    cursor: usize,
}

impl<'a> BitReader<'a> {
    /// Constructs a new `BitReader` positioned at bit 0.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    /// Total number of bits in the underlying slice.
    pub fn total_bits(&self) -> usize {
        self.bytes.len() * 8
    }

    /// Number of bits remaining ahead of the cursor.
    pub fn remaining_bits(&self) -> usize {
        self.total_bits().saturating_sub(self.cursor)
    }

    /// Advance the cursor by `n` bits without returning anything. Errors if
    /// the underlying slice is too short.
    pub fn skip_bits(&mut self, n: usize) -> Result<(), BitReaderError> {
        if self.cursor + n > self.total_bits() {
            return Err(BitReaderError(format!(
                "skip_bits({}) past end (cursor={}, total={})",
                n,
                self.cursor,
                self.total_bits()
            )));
        }
        self.cursor += n;
        Ok(())
    }

    /// Reads the next `n` bits as a big-endian unsigned integer. `n` must
    /// be in the range `0..=64`.
    pub fn read_bits(&mut self, n: usize) -> Result<u64, BitReaderError> {
        if n > 64 {
            return Err(BitReaderError(format!("read_bits({}) > 64", n)));
        }
        if n == 0 {
            return Ok(0);
        }
        if self.cursor + n > self.total_bits() {
            return Err(BitReaderError(format!(
                "read_bits({}) past end (cursor={}, total={})",
                n,
                self.cursor,
                self.total_bits()
            )));
        }

        let mut value: u64 = 0;
        let mut bits_left = n;
        let mut cursor = self.cursor;

        while bits_left > 0 {
            let byte_idx = cursor / 8;
            let bit_in_byte = cursor % 8;
            let byte = self.bytes[byte_idx];

            // How many bits can we take from this byte?
            let available = 8 - bit_in_byte;
            let take = bits_left.min(available);

            // Extract those bits. We want the `take` bits starting at
            // bit_in_byte from the MSB side.
            let shift = (available - take) as u32; // shift to drop trailing
            let mask: u8 = if take == 8 { 0xFF } else { (1u8 << take) - 1 };
            let chunk = (byte >> shift) & mask;

            value = (value << take) | chunk as u64;

            cursor += take;
            bits_left -= take;
        }

        self.cursor = cursor;
        Ok(value)
    }

    /// Reads a single bit as a bool.
    pub fn read_bool(&mut self) -> Result<bool, BitReaderError> {
        Ok(self.read_bits(1)? != 0)
    }

    /// Reads a 6-bit unsigned integer.
    pub fn read_u6(&mut self) -> Result<u8, BitReaderError> {
        Ok(self.read_bits(6)? as u8)
    }

    /// Reads a 12-bit unsigned integer.
    pub fn read_u12(&mut self) -> Result<u16, BitReaderError> {
        Ok(self.read_bits(12)? as u16)
    }

    /// Reads a 16-bit unsigned integer.
    pub fn read_u16(&mut self) -> Result<u16, BitReaderError> {
        Ok(self.read_bits(16)? as u16)
    }

    /// Reads a 24-bit unsigned integer.
    pub fn read_u24(&mut self) -> Result<u32, BitReaderError> {
        Ok(self.read_bits(24)? as u32)
    }

    /// Reads a 36-bit unsigned integer.
    pub fn read_u36(&mut self) -> Result<u64, BitReaderError> {
        self.read_bits(36)
    }

    /// Reads a 36-bit TCF datetime (deciseconds since Unix epoch) as a
    /// [`DateTime<Utc>`].
    pub fn read_datetime(&mut self) -> Result<DateTime<Utc>, BitReaderError> {
        let deciseconds = self.read_u36()? as i64;
        // TCF stores datetimes in deciseconds (tenths of a second).
        let millis = deciseconds
            .checked_mul(100)
            .ok_or_else(|| BitReaderError("datetime overflow".into()))?;
        Utc.timestamp_millis_opt(millis)
            .single()
            .ok_or_else(|| BitReaderError(format!("invalid datetime {}ds", deciseconds)))
    }

    /// Reads a TCF 12-bit language code (two 6-bit characters offset from
    /// `'A'`) and returns it as an uppercase 2-character String.
    pub fn read_language(&mut self) -> Result<String, BitReaderError> {
        let first = self.read_u6()?;
        let second = self.read_u6()?;
        let a = (b'A' + first) as char;
        let b = (b'A' + second) as char;
        Ok(format!("{}{}", a, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_bits_across_byte_boundary() {
        // 0b1010_1010 0b1100_1100 = 0xAA 0xCC
        let bytes = [0xAA, 0xCC];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(4).unwrap(), 0b1010);
        assert_eq!(r.read_bits(8).unwrap(), 0b1010_1100); // last 4 of 0xAA + high 4 of 0xCC
        assert_eq!(r.read_bits(4).unwrap(), 0b1100);
        assert_eq!(r.remaining_bits(), 0);
    }

    #[test]
    fn read_typed() {
        // 18-bit stream: version=2 (6 bits) || 0xABC (12 bits)
        //   000010 101010111100
        // = 00001010 10101111 00xxxxxx
        // = 0x0A 0xAF 0x00
        let bytes = [0x0A, 0xAF, 0x00];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_u6().unwrap(), 2);
        assert_eq!(r.read_u12().unwrap(), 0xABC);
    }

    #[test]
    fn read_bool_and_skip() {
        let bytes = [0b1010_0000];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bool().unwrap(), true);
        r.skip_bits(1).unwrap();
        assert_eq!(r.read_bool().unwrap(), true);
        assert_eq!(r.read_bool().unwrap(), false);
    }

    #[test]
    fn read_past_end_errors() {
        let bytes = [0xFF];
        let mut r = BitReader::new(&bytes);
        r.read_bits(8).unwrap();
        assert!(r.read_bits(1).is_err());
    }

    #[test]
    fn read_language_roundtrip() {
        // 'E'-'A' = 4, 'N'-'A' = 13. 6 bits each: 000100 001101 = 0x04 0x34 0x00 high
        // 000100001101 = 0x10D => bytes: 0b0001_0000 0b1101_0000
        let bytes = [0b0001_0000, 0b1101_0000];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_language().unwrap(), "EN");
    }
}
