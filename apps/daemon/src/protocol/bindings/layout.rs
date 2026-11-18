//! The layout of a declaration in the committed bindings: one field and one union member to a
//! line.

/// Lays a declaration out one field and one union member to a line. `ts-rs` writes each on a
/// single line, which would make a change to one field a change to a line of many kilobytes. Only
/// whitespace is added or dropped, outside the doc comments, which are kept as they are.
pub(super) fn pretty(declaration: &str) -> String {
    const INDENT: &str = "  ";
    let mut out = String::new();
    // What each open bracket is, so that a comma or a bar breaks a line only between the fields of
    // an object or the members of a union, not between generic arguments.
    let mut open: Vec<char> = Vec::new();
    let mut chars = declaration.chars().peekable();
    let newline = |out: &mut String, depth: usize| {
        while out.ends_with(' ') {
            out.pop();
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&INDENT.repeat(depth));
    };
    let depth = |open: &[char]| open.iter().filter(|bracket| **bracket == '{').count();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                for inner in chars.by_ref() {
                    out.push(inner);
                    if inner == '"' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                let mut comment = String::from('/');
                let mut previous = c;
                for inner in chars.by_ref() {
                    comment.push(inner);
                    if previous == '*' && inner == '/' {
                        break;
                    }
                    previous = inner;
                }
                let indent = INDENT.repeat(depth(&open));
                newline(&mut out, depth(&open));
                for (n, line) in comment.lines().enumerate() {
                    if n > 0 {
                        out.push('\n');
                        out.push_str(&indent);
                        if line.trim_start().starts_with('*') {
                            out.push(' ');
                        }
                    }
                    out.push_str(if n == 0 { line } else { line.trim_start() });
                }
                newline(&mut out, depth(&open));
            }
            '{' => {
                open.push('{');
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                if chars.peek() == Some(&'}') {
                    chars.next();
                    open.pop();
                    out.push_str("{}");
                } else {
                    out.push('{');
                    newline(&mut out, depth(&open));
                }
            }
            '}' => {
                open.pop();
                newline(&mut out, depth(&open));
                out.push('}');
            }
            '<' | '(' | '[' => {
                open.push(c);
                out.push(c);
            }
            '>' | ')' | ']' => {
                open.pop();
                out.push(c);
            }
            ',' if open.last() == Some(&'{') => {
                out.push(',');
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                if chars.peek() != Some(&'}') {
                    newline(&mut out, depth(&open));
                }
            }
            '|' if out.trim_end().ends_with('}') && open.iter().all(|bracket| *bracket == '(') => {
                newline(&mut out, depth(&open) + open.len());
                out.push_str("| ");
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
            }
            ' ' | '\n' => {
                // Spaces lead nothing and follow nothing but one; a source line break is a space.
                let leading = out.trim_end_matches(' ').ends_with('\n') || out.is_empty();
                if !leading && !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[test]
fn a_declaration_is_laid_out_by_field_and_by_union_member_and_nothing_else_changes() {
    let declaration = "type A = { \"type\": \"x, y\", a: Record<string, B | null>, \n/**\n * Says what b is.\n */\nb?: Array<{ c: number, }>, d: {  }, } | { e: [string, number], };";
    let expected = "\
type A = {
  \"type\": \"x, y\",
  a: Record<string, B | null>,
  /**
   * Says what b is.
   */
  b?: Array<{
    c: number,
  }>,
  d: {},
}
| {
  e: [string, number],
};";
    let laid_out = pretty(declaration);
    assert_eq!(laid_out, expected);
    let bare = |text: &str| text.split_whitespace().collect::<String>();
    assert_eq!(bare(&laid_out), bare(declaration));
}
