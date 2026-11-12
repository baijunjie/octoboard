//! Small text helpers every layer shares.

/// Free text as stored: trimmed, with blank read as unset.
pub(crate) fn trimmed_or_none(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// `text` cut to at most `limit` characters, marked when it was.
pub(crate) fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// `names` as a list in prose: "A", "A and B", "A, B and C". Empty for none.
pub(crate) fn list_names(names: &[impl AsRef<str>]) -> String {
    match names.split_last() {
        Some((last, [])) => last.as_ref().to_string(),
        Some((last, rest)) => format!(
            "{} and {}",
            rest.iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", "),
            last.as_ref()
        ),
        None => String::new(),
    }
}

/// `text` with the wrapper an agent puts around a paste taken off: the `<pasted_content id="…">`
/// line before it and the closing tag after it, when it has them. Claude Code wraps a paste so in
/// the prompt its hooks and records report. Grok Build's records have not been observed to carry
/// it, and are read through this only as a guard.
pub(crate) fn without_paste_wrapper(text: &str) -> &str {
    let text = text.trim_start();
    let Some(tag) = text.strip_prefix("<pasted_content") else {
        return text;
    };
    let inner = tag.split_once('>').map_or(tag, |(_, inner)| inner).trim();
    // The closing tag counts only as the last line: text typed after it, or a second paste, stays.
    match inner.rsplit_once('\n') {
        Some((rest, last)) if is_closing_tag(last) => rest.trim_end(),
        None if is_closing_tag(inner) => "",
        _ => inner,
    }
}

fn is_closing_tag(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("</pasted_content") && line.ends_with('>')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text within the limit is returned whole; longer text is cut to the limit in characters, never
    /// inside one, and marked.
    #[test]
    fn text_is_cut_to_a_character_limit_and_marked() {
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcd", 3), "abc…");
        assert_eq!(truncate("", 3), "");
        assert_eq!(truncate("abc", 0), "…");
        assert_eq!(truncate("界界界界", 2), "界界…");
        assert_eq!(truncate("a😀😀", 2), "a😀…");
        assert_eq!(truncate("界界", 2), "界界");
    }

    #[test]
    fn names_read_as_a_list_in_prose() {
        assert_eq!(list_names(&[] as &[&str]), "");
        assert_eq!(list_names(&["Codex"]), "Codex");
        assert_eq!(list_names(&["Codex", "Grok Build"]), "Codex and Grok Build");
        assert_eq!(
            list_names(&["Claude Code", "Codex", "Grok Build"]),
            "Claude Code, Codex and Grok Build"
        );
    }

    #[test]
    fn blank_and_absent_text_are_unset_and_the_rest_is_trimmed() {
        assert_eq!(trimmed_or_none(None), None);
        assert_eq!(trimmed_or_none(Some(" \t\n")), None);
        assert_eq!(trimmed_or_none(Some("  a b ")), Some("a b".to_string()));
    }

    /// Only a closing tag on the last line is the wrapper's: text after it, or a second paste,
    /// stays.
    #[test]
    fn the_paste_wrapper_comes_off_only_at_the_last_line() {
        assert_eq!(
            without_paste_wrapper("<pasted_content id=\"a\">\nbody\n</pasted_content id=\"a\">\n"),
            "body"
        );
        let typed_after = "<pasted_content id=\"a\">\nbody\n</pasted_content id=\"a\">\nthen this";
        assert_eq!(
            without_paste_wrapper(typed_after),
            "body\n</pasted_content id=\"a\">\nthen this"
        );
        let two = "<pasted_content id=\"a\">\none\n</pasted_content id=\"a\">\n<pasted_content id=\"b\">\ntwo\n</pasted_content id=\"b\">";
        assert_eq!(
            without_paste_wrapper(two),
            "one\n</pasted_content id=\"a\">\n<pasted_content id=\"b\">\ntwo"
        );
        assert_eq!(without_paste_wrapper("plain"), "plain");
    }
}
