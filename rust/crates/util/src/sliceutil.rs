//! Slice/collection utilities — mirrors Go `util/sliceutil` package.

use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};

/// Return a new Vec with duplicate strings removed, preserving first-occurrence order.
pub fn unique_strings(items: &[&str]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for &item in items {
        if seen.insert(item) {
            result.push(item.to_string());
        }
    }
    result
}

/// Check whether a string slice contains the given value.
pub fn contains_string(items: &[&str], target: &str) -> bool {
    items.iter().any(|&s| s == target)
}

/// Case-insensitive string containment check — mirrors Go `sliceutil.ContainsStringIgnoreCase`.
pub fn contains_string_ignore_case(items: &[String], target: &str) -> bool {
    items.iter().any(|s| s.eq_ignore_ascii_case(target))
}

/// Shuffle a Vec in-place using Fisher-Yates (via `rand`).
pub fn random_shuffle<T>(items: &mut Vec<T>) {
    let mut rng = rand::thread_rng();
    items.shuffle(&mut rng);
}

/// Returns the index of the first element for which `f` returns true, or -1.
/// Mirrors Go `sliceutil.IndexPointerFunc`.
pub fn index_func<T, F: Fn(&T) -> bool>(s: &[T], f: F) -> Option<usize> {
    s.iter().position(|item| f(item))
}

/// Delete all elements from the slice for which `f` returns true.
/// Mirrors Go `sliceutil.DeletePointerFunc`.
pub fn delete_func<T, F: Fn(&T) -> bool>(s: &mut Vec<T>, f: F) {
    s.retain(|item| !f(item));
}

/// Check if two slices contain the same elements regardless of order.
/// Mirrors Go `sliceutil.EqualIgnoreOrder`.
pub fn equal_ignore_order<T: Eq + std::hash::Hash>(s1: &[T], s2: &[T]) -> bool {
    if s1.len() != s2.len() {
        return false;
    }
    let mut counts1: HashMap<&T, i32> = HashMap::new();
    for item in s1 {
        *counts1.entry(item).or_insert(0) += 1;
    }
    let mut counts2: HashMap<&T, i32> = HashMap::new();
    for item in s2 {
        *counts2.entry(item).or_insert(0) += 1;
    }
    counts1 == counts2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_strings() {
        let result = unique_strings(&["a", "b", "a", "c", "b"]);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_contains_string_ignore_case() {
        let items = vec!["Hello".to_string(), "World".to_string()];
        assert!(contains_string_ignore_case(&items, "hello"));
        assert!(!contains_string_ignore_case(&items, "missing"));
    }

    #[test]
    fn test_equal_ignore_order() {
        assert!(equal_ignore_order(&[1, 2, 3], &[3, 1, 2]));
        assert!(!equal_ignore_order(&[1, 2], &[1, 3]));
        assert!(!equal_ignore_order(&[1, 2, 2], &[1, 2, 3]));
    }

    #[test]
    fn test_delete_func() {
        let mut items = vec![1, 2, 3, 4, 5];
        delete_func(&mut items, |x| *x % 2 == 0);
        assert_eq!(items, vec![1, 3, 5]);
    }

    #[test]
    fn test_index_func() {
        let items = vec![1, 2, 3, 4, 5];
        assert_eq!(index_func(&items, |x| *x == 3), Some(2));
        assert_eq!(index_func(&items, |x| *x == 99), None);
    }
}
