//! The lexing of a TypeScript declaration as `ts-rs` writes it, shared by the bindings, which
//! rewrite the text (`bindings/received.rs`), and the cross-check, which parses it
//! (`crosscheck.rs`), so the two read names, strings and comments by the same rules.

/// A piece of a declaration's text; the pieces in order are the text again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Piece<'a> {
    /// Whitespace or a comment.
    Gap(&'a str),
    /// A quoted string, its quotes included.
    Literal(&'a str),
    /// A name, a keyword or a number: a letter, a digit, `_` or `$`, then any of those or `.`.
    Name(&'a str),
    /// Any other character.
    Mark(&'a str),
}

impl<'a> Piece<'a> {
    pub(super) fn text(&self) -> &'a str {
        match *self {
            Piece::Gap(text) | Piece::Literal(text) | Piece::Name(text) | Piece::Mark(text) => text,
        }
    }
}

/// The pieces of a declaration's text, or what makes it unreadable: a string or a block comment
/// left open.
pub(super) fn pieces<'t>(text: &'t str) -> Result<Vec<Piece<'t>>, String> {
    let starts_name = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let continues_name = |c: char| starts_name(c) || c == '.';
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        let (length, piece): (usize, fn(&'t str) -> Piece<'t>) = if first.is_whitespace() {
            let end = rest.find(|c: char| !c.is_whitespace());
            (end.unwrap_or(rest.len()), Piece::Gap)
        } else if let Some(comment) = rest.strip_prefix("/*") {
            let end = comment.find("*/").ok_or("an unclosed comment")?;
            (end + 4, Piece::Gap)
        } else if rest.starts_with("//") {
            (rest.find('\n').unwrap_or(rest.len()), Piece::Gap)
        } else if first == '"' {
            let mut escaped = false;
            let (end, _) = rest
                .char_indices()
                .skip(1)
                .find(|&(_, c)| {
                    let closes = c == '"' && !escaped;
                    escaped = c == '\\' && !escaped;
                    closes
                })
                .ok_or("an unclosed string")?;
            (end + 1, Piece::Literal)
        } else if starts_name(first) {
            let end = rest.find(|c: char| !continues_name(c));
            (end.unwrap_or(rest.len()), Piece::Name)
        } else {
            (first.len_utf8(), Piece::Mark)
        };
        let (taken, left) = rest.split_at(length);
        out.push(piece(taken));
        rest = left;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pieces_are_the_text_again_and_a_quoted_string_is_read_whole() {
        let text = "type A = { \"a\\\"|b\": Foo, /* c */ d: 'x' }; // end\n";
        let pieces = pieces(text).unwrap();
        let all: String = pieces.iter().map(Piece::text).collect();
        assert_eq!(all, text);
        let literals: Vec<&str> = pieces
            .iter()
            .filter_map(|piece| match piece {
                Piece::Literal(text) => Some(*text),
                _ => None,
            })
            .collect();
        assert_eq!(literals, ["\"a\\\"|b\""]);
        assert!(pieces.contains(&Piece::Gap("/* c */")));
        assert!(pieces.contains(&Piece::Gap("// end")));
        assert!(pieces.contains(&Piece::Name("Foo")));
        assert!(pieces.contains(&Piece::Mark("'")));
    }

    #[test]
    fn a_comment_closes_at_its_first_end_mark() {
        assert_eq!(
            pieces("/*/ a */b").unwrap(),
            [Piece::Gap("/*/ a */"), Piece::Name("b")]
        );
        assert_eq!(pieces("/**/").unwrap(), [Piece::Gap("/**/")]);
    }

    #[test]
    fn a_name_runs_on_through_dots_after_its_first_character() {
        assert_eq!(
            pieces("a.b 1.5 .c").unwrap(),
            [
                Piece::Name("a.b"),
                Piece::Gap(" "),
                Piece::Name("1.5"),
                Piece::Gap(" "),
                Piece::Mark("."),
                Piece::Name("c"),
            ]
        );
    }

    #[test]
    fn a_string_or_a_comment_left_open_is_unreadable() {
        assert_eq!(pieces("a: \"b").unwrap_err(), "an unclosed string");
        assert_eq!(pieces("a: \"b\\\"").unwrap_err(), "an unclosed string");
        assert_eq!(pieces("a /* b").unwrap_err(), "an unclosed comment");
        assert_eq!(pieces("a /*/").unwrap_err(), "an unclosed comment");
    }
}
