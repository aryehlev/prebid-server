//! Bit reader for GPP / IAB consent payloads.
//!
//! Reads N-bit big-endian unsigned integers from a byte slice
//! starting from the most-significant bit of byte 0. Mirrors the
//! upstream `iabgpp-es` BitStringEncoder decoding logic.

use super::GppError;

/// A cursor over a byte slice yielding big-endian bit-packed ints.
#[derive(Debug, Clone)]
pub struct BitReader<'a> {
    bytes: &'a [u8],
    /// Current bit position (0-based, MSB-first within each byte).
    pos: usize,
}

impl<'a> BitReader<'a> {
    /// Create a new reader over `bytes`.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Number of bits still available.
    pub fn remaining_bits(&self) -> usize {
        self.bytes.len().saturating_mul(8).saturating_sub(self.pos)
    }

    /// Skip `n` bits.
    pub fn skip_bits(&mut self, n: usize) -> Result<(), GppError> {
        if self.remaining_bits() < n {
            return Err(GppError::Truncated);
        }
        self.pos += n;
        Ok(())
    }

    /// Read `n` bits (0..=64) as an unsigned big-endian integer.
    pub fn read_bits(&mut self, n: usize) -> Result<u64, GppError> {
        if n > 64 {
            return Err(GppError::BadBitfield(format!("n={} exceeds 64", n)));
        }
        if self.remaining_bits() < n {
            return Err(GppError::Truncated);
        }
        let mut value: u64 = 0;
        for _ in 0..n {
            let byte_idx = self.pos / 8;
            let bit_in_byte = 7 - (self.pos % 8);
            let bit = (self.bytes[byte_idx] >> bit_in_byte) & 1;
            value = (value << 1) | (bit as u64);
            self.pos += 1;
        }
        Ok(value)
    }

    /// Read a single bit as a bool.
    pub fn read_bool(&mut self) -> Result<bool, GppError> {
        Ok(self.read_bits(1)? == 1)
    }

    /// Read a 6-bit unsigned integer.
    pub fn read_u6(&mut self) -> Result<u8, GppError> {
        Ok(self.read_bits(6)? as u8)
    }

    /// Read a 12-bit unsigned integer.
    pub fn read_u12(&mut self) -> Result<u16, GppError> {
        Ok(self.read_bits(12)? as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_u6_and_u12_aligned() {
        // 6 bits = 0b000011 (=3), then 12 bits = 0b0000 0001 0010 (=18).
        // Packed: 000011 000000010010 -> 000011_00 0000_0100 10?? ????
        // bits: 0000 1100 0000 0100 1000 0000 -> 0x0C 0x04 0x80
        let bytes = [0x0C, 0x04, 0x80];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_u6().unwrap(), 3);
        assert_eq!(r.read_u12().unwrap(), 18);
    }

    #[test]
    fn read_bool_sequence() {
        // 10110100 = 0xB4
        let bytes = [0xB4];
        let mut r = BitReader::new(&bytes);
        assert!(r.read_bool().unwrap());
        assert!(!r.read_bool().unwrap());
        assert!(r.read_bool().unwrap());
        assert!(r.read_bool().unwrap());
        assert!(!r.read_bool().unwrap());
        assert!(r.read_bool().unwrap());
        assert!(!r.read_bool().unwrap());
        assert!(!r.read_bool().unwrap());
        assert_eq!(r.remaining_bits(), 0);
    }

    #[test]
    fn read_across_byte_boundary() {
        // Read 5 bits then 8 bits from 0xAB 0xCD = 1010 1011 1100 1101.
        // First 5 bits: 10101 = 21.
        // Next 8 bits:  011 11001 -> wait, next 8 bits starting at pos 5:
        // bit5..12 = 0,1,1,1,1,0,0,1 = 01111001 = 0x79.
        let bytes = [0xAB, 0xCD];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(5).unwrap(), 0b10101);
        assert_eq!(r.read_bits(8).unwrap(), 0x79);
        assert_eq!(r.remaining_bits(), 3);
    }

    #[test]
    fn skip_and_truncation() {
        let bytes = [0xFF];
        let mut r = BitReader::new(&bytes);
        r.skip_bits(4).unwrap();
        assert_eq!(r.remaining_bits(), 4);
        assert!(r.skip_bits(5).is_err());
    }
}
