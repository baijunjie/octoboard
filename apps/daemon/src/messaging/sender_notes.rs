//! Telling a session what became of messages it sent into another session that were not carried
//! out, or that it was told were not and that ran after all: every path that tells the user such a
//! thing about a message a session sent tells its sender through [`AppState::tell_senders_of`],
//! one note per sender in the words of `texts::sender_note`, so a sender is never left with the
//! "delivered" it was answered when the message went in.
//!
//! The rules are in docs/product/message-delivery.md's "Messages held until a session can take
//! them" section.

use std::sync::Arc;

use crate::messaging::texts::{self, SenderNote};
use crate::protocol::{AgentId, Notice};
use crate::session_tools::catalogue;
use crate::state::AppState;

impl AppState {
    /// Tells each session that sent one of `messages` into the session `session`, whose agent is
    /// `agent`, that `note` became of them: one note for each sender, naming every message of its
    /// own, in the order given. Each message is the identity the queue wrote it under, which finds
    /// its sender (`crate::messaging::outbox::Outbox::sender_of`), and its text; one with no
    /// identity, or one Octoboard or the user sent, tells nobody.
    pub(crate) fn tell_senders_of(
        self: &Arc<Self>,
        session: &str,
        agent: AgentId,
        note: SenderNote,
        messages: &[(Option<String>, String)],
    ) {
        self.composer_told(session, agent, note, messages);
        let mut senders: Vec<(String, Vec<&str>)> = Vec::new();
        for (logical_id, text) in messages {
            let Some(sender) = logical_id
                .as_deref()
                .and_then(|logical_id| self.outbox.sender_of(logical_id))
            else {
                continue;
            };
            match senders.iter_mut().find(|(known, _)| *known == sender) {
                Some((_, texts)) => texts.push(text),
                None => senders.push((sender, vec![text])),
            }
        }
        for (sender, sent) in senders {
            let get_session = catalogue::qualified_tool_name(
                self.session_record(&sender)
                    .map_or(agent, |record| record.agent),
                "get_session",
            );
            let note = texts::sender_note(note, session, agent.label(), &get_session, &sent);
            self.tell_sender(&sender, note);
        }
    }

    /// Places in the session's composer queue each of `messages` that is a composer's message
    /// (`crate::state::composer`): back in its place as not carried out, or, for a note that it
    /// ran after all, out of the queue again. The user's messages have no sender to tell, but the
    /// queue they came from takes them back so that the user can send them again.
    fn composer_told(
        &self,
        session: &str,
        agent: AgentId,
        note: SenderNote,
        messages: &[(Option<String>, String)],
    ) {
        use crate::messaging::{cleared_messages, untaken_messages, write_queue};
        // The notice the user is told with, by the same builder, its English text included.
        let notice: Option<fn(AgentId, &str) -> Notice> = match note {
            SenderNote::NotTaken => Some(untaken_messages::not_taken_notice),
            SenderNote::MaybeDroppedAtCancel => {
                Some(cleared_messages::maybe_dropped_at_cancel_notice)
            }
            SenderNote::DroppedAtCancel => Some(cleared_messages::dropped_at_cancel_notice),
            SenderNote::GivenBackUnsent => Some(cleared_messages::given_back_unsent_notice),
            SenderNote::NotDelivered => Some(write_queue::went_elsewhere_notice),
            SenderNote::RanAfterAll => None,
        };
        for (logical_id, text) in messages {
            let Some(logical_id) = logical_id else {
                continue;
            };
            let notice = notice.map(|notice| notice(agent, text));
            self.composer_logical_not_carried_out(session, logical_id, notice.as_ref());
        }
    }

    /// Tells the session `sender` `text`, about a message it sent, through the channel a session
    /// is told what became of what it sent: queued while it cannot take a message, kept for it
    /// while it is interrupted, refused once it is archived
    /// (`crate::messaging::delivery::write_or_keep`). On a thread of its own, since the write
    /// blocks and the caller is acting on what the agent reported; what is told to the sender
    /// sends nothing on, so nothing is told back about it.
    fn tell_sender(self: &Arc<Self>, sender: &str, text: String) {
        let (state, sender) = (self.clone(), sender.to_string());
        let spawned = std::thread::Builder::new()
            .name("tell-sender".to_string())
            .spawn(move || {
                let told = crate::messaging::delivery::write_or_keep(
                    &state,
                    &sender,
                    &text,
                    "this session is archived, so it takes nothing until it is reopened",
                    || format!("{text}\n\n{}", crate::messaging::delivery::kept_dating()),
                    None,
                    None,
                );
                if let Err(err) = told {
                    tracing::warn!(
                        session = %sender,
                        %err,
                        "telling a session what became of a message it sent failed"
                    );
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "starting the thread that tells a sender failed");
        }
    }
}
