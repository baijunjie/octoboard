//! Whether text is a full object id as `git` prints it.

/// Whether `text` is a full object id as `git` prints it: SHA-1's 40 or SHA-256's 64 lowercase hex
/// digits. An id in capitals is not one, since it would not compare equal to what `git` prints for
/// the same object.
pub fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_full_lowercase_hex_is_an_object_id() {
        assert!(is_object_id(&"a1".repeat(20)));
        assert!(is_object_id(&"0".repeat(64)));
        assert!(!is_object_id(&"A1".repeat(20)));
        assert!(!is_object_id(&"a".repeat(39)));
        assert!(!is_object_id(&"g".repeat(40)));
        assert!(!is_object_id(""));
    }
}
