//! Keeps the pieces of a message that look like image file paths from being read as images by
//! Claude Code, which would swallow the Enter written after the message.
//!
//! Claude Code splits pasted text into pieces on newlines and on a space right before `/` or a
//! Windows drive path, and a piece whose trimmed and quote-stripped text ends in `.png`, `.jpg`,
//! `.jpeg`, `.gif` or `.webp` takes an asynchronous image-read path whether or not the file
//! exists. The Enter that follows the paste is held while the paste is processed, and that path
//! drops it when it finishes: the message sits unsent in the input box, and the path is replaced
//! by an image the receiver never asked for. A report that lists produced screenshots one per
//! line is the common case. See "Writing into a running session" in `docs/agent-cli-reference.md`.
//!
//! A trailing space or invisible character does not help, since the check trims first. What does
//! is making the piece end in something else: the last word of the piece is wrapped in backticks,
//! which keeps a path readable as text (one containing a space is split by them, and a path line
//! inside a fenced code block is wrapped like any other), and a path that is already in backticks
//! or followed by punctuation does not match to begin with and is left alone.

const EXTENSIONS: [&str; 5] = [".png", ".jpg", ".jpeg", ".gif", ".webp"];

/// Wraps the trailing token of every piece Claude Code would read as an image path in backticks.
pub(super) fn guard_image_paths(text: &str) -> String {
    text.split('\n')
        .map(guard_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The pieces Claude Code splits a line into: at each space right before what [`starts_path`] sees.
fn pieces(line: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    for (i, c) in line.char_indices() {
        if c == ' ' && starts_path(&line[i + 1..]) {
            pieces.push(&line[start..i]);
            start = i + 1;
        }
    }
    pieces.push(&line[start..]);
    pieces
}

fn guard_line(line: &str) -> String {
    pieces(line)
        .into_iter()
        .map(guard_piece)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether the text begins what Claude Code splits before: `/`, or a drive letter and `:\`.
fn starts_path(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some('/') => true,
        Some(c) if c.is_ascii_alphabetic() => {
            chars.next() == Some(':') && chars.next() == Some('\\')
        }
        _ => false,
    }
}

fn guard_piece(piece: &str) -> String {
    let trimmed = piece.trim();
    if !is_image_path(trimmed) {
        return piece.to_string();
    }
    let lead = piece.len() - piece.trim_start().len();
    let end = lead + trimmed.len();
    // A quoted piece is one token; otherwise the path is the last word of the piece.
    let token_start = if is_quoted(trimmed) {
        lead
    } else {
        piece[..end].rfind(char::is_whitespace).map_or(lead, |at| {
            at + piece[at..].chars().next().map_or(1, char::len_utf8)
        })
    };
    format!(
        "{}`{}`{}",
        &piece[..token_start],
        &piece[token_start..end],
        &piece[end..]
    )
}

/// Claude Code's own test: quotes stripped, backslash escapes resolved, then the extension at the
/// very end, in any case.
fn is_image_path(trimmed: &str) -> bool {
    let unquoted = if is_quoted(trimmed) {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    let mut unescaped = String::with_capacity(unquoted.len());
    let mut chars = unquoted.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(next) => unescaped.push(next),
                None => unescaped.push(c),
            }
        } else {
            unescaped.push(c);
        }
    }
    let lower = unescaped.to_lowercase();
    EXTENSIONS
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn is_quoted(trimmed: &str) -> bool {
    trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_on_its_own_line_is_wrapped() {
        assert_eq!(guard_image_paths("/a/b.png"), "`/a/b.png`");
        assert_eq!(guard_image_paths("- /a/b.png"), "- `/a/b.png`");
    }

    #[test]
    fn every_listed_line_is_wrapped_and_prose_is_kept() {
        let text = "Done:\n- /tmp/shot1.png\n- /tmp/shot2.PNG\nall good";
        assert_eq!(
            guard_image_paths(text),
            "Done:\n- `/tmp/shot1.png`\n- `/tmp/shot2.PNG`\nall good"
        );
    }

    #[test]
    fn every_extension_matches_in_any_case() {
        for name in [
            "a.png", "a.jpg", "a.jpeg", "a.gif", "a.webp", "a.JPEG", "a.WebP",
        ] {
            assert_eq!(guard_image_paths(name), format!("`{name}`"));
        }
    }

    #[test]
    fn a_relative_name_mid_sentence_is_wrapped() {
        assert_eq!(guard_image_paths("see shot.png"), "see `shot.png`");
    }

    #[test]
    fn a_path_after_a_space_is_a_piece_of_its_own() {
        assert_eq!(guard_image_paths("see /a.png"), "see `/a.png`");
        // "/b.png and" is a piece of its own and does not end in an extension.
        assert_eq!(
            guard_image_paths("a /b.png and /c.png"),
            "a /b.png and `/c.png`"
        );
        assert_eq!(guard_image_paths("a /b.png /c.png"), "a `/b.png` `/c.png`");
        assert_eq!(guard_image_paths("at C:\\x\\a.png"), "at `C:\\x\\a.png`");
    }

    #[test]
    fn a_quoted_path_is_wrapped_whole() {
        assert_eq!(guard_image_paths("\"/a b/c.png\""), "`\"/a b/c.png\"`");
        assert_eq!(guard_image_paths("'/a.png'"), "`'/a.png'`");
    }

    #[test]
    fn a_path_that_does_not_match_is_left_alone() {
        for text in [
            "see /a.png.",
            "see /a.png, and",
            "`/a.png`",
            "- `/a.png`",
            "(/a.png)",
            "see '/a.png'",
            "/a.pngx",
            "a.png is here",
            "plain text",
            "",
        ] {
            assert_eq!(guard_image_paths(text), text);
        }
    }

    #[test]
    fn a_trailing_space_does_not_hide_a_path() {
        assert_eq!(guard_image_paths("/a.png "), "`/a.png` ");
    }

    #[test]
    fn a_guarded_message_has_no_piece_left_that_matches() {
        let text = "x /a.png\n  - 'b c.jpg'\n/d/e.gif /f.webp\nC:\\g\\h.jpeg";
        let guarded = guard_image_paths(text);
        for piece in guarded.split('\n').flat_map(pieces) {
            assert!(!is_image_path(piece.trim()), "{piece}");
        }
        assert_eq!(guard_image_paths(&guarded), guarded);
    }
}
