//! Small text helpers every layer shares.

/// Free text as stored: trimmed, with blank read as unset.
pub(crate) fn trimmed_or_none(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_absent_text_are_unset_and_the_rest_is_trimmed() {
        assert_eq!(trimmed_or_none(None), None);
        assert_eq!(trimmed_or_none(Some(" \t\n")), None);
        assert_eq!(trimmed_or_none(Some("  a b ")), Some("a b".to_string()));
    }
}
