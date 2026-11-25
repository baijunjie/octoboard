//! Text helpers the session's readings share.

/// `text` without its whitespace: what two readings of the same text are compared by, so that the
/// wrapping of a terminal row or of the input box does not count.
pub(super) fn bare(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
