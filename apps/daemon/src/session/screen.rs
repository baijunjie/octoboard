//! How one provider's terminal screens read: the [`ScreenReader`] every provider gives through
//! `crate::adapter::AgentProvider::screen`. The session code around it (`dialogs`, `input_box`,
//! `draft`, `swap`, `trust`, `title_activity`, `title_interrupt`, `interrupt`) owns what is done
//! with a reading; the reader owns only what the provider's screen, input box and terminal title
//! say. Every answer has a default that recognises nothing, so a provider implements only what has
//! been measured of its screens.

use super::dialogs::DialogPattern;
use super::title_activity::Shown;
use super::trust::Screen as TrustScreen;
use crate::adapter::{LaunchPlan, LaunchSpec};

/// How one provider's terminal screens, input box and terminal title read.
pub trait ScreenReader: Send + Sync {
    /// The dialogs only a person can answer that the provider raises and no hook announces
    /// (`crate::session::dialogs`).
    fn dialogs(&self) -> &'static [DialogPattern] {
        &[]
    }

    /// The rows of the provider's input box as `screen` (the visible rows, faint cells blanked)
    /// shows them, each trimmed and the prompt mark off the first, empty rows kept; `None` where no
    /// input box is recognised (`crate::session::input_box`).
    fn input_box_rows(&self, _screen: &str) -> Option<Vec<String>> {
        None
    }

    /// A draft typed into the input box as the provider reports it once submitted, for telling a
    /// prompt the provider reports sending apart from one sent before the draft
    /// (`crate::session::draft`).
    fn as_submitted<'a>(&self, typed: &'a str) -> &'a str {
        typed
    }

    /// Whether a pasted piece that ends in an image file's extension is read as an image, and the
    /// Enter after it lost, so a message's image paths have to be guarded
    /// (`crate::session::term::image_path`).
    fn pasted_image_path_loses_enter(&self) -> bool {
        false
    }

    /// Whether this launch may keep a draft in the provider's own stash while a message uses the
    /// input box: only on a measured version with its measured stash key intact
    /// (`crate::session::swap`).
    fn native_stash(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> bool {
        false
    }

    /// Whether `screen` (the visible rows) shows the provider's draft stash holding a draft.
    fn stash_shown(&self, _screen: &str) -> bool {
        false
    }

    /// The provider's folder-trust confirmation and how it is pressed (`crate::session::trust`).
    /// The default is a screen that is never sighted, its messages held for a hook up to the hook
    /// fallback.
    fn trust_screen(&self) -> &'static TrustScreen {
        &super::trust::NO_SCREEN
    }

    /// What a completed terminal title that started with `glyph` (one of the activity glyphs
    /// `crate::terminal::title` strips, or none) says about the provider's turn, or `None` when the
    /// title says nothing (`crate::session::title_activity`).
    fn title_activity(&self, _glyph: Option<char>) -> Option<Shown> {
        None
    }

    /// Whether a terminal title says the provider is waiting on the user for something no hook
    /// announces.
    fn title_needs_user(&self, _title: &str) -> bool {
        false
    }

    /// The title text the provider shows when only the user can act, as `title_needs_user`
    /// recognises it, for the copy that tells a caller what the session is waiting on.
    fn user_action_title(&self) -> Option<&'static str> {
        None
    }

    /// How a turn whose title went back to the prompt with no hook is closed, for a provider that
    /// leaves some cancels without any other trace (`crate::session::title_interrupt`); `None`
    /// where the title is not watched for one.
    fn title_cancel(&self) -> Option<TitleCancel> {
        None
    }

    /// How a turn is interrupted from outside (`crate::session::interrupt`); `None` where no key is
    /// known to interrupt one safely.
    fn interrupt(&self) -> Option<&'static Interrupt> {
        None
    }
}

/// The reader of a provider nothing has been measured of.
pub struct NoScreen;

impl ScreenReader for NoScreen {}

/// What a turn that went back to the prompt in the terminal title with no hook is closed as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleCancel {
    /// The session is put back at its prompt, paused, and the turn noted as ended, as the
    /// provider's record watch does for a cancel it finds.
    BackAtPrompt,
    /// The session is put back at its prompt and its turn closed as cancelled, so the hook
    /// backstop that may follow finds no open turn to report on.
    CloseAsCancelled,
}

/// How a provider's turn is interrupted.
#[derive(Debug, Clone, Copy)]
pub struct Interrupt {
    pub key: InterruptKey,
    /// Whether the provider puts the interrupted prompt back in its input box when the key lands
    /// before the model has shown anything, for the interrupt to clear it. Its input box is read
    /// before the key only then: treating an unread screen as permission to clear could erase a
    /// user's pending edit.
    pub restores_prompt: bool,
    /// What the caller is told became of the `written` messages Octoboard wrote into the session
    /// during the turn the key stopped, as a sentence to follow the interrupt's own.
    pub written_during_turn: fn(written: usize) -> String,
}

/// The keys that interrupt a provider's turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptKey {
    /// One Esc.
    Esc,
    /// Ctrl-C, which with text in the input box clears the text instead, so the box is emptied
    /// with Ctrl-U first.
    CtrlCAfterClearing {
        /// The key that may undo the clearing, told to the caller where a draft was cleared.
        undo: Option<&'static str>,
    },
}

#[cfg(test)]
mod tests {
    use crate::adapter::{provider, tests::spec_fixture};
    use crate::protocol::AgentId;
    use crate::session::trust::{StartSignal, HOOK_FALLBACK};

    /// A provider with no reader of its own is read as the default recognises: no dialog, no input
    /// box, no title activity, no draft stash and no interrupt key, all through the registry.
    #[test]
    fn a_provider_with_no_screen_reader_recognises_nothing() {
        let screen = provider(AgentId::TEST).screen();
        let text = "Do you want to proceed?\n1. Yes\n2. No\n❯ a draft\n";
        assert!(screen.dialogs().is_empty());
        assert!(crate::session::dialogs::recognise(AgentId::TEST, text).is_none());
        assert!(screen.input_box_rows(text).is_none());
        assert!(crate::session::input_box::input_box(AgentId::TEST, text).is_none());
        assert_eq!(screen.as_submitted("typed"), "typed");
        assert!(!screen.pasted_image_path_loses_enter());
        assert!(!screen.stash_shown(text));
        let fixture = spec_fixture();
        let spec = fixture.spec();
        let plan = provider(AgentId::TEST).plan(&spec).expect("a plan");
        assert!(!screen.native_stash(&spec, &plan));
        for glyph in [None, Some('\u{2733}'), Some('\u{280B}')] {
            assert!(screen.title_activity(glyph).is_none());
            assert!(crate::session::title_activity::shown(AgentId::TEST, glyph, false).is_none());
        }
        assert!(!screen.title_needs_user("anything"));
        assert!(screen.user_action_title().is_none());
        assert!(screen.title_cancel().is_none());
        assert!(screen.interrupt().is_none());
    }

    /// The default trust screen is never sighted and holds messages for a hook up to the hook
    /// fallback, as no other provider's measured screen does without the fallback.
    #[test]
    fn the_default_trust_screen_is_never_sighted_and_waits_for_a_hook_up_to_the_fallback() {
        let trust = provider(AgentId::TEST).screen().trust_screen();
        assert!(trust.first_draw.is_empty());
        assert!(matches!(trust.start_signal, StartSignal::Hook));
        assert_eq!(trust.hook_fallback, Some(HOOK_FALLBACK));
        assert!(!trust.waits_for_paste_mode);
    }
}
