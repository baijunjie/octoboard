//! How Grok Build's screens read: its dialogs, its composer drawn as a box of its own and its
//! draft stash, its folder-trust confirmation, its terminal title, and its interrupt key. The
//! measurements are in `docs/agent-cli-reference.md`, section "## Grok Build".

use crate::adapter::{AgentVersion, LaunchPlan, LaunchSpec};
use crate::session::dialogs::{DialogKind, DialogPattern, WrittenMessage};
use crate::session::input_box::box_rows;
use crate::session::screen::{Interrupt, InterruptKey, ScreenReader, TitleCancel};
use crate::session::title_activity::{spinner_activity, Shown};
use crate::session::trust::{Press, Screen, StartSignal, HOOK_FALLBACK};

pub(super) struct GrokScreen;

impl ScreenReader for GrokScreen {
    fn dialogs(&self) -> &'static [DialogPattern] {
        DIALOGS
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        composer(screen)
    }

    fn native_stash(&self, spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> bool {
        // 1.0.70 exposes no user-remappable stash binding, so the version is all there is to
        // check.
        spec.agent_version
            .is_some_and(|version| version >= STASH_MINIMUM)
    }

    fn stash_shown(&self, screen: &str) -> bool {
        screen.lines().any(|line| {
            let line = line.trim();
            line.starts_with('╭') && line.ends_with('╮') && line.contains("Stashed")
        })
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

/// The first version measured to stash with Ctrl-S.
const STASH_MINIMUM: AgentVersion = (1, 0, 70);

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
const PROMPT_MARK: char = '\u{276F}';

/// The name of the dialog Grok Build raises when its plan's usage limit is reached. It is the only
/// provider that raises one when a turn stops on the limit.
const GROK_USAGE_LIMIT: &str = "grok_usage_limit";

/// The rows of Grok Build's composer: those of the last box drawn with `╭─╮` and `╰─╯`, each
/// between its `│` borders, the first starting with `❯`.
fn composer(screen: &str) -> Option<Vec<String>> {
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
    Some(box_rows(inside.into_iter(), PROMPT_MARK))
}

/// Grok Build 1.0.50: the question, then "Yes, proceed y" and "No, quit n", with no cursor.
pub(crate) const TRUST: Screen = Screen {
    first_draw: &[&[
        "Doyoutrustthecontentsofthisdirectory?",
        "Yes,proceedyNo,quitn",
    ]],
    accept: Some("Yes,proceedy"),
    decline: &["No,quitn"],
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
