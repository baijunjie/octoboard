//! Writing a message again that was cleared from its session's input box unsent: the message was
//! written into the session, whose agent held it for the end of a turn and put it back in its
//! input box when the turn was cancelled, and it was then cleared out of the way of the next
//! message (`crate::session::draft_clear`). Its sender was answered "delivered" when it went in,
//! so the message is queued again, as it was written, right behind the message whose write cleared
//! it: nothing a session or Octoboard sent is lost to a cancel, whoever sent it and whatever has
//! become of the sender since.
//!
//! The rules, the bound on how often a message is written again and what the user is told where it
//! is not, are in docs/product/message-delivery.md's "Messages held until a session can take them"
//! section.
//!
//! A message that went through the session's own server or channel instead of its terminal is
//! written again when that server or channel says the agent gave it back
//! ([`AppState::message_returned`]), and never because of what an input box shows.
//!
//! An agent whose cancel drops the messages it held, putting nothing back in its input box, has
//! them queued again when it reports the turn cancelled
//! ([`AppState::write_dropped_messages_again`]); on a launch that cannot tell a message the turn
//! took from one the cancel dropped, none is written again and the user is told each may not
//! have been carried out.

use std::sync::Arc;

use crate::messaging::outbox::Returned;
use crate::messaging::texts::SenderNote;
use crate::protocol::{notice_code, Notice};
use crate::session::LiveSession;
use crate::state::AppState;

impl AppState {
    /// Queues the message written into the session `id` as `logical_id`, the id the queue wrote
    /// it under, to be written again right after the message being written now, and says whether
    /// it did ([`crate::messaging::outbox::Outbox::redeliver`]). Called from within that write, so
    /// the drain that is making it writes the message again by itself.
    ///
    /// With `held`, for a message an interrupt cleared, no message is being written: it is queued
    /// held until the next message is taken for the session, which it then follows
    /// ([`crate::messaging::outbox::Outbox::redeliver_held`]), so the agent stays stopped.
    pub(crate) fn redeliver_cleared_message(&self, id: &str, logical_id: &str, held: bool) -> bool {
        let again = if held {
            self.outbox.redeliver_held(id, logical_id)
        } else {
            self.outbox.redeliver(id, logical_id)
        };
        if again {
            self.note_activity(id);
        }
        again
    }

    /// Has the messages Octoboard wrote into the terminal of the session `id` while its turn ran
    /// written again, where the agent, which has just reported that turn cancelled, drops the
    /// messages it holds at a cancel, as its provider answered for the launch
    /// (`crate::adapter::AgentProvider::launch_cancel_drops_held_messages`). Called wherever a
    /// cancel is reported, by a hook or by the program's status reports, for a turn that was
    /// open. Each is queued held, in the order they were written in, as a message an interrupt
    /// cleared is: the agent was just stopped, and they follow the next message written into the
    /// session. One that cannot be queued again (the queue did not write it, it was more than
    /// text, it has been written again as often as a message is, or the daemon is stopping) is
    /// told to every client, with its text.
    ///
    /// Only the messages nothing has shown the agent to have taken are queued: one written
    /// before the turn began, or reported sent since, is not among them, and one the agent
    /// reports sending after all while it is still held is taken off the queue again
    /// ([`Self::withdraw_dropped_message`]).
    ///
    /// On a launch whose reader declares such a cancel but which is without the reports that
    /// tell a message the turn took from one it dropped (its provider answered false for the
    /// launch), none is written again, since one the turn took would run twice: every client is
    /// told of each message written during the turn that it may not have been carried out, with
    /// its text, for the user to check and send again what the session did not act on, and the
    /// session that sent it, where one did, is told the same ([`Self::tell_sender`]).
    pub(crate) fn write_dropped_messages_again(self: &Arc<Self>, id: &str) {
        let Some(live) = self.live_session(id) else {
            return;
        };
        self.tell_unseen_at_cancel(&live);
        let dropped = live.take_dropped_at_cancel();
        if dropped.is_empty() {
            return;
        }
        let mut again = 0;
        let mut lost = Vec::new();
        // Latest first: each is put at the front of the queue, so the oldest ends up first.
        for (logical_id, text) in dropped.into_iter().rev() {
            let queued = logical_id
                .as_deref()
                .is_some_and(|logical_id| self.outbox.redeliver_dropped(id, logical_id));
            if queued {
                again += 1;
            } else {
                lost.push((logical_id, text));
            }
        }
        if again > 0 {
            self.note_activity(id);
            tracing::info!(
                session = %id,
                messages = again,
                "the agent dropped messages it held when its turn was cancelled; they are queued \
                 to be written again after the next one"
            );
        }
        lost.reverse();
        self.tell_senders_of(id, live.agent, SenderNote::DroppedAtCancel, &lost);
        let agent = live.agent.label();
        for (_, text) in lost {
            tracing::warn!(
                session = %id,
                chars = text.chars().count(),
                "the agent dropped a message it held when its turn was cancelled; it could not \
                 be queued again, so it was not carried out"
            );
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_DROPPED_AT_CANCEL,
                    format!(
                        "{agent} dropped, unsent, a message it had held for the end of the turn \
                         when the turn was cancelled. It could not be written again, so it was \
                         not carried out. It read:\n{text}"
                    ),
                    &[("agent", agent), ("detail", &text)],
                )
                .about(id),
            );
        }
    }

    /// Tells every client of each message Octoboard wrote into the turn of `live` that a cancel
    /// may have dropped unseen (`LiveSession::take_unseen_at_cancel`): it is not written again,
    /// and may not have been carried out. The session that sent it, where one did, is told too.
    fn tell_unseen_at_cancel(self: &Arc<Self>, live: &LiveSession) {
        let unseen = live.take_unseen_at_cancel();
        if unseen.is_empty() {
            return;
        }
        let agent = live.agent.label();
        tracing::warn!(
            session = %live.id,
            messages = unseen.len(),
            "the turn was cancelled while the agent may have held messages written during it, and \
             nothing tells whether it took them; they are not written again"
        );
        self.tell_senders_of(
            &live.id,
            live.agent,
            SenderNote::MaybeDroppedAtCancel,
            &unseen,
        );
        for (_, text) in unseen {
            self.broadcast(
                Notice::new(
                    notice_code::MESSAGE_MAYBE_DROPPED_AT_CANCEL,
                    format!(
                        "A message {app} wrote while the turn ran may not have been carried out: \
                         the turn was cancelled, and {agent} may have dropped the message if it \
                         was still holding it. Nothing tells whether it was taken, so it is not \
                         written again. It read:\n{text}",
                        app = crate::APP_NAME,
                    ),
                    &[("agent", agent), ("detail", &text)],
                )
                .about(&live.id),
            );
        }
    }

    /// Takes off the queue a message put back because the agent of `live` dropped it at a cancel
    /// ([`Self::write_dropped_messages_again`]) and still held there, where `prompt`, a prompt
    /// the agent reports sending, is that message: the agent took it after all, and written again
    /// it would be delivered twice.
    pub(crate) fn withdraw_dropped_message(&self, live: &LiveSession, prompt: &str) {
        let withdrawn = self
            .outbox
            .withdraw_dropped(&live.id, |written| live.same_as_written(written, prompt));
        if withdrawn {
            tracing::info!(
                session = %live.id,
                "the agent reported sending a message that was queued again as dropped at a \
                 cancel; it is not written again"
            );
        }
    }

    /// Has the message the own server or channel of the session `id` took as `logical_id`, and
    /// whose agent then gave it back unsent, written again
    /// (`crate::session::term::MessageTransport::on_returned`). Such a message never was in the
    /// terminal, so nothing clears it from an input box and the queue's note of the latest message
    /// written there does not know it: the queue keeps what a server or channel took apart
    /// ([`crate::messaging::outbox::Outbox::redeliver_returned`]), and it is queued held, as a
    /// message an interrupt cleared is, since the agent gives one back when its turn is cancelled.
    /// Where it cannot be written again, every client is told it was not carried out, with its
    /// text.
    pub(crate) fn message_returned(&self, id: &str, logical_id: &str) {
        match self.outbox.redeliver_returned(id, logical_id) {
            Returned::Again => {
                self.note_activity(id);
                tracing::info!(
                    session = %id,
                    "the agent gave back a message its own server or channel had taken; it is \
                     queued to be written again after the next one"
                );
            }
            Returned::Dropped(text) => {
                tracing::warn!(
                    session = %id,
                    chars = text.chars().count(),
                    "the agent gave back a message its own server or channel had taken; it could \
                     not be queued again, so it was not carried out"
                );
                let Some(live) = self.live_session(id) else {
                    return;
                };
                // Reached from within a drain too, which holds no handle of the state to tell
                // the sender from: the session's own is used.
                if let Some(state) = live.app_state() {
                    state.tell_senders_of(
                        id,
                        live.agent,
                        SenderNote::GivenBackUnsent,
                        &[(Some(logical_id.to_string()), text.clone())],
                    );
                }
                let agent = live.agent.label();
                self.broadcast(
                    Notice::new(
                        notice_code::MESSAGE_GIVEN_BACK_UNSENT,
                        format!(
                            "{agent} gave back, unsent, a message its own server or channel had \
                             taken. It could not be written again, so it was not carried out. It \
                             read:\n{text}"
                        ),
                        &[("agent", agent), ("detail", &text)],
                    )
                    .about(id),
                );
            }
            // The drain writing it takes the give-back up once its write is over
            // (`PromptSignal::given_back_while_written`), and comes back here then.
            Returned::Writing => tracing::debug!(
                session = %id,
                "the agent gave back a message its own server or channel took while the queue \
                 was still writing it; it is taken up once the write is over"
            ),
            Returned::Unknown => tracing::warn!(
                session = %id,
                "the agent gave back a message the queue does not hold as taken by its own server \
                 or channel; nothing is written again"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::messaging::outbox::{Drain, MAX_REDELIVERIES};
    use crate::platform::term::PartialWrite;
    use crate::protocol::{notice_code, AgentId, Event, SessionStatus};
    use crate::session::term::{GivenBack, MessageTransport, QueueStart, Sent, TransportEvents};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session};

    /// A message written by the queue and then cleared is queued again with its own text; one the
    /// queue never wrote, or already queued again for that write, is not.
    #[test]
    fn a_cleared_message_is_queued_again_as_it_was_written() {
        let (state, dir) = app_state("cleared-message-again");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", AgentId::CLAUDE, 120, 40, "sleep 30");
        state.register_live(live.clone());
        let ticket = state
            .queue_message("s", "Report from your child session")
            .unwrap();
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(state.queued_count("s"), 0);

        let id = format!("{}:{ticket}", live.launch_id);
        assert!(!state.redeliver_cleared_message("s", "never written", false));
        assert!(state.redeliver_cleared_message("s", &id, false));
        assert!(!state.redeliver_cleared_message("s", &id, true));
        assert_eq!(state.take_queued("s"), ["Report from your child session"]);
    }

    /// A server of the session's own that takes every message and keeps the id it was sent under
    /// and what the session gave it to call when the agent gives one back.
    #[derive(Default)]
    struct GivingBack {
        taken: Mutex<Vec<(String, String)>>,
        returned: Mutex<Option<Box<GivenBack>>>,
        /// Whether the agent gives the next message back before the send that took it has
        /// returned, once.
        gives_back_at_once: std::sync::atomic::AtomicBool,
    }

    impl GivingBack {
        /// The id the message `text` was last sent under.
        fn id_of(&self, text: &str) -> String {
            let taken = self.taken.lock().unwrap();
            let (id, _) = taken
                .iter()
                .rev()
                .find(|(_, taken)| taken == text)
                .expect("the server took the message");
            id.clone()
        }

        /// The texts the server took, in order.
        fn texts(&self) -> Vec<String> {
            let taken = self.taken.lock().unwrap();
            taken.iter().map(|(_, text)| text.clone()).collect()
        }

        /// Says the agent gave the message `text` back, as the server's own code would.
        fn give_back(&self, text: &str) {
            let id = self.id_of(text);
            let returned = self.returned.lock().unwrap();
            returned.as_ref().expect("the session listens")(&id);
        }
    }

    impl MessageTransport for GivingBack {
        fn send(&self, text: &str, logical_id: &str) -> Result<Sent, PartialWrite> {
            self.taken
                .lock()
                .unwrap()
                .push((logical_id.to_string(), text.to_string()));
            if self
                .gives_back_at_once
                .swap(false, std::sync::atomic::Ordering::AcqRel)
            {
                let returned = self.returned.lock().unwrap();
                returned.as_ref().expect("the session listens")(logical_id);
            }
            Ok(Sent::Queued)
        }
        fn attach(&self, _: TransportEvents) {}
        fn on_returned(&self, returned: Box<GivenBack>) {
            *self.returned.lock().unwrap() = Some(returned);
        }
        fn start_queue(&self) -> QueueStart {
            QueueStart::Nothing
        }
        fn stop(&self) {}
    }

    /// A message the session's own server took and the agent then gave back is queued again with
    /// its own text, held until another message is taken, which it then follows; told of it a
    /// second time, the queue writes nothing more.
    #[test]
    fn a_message_the_agent_gave_back_to_its_server_is_written_again_after_the_next() {
        let (state, dir) = app_state("returned-message-again");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let server = Arc::new(GivingBack::default());
        let live = fake_live_session("s", AgentId::CLAUDE, 120, 40, "sleep 30")
            .with_transport(server.clone());
        state.register_live(live.clone());
        state.queue_message("s", "held for the turn").unwrap();
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(server.texts(), ["held for the turn"]);

        server.give_back("held for the turn");
        assert_eq!(state.queued_count("s"), 1);
        // Held: the agent was just stopped, and writing it now would start it again.
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(server.texts(), ["held for the turn"]);

        state.queue_message("s", "the next one").unwrap();
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(
            server.texts(),
            ["held for the turn", "the next one", "held for the turn"]
        );

        // The first write's id is done with: nothing comes of naming it again.
        let first = server.taken.lock().unwrap()[0].0.clone();
        state.message_returned("s", &first);
        assert_eq!(state.queued_count("s"), 0);
    }

    /// What a drain that stopped with nothing holding the queue leaves queued (a message that went
    /// elsewhere, say) is written out on the runtime by a drain of its own, with nothing else
    /// draining it.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_queue_is_written_out_again_soon_on_the_runtime() {
        let (state, dir) = app_state("retry-queue-soon");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let server = Arc::new(GivingBack::default());
        let live = fake_live_session("s", AgentId::CLAUDE, 120, 40, "sleep 30")
            .with_transport(server.clone());
        state.register_live(live.clone());
        state.queue_message("s", "left queued").unwrap();
        live.retry_queue_soon();
        let taken = server.clone();
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, move || {
                taken.texts() == ["left queued"]
            })
            .await,
            "the queue was never written out"
        );
    }

    /// A message the agent gave back before the send that took it had returned, when the queue
    /// does not hold it as taken yet, is written again all the same, after the next one. The
    /// server says so from inside its send, which holds the queue's write where the race lies.
    #[test]
    fn a_message_given_back_before_its_write_returned_is_written_again_after_the_next() {
        let (state, dir) = app_state("returned-message-during-write");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let server = Arc::new(GivingBack::default());
        let live = fake_live_session("s", AgentId::CLAUDE, 120, 40, "sleep 30")
            .with_transport(server.clone());
        state.register_live(live.clone());
        server
            .gives_back_at_once
            .store(true, std::sync::atomic::Ordering::Release);
        state.queue_message("s", "held for the turn").unwrap();
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(server.texts(), ["held for the turn"]);
        assert_eq!(state.queued_count("s"), 1, "queued again, held");

        state.queue_message("s", "the next one").unwrap();
        assert_eq!(
            state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(
            server.texts(),
            ["held for the turn", "the next one", "held for the turn"]
        );
    }

    /// Given back as often as a message is written again, it is dropped and every client told,
    /// with its text, under the code of a message given back: nothing was cleared from an input
    /// box. The session that sent it is told as well.
    #[test]
    fn a_message_given_back_every_time_is_dropped_and_the_user_told() {
        let (state, dir) = app_state("returned-message-dropped");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let server = Arc::new(GivingBack::default());
        let live = fake_live_session("s", AgentId::CLAUDE, 120, 40, "sleep 30")
            .with_transport(server.clone());
        state.register_live(live.clone());
        let mut events = state.subscribe();
        insert_record(&state, &dir, "sender", SessionStatus::Interrupted);
        let ticket = state.queue_message("s", "never taken").unwrap();
        state.outbox.note_sender(ticket, "sender");
        state.flush_outbox("s", SessionStatus::Working);
        for round in 0..MAX_REDELIVERIES {
            server.give_back("never taken");
            let other = format!("another {round}");
            state.queue_message("s", &other).unwrap();
            assert_eq!(
                state.flush_outbox("s", SessionStatus::Working),
                Drain::Clear
            );
        }
        assert_eq!(
            server
                .texts()
                .iter()
                .filter(|text| *text == "never taken")
                .count(),
            usize::from(MAX_REDELIVERIES) + 1
        );

        server.give_back("never taken");
        assert_eq!(state.queued_count("s"), 0);
        let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
            matches!(
                event,
                Event::SessionNotice { session, code, message, .. }
                    if session == "s"
                        && code == notice_code::MESSAGE_GIVEN_BACK_UNSENT
                        && message.ends_with("never taken")
            )
        });
        assert!(told, "the user is told what the message read");
        let kept = || state.store.kept_messages("sender").unwrap();
        assert!(
            crate::session::wait::wait_for(crate::test_support::PATIENCE, || !kept().is_empty()),
            "the sender was never told"
        );
        let kept = kept();
        assert!(kept[0].1.contains("gave it back unsent"), "{kept:?}");
    }
}
