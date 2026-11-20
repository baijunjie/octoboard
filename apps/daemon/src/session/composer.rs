//! The user's composer requests that are more than a paste and an Enter, written into the agent's
//! terminal: a message with images (`send_message`'s `images`) and a slash command
//! (`send_command`).
//!
//! **Images.** Each image path is pasted alone as its own bracketed paste, and the next paste
//! waits until the agent's input box shows one more image placeholder
//! ([`crate::session::screen::ScreenReader::image_placeholders`]): Claude Code loses the Enter
//! that follows a path it has not finished reading as an image, and Codex and Grok Build do not
//! need the wait, which costs them nothing. Then the message's text is pasted, if there is any,
//! and its Enter pressed as a pasted message's is ([`super::paste_apart::submit`]). Every wait is
//! bounded by the agent's pace on the slowest host, and a placeholder that is never drawn (a
//! provider that declares images and counts none) ends the message as one written and left in the
//! input box ([`super::paste_apart::NotSubmitted`]): its first paste is in, so it is told to the
//! user as a message not sent, which is the truth of it, and nothing is pressed blind.
//!
//! **Commands.** The command line is pasted alone and looked at before its Enter. An agent's
//! slash-command completion runs the highlighted command on an Enter, not what was written: Claude
//! Code run `/probecmd` for a pasted `/probe` whose own file was not loaded yet, and Grok Build only
//! completes a pasted prefix and sends nothing. So a completion that offers another command than
//! the one written has the input box cleared and the command refused
//! ([`super::term::CommandNotRecognised`]); no completion, or the one written, gets its Enter. An
//! agent's own commands report no prompt, so the Enter is confirmed by the box emptying, not by a
//! hook, and a command left in the box is a message not sent. A completion is shown only for a
//! lone `/word`, so a command with arguments has nothing to check; a provider that cannot clear its
//! box ([`super::screen::ScreenReader::clears_input_box_with_ctrl_u`]) is refused such a bare
//! command before anything is written (`coordinator::messages`), since a wrong completion could
//! not be taken back.
//!
//! The first paste goes in through the same check of an empty box, or the draft's preservation, as
//! any message (`LiveSession::write_message_input_finishing`), and the rest of the sequence runs
//! inside that same step ([`Tail`]), so that a draft stashed around the message is restored by
//! the agent only after the submit key, which is when it does. Every later key goes through a
//! write that is refused once anyone else has written since ([`Gated`]). A command is written only
//! into a box found or made empty, never as the box stands beside other text, which would run
//! together with it.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use super::input_box_clear::{clear_allowance, clear_input_box_in, press_timeout, Cleared};
use super::paste_apart::{self, not_submitted, Target, Timing};
use super::{wait, LiveSession, Tail, Written};
use crate::platform::term::{self, Input, PartialWrite};

/// A session that is written to only while nobody else writes: the writes of the keys after the
/// first, each refused once the session's input has seen a write that was not this one's.
struct Gated<'a> {
    live: &'a LiveSession,
    expected: Cell<u64>,
    /// The name of the command whose Enter is pressed, for the gate to look at the completion once
    /// more under the input lock: a popup that came up late offers another command.
    command: Option<&'a str>,
    /// The command a completion offered in place of it when the gate refused for that.
    late: RefCell<Option<String>>,
}

impl<'a> Gated<'a> {
    /// Gates writes on the session's input having counted `writes` and no more.
    fn after(live: &'a LiveSession, writes: u64, command: Option<&'a str>) -> Self {
        Self {
            live,
            expected: Cell::new(writes),
            command,
            late: RefCell::new(None),
        }
    }
}

impl Target for Gated<'_> {
    fn session(&self) -> &str {
        &self.live.id
    }

    fn box_rows(&self) -> Option<Vec<String>> {
        Target::box_rows(self.live)
    }

    fn gone(&self) -> bool {
        Target::gone(self.live)
    }

    fn agent(&self) -> Option<crate::protocol::AgentId> {
        Target::agent(self.live)
    }

    fn press(&self, keys: &[Input]) -> Result<bool, PartialWrite> {
        let written = self.live.write_message_keys_if(
            Some(self.expected.get()),
            || {
                let Some(name) = self.command else {
                    return true;
                };
                match self.live.other_completion(name) {
                    Some(offered) => {
                        *self.late.borrow_mut() = Some(offered);
                        false
                    }
                    None => true,
                }
            },
            keys,
        )?;
        if written {
            self.expected.set(self.expected.get() + 1);
        }
        Ok(written)
    }
}

/// Pastes `paste`, a later key of a message already begun, through `gated`; `written` counts what
/// of the message is in. A paste not written leaves the message in the box, as it stands.
fn paste_later(gated: &Gated<'_>, paste: &Input, written: &mut usize) -> Result<(), PartialWrite> {
    let size = term::encode(std::slice::from_ref(paste)).len();
    match gated.press(std::slice::from_ref(paste)) {
        Ok(true) => {
            *written += size;
            Ok(())
        }
        Ok(false) => {
            tracing::warn!(
                session = %gated.live.id,
                "a later paste of a message was not written: someone else wrote into the session, \
                 or it cannot take keys now"
            );
            Err(not_submitted(*written))
        }
        Err(partial) if partial.written == 0 => {
            tracing::warn!(session = %gated.live.id, error = %partial.error, "a later paste of a message could not be written");
            Err(not_submitted(*written))
        }
        Err(partial) => Err(PartialWrite {
            written: *written + partial.written,
            error: partial.error,
        }),
    }
}

impl LiveSession {
    /// Writes a message with `images` (paths of files that exist, at least one) and the paste of
    /// its `text`, if it has any; see the module header. `id` is the message's id in the queue.
    /// A failure is as a message's: nothing written at all, or [`paste_apart::NotSubmitted`] for
    /// one left in the input box. **Blocks.**
    pub(crate) fn write_images_message(
        &self,
        id: &str,
        images: &[String],
        text: Option<Input>,
        past_grace: bool,
    ) -> Result<(), PartialWrite> {
        self.write_images_with(id, images, text, past_grace, paste_apart::timing())
    }

    fn write_images_with(
        &self,
        id: &str,
        images: &[String],
        text: Option<Input>,
        past_grace: bool,
        timing: Timing,
    ) -> Result<(), PartialWrite> {
        let first = Input::Paste(images[0].clone());
        // The prompt the agent is expected to report is the text alone; an images-only message
        // records none.
        let prompt = match &text {
            Some(Input::Paste(text)) => text.as_str(),
            _ => "",
        };
        let tail = Tail {
            then: &|written| self.finish_images(&written, images, text.as_ref(), timing),
            records_prompt: text.is_some(),
            needs_empty_box: false,
            images: images.len(),
            bytes: images
                .iter()
                .map(|path| Input::Paste(path.clone()))
                .chain(text.clone())
                .map(|paste| term::encode(&[paste]).len())
                .sum(),
        };
        self.write_message_input_finishing(
            id,
            std::slice::from_ref(&first),
            prompt,
            past_grace,
            Some(&tail),
        )
    }

    /// What follows the first image's paste: the wait for its placeholder, the other images and
    /// the text, each after the placeholder of the one before, and the Enter.
    fn finish_images(
        &self,
        written: &Written,
        images: &[String],
        text: Option<&Input>,
        timing: Timing,
    ) -> Result<(), PartialWrite> {
        let first = Input::Paste(images[0].clone());
        let mut sent = term::encode(std::slice::from_ref(&first)).len();
        // What the box showed just before the first paste, so that a placeholder it held already
        // (a draft written as the box stands) is not taken for the image's.
        let baseline = written.box_before.as_deref().map_or(0, |text| {
            crate::adapter::provider(self.agent)
                .screen()
                .image_placeholders(text)
        });
        let gated = Gated::after(self, written.writes, None);
        for (shown, path) in images.iter().enumerate() {
            if shown > 0 {
                paste_later(&gated, &Input::Paste(path.clone()), &mut sent)?;
            }
            if !self.wait_for_placeholders(baseline + shown + 1, timing.paste_taken) {
                tracing::warn!(
                    session = %self.id,
                    agent = %self.agent.label(),
                    shown,
                    of = images.len(),
                    "an attached image was not seen as a placeholder in the input box; the message \
                     is left in it unsent"
                );
                return Err(not_submitted(sent));
            }
        }
        let now: Vec<Input> = text.into_iter().cloned().collect();
        if let Some(paste) = text {
            paste_later(&gated, paste, &mut sent)?;
        }
        paste_apart::submit_checked(
            &gated,
            timing,
            &now,
            &[Input::Enter],
            false,
            !written.stashed,
            |_| Ok(()),
        )
    }

    /// How many images the input box shows as attachments now, `None` where no box is recognised.
    fn image_placeholders(&self) -> Option<usize> {
        let text = self.input_box(&self.screen_text_undimmed())?;
        Some(
            crate::adapter::provider(self.agent)
                .screen()
                .image_placeholders(&text),
        )
    }

    /// Waits, for `within` at most, until the box shows at least `count` placeholders; says
    /// whether it did. A box that cannot be read shows none yet. Ends early, as not shown, for a
    /// session that is gone.
    fn wait_for_placeholders(&self, count: usize, within: Duration) -> bool {
        let mut shown = false;
        wait::wait_for(within, || {
            shown = self
                .image_placeholders()
                .is_some_and(|shown| shown >= count);
            shown || Target::gone(self)
        });
        shown
    }

    /// Writes the slash command `command` (one line, stripped) for the agent to run as one of its
    /// own; see the module header. A failure is as a message's, and
    /// [`super::term::CommandNotRecognised`] where the agent would have run another command, or
    /// [`super::term::Unwritable`] where the box holds text that could not be cleared. **Blocks.**
    pub(crate) fn write_command(
        &self,
        id: &str,
        command: &str,
        past_grace: bool,
    ) -> Result<(), PartialWrite> {
        self.write_command_with(id, command, past_grace, paste_apart::timing())
    }

    fn write_command_with(
        &self,
        id: &str,
        command: &str,
        past_grace: bool,
        timing: Timing,
    ) -> Result<(), PartialWrite> {
        let paste = [Input::Paste(command.to_string())];
        // A command reports no prompt of its own, so none is recorded for it.
        let tail = Tail {
            then: &|written| self.finish_command(&written, command, past_grace, timing),
            records_prompt: false,
            needs_empty_box: true,
            images: 0,
            bytes: term::encode(&paste).len(),
        };
        self.write_message_input_finishing(id, &paste, command, past_grace, Some(&tail))
    }

    /// What follows a command's paste: the look at the completion, and the Enter.
    fn finish_command(
        &self,
        written: &Written,
        command: &str,
        past_grace: bool,
        timing: Timing,
    ) -> Result<(), PartialWrite> {
        let name = command
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let gated = Gated::after(self, written.writes, Some(name));
        let paste = [Input::Paste(command.to_string())];
        let alone = self.checks_composer && !past_grace;
        let result = paste_apart::submit_checked(
            &gated,
            timing,
            &paste,
            &[Input::Enter],
            alone,
            !written.stashed,
            |_| match self.other_completion(name) {
                Some(offered) => Err(self.refuse_other_command(command, offered, &gated)),
                None => Ok(()),
            },
        );
        // A popup that came up after the look refused the Enter itself.
        match (result, gated.late.take()) {
            (Err(_), Some(offered)) => Err(self.refuse_other_command(command, offered, &gated)),
            (result, _) => result,
        }
    }

    /// The command the agent's completion offers in place of `name`, the one written, if it offers
    /// another: an Enter would run that one.
    fn other_completion(&self, name: &str) -> Option<String> {
        let offered = crate::adapter::provider(self.agent)
            .screen()
            .command_completion(&self.screen_text_undimmed())?;
        (offered.trim_start_matches('/') != name).then_some(offered)
    }

    /// Refuses the command `command`, with the box cleared, because `offered` would run in its
    /// place.
    fn refuse_other_command(
        &self,
        command: &str,
        offered: String,
        gated: &Gated<'_>,
    ) -> PartialWrite {
        let screen = crate::adapter::provider(self.agent).screen();
        let offered = offered.trim_start_matches('/').to_string();
        let cleared = if screen.clears_input_box_with_ctrl_u() {
            let mut writes = gated.expected.get();
            // Inside the replacement where the draft was stashed around the command: the clear is
            // the message's own.
            let (cleared, ..) = clear_input_box_in(
                self,
                &mut writes,
                press_timeout(),
                clear_allowance(),
                || self.takes_automatic_keys(),
                true,
            );
            gated.expected.set(writes);
            if cleared == Cleared::Empty {
                // The keys emptied the box: the typing record holds nothing of it any more, told
                // under the input lock, and only if nobody else has written since.
                let _input = self.pty.lock_input();
                if self.input_writes.load(std::sync::atomic::Ordering::Acquire) == writes {
                    self.draft
                        .lock()
                        .expect("draft lock poisoned")
                        .cleared_by_octoboard();
                }
            }
            cleared
        } else {
            Cleared::Left
        };
        tracing::warn!(
            session = %self.id,
            agent = %self.agent.label(),
            cleared = ?cleared,
            "a slash command would have run another command; it was not sent"
        );
        let reason = if cleared == Cleared::Empty {
            format!(
                "the agent would run /{offered} instead of {command}; the command was cleared \
                 from its input box and not sent"
            )
        } else {
            format!(
                "the agent would run /{offered} instead of {command}; it was not sent, but the \
                 command could not be cleared from its input box"
            )
        };
        PartialWrite {
            written: term::encode(&[Input::Paste(command.to_string())]).len(),
            error: std::io::Error::other(super::term::CommandNotRecognised {
                command: command.to_string(),
                offered: format!("/{offered}"),
                cleared: cleared == Cleared::Empty,
                reason,
            }),
        }
    }
}

#[cfg(all(test, unix))]
pub(crate) mod tests;
