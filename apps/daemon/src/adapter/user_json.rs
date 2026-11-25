//! Reading JSON a user wrote for an agent (a settings file, an inline configuration value) the way
//! that agent's CLI reads it. Agents differ in what they take beyond JSON: `//` and `/* */`
//! comments, a byte order mark, a comma before a closing bracket or brace, comment lines above the
//! JSON. A provider says which its agent takes ([`Takes`]), from what its CLI was measured to do
//! (`docs/agent-cli-reference.md`, the provider's section), and the rest is refused as the JSON
//! parser refuses it: a reader that took more than the agent does would start a session on a file
//! the agent then reads differently.
//!
//! What is taken off is replaced by spaces, line breaks kept, so the line and column of a parse
//! error are those of the text as the user wrote it (a byte order mark aside).

use serde_json::Value;

/// What an agent's CLI takes beyond JSON in a file or value the user wrote for it. The default
/// takes nothing: strict JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Takes {
    /// A UTF-8 byte order mark in front of everything else.
    pub byte_order_mark: bool,
    /// `//` and `/* */` comments, wherever they stand outside a string.
    pub comments: bool,
    /// One comma between the last element or member and the closing bracket or brace.
    pub trailing_commas: bool,
    /// Lines above the JSON that hold only a `//` comment, indented or not, for an agent whose
    /// CLI writes such a header above strict JSON. A comment anywhere else is not taken by this.
    pub leading_comment_lines: bool,
}

/// The JSON value `text` holds once what `takes` allows is off, or the parser's refusal of what
/// is left.
pub fn parse(text: &str, takes: Takes) -> serde_json::Result<Value> {
    serde_json::from_str(&strict_text(text, takes))
}

/// `text` with what `takes` allows beyond JSON replaced by spaces (a byte order mark is dropped),
/// for the JSON parser to read. What is inside a string is left as it is.
pub fn strict_text(text: &str, takes: Takes) -> String {
    let mut rest = text;
    if takes.byte_order_mark {
        rest = rest.strip_prefix('\u{FEFF}').unwrap_or(rest);
    }
    let mut out = String::with_capacity(rest.len());
    if takes.leading_comment_lines {
        rest = blank_leading_comment_lines(rest, &mut out);
    }

    enum At {
        Code,
        Text,
        TextEscape,
        Line,
        Block,
    }
    let mut at = At::Code;
    // The last character written outside a string, comment or white space: what a comma follows.
    let mut last: Option<char> = None;
    // Where in `out` the last comma that may be a trailing one is, while only white space and
    // comments have followed it.
    let mut comma: Option<usize> = None;
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        at = match (at, c) {
            (At::Code, '"') => {
                comma = None;
                out.push(c);
                At::Text
            }
            (At::Code, '/') if takes.comments && chars.peek() == Some(&'/') => {
                chars.next();
                out.push_str("  ");
                At::Line
            }
            (At::Code, '/') if takes.comments && chars.peek() == Some(&'*') => {
                chars.next();
                out.push_str("  ");
                At::Block
            }
            (At::Code, ' ' | '\t' | '\n' | '\r') => {
                out.push(c);
                At::Code
            }
            (At::Code, ',') => {
                // Only a comma after a value can be a trailing one: `[,]` and `[1,,]` stay
                // refused.
                let after_value = !matches!(last, None | Some('[' | '{' | ',' | ':'));
                comma = (takes.trailing_commas && after_value).then_some(out.len());
                last = Some(c);
                out.push(c);
                At::Code
            }
            (At::Code, '}' | ']') => {
                if let Some(index) = comma.take() {
                    out.replace_range(index..=index, " ");
                }
                last = Some(c);
                out.push(c);
                At::Code
            }
            (At::Code, _) => {
                comma = None;
                last = Some(c);
                out.push(c);
                At::Code
            }
            (At::Text, '\\') => {
                out.push(c);
                At::TextEscape
            }
            (At::Text, '"') => {
                last = Some(c);
                out.push(c);
                At::Code
            }
            (At::Text | At::TextEscape, _) => {
                out.push(c);
                At::Text
            }
            // A line comment ends before its line break, which is kept, as one inside a block
            // comment is, so line numbers stay what they were.
            (At::Line, '\n' | '\r') => {
                out.push(c);
                At::Code
            }
            (At::Line, _) => {
                out.push(' ');
                At::Line
            }
            (At::Block, '*') if chars.peek() == Some(&'/') => {
                chars.next();
                out.push_str("  ");
                At::Code
            }
            (At::Block, '\n' | '\r') => {
                out.push(c);
                At::Block
            }
            (At::Block, _) => {
                out.push(' ');
                At::Block
            }
        };
    }
    out
}

/// Writes the lines at the head of `text` that are blank or hold only a `//` comment to `out`,
/// the comments as spaces, and returns the text from the first other line on.
fn blank_leading_comment_lines<'a>(text: &'a str, out: &mut String) -> &'a str {
    let mut rest = text;
    while !rest.is_empty() {
        let end = rest.find('\n').map_or(rest.len(), |at| at + 1);
        let line = &rest[..end];
        let content = line.trim_start();
        if content.starts_with("//") {
            out.extend(
                line.chars()
                    .map(|c| if matches!(c, '\n' | '\r') { c } else { ' ' }),
            );
        } else if content.is_empty() {
            out.push_str(line);
        } else {
            break;
        }
        rest = &rest[end..];
    }
    rest
}

#[cfg(test)]
mod tests;
