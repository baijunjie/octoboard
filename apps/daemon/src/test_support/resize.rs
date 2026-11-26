//! A shell script that reports its terminal's size each time it is resized, for the tests of what a
//! resize does to a program on a PTY. The only place the daemon's sources trap the window-change
//! signal: `layering_tests::source_rules` fails on a trap for it anywhere else.

/// A script that runs `before` and then reports its size, as `size <rows> <cols>`, each time it is
/// resized. The trap only notes the resize, and the loop reads and reports the size: bash 5 clears a
/// signal's pending count once that signal's trap has run, so a resize that comes while the trap
/// runs is forgotten. The trap therefore stays a builtin assignment, which takes no time a second
/// resize of a repaint nudge could come in, and the loop reads the size after the last resize the
/// trap noted. The report is written without a command substitution: bash 5.2 can fail to parse
/// a `$(…)` when the signal comes while it reads it ("unexpected EOF while looking for matching
/// `)'").
pub(crate) fn reports_size(before: &str) -> String {
    format!(
        "trap 'resized=1' WINCH; {before}; while :; do \
         if [ -n \"$resized\" ]; then resized=; printf 'size '; stty size; fi; sleep 0.05; done"
    )
}
