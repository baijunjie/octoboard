//! Reading what an agent's input box holds off its screen, for clearing a prompt the agent puts
//! back there after an interrupt: Claude Code's box between two horizontal rules, Grok Build's
//! composer drawn as a box of its own. Both mark the first row with `❯` and indent the rest.
//!
//! The screen given is the one with faint cells blanked
//! (`crate::terminal_snapshot::TerminalSnapshot::visible_text_undimmed`): Claude Code writes the
//! placeholder of a fresh session's empty box (`❯ Try "…"`) faint, and it is no text of the box.
//! Grok Build writes nothing in its composer faint (1.0.70).

use crate::protocol::Agent;

/// The prompt row's mark in Claude Code's input box and in Grok Build's composer.
const PROMPT_MARK: char = '\u{276F}';

/// The text in the agent's input box as `screen` (the visible rows) shows it, one row a line with
/// the indent taken off and empty rows at the end dropped; empty for an empty box, `None` where no
/// input box is recognised. Codex requires a recognised final status footer to locate its box.
pub(crate) fn input_box(agent: Agent, screen: &str) -> Option<String> {
    let mut lines = input_box_rows(agent, screen)?;
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    Some(lines.join("\n"))
}

/// The rows of the agent's input box, each trimmed and the prompt mark off the first, empty rows
/// kept: a Ctrl-U that removes an empty row changes these and not [`input_box`]'s text.
pub(super) fn input_box_rows(agent: Agent, screen: &str) -> Option<Vec<String>> {
    match agent {
        Agent::Claude => claude_input(screen),
        Agent::Grok => grok_composer(screen),
        Agent::Codex => codex_composer(screen),
    }
}

/// An input box's rows, the prompt mark off the first and each trimmed.
fn box_rows<'a>(rows: impl Iterator<Item = &'a str>) -> Vec<String> {
    rows.enumerate()
        .map(|(at, row)| {
            let row = row.trim();
            if at == 0 {
                row.trim_start_matches(PROMPT_MARK).trim().to_string()
            } else {
                row.to_string()
            }
        })
        .collect()
}

/// Codex has no composer border. Its last visible row is a measured status footer, separated
/// from the composer by one blank row. Unknown footer shapes hold delivery instead of guessing
/// where a draft ends; empty lines inside the composer remain text.
fn codex_composer(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let footer = rows.iter().rposition(|row| !row.trim().is_empty())?;
    if !codex_footer(rows[footer].trim()) || footer < 2 || !rows[footer - 1].trim().is_empty() {
        return None;
    }
    let start = rows[..footer - 1]
        .iter()
        .rposition(|row| row.starts_with('›'))?;
    let mut composer = rows[start..footer - 1]
        .iter()
        .map(|row| row.trim().to_string())
        .collect::<Vec<_>>();
    composer[0] = composer[0].trim_start_matches('›').trim().to_string();
    Some(composer)
}

fn codex_footer(row: &str) -> bool {
    let body = if let Some((body, warning)) = row.rsplit_once('⚠') {
        let warning = warning.trim();
        let count = warning
            .strip_suffix(" warning · f2 to view")
            .filter(|count| count.trim() == "1")
            .or_else(|| {
                warning
                    .strip_suffix(" warnings · f2 to view")
                    .filter(|count| count.trim().parse::<u32>().is_ok_and(|count| count > 1))
            });
        if count.is_none() {
            return false;
        }
        let body = body.trim_end();
        // A live spinner can remain between the directory and its warnings while startup settles.
        match body.rsplit_once(" · ") {
            Some((body, spinner))
                if !spinner.is_empty()
                    && spinner
                        .chars()
                        .all(|c| ('\u{2800}'..='\u{28ff}').contains(&c)) =>
            {
                body
            }
            _ => body,
        }
    } else {
        row
    };
    let model_directory = body.strip_suffix(" · ← for agents").unwrap_or(body);
    if let Some((model, directory)) = model_directory.split_once(" · ") {
        if model.split_whitespace().count() >= 2
            && !directory.contains(" · ")
            && std::path::Path::new(directory).is_absolute()
        {
            return true;
        }
    }
    let Some(percent) = body.strip_suffix("% context left") else {
        return false;
    };
    percent.parse::<u8>().is_ok_and(|percent| percent <= 100)
}

/// The rows of Grok Build's composer: those of the last box drawn with `╭─╮` and `╰─╯`, each
/// between its `│` borders, the first starting with `❯`.
fn grok_composer(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim).collect();
    let top = rows.iter().rposition(|row| {
        row.strip_prefix('\u{256D}')
            .and_then(|row| row.strip_suffix('\u{256E}'))
            .is_some_and(|rule| {
                let rule = rule.replace("Stashed", "");
                rule.chars().filter(|c| *c == '\u{2500}').count() >= 20
                    && rule.chars().all(|c| c == '\u{2500}' || c.is_whitespace())
            })
    })?;
    let bottom = top
        + 1
        + rows[top + 1..]
            .iter()
            .position(|row| row.starts_with('\u{2570}'))?;
    let inside: Vec<&str> = rows[top + 1..bottom]
        .iter()
        .map(|row| row.strip_prefix('\u{2502}')?.strip_suffix('\u{2502}'))
        .collect::<Option<_>>()?;
    if !inside.first()?.trim_start().starts_with(PROMPT_MARK) {
        return None;
    }
    Some(box_rows(inside.into_iter()))
}

/// The rows of Claude Code's input box: from the last one starting with `❯` right below a
/// horizontal rule up to the next rule.
fn claude_input(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let start = (1..rows.len())
        .rev()
        .find(|&row| rows[row].starts_with(PROMPT_MARK) && is_rule(rows[row - 1]))?;
    let end = start + 1 + rows[start + 1..].iter().position(|row| is_rule(row))?;
    Some(box_rows(rows[start..end].iter().copied()))
}

/// Whether a row is one of the horizontal rules around Claude Code's input box.
fn is_rule(row: &str) -> bool {
    let row = row.trim();
    row.chars().count() >= 20 && row.chars().all(|c| c == '\u{2500}')
}

/// The screens the readers are checked against, shared with the tests of `crate::interrupt`.
#[cfg(test)]
pub(super) mod captures {
    /// Captures from Claude Code 2.1.310; see `testdata/claude-input-box.md`.
    pub(in crate::interrupt) const RESTORED: &str =
        include_str!("../../testdata/claude_input_restored.txt");
    pub(in crate::interrupt) const RESTORED_LINES: &str =
        include_str!("../../testdata/claude_input_restored_lines.txt");
    pub(in crate::interrupt) const ONE_CTRL_U: &str =
        include_str!("../../testdata/claude_input_one_ctrl_u.txt");
    pub(in crate::interrupt) const CLEARED: &str =
        include_str!("../../testdata/claude_input_cleared.txt");
    pub(in crate::interrupt) const DRAFT: &str =
        include_str!("../../testdata/claude_input_draft.txt");

    /// Captures from Grok Build 1.0.70; see `testdata/grok-composer.md`.
    pub(in crate::interrupt) const GROK_RESTORED: &str =
        include_str!("../../testdata/grok_composer_restored.txt");
    pub(in crate::interrupt) const GROK_RESTORED_LINES: &str =
        include_str!("../../testdata/grok_composer_restored_lines.txt");
    pub(in crate::interrupt) const GROK_RESTORED_WRAPPED: &str =
        include_str!("../../testdata/grok_composer_restored_wrapped.txt");
    pub(in crate::interrupt) const GROK_ONE_CTRL_U: &str =
        include_str!("../../testdata/grok_composer_one_ctrl_u.txt");
    pub(in crate::interrupt) const GROK_CLEARED: &str =
        include_str!("../../testdata/grok_composer_cleared.txt");
    pub(in crate::interrupt) const GROK_WORKING: &str =
        include_str!("../../testdata/grok_composer_working.txt");
    pub(in crate::interrupt) const GROK_DRAFT: &str =
        include_str!("../../testdata/grok_composer_draft.txt");
}

#[cfg(test)]
mod tests {
    use super::captures::*;
    use super::*;

    #[test]
    fn grok_s_composer_is_read_off_the_screen() {
        let read = |screen| input_box(Agent::Grok, screen);
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
        assert_eq!(input_box(Agent::Claude, GROK_RESTORED), None);
        assert_eq!(input_box(Agent::Codex, RESTORED), None);
    }

    #[test]
    fn claude_s_input_box_is_read_off_the_screen() {
        assert_eq!(
            input_box(Agent::Claude, RESTORED).as_deref(),
            Some(
                "Think carefully about the history of the number zero and then write a 600-word \
                 essay about it."
            )
        );
        assert_eq!(
            input_box(Agent::Claude, RESTORED_LINES).as_deref(),
            Some(
                "Line one: think carefully about zero.\nLine two: then write a 600-word essay.\n\
                 Line three: be thorough."
            )
        );
        // The line a Ctrl-U emptied is still there until the next one removes its line break.
        assert_eq!(
            input_box(Agent::Claude, ONE_CTRL_U).as_deref(),
            Some("Line one: think carefully about zero.\nLine two: then write a 600-word essay.")
        );
        assert_eq!(input_box(Agent::Claude, CLEARED).as_deref(), Some(""));
        assert_eq!(
            input_box(Agent::Claude, DRAFT).as_deref(),
            Some("draft words")
        );
        // The emptied row is still a row, so the next press shows as a change.
        assert_eq!(
            input_box_rows(Agent::Claude, ONE_CTRL_U).map(|rows| rows.len()),
            Some(3)
        );
        assert_eq!(
            input_box(Agent::Claude, "Not Claude Code\n> prompt\n"),
            None
        );
    }

    #[test]
    fn codex_reads_the_whole_composer_including_empty_leading_rows() {
        let read = |bytes: &[u8]| {
            let mut screen = crate::terminal_snapshot::TerminalSnapshot::new(45, 140);
            screen.process(bytes);
            input_box(Agent::Codex, &screen.visible_text_undimmed())
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
            input_box(Agent::Codex, screen).as_deref(),
            Some("\n\n100% context left")
        );
        assert_eq!(
            input_box(
                Agent::Codex,
                "›\n\n  100% context left\n\n  unknown footer\n"
            ),
            None
        );
        assert_eq!(
            input_box(Agent::Codex, "›\n  100% context left\n"),
            None,
            "a separator row is required"
        );
        assert_eq!(
            input_box(
                Agent::Codex,
                "›\n\n  probe default · /tmp/project            ⚠ 1 warning · f2 to view\n"
            )
            .as_deref(),
            Some("")
        );
        assert_eq!(
            input_box(
                Agent::Codex,
                "›\n\n  100% context left  ⚠ 1 warning · f2 to view\n"
            )
            .as_deref(),
            Some("")
        );
        assert_eq!(
            input_box(
                Agent::Codex,
                "›\n\n  100% context left  ⚠ 0 warnings · f2 to view\n"
            ),
            None
        );
    }

    /// Raw captures of a real session's output (Claude Code 2.1.310, Grok Build 1.0.70), replayed
    /// into the daemon's own screen.
    fn replayed(bytes: &[u8], undimmed: bool) -> String {
        let mut screen = crate::terminal_snapshot::TerminalSnapshot::new(40, 120);
        screen.process(bytes);
        if undimmed {
            screen.visible_text_undimmed()
        } else {
            screen.visible_text()
        }
    }

    /// Claude Code writes a fresh session's placeholder faint, so it is no text of the box once
    /// faint cells are blanked, while a prompt it puts back is written plain and stays.
    #[test]
    fn claude_s_faint_placeholder_is_no_text_of_its_box() {
        const PLACEHOLDER: &[u8] = include_bytes!("../../testdata/claude_input_placeholder.bin");
        const RESTORED_RAW: &[u8] = include_bytes!("../../testdata/claude_input_restored.bin");
        assert_eq!(
            input_box(Agent::Claude, &replayed(PLACEHOLDER, false)).as_deref(),
            Some("Try \"write a test for <filepath>\""),
            "the placeholder reads as text on the plain screen"
        );
        assert_eq!(
            input_box(Agent::Claude, &replayed(PLACEHOLDER, true)).as_deref(),
            Some("")
        );
        assert_eq!(
            input_box(Agent::Claude, &replayed(RESTORED_RAW, true)).as_deref(),
            Some("Think carefully about the history of zero, then write a 600-word essay.")
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
                input_box(Agent::Grok, &replayed(bytes, true)).as_deref(),
                Some(text)
            );
        }
    }
}
