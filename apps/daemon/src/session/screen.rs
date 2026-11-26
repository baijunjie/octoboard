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
use crate::platform::term::Input;

/// How one provider's terminal screens, input box and terminal title read.
pub trait ScreenReader: Send + Sync {
    /// The dialogs only a person can answer that the provider raises and no hook announces
    /// (`crate::session::dialogs`).
    fn dialogs(&self) -> &'static [DialogPattern] {
        &[]
    }

    /// The rows of the provider's input box as `screen` (the visible rows, as
    /// [`Self::input_box_screen`] asks for them) shows them, each trimmed and the prompt mark off
    /// the first, empty rows kept; `None` where no input box is recognised
    /// (`crate::session::input_box`).
    fn input_box_rows(&self, _screen: &str) -> Option<Vec<String>> {
        None
    }

    /// Which screen [`Self::input_box_rows`] is given wherever the box is read for what it holds:
    /// the one with faint cells blanked, on which a hint the provider draws faint in an empty box
    /// is no text, unless the provider's box can only be found by something it draws faint, such
    /// as the rules that frame it. A reader given the screen as drawn tells the box's own text
    /// from a faint hint by itself, or names the hint in [`Self::input_placeholder`], which is
    /// then applied on every host.
    fn input_box_screen(&self) -> ScreenText {
        ScreenText::FaintBlanked
    }

    /// Whether `row`, the first row of the input box as [`Self::input_box_rows`] gives it and the
    /// only one with text, is a placeholder the provider shows in an empty box, read as such where
    /// the attribute it is drawn with may be lost (`crate::session::input_box`).
    fn input_placeholder(&self, _row: &str) -> bool {
        false
    }

    /// Whether `reported`, a prompt the provider reports sending, is `typed`, a prompt recorded
    /// as submitted (a draft typed into the input box, a message written, the prompt of the
    /// launch), for telling a prompt the provider reports sending apart from one sent before it
    /// (`crate::session::draft`). The same text, unless the provider changes a prompt between
    /// taking it and reporting it; then what it changes is ignored on both sides.
    fn same_as_submitted(&self, typed: &str, reported: &str) -> bool {
        typed == reported
    }

    /// Whether a pasted piece that ends in an image file's extension is read as an image, and the
    /// Enter after it lost, so a message's image paths have to be guarded
    /// (`crate::session::term::image_path`).
    fn pasted_image_path_loses_enter(&self) -> bool {
        false
    }

    /// How many images `text`, the whole of what the input box shows, holds as attachments: the
    /// placeholders the provider draws for each image path pasted on its own (`send_message`'s
    /// `images`).
    fn image_placeholders(&self, _text: &str) -> usize {
        0
    }

    /// `text`, the whole of what the input box shows, without the placeholders
    /// [`Self::image_placeholders`] counts, for reading what else the box holds beside them (a
    /// long paste's own placeholder, [`Self::collapsed_paste`]).
    fn without_image_placeholders(&self, text: &str) -> String {
        text.to_string()
    }

    /// The command the provider's slash-command completion offers on `screen` (the visible rows),
    /// by its name without the slash, which an Enter would run in place of what the box holds;
    /// `None` where no completion is up (`send_command`).
    fn command_completion(&self, _screen: &str) -> Option<String> {
        None
    }

    /// Whether a list the provider draws by its input box (a slash-command completion, a menu) is
    /// up on `screen` (the visible rows), or `None` where the reader cannot tell. A provider that
    /// declares [`Self::writes_lone_command_with_trailing_space`] answers it: the space after a
    /// lone command is typed only over `Some(true)`, and the Enter after the space waits for
    /// `Some(false)`; neither is pressed without it (`crate::session::composer`).
    fn list_shown(&self, _screen: &str) -> Option<bool> {
        None
    }

    /// Whether the slash-command list up on `screen` (the visible rows) says that no command
    /// matches what the box holds, as a list that offers no completion at all: a lone command
    /// written then is one the agent does not have, refused as not recognised with the box
    /// cleared rather than run or sent to the model (`crate::session::composer`). False by
    /// default, for a provider whose list offers a command or is not drawn.
    fn list_offers_no_command(&self, _screen: &str) -> bool {
        false
    }

    /// Whether a lone command (`/name`, with no arguments) gets a space typed after it before its
    /// Enter: where the provider's first Enter on a lone `/name` takes its completion into the box
    /// rather than running it, and `/name ` runs on one Enter, as measured. The command is pasted
    /// and its completion looked at as for any provider; the space follows as a key of its own,
    /// and the Enter waits for the list to be read closed ([`Self::list_shown`]), which such a
    /// provider answers (`crate::session::composer`).
    fn writes_lone_command_with_trailing_space(&self) -> bool {
        false
    }

    /// What goes before a message's leading slash in a paste so that the provider reads it as text
    /// rather than run it as a slash command (`crate::session::term::frame_message`): a space,
    /// which is what Claude Code and Codex are measured to need, unless the provider says
    /// otherwise.
    fn leading_slash_guard(&self) -> &'static str {
        " "
    }

    /// Whether the provider, handed a paste as typed keys (`TermFacts::paste_as_keys`), tells it
    /// from typing by how fast the keys come and reads an Enter that arrives within that burst as
    /// a line break in the text, so a message's submit key is written only once the paste has been
    /// taken (`crate::session::paste_apart`).
    fn joins_enter_to_key_paste(&self) -> bool {
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

    /// The row of `screen` where the provider shows its stash indicator, for the log when a stash
    /// key emptied the box but [`Self::stash_shown`] did not read a stash. Only a row that holds
    /// the agent's own indicators, never the user's text; `None` where there is no such row.
    fn stash_indicator_row(&self, _screen: &str) -> Option<String> {
        None
    }

    /// Whether `text`, the whole of what the input box shows, is the placeholder the provider shows
    /// for a long paste instead of its text, for telling that a box holds nothing but a message
    /// written there (`crate::session::LiveSession::retry_unsent_submit`).
    fn collapsed_paste(&self, _text: &str) -> bool {
        false
    }

    /// Whether Ctrl-U is measured to delete a line of the provider's input box a press (the line
    /// break before the cursor at the start of a line) and to do nothing else, so that a draft can
    /// be cleared out of the way of a message past the draft's grace
    /// (`crate::session::draft_clear`): the key is pressed again, each press seen on the screen
    /// before the next, until the box reads empty (`crate::session::input_box_clear`), so a box
    /// of several rows needs no key that empties it at once. Where it is not, such a message
    /// waits for the box to be emptied by the user, for a bounded time, and then goes in as the
    /// box stands.
    fn clears_input_box_with_ctrl_u(&self) -> bool {
        false
    }

    /// A key measured to empty the provider's whole input box at once, however many rows it
    /// holds, at its idle prompt, and to do nothing else there while the box holds text, or `None`
    /// where there is none. It is pressed once a clear, only while the session reads at its idle
    /// prompt ([`crate::session::turn_reading::TurnReading::at_idle_prompt`]) and the box holds
    /// text, since such a key may mean something else mid-turn or at an empty box (Copilot's
    /// Ctrl-C interrupts a turn, and at an empty prompt arms its exit); a clear mid-turn falls
    /// back to [`Self::clears_input_box_with_ctrl_u`] where that is declared
    /// (`crate::session::input_box_clear`).
    fn input_box_clearing_key(&self) -> Option<Input> {
        None
    }

    /// Whether a cancel puts the messages the provider still held for the end of the turn
    /// (written into its input box while the turn ran, and taken out of it) back into the box,
    /// unsent. A box that then begins with messages Octoboard wrote while the turn ran, the
    /// latest alone or several in the order written, holds messages the provider gave back
    /// rather than ones left unsent: it is cleared out of the way of the next message as a draft
    /// is, the messages are queued to be written again, and only text the user typed after them
    /// is told as a draft (`crate::session::draft_clear`).
    fn cancel_restores_held_messages(&self) -> bool {
        false
    }

    /// Whether a cancel can drop the messages the provider still held for the end of the turn
    /// (written into its input box while the turn ran, and taken out of it), with nothing put
    /// back in the box. Where it can, the messages Octoboard wrote into a turn the provider then
    /// reports cancelled are queued to be written again after the next message
    /// (`crate::messaging::cleared_messages`), and one that cannot be is told to the user. A
    /// provider can answer otherwise for one launch
    /// (`crate::adapter::AgentProvider::launch_cancel_drops_held_messages`): such a launch has
    /// none written again, and each is told to the user as one that may not have been carried
    /// out.
    ///
    /// Declared only by a provider that reports a held message it takes into the turn as a
    /// prompt sent, with its text: that is all that tells a message the cancel dropped from one
    /// the turn had taken before it, and without it a message taken would be written twice. It
    /// may be declared beside [`Self::cancel_restores_held_messages`], by a provider one of whose
    /// cancel keys gives the messages back and another drops them: what is then found in the box
    /// is cleared as what is left of messages already queued again.
    fn cancel_drops_held_messages(&self) -> bool {
        false
    }

    /// Whether an input box that reads empty off a whole frame, after a write the agent did not
    /// report sending, is measured to mean the message went elsewhere: into a dialog the reader
    /// does not recognise, drawn over the whole empty box. Where it is, the message is told to its
    /// sender and the user as not delivered and possibly lost, nothing holds the queue for it, and
    /// no submit key is retried into the empty box
    /// (`crate::session::LiveSession::write_went_elsewhere`).
    /// Where it is not, an empty box tells nothing (the agent may have sent the message and be
    /// late reporting it), and the message is left unsent as any other.
    fn empty_box_means_write_went_elsewhere(&self) -> bool {
        false
    }

    /// The provider's folder-trust confirmation and how it is pressed (`crate::session::trust`).
    /// The default is a screen that is never sighted, its messages held for a hook up to the hook
    /// fallback.
    fn trust_screen(&self) -> &'static TrustScreen {
        &super::trust::NO_SCREEN
    }

    /// The heading and footer that enclose the options on the rendered trust screen, when the
    /// provider also draws the cursor glyph elsewhere during the confirmation.
    fn trust_cursor_bounds(&self) -> Option<(&'static str, &'static str)> {
        None
    }

    /// What a completed terminal title that started with `glyph` (one of the activity glyphs
    /// `crate::terminal::title` strips, or none) says about the provider's turn, or `None` when the
    /// title says nothing (`crate::session::title_activity`).
    fn title_activity(&self, _glyph: Option<char>) -> Option<Shown> {
        None
    }

    /// Whether [`Self::input_box_rows`] recognises the provider's input box wherever the agent is
    /// at its prompt, so that a screen with no box on it is one where the agent is not: then a
    /// turn opened off the title closes only while the box is drawn
    /// (`crate::session::title_status`). By default no reader is relied on for that, and the
    /// title's settle, the dialogs and the interaction reader decide alone.
    fn reads_input_box(&self) -> bool {
        false
    }

    /// Whether a new prompt hook together with a newly stopped title proves that the
    /// interrupted turn ended. Unmeasured providers make no such inference.
    fn prompt_after_title_stop_ends_turn(&self) -> bool {
        false
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
    /// where the title is not watched for one. The title is not read while the provider's hooks
    /// report a tool call open, so a provider that opts in must report a cancel made while a tool
    /// runs some other way, by a hook or a record watch.
    fn title_cancel(&self) -> Option<TitleCancel> {
        None
    }

    /// How a turn is interrupted from outside (`crate::session::interrupt`); `None` where no key is
    /// known to interrupt one safely.
    fn interrupt(&self) -> Option<&'static Interrupt> {
        None
    }

    /// A draw the provider makes only during a turn, for terminals without a running title or
    /// status report. The interrupt uses it alongside hook status and the dialog check.
    fn interrupt_running_screen(&self) -> Option<fn(&str) -> bool> {
        None
    }
}

/// The reader of a provider nothing has been measured of.
pub struct NoScreen;

impl ScreenReader for NoScreen {}

/// Which text of the screen a reading is made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenText {
    /// The visible rows with every faint cell shown as a space
    /// (`crate::terminal::snapshot::TerminalSnapshot::visible_text_undimmed`).
    FaintBlanked,
    /// The visible rows as drawn, faint cells included
    /// (`crate::terminal::snapshot::TerminalSnapshot::visible_text`).
    AsDrawn,
}

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
    /// Whether the provider puts text back in its input box at a cancel, for the interrupt to
    /// clear it: the interrupted prompt, when the key lands before the model has shown anything,
    /// or the messages it still held ([`ScreenReader::cancel_restores_held_messages`]). Its input
    /// box is read before the key only then: treating an unread screen as permission to clear
    /// could erase a user's pending edit.
    pub restores_prompt: bool,
    /// What the caller is told became of the `written` messages Octoboard wrote into the session's
    /// terminal during the turn the key stopped, as a sentence to follow the interrupt's own.
    pub written_during_turn: fn(written: usize) -> String,
    /// For a provider whose own server queues Octoboard's messages
    /// (`crate::session::term::MessageTransport`): what the caller is told of Octoboard's messages
    /// that waited in that queue past the cancel, by what starting the queue came to, as a sentence
    /// to follow the interrupt's own; `None` where there is nothing to say.
    pub queued_waiting: Option<fn(&crate::session::term::QueueStart) -> Option<String>>,
}

/// The keys that interrupt a provider's turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptKey {
    /// One Esc.
    Esc,
    /// Esc pressed twice, for a provider whose first Esc only arms the interrupt and whose second
    /// makes it. The second goes in a tenth of a second after the first, and only while the turn
    /// still runs.
    EscTwice,
    /// One Ctrl-C, for a provider whose Ctrl-C stops a running turn whatever its input box holds,
    /// and leaves a draft there as it was: nothing is cleared first.
    CtrlC,
    /// Ctrl-C, which with text in the input box clears the text instead, so the box is emptied
    /// with Ctrl-U first.
    CtrlCAfterClearing {
        /// The key that may undo the clearing, told to the caller where a draft was cleared.
        undo: Option<&'static str>,
    },
}

/// The byte ranges of the `[Image #<n>]` placeholders in `text`, for a provider that draws an
/// image path pasted on its own in that form ([`ScreenReader::image_placeholders`]).
fn image_placeholder_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    text.match_indices("[Image #")
        .filter_map(|(at, marker)| {
            let from = at + marker.len();
            let digits = text[from..].bytes().take_while(u8::is_ascii_digit).count();
            (digits > 0 && text[from + digits..].starts_with(']')).then_some(at..from + digits + 1)
        })
        .collect()
}

/// How many `[Image #<n>]` placeholders `text` holds, for a provider's
/// [`ScreenReader::image_placeholders`] to answer with.
pub(crate) fn count_image_placeholders(text: &str) -> usize {
    image_placeholder_spans(text).len()
}

/// `text` without its `[Image #<n>]` placeholders.
pub(crate) fn strip_image_placeholders(text: &str) -> String {
    let mut rest = String::new();
    let mut from = 0;
    for span in image_placeholder_spans(text) {
        rest.push_str(&text[from..span.start]);
        from = span.end;
    }
    rest.push_str(&text[from..]);
    rest
}

/// Whether the provider of `reader` has a way to clear its input box: Ctrl-U a line a press
/// ([`ScreenReader::clears_input_box_with_ctrl_u`]), or a key that empties it at once at the idle
/// prompt ([`ScreenReader::input_box_clearing_key`]).
pub(crate) fn clears_input_box(reader: &dyn ScreenReader) -> bool {
    reader.clears_input_box_with_ctrl_u() || reader.input_box_clearing_key().is_some()
}

#[cfg(test)]
mod tests {
    use crate::adapter::{provider, tests::spec_fixture};
    use crate::protocol::AgentId;
    use crate::session::trust::{StartSignal, HOOK_FALLBACK};

    /// A reader that overrides nothing, as the trait's defaults.
    struct Bare;
    impl super::ScreenReader for Bare {}

    /// A provider with no reader of its own is read as the default recognises: no dialog, no input
    /// box, no title activity, no draft stash and no interrupt key. The test provider reads a
    /// made-up input box (`adapter::test_provider::TestScreen`), so those are the defaults' own.
    #[test]
    fn a_provider_with_no_screen_reader_recognises_nothing() {
        let screen = &Bare as &dyn super::ScreenReader;
        let text = "Do you want to proceed?\n1. Yes\n2. No\n❯ a draft\n";
        assert!(screen.dialogs().is_empty());
        assert!(crate::session::dialogs::recognise(AgentId::TEST, text).is_none());
        assert!(screen.input_box_rows(text).is_none());
        assert!(!screen.reads_input_box());
        assert_eq!(screen.input_box_screen(), super::ScreenText::FaintBlanked);
        assert!(!screen.cancel_restores_held_messages());
        assert!(screen.same_as_submitted("typed", "typed"));
        assert!(!screen.same_as_submitted(" typed", "typed"));
        assert!(!screen.pasted_image_path_loses_enter());
        assert_eq!(screen.leading_slash_guard(), " ");
        assert_eq!(screen.image_placeholders("[Image #1] text"), 0);
        assert!(screen.command_completion("/help Show help").is_none());
        assert!(!screen.writes_lone_command_with_trailing_space());
        assert!(!screen.stash_shown(text));
        assert_eq!(screen.stash_indicator_row(text), None);
        assert!(!screen.clears_input_box_with_ctrl_u());
        assert!(!screen.collapsed_paste("[Pasted text #1]"));
        let fixture = spec_fixture();
        let spec = fixture.spec();
        let plan = provider(AgentId::TEST).plan(&spec).expect("a plan");
        assert!(!screen.native_stash(&spec, &plan));
        for glyph in [None, Some('\u{2733}'), Some('\u{280B}')] {
            assert!(screen.title_activity(glyph).is_none());
            assert!(crate::session::title_activity::shown(AgentId::TEST, glyph, false).is_none());
        }
        assert!(!screen.title_needs_user("anything"));
        assert!(!screen.prompt_after_title_stop_ends_turn());
        assert!(screen.user_action_title().is_none());
        assert!(screen.title_cancel().is_none());
        assert!(screen.interrupt().is_none());
    }

    #[test]
    fn a_prompt_after_a_stopped_title_ends_a_turn_only_for_the_measured_reader() {
        assert!(provider(AgentId::CLAUDE)
            .screen()
            .prompt_after_title_stop_ends_turn());
        for agent in [AgentId::CODEX, AgentId::GROK, AgentId::TEST] {
            assert!(
                !provider(agent).screen().prompt_after_title_stop_ends_turn(),
                "{agent:?}"
            );
        }
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
