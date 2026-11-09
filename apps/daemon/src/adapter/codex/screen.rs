//! How Codex's screens read: its dialogs, its composer above its status footer, its folder-trust
//! confirmation, its terminal title, and its interrupt key. The measurements are in
//! `docs/agent-cli-reference.md`, section "## Codex".

use crate::platform::names::PathStyle;
use crate::session::dialogs::{DialogPattern, WrittenMessage};
use crate::session::screen::{Interrupt, InterruptKey, ScreenReader};
use crate::session::title_activity::{spinner_activity, Shown};
use crate::session::trust::{Press, Screen, StartSignal};

pub(super) struct CodexScreen;

impl ScreenReader for CodexScreen {
    fn dialogs(&self) -> &'static [DialogPattern] {
        DIALOGS
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        composer(screen, crate::platform::names::PATH_STYLE)
    }

    fn input_placeholder(&self, row: &str) -> bool {
        // Written faint in an empty composer (0.163.0; `testdata/codex-composer.md`).
        row == "Ask Codex to do anything"
    }

    fn as_submitted<'a>(&self, typed: &'a str) -> &'a str {
        typed.trim()
    }

    fn trust_screen(&self) -> &'static Screen {
        &TRUST
    }

    /// Its paste burst: 0.166.0 on a Windows console, which hands it a paste as keys, drew a
    /// message whose Enter came in the same write as the paste with that Enter as a line break
    /// in the composer, and a further Enter sent it.
    fn joins_enter_to_key_paste(&self) -> bool {
        true
    }

    fn title_activity(&self, glyph: Option<char>) -> Option<Shown> {
        // A title with no spinner is its project or thread name.
        spinner_activity(glyph)
    }

    fn title_needs_user(&self, title: &str) -> bool {
        super::action_required_title(title)
    }

    fn user_action_title(&self) -> Option<&'static str> {
        Some("Action Required")
    }

    fn interrupt(&self) -> Option<&'static Interrupt> {
        Some(&INTERRUPT)
    }
}

/// Esc stops a turn. Ctrl-C is never sent, since at an idle Codex it exits, nor a second Esc,
/// since at an idle Codex two open its transcript rewind. Codex does not restore the prompt after
/// a cancel: its composer is read for delivery only.
const INTERRUPT: Interrupt = Interrupt {
    key: InterruptKey::Esc,
    restores_prompt: false,
    written_during_turn,
    queued_waiting: Some(queued_waiting),
};

/// A message written into a Codex turn through its terminal is steered into that turn, which the
/// cancel throws away with it, unless the turn had acted on it already.
fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn. Codex \
         steers such messages into the turn the cancel threw away, so unless the turn had already \
         acted on them they were not acted on: resend whatever still matters."
    )
}

/// A message Codex's own queue took is no part of the turn a cancel throws away, and Codex leaves
/// it queued past the cancel; Octoboard starts the queue, which runs one message a turn, unless
/// something someone else queued is ahead of Octoboard's there.
fn queued_waiting(started: &crate::session::term::QueueStart) -> Option<String> {
    use crate::session::term::QueueStart;
    Some(match started {
        QueueStart::Nothing => return None,
        QueueStart::Started(waiting) => format!(
            " {waiting} message(s) Octoboard put into Codex's own queue were waiting there. They \
             were not part of the turn the cancel threw away, and now run as turns of their own, \
             one after another: do not send them again."
        ),
        QueueStart::BehindOthers(waiting) => format!(
            " {waiting} message(s) Octoboard put into Codex's own queue wait there behind messages \
             someone else queued, so Octoboard did not start them ahead of those; they were not \
             part of the turn the cancel threw away. Check the session with `get_session` before \
             sending them again."
        ),
        QueueStart::Failed(_) => " Messages Octoboard put into Codex's own queue may still be \
             waiting there: Octoboard could not tell, nor start them. Check the session with \
             `get_session` before sending any again."
            .to_string(),
    })
}

/// Codex has no composer border. Its last visible row is a measured status footer, separated
/// from the composer by one blank row. Unknown footer shapes hold delivery instead of guessing
/// where a draft ends; empty lines inside the composer remain text.
///
/// The footer may also take two rows, a model and directory row and then a hints row, as Codex
/// 0.166.0 was captured drawing it by default on Windows and on macOS
/// (`testdata/conhost-captures.md`, `testdata/codex-composer.md`). Its directory may be named
/// from the home directory on any host, and the Windows way only where the host's paths are
/// Windows ones (`style`). Either layout is read in each shape Codex 0.166.0's source gives it at
/// narrower widths and in other states (`testdata/codex-composer.md`, section "Footer shapes from
/// Codex 0.166.0's source").
fn composer(screen: &str, style: PathStyle) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let last = rows.iter().rposition(|row| !row.trim().is_empty())?;
    let footer = if last >= 1
        && is_hints(rows[last].trim())
        && is_footer(rows[last - 1].trim(), style, true)
    {
        last - 1
    } else if is_footer(rows[last].trim(), style, false) {
        last
    } else {
        return None;
    };
    if footer < 2 || !rows[footer - 1].trim().is_empty() {
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

/// The shortcuts hint at an empty composer, with Codex's default keys.
const SHORTCUTS_HINTS: [&str; 2] = ["? for shortcuts", "← for agents · ? for shortcuts"];

/// The queue hint while a turn runs and the composer holds a draft, whole and shortened.
const QUEUE_HINTS: [&str; 2] = ["tab to queue message", "tab to queue"];

/// The hints for a moment after an Esc, a Ctrl-C or a Ctrl-D at an idle composer, the key named as
/// each platform's Codex names it.
const KEY_HINTS: [&str; 8] = [
    "esc esc to edit previous message",
    "esc again to edit previous message",
    "ctrl+c again to quit",
    "ctrl+d again to quit",
    "⌃c again to quit",
    "⌃d again to quit",
    "^c again to quit",
    "^d again to quit",
];

/// The agents hint Codex appends to its status line at an empty composer.
const AGENTS_HINT: &str = "← for agents";

/// The fewest characters of a clipped hint taken as one: fewer say too little to tell a hint
/// from the last row of a draft.
const CLIPPED_HINT_MIN: usize = 3;

/// Whether `warning`, what follows the `⚠` of a footer row, counts the warnings as Codex does: in
/// full, or as the count and its key alone where the full count would take more than half the
/// row.
fn is_warning_count(warning: &str) -> bool {
    let warning = warning.trim();
    let parsed = |number: &str| number.trim().parse::<u32>().ok();
    if let Some(one) = warning.strip_suffix(" warning · f2 to view") {
        return one.trim() == "1";
    }
    if let Some(many) = warning.strip_suffix(" warnings · f2 to view") {
        return parsed(many).is_some_and(|total| total > 1);
    }
    warning
        .strip_suffix(" · f2")
        .and_then(parsed)
        .is_some_and(|total| total > 0)
}

/// The second row of a two-row footer: a hint and the warnings, either of them alone (the
/// shortcuts hint is gone while a draft is typed). Codex clips the hint, unlike its status line,
/// at the edge of the room the warnings leave it, so a narrow row shows only its beginning. Only
/// the shortcuts hint shares the row with the warnings, which Codex shows only at a passive
/// composer.
fn is_hints(row: &str) -> bool {
    let (hint, warned) = match row.rsplit_once('⚠') {
        Some((hint, warning)) if is_warning_count(warning) => (hint.trim_end(), true),
        Some(_) => return false,
        None => (row, false),
    };
    if hint.is_empty() {
        return true;
    }
    let clipped = |full: &&str| hint.chars().count() >= CLIPPED_HINT_MIN && full.starts_with(hint);
    SHORTCUTS_HINTS.iter().any(clipped)
        || !warned && (QUEUE_HINTS.iter().any(clipped) || KEY_HINTS.iter().any(clipped))
}

/// A one-row footer, or the first row of two (`first_of_two`), which is only ever the status
/// line. A one-row footer is the status line with the warnings at its right, or a row without it
/// ([`is_hint_row`]).
fn is_footer(row: &str, style: PathStyle, first_of_two: bool) -> bool {
    let warned = row.contains('⚠');
    let body = if let Some((body, warning)) = row.rsplit_once('⚠') {
        if !is_warning_count(warning) {
            return false;
        }
        let body = body.trim_end();
        // A live spinner can remain between the directory and its warnings while startup settles.
        match body.rsplit_once(" · ") {
            Some((body, spinner))
                if !spinner.is_empty()
                    && spinner
                        .chars()
                        .all(crate::terminal::title::is_braille_spinner_glyph) =>
            {
                body
            }
            _ => body,
        }
    } else {
        row
    };
    is_status_line(without_indicator(body), style, first_of_two)
        || !first_of_two && is_hint_row(body, warned)
}

/// Whether `body` is Codex's status line with its default items: the model with its reasoning
/// effort, the directory, and the thread's name once the thread has one; at an empty composer the
/// agents hint follows them.
///
/// A row too narrow for the line ends it with `…` where it was cut, so a cut directory need only
/// begin like one, and the line may be cut right after the model's separator. `bare_model`, for
/// the first row of two, where the hints row below confirms the footer, also takes the model
/// alone: cut, or whole and naming a reasoning effort, as the inbox console host was seen drawing
/// that row in a narrow terminal.
fn is_status_line(body: &str, style: PathStyle, bare_model: bool) -> bool {
    let (body, cut, separated) = match body.strip_suffix('…') {
        Some(kept) => {
            let kept = kept.trim_end();
            let line = kept.trim_end_matches('·').trim_end();
            (line, true, line.len() < kept.len())
        }
        None => (body, false, false),
    };
    let mut segments: Vec<&str> = body.split(" · ").collect();
    let mut last_cut = cut;
    if segments.len() > 2 {
        let last = segments[segments.len() - 1];
        if last == AGENTS_HINT || cut && last.starts_with('←') && AGENTS_HINT.starts_with(last) {
            segments.pop();
            last_cut = false;
        }
    }
    let Some((model, rest)) = segments.split_first() else {
        return false;
    };
    if model.split_whitespace().count() < 2 {
        return false;
    }
    match rest {
        [] => separated || bare_model && (cut || names_effort(model)),
        [directory] => {
            shows_directory(directory, style) || last_cut && begins_directory(directory, style)
        }
        [directory, thread @ ..] => {
            shows_directory(directory, style) && thread.iter().any(|part| !part.is_empty())
        }
    }
}

/// Whether `model`, the model as the status line names it, names a reasoning effort after the
/// model's own name, as Codex names the efforts it knows ("default" where none is set).
fn names_effort(model: &str) -> bool {
    const EFFORTS: [&str; 9] = [
        "default",
        "minimal",
        "low",
        "medium",
        "high",
        "xhigh",
        "max",
        "ultra",
        "persistent",
    ];
    model
        .split_whitespace()
        .skip(1)
        .any(|word| EFFORTS.contains(&word))
}

/// The labels Codex right-aligns on its status line's row: the collaboration mode or the goal's
/// status, then "IDE context", joined by " · ".
const INDICATORS: [&str; 9] = [
    "Plan mode",
    "Plan mode (⇧tab to cycle)",
    "Plan mode (shift+tab to cycle)",
    "Pursuing goal",
    "Goal paused (/goal resume)",
    "Goal stalled (/goal resume)",
    "Goal hit usage limits (/goal resume)",
    "Goal abandoned",
    "Goal achieved",
];

/// The goal statuses that name the goal's usage in parentheses after them.
const GOAL_USAGE_INDICATORS: [&str; 3] = ["Pursuing goal (", "Goal unmet (", "Goal achieved ("];

/// `body` without the labels at its right ([`INDICATORS`]), which stand at least one blank after
/// some text of the line; `body` itself where it has none.
fn without_indicator(body: &str) -> &str {
    // "IDE context" after a separator follows a mode or a goal status.
    let (head, ide_alone) = match body.strip_suffix("IDE context") {
        Some(left) => match left.strip_suffix(" · ") {
            Some(head) => (head, false),
            None => (left, true),
        },
        None => (body, false),
    };
    let left = if ide_alone {
        Some(head)
    } else {
        INDICATORS
            .iter()
            .find_map(|label| head.strip_suffix(label))
            .or_else(|| {
                let usage = head.strip_suffix(')')?;
                GOAL_USAGE_INDICATORS.iter().find_map(|label| {
                    let at = usage.rfind(label)?;
                    (!usage[at + label.len()..].contains(['(', ')'])).then(|| &head[..at])
                })
            })
    };
    match left {
        Some(left) if left.ends_with(' ') && !left.trim().is_empty() => left.trim_end(),
        _ => body,
    }
}

/// Whether `body` is a one-row footer that shows no status line (configured off, or while a turn
/// runs and the composer holds a draft): a hint at the left and the context window left at the
/// right, or the context alone. Without the context a row shows only the queue hint, once the
/// context no longer fits beside it, or a hint after a key ([`KEY_HINTS`]). Neither is drawn
/// beside the warnings (`warned`).
fn is_hint_row(body: &str, warned: bool) -> bool {
    let hint = if let Some(head) = body.strip_suffix("% context left") {
        let (hint, percent) = head.rsplit_once(' ').unwrap_or(("", head));
        if !percent.parse::<u8>().is_ok_and(|percent| percent <= 100) {
            return false;
        }
        hint.trim_end()
    } else if let Some(head) = body.strip_suffix(" used") {
        let (hint, tokens) = head.rsplit_once(' ').unwrap_or(("", head));
        if !is_token_count(tokens) {
            return false;
        }
        hint.trim_end()
    } else {
        return !warned && (QUEUE_HINTS.contains(&body) || KEY_HINTS.contains(&body));
    };
    hint.is_empty()
        || hint == AGENTS_HINT
        || SHORTCUTS_HINTS.contains(&hint)
        || !warned && QUEUE_HINTS.contains(&hint)
}

/// Whether `tokens` is a count of tokens as Codex writes it compactly when it knows no context
/// window to take a percentage of: a whole number under a thousand, or a number of up to three
/// digits, maybe with decimals, and K, M, B or T.
fn is_token_count(tokens: &str) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    let Some(number) = tokens.strip_suffix(['K', 'M', 'B', 'T']) else {
        return tokens.len() <= 3 && digits(tokens);
    };
    match number.split_once('.') {
        Some((whole, fraction)) => whole.len() <= 3 && digits(whole) && digits(fraction),
        None => number.len() <= 3 && digits(number),
    }
}

/// Whether `directory`, as Codex's footer prints it, names a directory: an absolute Unix path or
/// one from the home directory (`~`, `~/…`), as Codex prints them on a Unix host, on any host (a
/// macOS capture replayed on Windows shows them), and on a host whose paths are Windows ones
/// (`style`) also a drive or UNC path or one from the home directory the Windows way (`~\…`).
fn shows_directory(directory: &str, style: PathStyle) -> bool {
    if directory.starts_with('/') || directory == "~" || directory.starts_with("~/") {
        return true;
    }
    if style != PathStyle::Windows {
        return false;
    }
    let bytes = directory.as_bytes();
    directory.starts_with("\\\\")
        || directory.starts_with("~\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// Whether `prefix`, the part of a directory left before a cut, can begin one that
/// [`shows_directory`] takes.
fn begins_directory(prefix: &str, style: PathStyle) -> bool {
    if prefix.starts_with('/') || prefix == "~" || prefix.starts_with("~/") {
        return true;
    }
    if style != PathStyle::Windows {
        return false;
    }
    match prefix.as_bytes() {
        [b'~', b'\\', ..] | [b'\\'] | [b'\\', b'\\', ..] => true,
        [drive] | [drive, b':'] | [drive, b':', b'\\' | b'/', ..] => drive.is_ascii_alphabetic(),
        _ => false,
    }
}

/// Codex trust screens offer "› 1. Trust and continue" and either "2. Quit" (0.161.0) or
/// "2. Back to Agent Command Center" (the folder-access form). Codex redraws only
/// the cells that changed, so its text can lose characters wherever the frame before held the
/// same one; only phrases measured intact in its first drawing are matched. Its hook-review modal
/// ("Trust all and continue") holds neither piece.
pub(crate) const TRUST: Screen = Screen {
    first_draw: &[
        &["Trustthisfolder?", "\u{203A}1.Trustandcontinue2.Quit"],
        &[
            "Folderaccess",
            "Trustthisfolder?",
            "\u{203A}1.Trustandcontinue2.BacktoAgentCommandCenter",
        ],
    ],
    accept: Some("1.Trustandcontinue"),
    decline: &["2.Quit", "2.BacktoAgentCommandCenter"],
    cursor: Some('\u{203A}'),
    press: Press::Enter,
    // Its banner, ">_ OpenAI Codex (v…)", drawn whole over the screen it replaces.
    next_screen: Some("OpenAICodex"),
    // Codex runs no hook until its first prompt, so an Enter pressed in the terminal would
    // otherwise leave the screen looking unanswered until then.
    input_ends_wait: true,
    start_signal: StartSignal::TerminalTitle,
    hook_fallback: None,
    waits_for_paste_mode: false,
};

const DIALOGS: &[DialogPattern] = &[
    DialogPattern::all_of(
        "codex_sign_in",
        WrittenMessage::Discarded,
        &[
            "WelcometoCodex",
            "SigninwithChatGPT",
            "Pressentertocontinue",
        ],
    ),
    DialogPattern::all_of(
        "codex_resume_cwd",
        WrittenMessage::Discarded,
        &[
            "Usesessiondirectory",
            "Usecurrentdirectory",
            "escusesession",
        ],
    ),
    DialogPattern::all_of(
        "codex_model_migration",
        WrittenMessage::Discarded,
        &["Trynewmodel", "Useexistingmodel", "enter/escconfirm"],
    ),
    DialogPattern::all_of(
        "codex_rate_limit_nudge",
        WrittenMessage::Discarded,
        &["Approachingratelimits", "Keepcurrentmodel"],
    ),
];

#[cfg(test)]
mod tests {
    use super::composer;
    use crate::platform::names::PathStyle;

    /// The footer's directory is the agent's text: an absolute Unix path or one from the home
    /// directory (`~`, `~/…`) ends the composer on any host, in a footer of one row or two and
    /// followed by the thread's name; a Windows one only where the host's paths are Windows ones.
    #[test]
    fn codex_s_footer_ends_the_composer_in_the_shapes_of_the_host() {
        let hints = "  ? for shortcuts                ⚠ 2 warnings · f2 to view";
        let one_row = |directory: &str| format!("› draft\n\n  probe default · {directory}\n");
        let two_rows =
            |directory: &str| format!("› draft\n\n  probe default · {directory}\n{hints}\n");
        for style in [PathStyle::Unix, PathStyle::Windows] {
            for directory in ["/tmp/project", "~/project", "~"] {
                assert!(
                    composer(&one_row(directory), style).is_some(),
                    "{directory}"
                );
                assert!(
                    composer(&two_rows(directory), style).is_some(),
                    "{directory}"
                );
                let named = format!("{directory} · Say hi");
                assert!(composer(&one_row(&named), style).is_some(), "{named}");
                assert!(composer(&two_rows(&named), style).is_some(), "{named}");
            }
        }
        for directory in ["~\\project", "C:\\project", "\\\\host\\share"] {
            assert!(composer(&one_row(directory), PathStyle::Windows).is_some());
            assert!(composer(&one_row(directory), PathStyle::Unix).is_none());
            assert!(composer(&two_rows(directory), PathStyle::Windows).is_some());
            assert!(composer(&two_rows(directory), PathStyle::Unix).is_none());
        }
        let named = "› draft\n\n  probe default · ~\\project · Say hi\n";
        assert!(composer(named, PathStyle::Windows).is_some());
        assert!(composer(named, PathStyle::Unix).is_none());
        for screen in [
            "› draft\n\n  probe default · project\n",
            "› draft\n\n  probe default · ~project\n",
            "› draft\n\n  probe default · /tmp/project\n  more of the draft\n",
            "› draft\n\n  ? for shortcuts\n",
            "› draft\n  probe default · /tmp/project\n  ? for shortcuts\n",
        ] {
            for style in [PathStyle::Unix, PathStyle::Windows] {
                assert!(composer(screen, style).is_none(), "{screen:?}");
            }
        }
    }

    /// The inbox console host's captures of Codex 0.166.0 (`testdata/conhost-captures.md`): the
    /// composer is read where the host's paths are Windows ones, before the first turn and after
    /// it, when the footer's model row ends with the thread's name.
    #[test]
    fn codex_s_composer_reads_through_the_inbox_console_host_s_repaint() {
        for (capture, draft) in [
            (
                &include_bytes!("../../../testdata/conhost_codex_fresh_empty.bin")[..],
                "",
            ),
            (
                include_bytes!("../../../testdata/conhost_codex_fresh_draft.bin"),
                "say hi in two words",
            ),
            (
                include_bytes!("../../../testdata/conhost_codex_turn_finished.bin"),
                "",
            ),
            (
                include_bytes!("../../../testdata/conhost_codex_interrupted.bin"),
                "",
            ),
        ] {
            let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
            screen.process(capture);
            let rows = composer(&screen.visible_text_undimmed(), PathStyle::Windows)
                .unwrap_or_else(|| panic!("a composer in\n{}", screen.visible_text()));
            let text = rows.join("\n");
            assert_eq!(text.trim_start_matches('›').trim(), draft, "{rows:?}");
        }
    }

    /// Codex 0.166.0's default two-row footer as a macOS terminal recorded it at 120 and at 44
    /// columns (`testdata/codex-composer.md`): the empty composer after the folder-trust screen,
    /// then the same screen once `draft line one` is typed.
    #[test]
    fn codex_s_two_row_footer_is_read_from_a_macos_recording() {
        for (capture, columns, empty_at) in [
            (
                &include_bytes!("../../../testdata/codex_0166_macos_120x40.bin")[..],
                120,
                12769,
            ),
            (
                include_bytes!("../../../testdata/codex_0166_macos_44x40.bin"),
                44,
                6009,
            ),
        ] {
            for (bytes, draft) in [(&capture[..empty_at], ""), (capture, "draft line one")] {
                let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, columns);
                screen.process(bytes);
                let rows = composer(&screen.visible_text_undimmed(), PathStyle::Unix)
                    .unwrap_or_else(|| panic!("a composer in\n{}", screen.visible_text()));
                assert_eq!(rows, [draft], "{columns}");
            }
        }
    }

    /// `text` cut with `…` to `room` columns, as Codex 0.166.0 cuts its status line
    /// (`truncate_line_with_ellipsis_if_overflow`).
    fn ellipsized(text: &str, room: usize) -> String {
        if text.chars().count() <= room {
            return text.to_string();
        }
        let kept: String = text.chars().take(room.saturating_sub(1)).collect();
        format!("{kept}…")
    }

    /// `text` clipped to `room` columns, as the paragraph Codex 0.166.0 draws a hint in clips it.
    fn clipped(text: &str, room: usize) -> String {
        text.chars().take(room).collect()
    }

    /// A footer row `width` columns wide: `left` after the two-column indent, and `right` ending
    /// `pad` columns before the edge: one for the warnings (`warning_notice_layout`), two for
    /// the context and the status line's labels (`render_context_right`).
    fn laid_out(width: usize, left: &str, right: &str, pad: usize) -> String {
        let left = format!("  {left}");
        if right.is_empty() {
            return left;
        }
        let at = width - right.chars().count() - pad;
        assert!(left.chars().count() < at, "{left:?} runs into {right:?}");
        format!("{left:<at$}{right}")
    }

    /// `count` warnings, none for none, at the right of a footer `width` columns wide
    /// (`warning_notice_layout`): in full where they take at most half the row, the count and its
    /// key otherwise.
    fn warnings(width: usize, count: usize) -> String {
        if count == 0 {
            return String::new();
        }
        let available = width - 3;
        let budget = (available / 2).max(14).min(available);
        let full = format!("⚠ {count} warnings · f2 to view");
        if full.chars().count() <= budget {
            full
        } else {
            format!("⚠ {count} · f2")
        }
    }

    /// The room a footer row `width` columns wide leaves its left part, after the indent and
    /// before the warnings, if any, and the two columns kept clear of them.
    fn room(width: usize, warning: &str) -> usize {
        match warning.chars().count() {
            0 => width - 2,
            taken => (width - 2).saturating_sub(taken + 3),
        }
    }

    /// The two rows Codex 0.166.0 draws below its composer where it owns the screen
    /// (`chat_composer/status_surface.rs`, `render_with_options`): the status line, cut to the
    /// row, and the hint, clipped before the `warned` warnings.
    fn two_row_footer(width: usize, status: &str, hint: &str, warned: usize) -> String {
        let warning = warnings(width, warned);
        let status = format!("  {}", ellipsized(status, width - 2));
        let hint = laid_out(width, &clipped(hint, room(width, &warning)), &warning, 1);
        format!("{status}\n{hint}")
    }

    /// The one row Codex 0.166.0 draws below its composer in the terminal's own scrollback at a
    /// passive composer: the status line, cut before the `warned` warnings.
    fn status_row(width: usize, status: &str, warned: usize) -> String {
        let warning = warnings(width, warned);
        laid_out(
            width,
            &ellipsized(status, room(width, &warning)),
            &warning,
            1,
        )
    }

    /// The one row Codex 0.166.0 draws there while a turn runs and the composer holds a draft
    /// (`single_line_footer_layout`): the queue hint, shortened, then without the context.
    fn queue_row(width: usize, context: &str) -> String {
        let fits_beside = |hint: &str| 2 + hint.len() < width - context.len() - 2;
        match ["tab to queue message", "tab to queue"]
            .into_iter()
            .find(|hint| fits_beside(hint))
        {
            Some(hint) => laid_out(width, hint, context, 2),
            None if width >= 22 => "  tab to queue message".to_string(),
            None => "  tab to queue".to_string(),
        }
    }

    /// The layouts above give Codex 0.166.0's own snapshots of them.
    #[test]
    fn codex_s_footer_layouts_match_its_snapshots() {
        // `footer_collapse_queue_*` (`bottom_pane/snapshots/`), at their widths.
        for (width, snapshot) in [
            (120, "  tab to queue message                                                                                98% context left"),
            (50, "  tab to queue message          98% context left"),
            (40, "  tab to queue        98% context left"),
            (30, "  tab to queue message"),
            (20, "  tab to queue"),
        ] {
            assert_eq!(queue_row(width, "98% context left"), snapshot, "{width}");
        }
        // `warning_notice_styles` (`chat_composer/snapshots/`), 80 columns, a draft.
        assert_eq!(
            two_row_footer(80, "MODEL · ~/project", "", 3),
            "  MODEL · ~/project\n                                                      ⚠ 3 warnings · f2 to view"
        );
    }

    /// The footer of two rows Codex 0.166.0 draws where it owns the screen, read on either host,
    /// with a directory of its kind, at every width that leaves the model whole: the shortcuts
    /// hint or the queue hint, clipped; the warnings in full or as their count; the status line
    /// whole or cut, or the model alone, as the inbox console host was seen drawing it.
    #[test]
    fn codex_s_two_row_footer_is_read_at_every_width() {
        for (style, directory) in [
            (PathStyle::Windows, "~\\code\\codex-trust"),
            (PathStyle::Unix, "~/code/codex-trust"),
        ] {
            two_row_footer_is_read_at_every_width(style, directory);
        }
        // The VM's captures: the queue hint below the model row, and the narrow row's warnings.
        for footer in [
            "  GPT-6.1-Sol default · ~\\code\\codex-trust\n  tab to queue message",
            "  GPT-6.1-Sol default\n  ? for shortcuts                 ⚠ 2 · f2",
        ] {
            let screen = format!("› draft\n\n{footer}\n");
            assert!(composer(&screen, PathStyle::Windows).is_some(), "{screen}");
        }
    }

    /// The two-row footer read where the host's paths are `style` ones, with `directory` on its
    /// status line, at every width, hint and warnings count above.
    fn two_row_footer_is_read_at_every_width(style: PathStyle, directory: &str) {
        let status = format!("GPT-6.1-Sol default · {directory}");
        let named = format!("{status} · Say hi");
        for width in [120, 80, 52, 44, 30, 24, 16] {
            for status in [&status, &named, "GPT-6.1-Sol default"] {
                for (draft, hint, warned) in [
                    ("", "? for shortcuts", 2),
                    ("", "? for shortcuts", 0),
                    ("", "← for agents · ? for shortcuts", 2),
                    ("say hi in two words", "", 12),
                    ("say hi in two words", "tab to queue message", 0),
                ] {
                    let screen = format!(
                        "› {draft}\n\n{}\n",
                        two_row_footer(width, status, hint, warned)
                    );
                    let rows = composer(&screen, style)
                        .unwrap_or_else(|| panic!("a composer in\n{screen}"));
                    assert_eq!(rows.join("\n"), draft, "{screen}");
                }
            }
        }
    }

    /// The footer of one row Codex 0.166.0 draws in the terminal's own scrollback, read on any
    /// host at every width that leaves the model and its separator whole: the status line whole
    /// or cut, with the agents hint at an empty composer and the warnings in full or as their
    /// count; the queue hint with the context while a turn runs and the composer holds a draft.
    #[test]
    fn codex_s_one_row_footer_is_read_at_every_width() {
        for style in [PathStyle::Unix, PathStyle::Windows] {
            for width in [140, 80, 52, 44, 30] {
                for (draft, footer) in [
                    (
                        "",
                        status_row(width, "probe default · /tmp/project · ← for agents", 2),
                    ),
                    (
                        "",
                        status_row(width, "probe default · /tmp/project · ← for agents", 0),
                    ),
                    (
                        "draft",
                        status_row(width, "probe default · /tmp/project", 3),
                    ),
                    (
                        "draft",
                        status_row(width, "probe default · /tmp/project", 0),
                    ),
                    ("draft", queue_row(width, "100% context left")),
                    ("draft", queue_row(width, "1.2K used")),
                ] {
                    let screen = format!("› {draft}\n\n{footer}\n");
                    let rows = composer(&screen, style)
                        .unwrap_or_else(|| panic!("a composer in\n{screen}"));
                    assert_eq!(rows.join("\n"), draft, "{screen}");
                }
            }
            for footer in [
                // `footer_collapse_empty_*` and `footer_mode_hidden_while_typing`: no status line.
                "  ? for shortcuts                        100% context left",
                "       100% context left",
                // `footer_mode_esc_hint_*` and `footer_mode_ctrl_c_quit`.
                "  esc esc to edit previous message",
                "  esc again to edit previous message",
                "  ⌃c again to quit",
                "  ctrl+c again to quit",
                // An agents hint beside the context, with the shortcuts hint and without it.
                "  ← for agents · ? for shortcuts         100% context left",
                "  ← for agents                           100% context left",
                // The labels at the right of the status line (`status_line_right_indicator_line`).
                "  probe default · /tmp/project                       Plan mode (⇧tab to cycle)",
                "  probe default · /tmp/project                  Plan mode (shift+tab to cycle)",
                "  probe default · /tmp/project                                     IDE context",
                "  probe default · /tmp/project             Goal unmet (63.9K / 50K tokens)",
                "  probe default · /tmp/project · ← for agents    Goal achieved (12K tokens)",
                "  probe default · /tmp/pr… Goal paused (/goal resume) · IDE context",
                "  probe default · /tmp/project       Pursuing goal (12K / 50K)   ⚠ 2 warnings · f2 to view",
            ] {
                let screen = format!("› draft\n\n{footer}\n");
                assert!(composer(&screen, style).is_some(), "{screen}");
            }
        }
    }

    /// Shapes Codex does not draw, or draws only with keys or settings of the user's own, still
    /// hold delivery: no guess is made where a draft ends.
    #[test]
    fn codex_s_unknown_footers_give_no_composer() {
        for (style, directory) in [
            (PathStyle::Windows, "~\\code\\codex-trust"),
            (PathStyle::Unix, "~/code/codex-trust"),
        ] {
            unknown_two_row_footers_give_no_composer(style, directory);
        }
        for style in [PathStyle::Unix, PathStyle::Windows] {
            for footer in [
                // One row: the model alone, or cut before its separator.
                "  GPT-6.1-Sol default".to_string(),
                status_row(26, "probe default · /tmp/project", 2),
                // A status line of the user's own (`footer_status_line_with_active_agent_label`).
                "  Status line content · Robie [explorer]".to_string(),
                "  tab to queue message            ⚠ 2 · f2".to_string(),
                "  tab to queue message   Context 20% used".to_string(),
                "  tab to queue message   1000 used".to_string(),
                "  tab to queue message   1.K used".to_string(),
                "  tab to queue message   12X used".to_string(),
                "  tab to queue message   1000K used".to_string(),
                // Codex draws the shortcuts hint alone only while the context window is pending;
                // a row that may as well be the second of an unknown footer of two is not read.
                "  ? for shortcuts".to_string(),
            ] {
                let screen = format!("› draft\n\n{footer}\n");
                assert!(composer(&screen, style).is_none(), "{screen}");
            }
        }
    }

    /// Two-row footers Codex does not draw with its default keys, with `directory` on the status
    /// line, give no composer where the host's paths are `style` ones.
    fn unknown_two_row_footers_give_no_composer(style: PathStyle, directory: &str) {
        let two_rows =
            |hints: &str| format!("› draft\n\n  GPT-6.1-Sol default · {directory}\n{hints}\n");
        for screen in [
            // The transcript's own hints, while it is scrolled back.
            two_rows("  New activity"),
            // A remapped warnings key, or queue key.
            two_rows("  ? for shortcuts                 ⚠ 2 · f3"),
            two_rows("  ctrl+q to queue message"),
            // The queue hint is never drawn beside the warnings.
            two_rows("  tab to queue message            ⚠ 2 · f2"),
            // The model cut to one word.
            format!(
                "› draft\n\n{}\n",
                two_row_footer(12, "GPT-6.1-Sol default", "", 2)
            ),
        ] {
            assert!(composer(&screen, style).is_none(), "{screen}");
        }
    }

    /// A one-row footer cut inside a directory from the home directory is read on any host, and
    /// one cut inside a Windows directory where the host's paths are Windows ones, from wherever
    /// the cut leaves the directory's beginning.
    #[test]
    fn codex_s_one_row_footer_is_read_cut_inside_a_home_or_windows_directory() {
        for directory in ["~…", "~/co…"] {
            let screen = format!("› draft\n\n  GPT-6.1-Sol default · {directory}\n");
            for style in [PathStyle::Unix, PathStyle::Windows] {
                assert!(composer(&screen, style).is_some(), "{screen}");
            }
        }
        for directory in ["C:…", "C:\\Us…", "~\\co…", "\\…", "\\\\h…"] {
            let screen = format!("› draft\n\n  GPT-6.1-Sol default · {directory}\n");
            assert!(composer(&screen, PathStyle::Windows).is_some(), "{screen}");
            assert!(composer(&screen, PathStyle::Unix).is_none(), "{screen}");
        }
        for directory in ["project…", "C\\…", "~project…"] {
            let screen = format!("› draft\n\n  GPT-6.1-Sol default · {directory}\n");
            assert!(composer(&screen, PathStyle::Windows).is_none(), "{screen}");
        }
    }

    /// A draft whose first row is empty and whose last rows look like a footer, above a footer
    /// Codex left blank, is not read as an empty box: the first of two rows is only ever a status
    /// line, a model alone names a reasoning effort, and a hint clipped to fewer than three
    /// characters is no hint.
    #[test]
    fn codex_s_draft_that_looks_like_a_footer_is_not_an_empty_box() {
        for tail in [
            "  50% context left\n  ? for shortcuts",
            "  tab to queue message\n  esc",
            "  esc again to edit previous message\n  ? for shortcuts",
            "  hello world\n  ? for shortcuts",
            "  hello world\n  ?",
            "  hello world\n  ta",
        ] {
            let screen = format!("›\n\n{tail}\n\n\n\n");
            for style in [PathStyle::Unix, PathStyle::Windows] {
                assert_ne!(
                    composer(&screen, style).map(|rows| rows.join("\n")),
                    Some(String::new()),
                    "{screen}"
                );
            }
        }
    }
}
