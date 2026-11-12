//! Reading what an agent's input box holds off its screen: for clearing a prompt the agent puts
//! back there after an interrupt (`interrupt/`), and for telling whether it holds a draft before a
//! message is written into it or stashed (`mod.rs`, `swap.rs`). Where each provider's box is on
//! its screen is its screen reader's
//! (`crate::session::screen::ScreenReader::input_box_rows`): Claude Code's box between two
//! horizontal rules, Grok Build's composer drawn as a box of its own, Codex's composer above its
//! status footer.
//!
//! The screen given is the one with faint cells blanked
//! (`crate::terminal::snapshot::TerminalSnapshot::visible_text_undimmed`): Claude Code writes the
//! hints of an empty box faint (a fresh session's `❯ Try "…"`, `❯ Press up to edit queued
//! messages` while a message waits for the turn to end), and they are no text of the box.
//! Grok Build writes nothing in its composer faint (1.0.70).
//!
//! Where the host renders what the program draws (`TermFacts::host_renders`), the host may draw a
//! faint cell plain, so a box that shows nothing but one of the provider's placeholders
//! (`crate::session::screen::ScreenReader::input_placeholder`) is read as empty whatever it is
//! drawn with ([`placeholder_as_empty`], which `crate::session::LiveSession::input_box_rows`
//! applies); the faint cells are still blanked there too.

use crate::protocol::AgentId;

/// The text in the agent's input box as `screen` (the visible rows) shows it, one row a line with
/// the indent taken off and empty rows at the end dropped; empty for an empty box, `None` where no
/// input box is recognised. Codex requires a recognised final status footer to locate its box.
/// What a session reads is `crate::session::LiveSession::input_box`, which reads it this way where
/// its host does not render what the program draws.
#[cfg(test)]
pub(super) fn input_box(agent: AgentId, screen: &str) -> Option<String> {
    Some(text_of(input_box_rows(agent, screen)?))
}

/// The text of an input box's rows: one row a line, empty rows at the end dropped.
pub(super) fn text_of(mut lines: Vec<String>) -> String {
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// The rows of `agent`'s input box with its placeholder, if that is all the box shows, read as an
/// empty row: the first row one of the provider's placeholders and every row after it empty.
pub(super) fn placeholder_as_empty(agent: AgentId, mut rows: Vec<String>) -> Vec<String> {
    let screen = crate::adapter::provider(agent).screen();
    if rows
        .first()
        .is_some_and(|first| screen.input_placeholder(first))
        && rows[1..].iter().all(String::is_empty)
    {
        rows[0].clear();
    }
    rows
}

/// The rows of the agent's input box, each trimmed and the prompt mark off the first, empty rows
/// kept: a Ctrl-U that removes an empty row changes these and not the text they hold ([`text_of`]).
pub(super) fn input_box_rows(agent: AgentId, screen: &str) -> Option<Vec<String>> {
    crate::adapter::provider(agent)
        .screen()
        .input_box_rows(screen)
}

/// An input box's rows, the prompt `mark` off the first and each trimmed.
pub(crate) fn box_rows<'a>(rows: impl Iterator<Item = &'a str>, mark: char) -> Vec<String> {
    rows.enumerate()
        .map(|(at, row)| {
            let row = row.trim();
            if at == 0 {
                row.trim_start_matches(mark).trim().to_string()
            } else {
                row.to_string()
            }
        })
        .collect()
}

/// The screens the readers are checked against, shared with the tests of
/// `crate::session::interrupt`.
#[cfg(test)]
pub(super) mod captures {
    /// Captures from Claude Code 2.1.310; see `testdata/claude-input-box.md`.
    pub(in crate::session) const RESTORED: &str =
        include_str!("../../testdata/claude_input_restored.txt");
    pub(in crate::session) const RESTORED_LINES: &str =
        include_str!("../../testdata/claude_input_restored_lines.txt");
    pub(in crate::session) const ONE_CTRL_U: &str =
        include_str!("../../testdata/claude_input_one_ctrl_u.txt");
    pub(in crate::session) const CLEARED: &str =
        include_str!("../../testdata/claude_input_cleared.txt");
    pub(in crate::session) const DRAFT: &str =
        include_str!("../../testdata/claude_input_draft.txt");
    /// Captured from Claude Code 2.1.322; see `testdata/claude-input-box.md`.
    pub(in crate::session) const HISTORY: &str =
        include_str!("../../testdata/claude_input_history.txt");

    /// Captures from Grok Build 1.0.70; see `testdata/grok-composer.md`.
    pub(in crate::session) const GROK_RESTORED: &str =
        include_str!("../../testdata/grok_composer_restored.txt");
    pub(in crate::session) const GROK_RESTORED_LINES: &str =
        include_str!("../../testdata/grok_composer_restored_lines.txt");
    pub(in crate::session) const GROK_RESTORED_WRAPPED: &str =
        include_str!("../../testdata/grok_composer_restored_wrapped.txt");
    pub(in crate::session) const GROK_ONE_CTRL_U: &str =
        include_str!("../../testdata/grok_composer_one_ctrl_u.txt");
    pub(in crate::session) const GROK_CLEARED: &str =
        include_str!("../../testdata/grok_composer_cleared.txt");
    pub(in crate::session) const GROK_WORKING: &str =
        include_str!("../../testdata/grok_composer_working.txt");
    pub(in crate::session) const GROK_DRAFT: &str =
        include_str!("../../testdata/grok_composer_draft.txt");
}

#[cfg(test)]
mod tests {
    use super::captures::*;
    use super::*;

    #[test]
    fn grok_s_composer_is_read_off_the_screen() {
        let read = |screen| input_box(AgentId::GROK, screen);
        assert_eq!(read(GROK_RESTORED).as_deref(), Some("NOTOKEN wait"));
        assert_eq!(
            read(GROK_RESTORED_LINES).as_deref(),
            Some("NOTOKEN line one about zero.\nLine two: then write an essay.\nLine three: be thorough.")
        );
        assert_eq!(
            read(GROK_RESTORED_WRAPPED).as_deref(),
            Some(
                "NOTOKEN this is one long line that goes on and on so that it wraps past the width \
                 of the composer box, which\nis about one hundred and ten columns wide, and then some \
                 more words."
            )
        );
        // The row a Ctrl-U emptied stays until the next one removes its line break.
        assert_eq!(
            read(GROK_ONE_CTRL_U).as_deref(),
            Some("NOTOKEN line one about zero.\nLine two: then write an essay.")
        );
        assert_eq!(read(GROK_CLEARED).as_deref(), Some(""));
        // The prompt echoed above the composer while a turn runs is not the composer.
        assert_eq!(read(GROK_WORKING).as_deref(), Some(""));
        assert_eq!(read(GROK_DRAFT).as_deref(), Some("draft text"));
        // Each agent's reader reads only its own screen.
        assert_eq!(read(RESTORED), None);
        assert_eq!(input_box(AgentId::CLAUDE, GROK_RESTORED), None);
        assert_eq!(input_box(AgentId::CODEX, RESTORED), None);
    }

    #[test]
    fn claude_s_input_box_is_read_off_the_screen() {
        assert_eq!(
            input_box(AgentId::CLAUDE, RESTORED).as_deref(),
            Some(
                "Think carefully about the history of the number zero and then write a 600-word \
                 essay about it."
            )
        );
        assert_eq!(
            input_box(AgentId::CLAUDE, RESTORED_LINES).as_deref(),
            Some(
                "Line one: think carefully about zero.\nLine two: then write a 600-word essay.\n\
                 Line three: be thorough."
            )
        );
        // The line a Ctrl-U emptied is still there until the next one removes its line break.
        assert_eq!(
            input_box(AgentId::CLAUDE, ONE_CTRL_U).as_deref(),
            Some("Line one: think carefully about zero.\nLine two: then write a 600-word essay.")
        );
        assert_eq!(input_box(AgentId::CLAUDE, CLEARED).as_deref(), Some(""));
        // A prompt brought back with Up, under a rule that says where in the history it is.
        assert_eq!(
            input_box(AgentId::CLAUDE, HISTORY).as_deref(),
            Some("say only: pong-f")
        );
        // The same as read with its faint label blanked.
        let blanked = HISTORY.replace("History 100/100", &" ".repeat(15));
        assert_eq!(
            input_box(AgentId::CLAUDE, &blanked).as_deref(),
            Some("say only: pong-f")
        );
        let dashes = |count: usize| "\u{2500}".repeat(count);
        let labelled = |lead: usize, label: &str, width: usize| {
            let used = lead + label.chars().count();
            format!(
                "{}{label}{}",
                dashes(lead),
                dashes(width.saturating_sub(used))
            )
        };
        let read = |top: &str, bottom: usize| {
            input_box(
                AgentId::CLAUDE,
                &format!("{top}\n\u{276F} text\n{}\n", dashes(bottom)),
            )
        };
        assert_eq!(
            read(&labelled(3, " History 1/1 ", 40), 40).as_deref(),
            Some("text")
        );
        assert_eq!(
            read(&labelled(3, &" ".repeat(9), 40), 40).as_deref(),
            Some("text")
        );
        for (near_miss, bottom) in [
            (labelled(3, " History of zero ", 40), 40),
            (
                "\u{2500}\u{2500}\u{2500}\u{2500} Slow probe agent \u{2500}".to_string(),
                40,
            ),
            // A gap narrower or wider than any History label leaves.
            (labelled(3, &" ".repeat(8), 40), 40),
            (labelled(3, &" ".repeat(22), 40), 40),
            // Too short for a rule, a lead of two dashes, a width other than the rule below.
            (labelled(3, " History 1/1 ", 19), 19),
            (labelled(2, " History 1/1 ", 40), 40),
            (labelled(3, " History 1/1 ", 40), 41),
        ] {
            assert_eq!(read(&near_miss, bottom), None, "{near_miss}");
        }
        assert_eq!(
            input_box(AgentId::CLAUDE, DRAFT).as_deref(),
            Some("draft words")
        );
        // The emptied row is still a row, so the next press shows as a change.
        assert_eq!(
            input_box_rows(AgentId::CLAUDE, ONE_CTRL_U).map(|rows| rows.len()),
            Some(3)
        );
        assert_eq!(
            input_box(AgentId::CLAUDE, "Not Claude Code\n> prompt\n"),
            None
        );
    }

    #[test]
    fn codex_reads_the_whole_composer_including_empty_leading_rows() {
        let read = |bytes: &[u8]| {
            let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(45, 140);
            screen.process(bytes);
            input_box(AgentId::CODEX, &screen.visible_text_undimmed())
        };
        assert_eq!(
            read(include_bytes!("../../testdata/codex_fresh_empty.bin")).as_deref(),
            Some("")
        );
        assert_eq!(
            read(include_bytes!("../../testdata/codex_hooked_empty.bin")).as_deref(),
            Some("")
        );
        assert_eq!(
            read(include_bytes!("../../testdata/codex_fresh_draft.bin")).as_deref(),
            Some("FRESH_DRAFT_ONE\nFRESH_DRAFT_TWO")
        );
        assert_eq!(
            read(include_bytes!("../../testdata/codex_blank_first_draft.bin")).as_deref(),
            Some("\nFRESH_BLANK_FIRST_ROW\nLAST_DRAFT_ROW")
        );
        assert_eq!(
            read(include_bytes!(
                "../../testdata/codex_blank_first_no_status_draft.bin"
            ))
            .as_deref(),
            Some("\n\n\nFRESH_BLANK_FIRST_ROW\nLAST_DRAFT_ROW")
        );
        assert_eq!(read(b"unrecognised screen"), None);
    }

    #[test]
    fn a_status_like_draft_row_does_not_end_codex_s_composer() {
        let screen = "›\n\n  100% context left\n\n  99% context left\n";
        assert_eq!(
            input_box(AgentId::CODEX, screen).as_deref(),
            Some("\n\n100% context left")
        );
        assert_eq!(
            input_box(
                AgentId::CODEX,
                "›\n\n  100% context left\n\n  unknown footer\n"
            ),
            None
        );
        assert_eq!(
            input_box(AgentId::CODEX, "›\n  100% context left\n"),
            None,
            "a separator row is required"
        );
        assert_eq!(
            input_box(
                AgentId::CODEX,
                "›\n\n  probe default · /tmp/project            ⚠ 1 warning · f2 to view\n"
            )
            .as_deref(),
            Some("")
        );
        assert_eq!(
            input_box(
                AgentId::CODEX,
                "›\n\n  100% context left  ⚠ 1 warning · f2 to view\n"
            )
            .as_deref(),
            Some("")
        );
        assert_eq!(
            input_box(
                AgentId::CODEX,
                "›\n\n  100% context left  ⚠ 0 warnings · f2 to view\n"
            ),
            None
        );
    }

    /// Raw captures of a real session's output (Claude Code 2.1.310 and 2.1.316, Grok Build
    /// 1.0.70, and from the inbox console host Claude Code 2.1.319 and Codex 0.166.0), replayed
    /// into the daemon's own screen.
    fn replayed(bytes: &[u8], undimmed: bool) -> String {
        let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
        screen.process(bytes);
        if undimmed {
            screen.visible_text_undimmed()
        } else {
            screen.visible_text()
        }
    }

    /// The captures of the inbox console host's repaint (`testdata/conhost-captures.md`) read as
    /// the macOS ones do: the host keeps the placeholder faint, so it is no text of the box once the
    /// faint cells are blanked, and a draft or a restored prompt is the box's text. Codex's footer
    /// is the host's own, a home-relative directory on a second row with the hints.
    #[test]
    fn the_inbox_console_host_s_repaint_of_an_input_box_is_read_off_the_screen() {
        for (agent, capture, plain, undimmed) in [
            (
                AgentId::CLAUDE,
                &include_bytes!("../../testdata/conhost_claude_input_placeholder.bin")[..],
                "Try \"refactor <filepath>\"",
                "",
            ),
            (
                AgentId::CLAUDE,
                include_bytes!("../../testdata/conhost_claude_input_pasted.bin"),
                "say hi in two words",
                "say hi in two words",
            ),
            (
                AgentId::CLAUDE,
                include_bytes!("../../testdata/conhost_claude_input_restored.bin"),
                "write a 400 word story about a lighthouse keeper",
                "write a 400 word story about a lighthouse keeper",
            ),
            (
                AgentId::CODEX,
                include_bytes!("../../testdata/conhost_codex_fresh_empty.bin"),
                "Ask Codex to do anything",
                "",
            ),
            (
                AgentId::CODEX,
                include_bytes!("../../testdata/conhost_codex_fresh_draft.bin"),
                "say hi in two words",
                "say hi in two words",
            ),
        ] {
            // Codex's footer names its directory the Windows way, which only such a host reads.
            let reads = agent != AgentId::CODEX
                || crate::platform::names::PATH_STYLE == crate::platform::names::PathStyle::Windows;
            assert_eq!(
                input_box(agent, &replayed(capture, false)).as_deref(),
                reads.then_some(plain),
                "{agent:?} on the plain screen"
            );
            assert_eq!(
                input_box(agent, &replayed(capture, true)).as_deref(),
                reads.then_some(undimmed),
                "{agent:?} with faint cells blanked"
            );
        }
    }

    /// Claude Code writes a fresh session's placeholder faint, so it is no text of the box once
    /// faint cells are blanked, while a prompt it puts back is written plain and stays.
    #[test]
    fn claude_s_faint_placeholder_is_no_text_of_its_box() {
        const PLACEHOLDER: &[u8] = include_bytes!("../../testdata/claude_input_placeholder.bin");
        const RESTORED_RAW: &[u8] = include_bytes!("../../testdata/claude_input_restored.bin");
        assert_eq!(
            input_box(AgentId::CLAUDE, &replayed(PLACEHOLDER, false)).as_deref(),
            Some("Try \"write a test for <filepath>\""),
            "the placeholder reads as text on the plain screen"
        );
        assert_eq!(
            input_box(AgentId::CLAUDE, &replayed(PLACEHOLDER, true)).as_deref(),
            Some("")
        );
        assert_eq!(
            input_box(AgentId::CLAUDE, &replayed(RESTORED_RAW, true)).as_deref(),
            Some("Think carefully about the history of zero, then write a 600-word essay.")
        );
    }

    /// While a message waits for the turn to end, Claude Code's empty box shows a faint hint whose
    /// first character is its cursor, drawn inverse rather than faint; text typed meanwhile
    /// replaces the hint.
    #[test]
    fn claude_s_queued_message_hint_is_no_text_of_its_box() {
        const QUEUED: &[u8] = include_bytes!("../../testdata/claude_input_queued.bin");
        const QUEUED_DRAFT: &[u8] = include_bytes!("../../testdata/claude_input_queued_draft.bin");
        assert_eq!(
            input_box(AgentId::CLAUDE, &replayed(QUEUED, false)).as_deref(),
            Some("Press up to edit queued messages"),
            "the hint reads as text on the plain screen"
        );
        assert_eq!(
            input_box(AgentId::CLAUDE, &replayed(QUEUED, true)).as_deref(),
            Some("")
        );
        assert_eq!(
            input_box(AgentId::CLAUDE, &replayed(QUEUED_DRAFT, true)).as_deref(),
            Some("draft words")
        );
    }

    /// Grok Build writes nothing in its composer faint: a draft and a restored prompt read the
    /// same with faint cells blanked.
    #[test]
    fn grok_s_composer_text_is_not_faint() {
        for (bytes, text) in [
            (
                &include_bytes!("../../testdata/grok_composer_draft.bin")[..],
                "draft text",
            ),
            (
                &include_bytes!("../../testdata/grok_composer_restored.bin")[..],
                "NOTOKEN wait",
            ),
        ] {
            assert_eq!(
                input_box(AgentId::GROK, &replayed(bytes, true)).as_deref(),
                Some(text)
            );
        }
    }
}
