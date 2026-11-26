//! How Grok Build's screens read: its dialogs, its composer drawn as a box of its own and its draft
//! stash, its folder-trust confirmation, its terminal title, and its interrupt key. The
//! measurements are in `docs/agent-cli-reference.md`, section "Grok Build".

use crate::adapter::{LaunchPlan, LaunchSpec};
use crate::session::dialogs::{DialogKind, DialogPattern, WrittenMessage};
use crate::session::input_box::box_rows;
use crate::session::screen::{Interrupt, InterruptKey, ScreenReader, TitleCancel};
use crate::session::title_activity::{spinner_activity, Shown};
use crate::session::trust::{Press, Screen, StartSignal, HOOK_FALLBACK};

pub(super) struct GrokScreen;

/// U+200B ZERO WIDTH SPACE, what keeps a pasted message's leading slash as text
/// ([`ScreenReader::leading_slash_guard`]).
const SLASH_GUARD: &str = "\u{200B}";

impl ScreenReader for GrokScreen {
    fn dialogs(&self) -> &'static [DialogPattern] {
        DIALOGS
    }

    /// Grok's boxed composer is read wherever it sits at its prompt (`input_box_rows`).
    fn reads_input_box(&self) -> bool {
        true
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        composer(screen)
    }

    /// A zero-width space: Grok Build 1.0.89 runs a slash command pasted after a leading space
    /// (or a line break, or a no-break space) as it does one with nothing before it, and takes the
    /// zero-width space as text, reporting it in the prompt (`testdata/grok-slash-guard.md`).
    fn leading_slash_guard(&self) -> &'static str {
        SLASH_GUARD
    }

    /// The `[Image #N]` Grok Build 1.0.89 draws in its composer for each image path pasted on its
    /// own (`testdata/grok-interaction-captures.md`).
    fn image_placeholders(&self, text: &str) -> usize {
        crate::session::screen::count_image_placeholders(text)
    }

    fn without_image_placeholders(&self, text: &str) -> String {
        crate::session::screen::strip_image_placeholders(text)
    }

    /// The highlighted row of the completion Grok Build 1.0.89 lists between two rules above its
    /// composer once a `/word` is written (`❯ /session-info Show session info`), which Enter
    /// runs or accepts in place of what the composer holds: pasted `/sess`, it ran
    /// `/session-info`, while `/probe` was only completed into `/probecmd ` (a command that takes
    /// arguments) and not sent; why they differ is not measured
    /// (`testdata/grok-interaction-captures.md`).
    fn command_completion(&self, screen: &str) -> Option<String> {
        let rows: Vec<&str> = screen.lines().map(str::trim).collect();
        let top = composer_top(&rows)?;
        let popup_end = top.checked_sub(1).filter(|end| is_rule(rows[*end]))?;
        rows[..popup_end]
            .iter()
            .rev()
            .take_while(|row| !is_rule(row))
            .find_map(|row| {
                let name = row
                    .strip_prefix(PROMPT_MARK)?
                    .trim_start()
                    .strip_prefix('/')?;
                let name = name.split_whitespace().next()?;
                Some(name.to_string())
            })
    }

    /// Every launch: Grok Build 1.0.70 stashes with Ctrl-S and exposes no user-remappable binding
    /// for it, and no release is known to lack it, so no version switches it off. Where Ctrl-S
    /// draws no `Stashed` (`stash_shown`), the swap stops using the stash (`crate::session::swap`).
    fn native_stash(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> bool {
        true
    }

    fn stash_shown(&self, screen: &str) -> bool {
        screen.lines().any(|line| {
            let line = line.trim();
            line.starts_with('╭') && line.ends_with('╮') && line.contains("Stashed")
        })
    }

    /// Measured on 1.0.70 in `docs/agent-cli-reference.md`, "Keys that interrupt a turn" under
    /// "Grok Build": the interrupt clears a draft with it mid-turn, before its Ctrl-C.
    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    fn trust_screen(&self) -> &'static Screen {
        &TRUST
    }

    fn title_activity(&self, glyph: Option<char>) -> Option<Shown> {
        // A title with no spinner is Grok's own name.
        spinner_activity(glyph)
    }

    fn title_cancel(&self) -> Option<TitleCancel> {
        Some(TitleCancel::CloseAsCancelled)
    }

    fn interrupt(&self) -> Option<&'static Interrupt> {
        Some(&INTERRUPT)
    }
}

/// Grok Build ignores Esc and stops on Ctrl-C, which with text in its composer clears the text
/// instead; Ctrl-C at an idle Grok asks to be pressed again to quit.
const INTERRUPT: Interrupt = Interrupt {
    key: InterruptKey::CtrlCAfterClearing {
        undo: Some("ctrl+z"),
    },
    restores_prompt: true,
    written_during_turn,
    queued_waiting: None,
};

/// Grok Build keeps a message written into a turn queued until the turn is over.
fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn; Grok \
         Build keeps them queued and runs them next, as a new turn. Check it with `get_session` \
         before sending the next step."
    )
}

/// The prompt row's mark in Grok Build's composer.
pub(super) const PROMPT_MARK: char = '\u{276F}';

/// The name of the dialog Grok Build raises when its plan's usage limit is reached.
const GROK_USAGE_LIMIT: &str = "grok_usage_limit";

/// The rows of Grok Build's composer: those of the last box drawn with `╭─╮` and `╰─╯`, each
/// between its `│` borders, the first starting with `❯`.
pub(super) fn composer(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim).collect();
    let top = composer_top(&rows)?;
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
    Some(box_rows(inside.into_iter(), PROMPT_MARK))
}

/// The row of the top border of the last box drawn as Grok Build's composer is.
fn composer_top(rows: &[&str]) -> Option<usize> {
    rows.iter().rposition(|row| {
        row.strip_prefix('\u{256D}')
            .and_then(|row| row.strip_suffix('\u{256E}'))
            .is_some_and(|rule| {
                let rule = rule.replace("Stashed", "");
                rule.chars().filter(|c| *c == '\u{2500}').count() >= 20
                    && rule.chars().all(|c| c == '\u{2500}' || c.is_whitespace())
            })
    })
}

/// A full-width horizontal rule, which may end in the count of the rows between two (`──1─`).
fn is_rule(row: &str) -> bool {
    row.chars().filter(|c| *c == '\u{2500}').count() >= 20
        && row
            .chars()
            .all(|c| c == '\u{2500}' || c.is_ascii_digit() || c.is_whitespace())
}

/// Grok Build 1.0.50: the question, then "Yes, proceed y" and "No, quit n", with no cursor.
pub(crate) const TRUST: Screen = Screen {
    first_draw: &[&[
        "Doyoutrustthecontentsofthisdirectory?",
        "Yes,proceedyNo,quitn",
    ]],
    accept: Some("Yes,proceedy"),
    others: &["No,quitn"],
    cursor: None,
    press: Press::Yes,
    next_screen: None,
    input_ends_wait: true,
    start_signal: StartSignal::Hook,
    // Its `SessionStart` normally comes within a second, but a managed policy that allows only
    // managed hooks (`allow_managed_hooks_only`) runs none of Octoboard's, and the session would
    // hold its messages for good.
    hook_fallback: Some(HOOK_FALLBACK),
    waits_for_paste_mode: false,
};

const DIALOGS: &[DialogPattern] = &[DialogPattern::all_of(
    GROK_USAGE_LIMIT,
    WrittenMessage::Discarded,
    &["Upgradetier", "Buymorecredits", "Enter:submit"],
)
.of_kind(DialogKind::UsageLimit)];

#[cfg(test)]
mod tests {
    use crate::session::screen::ScreenReader;
    use crate::terminal::snapshot::TerminalSnapshot;

    /// The reader declares that it reads its composer wherever the agent is at its prompt, so a turn
    /// opened off the title closes only while the box is drawn (`crate::session::title_status`).
    #[test]
    fn the_input_box_is_read_wherever_the_agent_is_at_its_prompt() {
        assert!(super::GrokScreen.reads_input_box());
    }

    /// What a 120x40 terminal shows after `bytes`, a capture of Grok's output from its launch.
    fn replayed(bytes: &[u8]) -> String {
        let mut screen = TerminalSnapshot::new(40, 120);
        screen.process(bytes);
        screen.visible_text_undimmed()
    }

    /// Grok Build offers a command for a slash pasted after a space, which the Enter then runs;
    /// after the zero-width space it guards with, the slash stays text, with no command offered
    /// (`testdata/grok-slash-guard.md`).
    #[test]
    fn grok_offers_a_command_after_a_space_but_not_after_its_guard() {
        let offered = |screen: &str| screen.contains("/session-info Show session info");
        let space = replayed(include_bytes!(
            "../../../testdata/grok_slash_space_guarded.bin"
        ));
        assert!(offered(&space), "{space}");
        let guarded = replayed(include_bytes!(
            "../../../testdata/grok_slash_zwsp_guarded.bin"
        ));
        assert!(!offered(&guarded), "{guarded}");
        let composer = super::GrokScreen
            .input_box_rows(&guarded)
            .expect("the composer is read");
        assert_eq!(composer.join("\n").replace('\u{200B}', ""), "/session-info");
    }

    macro_rules! capture {
        ($name:literal) => {
            replayed(include_bytes!(concat!(
                "../../../testdata/grok_interaction_",
                $name,
                ".bin"
            )))
        };
    }

    #[test]
    fn image_placeholders_are_counted_in_the_composer() {
        let screen = super::GrokScreen;
        assert_eq!(screen.image_placeholders("[Image #1] [Image #2]"), 2);
        assert_eq!(screen.image_placeholders("look [Image #12] there"), 1);
        assert_eq!(screen.image_placeholders("[Image #] [Image 1] Image #1"), 0);
        let two = capture!("two_images");
        let rows = screen.input_box_rows(&two).expect("the composer");
        assert_eq!(screen.image_placeholders(&rows.join("\n")), 2);
    }

    /// The row Grok highlights in its completion list, which may be a longer name than the one
    /// written (`/probe` offers `/probecmd`, which Enter then only completes).
    #[test]
    fn the_completion_offered_above_the_composer_is_reported_by_name() {
        let screen = super::GrokScreen;
        let space = replayed(include_bytes!(
            "../../../testdata/grok_slash_space_guarded.bin"
        ));
        assert_eq!(
            screen.command_completion(&space).as_deref(),
            Some("session-info")
        );
        assert_eq!(
            screen
                .command_completion(&capture!("slash_partial"))
                .as_deref(),
            Some("probecmd")
        );
        // Nothing offered after the guard, once accepted, or with no slash.
        let guarded = replayed(include_bytes!(
            "../../../testdata/grok_slash_zwsp_guarded.bin"
        ));
        assert_eq!(screen.command_completion(&guarded), None);
        assert_eq!(
            screen.command_completion(&capture!("slash_completed")),
            None
        );
        assert_eq!(screen.command_completion(&capture!("idle")), None);
    }
}
