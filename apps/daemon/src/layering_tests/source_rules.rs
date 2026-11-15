//! Rules about how the daemon's sources, test sources included, write a few things that went
//! wrong and are easy to write again. Each rule reads every `.rs` file under `src` (but this
//! directory, which names the tokens) with its comments removed and its string literals kept,
//! since the shell scripts the tests run are literals; a rule names the sanctioned places
//! for what it bans, and has no allow-list of sites.
//!
//! - A shell `trap` for the window-change signal (`WINCH`) is written only in
//!   `test_support/resize.rs`: a trap that does work loses the resize that arrives while it runs.
//!   A mention of `SIGWINCH` in Rust code is not a trap and passes.
//! - `TerminateProcess(` is called only where the process is then waited on (`SANCTIONED_ENDS`),
//!   and nowhere else, test support included.
//! - A temporary file written before it is renamed into place is not named with
//!   `with_extension` or `set_extension`, whose name two files differing in their extension share.

use std::path::Path;

use super::scan::collect_rs;

/// The only file that traps the window-change signal.
const RESIZE_FILE: &str = "test_support/resize.rs";

/// The files that may call `TerminateProcess(`: each waits, bounded, until what it ended is gone.
const SANCTIONED_ENDS: &[&str] = &["platform/proc/windows.rs", "platform/proc/crash/windows.rs"];

/// The extensions `with_extension` is given for a temporary file.
const TEMPORARY_EXTENSIONS: &[&str] = &["partial", "tmp", "temp", "new", "staging"];

/// A source file: its path relative to `src`, and its text without comments.
pub(super) struct Source {
    pub(super) file: String,
    pub(super) text: String,
}

/// Every source file of the crate but those of `layering_tests`.
pub(super) fn sources() -> Vec<Source> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    collect_rs(&src, &mut |path| {
        let file = path
            .strip_prefix(&src)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if !file.starts_with("layering_tests/") {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
            found.push(Source {
                file,
                text: without_comments(&text),
            });
        }
    });
    found.sort_by(|a, b| a.file.cmp(&b.file));
    found
}

fn report(source: &Source, offset: usize, what: &str, problems: &mut Vec<String>) {
    let line = source.text[..offset].matches('\n').count() + 1;
    problems.push(format!("apps/daemon/src/{}:{line}: {what}", source.file));
}

/// A shell `trap` for the window-change signal anywhere but `RESIZE_FILE`: a `trap` command whose
/// signal arguments, after its handler, name it (`WINCH`, `SIGWINCH` in either case, or 28), as in
/// `trap 'resized=1' WINCH`.
pub(super) fn check_window_change(sources: &[Source], problems: &mut Vec<String>) {
    for source in sources.iter().filter(|s| s.file != RESIZE_FILE) {
        for (offset, word) in source.text.match_indices("trap") {
            if starts_command(&source.text[..offset])
                && traps_window_change(&source.text.as_bytes()[offset + word.len()..])
            {
                report(
                    source,
                    offset,
                    "a shell trap for the window-change signal; use \
                     `test_support::resize::reports_size`",
                    problems,
                );
            }
        }
    }
}

/// The shell keywords after which a command starts.
const COMMAND_KEYWORDS: &[&str] = &["then", "do", "else", "if", "while", "until"];

/// Whether a shell command word may start right after `before`: at the start of the text, a line
/// or a quoted script, after a separator (`&&` and `||` end in one), `!`, or a shell keyword as a
/// whole word. This keeps Rust such as `let trap = SIGWINCH` and `do_trap(` out.
fn starts_command(before: &str) -> bool {
    let mut before = before.trim_end_matches([' ', '\t']);
    // A `\t` escape in a Rust literal is blank too.
    while let Some(rest) = before.strip_suffix("\\t") {
        before = rest.trim_end_matches([' ', '\t']);
    }
    before.is_empty()
        || before.ends_with("\\n")
        || before.ends_with(['\n', ';', '&', '|', '!', '(', '{', '"', '\'', '`'])
        || COMMAND_KEYWORDS.iter().any(|keyword| {
            before.strip_suffix(keyword).is_some_and(|rest| {
                rest.chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric() && c != '_')
            })
        })
}

/// Whether the text after a `trap` word is a trap for `WINCH`: a handler, then signals up to the
/// end of the statement (an unquoted `;`, `&`, `|` or line end), a trailing backslash continuing it.
fn traps_window_change(b: &[u8]) -> bool {
    let ends = |c: u8| matches!(c, b';' | b'&' | b'|' | b'\n');
    let blank = |b: &[u8], mut i: usize| loop {
        match b.get(i) {
            Some(b' ' | b'\t') => i += 1,
            Some(b'\\') if b.get(i + 1) == Some(&b'\n') => i += 2,
            _ => return i,
        }
    };
    if !matches!(b.first(), Some(b' ' | b'\t')) {
        return false;
    }
    let mut i = 0;
    let mut options = true;
    while options {
        i = blank(b, i);
        let start = i;
        match b.get(i) {
            None => return false,
            Some(&c) if ends(c) => return false,
            Some(b'\'') => match b[i + 1..].iter().position(|&c| c == b'\'') {
                Some(close) => i += close + 2,
                None => return false,
            },
            Some(b'\\') if b.get(i + 1) == Some(&b'"') => {
                match b[i + 2..].windows(2).position(|w| w == b"\\\"") {
                    Some(close) => i += close + 4,
                    None => return false,
                }
            }
            Some(b'"') => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            // Not a handler: Rust's block or comparison after a variable named `trap`.
            Some(b'{' | b'}' | b'(' | b')' | b'=' | b'<' | b'>') => return false,
            Some(_) => {
                while i < b.len() && !b[i].is_ascii_whitespace() && !ends(b[i]) {
                    i += 1;
                }
            }
        }
        // `--` ends the options, and the handler follows it.
        options = &b[start..i] == b"--";
    }
    loop {
        i = blank(b, i);
        match b.get(i) {
            None => return false,
            Some(&c) if ends(c) => return false,
            // A `\n` escape in a Rust literal is the end of the line.
            Some(b'\\') if b.get(i + 1) == Some(&b'n') => return false,
            _ => {}
        }
        let start = i;
        while i < b.len()
            && !b[i].is_ascii_whitespace()
            && !ends(b[i])
            && !matches!(b[i], b'\\' | b'"' | b'\'')
        {
            i += 1;
        }
        let signal = &b[start..i];
        // Bash takes signal names in either case, with or without `SIG`; 28 is `SIGWINCH` on
        // Linux and macOS.
        let signal = signal
            .get(..3)
            .filter(|prefix| prefix.eq_ignore_ascii_case(b"SIG"))
            .map_or(signal, |_| &signal[3..]);
        if signal.eq_ignore_ascii_case(b"WINCH") || signal == b"28" {
            return true;
        }
        if i == start {
            i += 1;
        } else if !signal
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            // Not a `trap` command's arguments, but Rust that has a variable named `trap`.
            return false;
        }
    }
}

/// A call of `TerminateProcess(` outside `SANCTIONED_ENDS`.
pub(super) fn check_terminate_process(sources: &[Source], problems: &mut Vec<String>) {
    for source in sources {
        if SANCTIONED_ENDS.contains(&source.file.as_str()) {
            continue;
        }
        for (offset, _) in source.text.match_indices("TerminateProcess(") {
            report(
                source,
                offset,
                "calls `TerminateProcess` without waiting for the process to be gone; end it \
                 through `platform::proc`",
                problems,
            );
        }
    }
}

/// A `with_extension(…)` or `set_extension(…)` that names a temporary: a string literal in its
/// argument that ends in an extension of `TEMPORARY_EXTENSIONS` (`"tmp"`, `".tmp"`, `"json.tmp"`,
/// the format string `"{pid}.tmp"`), or the process's id.
pub(super) fn check_temporary_names(sources: &[Source], problems: &mut Vec<String>) {
    for source in sources {
        for call in ["with_extension(", "set_extension("] {
            for (offset, _) in source.text.match_indices(call) {
                let argument = argument_at(&source.text[offset + call.len()..]);
                let temporary = string_literals(argument).iter().any(|literal| {
                    let extension = literal.rsplit('.').next().unwrap_or_default();
                    TEMPORARY_EXTENSIONS.contains(&extension)
                }) || argument.contains("process::id");
                if temporary {
                    report(
                        source,
                        offset,
                        &format!(
                            "names a temporary file with `{}`; use `platform::names::with_suffix`",
                            call.trim_end_matches('(')
                        ),
                        problems,
                    );
                }
            }
        }
    }
}

/// The bodies of the string literals in `text`, escapes left as written.
fn string_literals(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut open = None;
    let mut chars = text.char_indices();
    while let Some((index, c)) = chars.next() {
        match (c, open) {
            ('\\', Some(_)) => {
                chars.next();
            }
            ('"', None) => open = Some(index + 1),
            ('"', Some(start)) => {
                found.push(&text[start..index]);
                open = None;
            }
            _ => {}
        }
    }
    found
}

/// The text up to the parenthesis that closes the call whose arguments `text` starts.
fn argument_at(text: &str) -> &str {
    let mut depth = 0;
    for (index, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return &text[..index],
            ')' => depth -= 1,
            _ => {}
        }
    }
    text
}

/// Where a raw string literal (`r"…"`, `r#"…"#`, with a `b` or `c` prefix or not) starts at
/// `chars[at]`: the number of `#` it opens with and the index of its first body character.
fn raw_string_open(chars: &[char], at: usize) -> Option<(usize, usize)> {
    if at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_') {
        return None;
    }
    let mut i = at;
    if matches!(chars.get(i), Some('b' | 'c')) {
        i += 1;
    }
    if chars.get(i) != Some(&'r') {
        return None;
    }
    i += 1;
    let hashes = chars[i..].iter().take_while(|&&c| c == '#').count();
    i += hashes;
    (chars.get(i) == Some(&'"')).then_some((hashes, i + 1))
}

/// `src` with every comment replaced by spaces, newlines kept, and string and character literals
/// left as written.
fn without_comments(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        let (c, next) = (chars[i], chars.get(i + 1).copied());
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                let pair = (chars[i], chars.get(i + 1).copied());
                let step = if pair == ('/', Some('*')) {
                    depth += 1;
                    2
                } else if pair == ('*', Some('/')) {
                    depth -= 1;
                    2
                } else {
                    1
                };
                for &k in &chars[i..(i + step).min(chars.len())] {
                    out.push(if k == '\n' { '\n' } else { ' ' });
                }
                i += step;
                if depth == 0 {
                    break;
                }
            }
        } else if let Some((hashes, body)) = raw_string_open(&chars, i) {
            // A raw string has no escapes: it ends at the first quote followed by as many `#` as
            // it opened with.
            let close: Vec<char> = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            let end = chars[body..]
                .windows(close.len())
                .position(|w| w == close.as_slice())
                .map_or(chars.len(), |p| body + p + close.len());
            out.extend(chars[i..end].iter());
            i = end;
        } else if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                let take = if chars[i] == '\\' { 2 } else { 1 };
                out.extend(chars[i..(i + take).min(chars.len())].iter());
                i += take;
            }
            if i < chars.len() {
                out.push('"');
            }
            i += 1;
        } else if c == '\'' && next == Some('\\') {
            // An escaped character literal: `'\n'`, `'\''`, `'\u{…}'`.
            let close = chars[i + 3..]
                .iter()
                .position(|&k| k == '\'')
                .map(|p| i + 3 + p);
            let end = close.map_or(i + 1, |close| close + 1);
            out.extend(chars[i..end].iter());
            i = end;
        } else if c == '\'' && chars.get(i + 2) == Some(&'\'') {
            out.extend(chars[i..i + 3].iter());
            i += 3;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(file: &str, text: &str) -> Source {
        Source {
            file: file.to_string(),
            text: without_comments(text),
        }
    }

    fn problems_of(check: fn(&[Source], &mut Vec<String>), file: &str, text: &str) -> Vec<String> {
        let mut problems = Vec::new();
        check(&[source(file, text)], &mut problems);
        problems
    }

    #[test]
    fn comments_are_dropped_and_literals_kept() {
        let text = without_comments("a // WINCH\n/* b /* WINCH */ c */ \"x // WINCH\" '\"' d");
        assert_eq!(text.matches("WINCH").count(), 1);
        assert_eq!(text.matches('\n').count(), 1);
        assert!(text.contains('d'));
    }

    #[test]
    fn a_trap_for_the_window_change_signal_is_found_outside_its_one_file() {
        let trap = "let s = \"trap 'echo $(stty size)' WINCH\";";
        assert_eq!(
            problems_of(check_window_change, "a/tests.rs", trap).len(),
            1
        );
        assert!(problems_of(check_window_change, RESIZE_FILE, trap).is_empty());
        assert!(problems_of(check_window_change, "a.rs", "// WINCH\n").is_empty());
        // Product code that handles the signal is not a trap.
        for product in [
            "let kind = libc::SIGWINCH;",
            "signal(SignalKind::from_raw(libc::SIGWINCH));",
            "let trapped = true; // trap WINCH\nlet n = SIGWINCH;",
        ] {
            assert!(
                problems_of(check_window_change, "a.rs", product).is_empty(),
                "{product}"
            );
        }
    }

    #[test]
    fn terminating_a_process_is_found_outside_the_places_that_wait() {
        let call = "unsafe { TerminateProcess(handle, 1) };";
        assert_eq!(problems_of(check_terminate_process, "a.rs", call).len(), 1);
        assert!(problems_of(check_terminate_process, SANCTIONED_ENDS[0], call).is_empty());
        assert_eq!(
            problems_of(check_terminate_process, "test_support/p.rs", call).len(),
            1
        );
    }

    #[test]
    fn a_temporary_named_with_an_extension_is_found() {
        for plant in [
            "p.with_extension(\"partial\")",
            "Path::new(&r).with_extension(\"tmp\")",
            "p.with_extension(format!(\"{}\", std::process::id()))",
        ] {
            let found = problems_of(check_temporary_names, "a.rs", plant);
            assert_eq!(found.len(), 1, "{plant}");
        }
        for fine in [
            "p.with_extension(\"json\")",
            "p.with_extension(\"\")",
            "p.with_extension(kind)",
        ] {
            let found = problems_of(check_temporary_names, "a.rs", fine);
            assert!(found.is_empty(), "{fine}");
        }
    }

    #[test]
    fn comment_removal_keeps_every_line_and_reads_escapes_and_lifetimes() {
        let src = "let a = \"x \\\" // WINCH\"; // c\nfn f<'a>(s: &'a str) { /* TerminateProcess( */ }\nlet q = '\\'';\nlet z = 'z'; // tail";
        let text = without_comments(src);
        assert_eq!(text.matches('\n').count(), src.matches('\n').count());
        // The escaped quote did not end the literal, so what follows it is still text.
        assert!(text.contains("WINCH"));
        assert!(!text.contains("TerminateProcess"));
        assert!(!text.contains("tail"));
        assert!(text.contains("fn f<'a>(s: &'a str)"));
        assert!(text.contains("'\\''"));
    }

    #[test]
    fn comment_removal_reads_raw_strings_of_any_hash_count() {
        // A raw string has no escapes: `r"x\"` ends at its backslash's quote, so the rest of the
        // line is a comment.
        let text = without_comments("let a = r\"x\\\"; // WINCH\nlet b = 1;");
        assert!(text.contains("r\"x\\\";") && !text.contains("WINCH"));
        assert_eq!(text.matches('\n').count(), 1);
        // A quote inside one does not end it unless as many `#` follow, and what is inside is
        // kept, `//` and `/*` included.
        for (src, kept) in [
            ("r#\"a \" // WINCH\"#", "a \" // WINCH"),
            ("r##\"a \"# /* WINCH */\"##", "a \"# /* WINCH */"),
            ("br#\"a \\\" // WINCH\"#", "a \\\" // WINCH"),
            ("cr#\"a \" // WINCH\"#", "a \" // WINCH"),
            ("r###\"a \"## // WINCH\"###", "a \"## // WINCH"),
        ] {
            let text = without_comments(&format!("let a = {src}; // tail\nlet b = 1;"));
            assert!(text.contains(kept), "{src}");
            assert!(!text.contains("tail"), "{src}");
            assert_eq!(text.matches('\n').count(), 1, "{src}");
        }
        // An identifier that ends in `r` before a string is no raw prefix.
        let text = without_comments("for\"x\\\" // WINCH\"; // tail");
        assert!(text.contains("WINCH") && !text.contains("tail"));
    }

    #[test]
    fn a_trap_for_the_window_change_signal_is_found_however_it_is_written() {
        for trap in [
            "\"trap 'resized=1' WINCH\"",
            "\"trap \\\"resized=1\\\" WINCH\"",
            "\"trap 'resized=1' SIGWINCH\"",
            "\"trap 'resized=1' HUP WINCH\"",
            "\"  trap 'resized=1' WINCH\"",
            "\"before; trap 'resized=1' WINCH\"",
            "\"trap 'resized=1' \\\n  WINCH\"",
            "\"if true; then trap 'resized=1' WINCH; fi\"",
            "\"for i in 1; do trap 'resized=1' WINCH; done\"",
            "\"if a; then b; else trap 'resized=1' WINCH; fi\"",
            "\"a && trap 'resized=1' WINCH\"",
            "\"a || trap 'resized=1' WINCH\"",
            "\"! trap 'resized=1' WINCH\"",
            "\"trap 'resized=1' HUP \\\n INT WINCH\"",
            // Bash takes signal names in either case, and the number 28 is the same signal.
            "\"trap 'resized=1' winch\"",
            "\"trap 'resized=1' sigwinch\"",
            "\"trap 'resized=1' SigWinch\"",
            "\"trap 'resized=1' 28\"",
            "\"trap 'resized=1' HUP 28\"",
            "\"trap - 28\"",
            "\"trap -- 'resized=1' WINCH\"",
            "\"trap resized=1 WINCH\"",
            // A `\\t` escape, or a tab, before the word.
            "\"a;\\ttrap 'resized=1' WINCH\"",
            "\"\\t\\ttrap 'resized=1' WINCH\"",
            "\"a;\ttrap 'resized=1' WINCH\"",
        ] {
            assert_eq!(
                problems_of(check_window_change, "a/tests.rs", trap).len(),
                1,
                "{trap}"
            );
        }
    }

    /// A trap is a trap whatever follows it on its line, as a script's first line goes on with
    /// its next command.
    #[test]
    fn a_trap_for_the_window_change_signal_is_found_before_other_commands() {
        for trap in [
            "\"trap 'resized=1' WINCH; sleep 1\"",
            "\"trap 'resized=1' WINCH && sleep 1\"",
        ] {
            assert_eq!(
                problems_of(check_window_change, "a/tests.rs", trap).len(),
                1,
                "{trap}"
            );
        }
    }

    /// The handler a trap runs is the work that loses a resize, so a handler of several commands is
    /// the trap the rule is about, not an exception to it.
    #[test]
    fn a_trap_for_the_window_change_signal_whose_handler_holds_several_commands_is_found() {
        for trap in [
            "\"trap 'a=1; b=2' WINCH\"",
            "\"trap 'a=1 && b=2' WINCH\"",
            "\"trap 'a=1 & b=2' WINCH\"",
        ] {
            assert_eq!(
                problems_of(check_window_change, "a/tests.rs", trap).len(),
                1,
                "{trap}"
            );
        }
    }

    #[test]
    fn the_window_change_signal_is_no_trap_without_one() {
        for fine in [
            "\"echo WINCH\"",
            "\"trap 'cleanup' EXIT\"",
            "\"retrap WINCH\"",
            "\"kill -WINCH $pid\"",
            "let trap = SIGWINCH;",
            "trap_signal(SIGWINCH);",
            "let then_trap = SIGWINCH;",
            "do_trap(SIGWINCH);",
            "let undo = trap = SIGWINCH;",
            "\"trap 'resized=1' HUP\\n echo WINCH\"",
            "\"trap 'resized=1' HUP; echo WINCH\"",
            // Rust with a variable named `trap` is not the shell command.
            "if trap { x = SIGWINCH }",
            "if trap == SIGWINCH {}",
            "if trap as u8 == 28 {}",
            "while trap { n = 28; }",
            "if trap < 28 {}",
            "\"trap 'cleanup' EXIT\\n echo 28\"",
            "\"kill -28 $pid\"",
        ] {
            assert!(
                problems_of(check_window_change, "a/tests.rs", fine).is_empty(),
                "{fine}"
            );
        }
    }

    #[test]
    fn terminating_a_process_is_allowed_in_every_place_that_waits_and_only_there() {
        let call = "unsafe { TerminateProcess(handle, 1) };";
        for file in SANCTIONED_ENDS {
            assert!(
                problems_of(check_terminate_process, file, call).is_empty(),
                "{file}"
            );
        }
        // Naming the function, or writing about it, is not calling it.
        for fine in [
            "use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess};",
            "// TerminateProcess(handle, 1) only starts the end\nlet x = 1;",
            "/* TerminateProcess( */",
        ] {
            assert!(
                problems_of(check_terminate_process, "a.rs", fine).is_empty(),
                "{fine}"
            );
        }
        // A second call in a place that waits is no reason to let one elsewhere through.
        let found = problems_of(check_terminate_process, "a.rs", &format!("{call}\n{call}"));
        assert_eq!(found.len(), 2);
        assert!(found[1].contains(":2:"), "{found:?}");
    }

    #[test]
    fn the_places_that_may_end_a_process_exist() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for file in SANCTIONED_ENDS {
            assert!(src.join(file).is_file(), "{file} is named but not there");
        }
        assert!(
            src.join(RESIZE_FILE).is_file(),
            "{RESIZE_FILE} is not there"
        );
    }

    #[test]
    fn every_temporary_extension_is_found_however_the_call_is_written() {
        for extension in TEMPORARY_EXTENSIONS {
            for plant in [
                format!("p.with_extension(\"{extension}\")"),
                format!("p.with_extension(\".{extension}\")"),
                format!("p.with_extension( \"{extension}\" )"),
            ] {
                assert_eq!(
                    problems_of(check_temporary_names, "a.rs", &plant).len(),
                    1,
                    "{plant}"
                );
            }
        }
        for plant in [
            "p.with_extension(format!(\"{}.exe\", std::process::id()))",
            "p.with_extension(format!(\n    \"{}\",\n    std::process::id()\n))",
            "p.with_extension(format!(\"{}\", id(std::process::id())))",
            // `set_extension` names the same file, a literal may carry more than the extension,
            // and a format string with the suffix is a temporary whatever its argument is.
            "p.set_extension(\"tmp\")",
            "p.set_extension(\".partial\")",
            "p.set_extension(format!(\"{}\", std::process::id()))",
            "p.with_extension(\"json.tmp\")",
            "p.set_extension(\"tar.gz.partial\")",
            "p.with_extension(format!(\"{pid}.tmp\"))",
            "p.with_extension(format!(\"{}.partial\", counter))",
            "p.with_extension(format!(\"{unique}.new\"))",
        ] {
            assert_eq!(
                problems_of(check_temporary_names, "a.rs", plant).len(),
                1,
                "{plant}"
            );
        }
    }

    #[test]
    fn an_extension_that_is_a_real_one_or_only_mentioned_is_no_temporary() {
        for fine in [
            "p.with_extension(\"exe\")",
            "p.with_extension(\"json.unreadable\")",
            "// p.with_extension(\"tmp\")\nlet x = 1;",
            "/* p.with_extension(\"partial\") */",
            "p.with_file_name(\"x.partial\")",
            "p.set_extension(\"json\")",
            "p.set_extension(\"\")",
            "p.with_extension(format!(\"{pid}.json\"))",
            "p.with_extension(\"tmp.json\")",
        ] {
            assert!(
                problems_of(check_temporary_names, "a.rs", fine).is_empty(),
                "{fine}"
            );
        }
        // The report names the line of the call.
        let found = problems_of(
            check_temporary_names,
            "a.rs",
            "let a = 1;\nlet b = p.with_extension(\"tmp\");",
        );
        assert!(found[0].contains("a.rs:2:"), "{found:?}");
    }
}
