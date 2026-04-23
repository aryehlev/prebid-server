//! String utilities — mirrors Go `util/stringutil` package.

/// Parse a comma-separated string of integers into a Vec<i8>.
/// Mirrors Go `stringutil.StrToInt8Slice`.
pub fn str_to_int8_slice(s: &str) -> Result<Vec<i8>, std::num::ParseIntError> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|part| part.trim().parse::<i8>())
        .collect()
}

/// Truncate a string to at most `max_len` bytes, ensuring we don't split a UTF-8 character.
pub fn truncate_utf8(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_str_to_int8_slice() {
        assert_eq!(str_to_int8_slice("1,2,3").unwrap(), vec![1i8, 2, 3]);
        assert_eq!(str_to_int8_slice("-1,0,127").unwrap(), vec![-1i8, 0, 127]);
        assert!(str_to_int8_slice("").unwrap().is_empty());
        assert!(str_to_int8_slice("256").is_err());
    }

    #[test]
    fn test_truncate_utf8() {
        assert_eq!(truncate_utf8("hello world", 5), "hello");
        assert_eq!(truncate_utf8("hi", 10), "hi");
        assert_eq!(truncate_utf8("café", 4), "caf");
        assert_eq!(truncate_utf8("café", 5), "café");
        assert_eq!(truncate_utf8("hello", 0), "");
    }
}
