//! Parse and serialize KEY=VALUE pairs stored in a Bitwarden note's notes field.

use std::collections::HashMap;

/// Parse note content into a map of env-var key → value.
/// - Splits on the **first** `=` only (values may contain `=`).
/// - Skips blank lines and lines starting with `#`.
pub fn parse(notes: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in notes.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

/// Serialize a map into sorted `KEY=VALUE` lines.
pub fn serialize(pairs: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = pairs.keys().collect();
    keys.sort();
    keys.iter()
        .map(|k| format!("{}={}", k, pairs[*k]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Upsert a single key in existing note content, preserving other lines.
pub fn update(existing: &str, key: &str, value: &str) -> String {
    let mut pairs = parse(existing);
    pairs.insert(key.to_string(), value.to_string());
    serialize(&pairs)
}

/// Remove a single key from existing note content, preserving other lines.
/// Returns `None` if the key was not present.
pub fn remove(existing: &str, key: &str) -> Option<String> {
    let mut pairs = parse(existing);
    if pairs.remove(key).is_none() {
        return None;
    }
    Some(serialize(&pairs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let m = parse("A=1\nB=hello=world\n");
        assert_eq!(m["A"], "1");
        assert_eq!(m["B"], "hello=world");
    }

    #[test]
    fn parse_skips_comments_and_blanks() {
        let m = parse("# comment\n\nA=1\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m["A"], "1");
    }

    #[test]
    fn roundtrip() {
        let original = "A=1\nB=2\n";
        let m = parse(original);
        let s = serialize(&m);
        assert_eq!(s, "A=1\nB=2");
    }

    #[test]
    fn update_existing() {
        let s = update("A=1\nB=2", "A", "99");
        let m = parse(&s);
        assert_eq!(m["A"], "99");
        assert_eq!(m["B"], "2");
    }

    #[test]
    fn remove_key() {
        let s = remove("A=1\nB=2", "A").unwrap();
        let m = parse(&s);
        assert!(!m.contains_key("A"));
        assert_eq!(m["B"], "2");
    }

    #[test]
    fn remove_missing_returns_none() {
        assert!(remove("A=1", "MISSING").is_none());
    }
}
