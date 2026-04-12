//! Tiny standalone macro-substitution helper used by the injectors.
//!
//! We do NOT depend on the `macros` crate here (per the architectural
//! constraint that these three crates must not depend on each other).
//! Instead we provide a minimal `##KEY##` replacer that operates on a
//! plain `HashMap<String, String>`.

use std::collections::HashMap;

/// Map of macro key (without delimiters) to replacement value.
pub type MacroMap = HashMap<String, String>;

const DELIM: &str = "##";

/// Very small `##KEY##` resolver. Unknown keys are left verbatim.
#[derive(Debug, Default, Clone, Copy)]
pub struct SimpleMacros;

impl SimpleMacros {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve(&self, template: &str, macros: &MacroMap) -> String {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find(DELIM) {
            out.push_str(&rest[..start]);
            let after = &rest[start + DELIM.len()..];
            match after.find(DELIM) {
                Some(end) => {
                    let key = &after[..end];
                    match macros.get(key) {
                        Some(v) => out.push_str(v),
                        None => {
                            out.push_str(DELIM);
                            out.push_str(key);
                            out.push_str(DELIM);
                        }
                    }
                    rest = &after[end + DELIM.len()..];
                }
                None => {
                    out.push_str(DELIM);
                    out.push_str(after);
                    return out;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_and_preserves_unknown() {
        let mut m = MacroMap::new();
        m.insert("PBS-BIDID".into(), "xyz".into());
        let r = SimpleMacros::new();
        let out = r.resolve("a=##PBS-BIDID## b=##MISSING##", &m);
        assert_eq!(out, "a=xyz b=##MISSING##");
    }

    #[test]
    fn empty_template() {
        let r = SimpleMacros::new();
        assert_eq!(r.resolve("", &MacroMap::new()), "");
    }
}
