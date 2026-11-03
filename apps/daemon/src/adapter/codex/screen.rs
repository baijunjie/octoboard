//! How Codex's screens read: its dialogs, its composer above its status footer, its folder-trust
//! confirmation, its terminal title, and its interrupt key. The measurements are in
//! `docs/agent-cli-reference.md`, section "## Codex".

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
        composer(screen)
    }

    fn as_submitted<'a>(&self, typed: &'a str) -> &'a str {
        typed.trim()
    }

    fn trust_screen(&self) -> &'static Screen {
        &TRUST
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
};

/// A message written into a Codex turn is steered into that turn, which the cancel throws away
/// with it, unless the turn had acted on it already.
fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn. Codex \
         steers such messages into the turn the cancel threw away, so unless the turn had already \
         acted on them they were not acted on: resend whatever still matters."
    )
}

/// Codex has no composer border. Its last visible row is a measured status footer, separated
/// from the composer by one blank row. Unknown footer shapes hold delivery instead of guessing
/// where a draft ends; empty lines inside the composer remain text.
fn composer(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let footer = rows.iter().rposition(|row| !row.trim().is_empty())?;
    if !is_footer(rows[footer].trim()) || footer < 2 || !rows[footer - 1].trim().is_empty() {
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

fn is_footer(row: &str) -> bool {
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
                        .all(crate::terminal::title::is_braille_spinner_glyph) =>
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
