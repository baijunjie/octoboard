//! Telling a message no turn took: for an agent whose writes are confirmed only by its report of
//! each prompt it sends (`crate::providers::DeliveryQuirks::confirms_by_reported_prompt`), a
//! message Octoboard wrote while a turn ran that the agent never reported taking was not carried
//! out. Claude Code reports a message it queues mid-turn as it queues it; OpenCode reports one
//! when a turn takes it, which can be a turn of its own that starts just after the idle. So what is
//! left of the messages written during a turn ([`crate::session::LiveSession::take_untaken`]) once
//! the turn has ended and a grace has passed with no report of it is told to the user and to the
//! session that sent it, as not sent. It is not written again: it may still be in the input box,
//! and the next message would carry it in. A report of it that comes later still withdraws the
//! notice and tells the sender it ran after all (`AppState::ran_after_all`). What is left untaken
//! when the session's process ends, or its terminal is handed to the next daemon, is told at once
//! ([`AppState::tell_untaken_now`]): nothing is left to wait on.
//!
//! The rules are in docs/product/message-delivery.md's "Messages held until a session can take
//! them" section.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::messaging::texts::SenderNote;
use crate::protocol::{notice_code, AgentId, Notice};
use crate::session::{LiveSession, ToldLost};
use crate::state::AppState;

/// How long after a turn's end a message written during it may still be reported taken: OpenCode
/// takes one written as the turn ends as a turn of its own once it is idle, within moments. A
/// report that comes later than this withdraws the notice it gave and tells the sender the message
/// ran after all.
pub(crate) const UNTAKEN_GRACE: Duration = Duration::from_secs(10);

#[cfg(test)]
thread_local! {
    /// The wait the next turn end this test thread closes takes in place of [`UNTAKEN_GRACE`]
    /// ([`GraceOverride`]): left unset, a test sees the real grace, so a timer it starts never
    /// tells anything while the test still runs.
    static GRACE_OVERRIDE: std::cell::RefCell<Option<TestWait>> = const { std::cell::RefCell::new(None) };
}

/// What the wait after one turn end waits for under test, in place of the grace: the test's word
/// to go on, and whom to tell once it has told what it found.
#[cfg(test)]
struct TestWait {
    go: std::sync::mpsc::Receiver<()>,
    told: std::sync::mpsc::Sender<()>,
}

/// Holds the wait of the next turn end this thread closes until [`Self::release`], and signals
/// once that wait has told what it found ([`Self::told_within`]). Dropped unreleased, the wait
/// ends telling nothing: no thread outlives the test asleep.
#[cfg(test)]
pub(crate) struct GraceOverride {
    go: Option<std::sync::mpsc::Sender<()>>,
    told: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
impl GraceOverride {
    pub(crate) fn held() -> Self {
        let (go, go_rx) = std::sync::mpsc::channel();
        let (told_tx, told) = std::sync::mpsc::channel();
        GRACE_OVERRIDE.with(|cell| {
            *cell.borrow_mut() = Some(TestWait {
                go: go_rx,
                told: told_tx,
            });
        });
        Self { go: Some(go), told }
    }

    /// Lets the held wait go on, as its grace passing would.
    pub(crate) fn release(&self) {
        if let Some(go) = &self.go {
            let _ = go.send(());
        }
    }

    /// Whether the wait told what it found within `timeout` of the call.
    pub(crate) fn told_within(&self, timeout: Duration) -> bool {
        self.told.recv_timeout(timeout).is_ok()
    }
}

#[cfg(test)]
impl Drop for GraceOverride {
    fn drop(&mut self) {
        GRACE_OVERRIDE.with(|cell| cell.borrow_mut().take());
    }
}

/// How the wait after a turn's end ends: once [`UNTAKEN_GRACE`] has passed, or under test on what
/// [`GraceOverride`] set. Read on the thread that closes the turn, before the wait moves onto a
/// thread of its own.
struct Wait {
    #[cfg(test)]
    test: Option<TestWait>,
}

impl Wait {
    fn taken() -> Self {
        Self {
            #[cfg(test)]
            test: GRACE_OVERRIDE.with(|cell| cell.borrow_mut().take()),
        }
    }

    /// Waits; says whether to tell what is left (a test's wait dropped unreleased says not), and
    /// hands back whom to signal once told.
    fn wait(self) -> (bool, Option<Box<dyn FnOnce() + Send>>) {
        #[cfg(test)]
        if let Some(test) = self.test {
            let go = test.go.recv().is_ok();
            let told = test.told;
            return (
                go,
                Some(Box::new(move || {
                    let _ = told.send(());
                })),
            );
        }
        std::thread::sleep(UNTAKEN_GRACE);
        (true, None)
    }
}

impl AppState {
    /// Notes that the turn of the session `id` ended, not cancelled, and where its agent confirms
    /// writes by reported prompt and messages written during the turn are not reported taken,
    /// has them told once [`UNTAKEN_GRACE`] has passed with no report of them
    /// ([`Self::tell_untaken_messages`]). Called wherever a turn is closed by its end. Messages
    /// more than are kept for the wait are told at once.
    pub(crate) fn watch_untaken_messages(self: &Arc<Self>, id: &str) {
        let Some(live) = self.live_session(id) else {
            return;
        };
        // A launch whose hooks are absent reports no prompt it sends, so nothing would confirm one.
        if !live
            .agent
            .descriptor()
            .capabilities
            .delivery
            .confirms_by_reported_prompt
            || live.hooks_absent()
        {
            return;
        }
        let ended = Instant::now();
        let wait = Wait::taken();
        let (any, pushed_off) = live.note_turn_ended_untaken(ended);
        self.tell_untaken(&live, pushed_off);
        if !any {
            return;
        }
        let (state, session) = (self.clone(), id.to_string());
        let spawned = std::thread::Builder::new()
            .name("untaken-messages".to_string())
            .spawn(move || {
                let (tell, told) = wait.wait();
                if tell {
                    state.tell_untaken_messages(&session, ended);
                }
                if let Some(told) = told {
                    told();
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(session = %id, %err, "starting the wait for untaken messages failed");
        }
    }

    /// Tells every client, and the session that sent it where one did
    /// ([`Self::tell_senders_of`]), of each message written during a turn of the session `id`
    /// that ended no later than `ended_by` which the agent has not reported taking since: it was
    /// not sent.
    pub(crate) fn tell_untaken_messages(self: &Arc<Self>, id: &str, ended_by: Instant) {
        let Some(live) = self.live_session(id) else {
            return;
        };
        let untaken = live.take_untaken(ended_by);
        self.tell_untaken(&live, untaken);
    }

    /// Tells at once what is left untaken of every turn of `live` that has ended, without waiting
    /// out the grace: its process has ended, or its terminal is being handed to the next daemon,
    /// and nothing would tell it later. Called wherever a live session is dropped or handed over.
    pub(crate) fn tell_untaken_now(self: &Arc<Self>, live: &LiveSession) {
        let untaken = live.take_untaken(Instant::now());
        self.tell_untaken(live, untaken);
    }

    /// Tells every client and each sender of `untaken`, messages written into the turns of `live`
    /// the agent did not take, that they were not sent, and keeps each for a late report of it
    /// (`LiveSession::note_told_lost`).
    fn tell_untaken(self: &Arc<Self>, live: &LiveSession, untaken: Vec<(Option<String>, String)>) {
        if untaken.is_empty() {
            return;
        }
        tracing::warn!(
            session = %live.id,
            messages = untaken.len(),
            "the turn ended without the agent reporting messages written during it as taken"
        );
        self.tell_senders_of(&live.id, live.agent, SenderNote::NotTaken, &untaken);
        for (logical_id, text) in untaken {
            self.broadcast(not_taken_notice(live.agent, &text).about(&live.id));
            live.note_told_lost(ToldLost::NotTaken, logical_id, text);
        }
    }
}

/// The `message_not_taken` notice for a message `agent` did not take in the turn it was written
/// during, with its `text`.
pub(crate) fn not_taken_notice(agent: AgentId, text: &str) -> Notice {
    let label = agent.label();
    Notice::new(
        notice_code::MESSAGE_NOT_TAKEN,
        format!(
            "{label} did not take a message {app} wrote while its turn ran: the turn ended \
             without it, so it was not sent and may still be in the input box. It is not written \
             again. It read:\n{text}",
            app = crate::APP_NAME,
        ),
        &[("agent", label), ("detail", text)],
    )
}
