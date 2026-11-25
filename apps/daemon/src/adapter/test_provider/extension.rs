//! The extension of the test-only provider: what stands in for an extension inside an agent's
//! process that opens the session's channel (`crate::session::channel`). It runs the channel
//! helper's own loops (`crate::cli::channel`) against a daemon's port, as the helper started from
//! the session's channel helper runs them, and plays the agent's side of each message: taking it,
//! refusing it, saying nothing, or giving back one it took. It shows the generic delivery path to
//! deliver, confirm and report through a channel as it does through a terminal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::cli::channel::{hand_over_messages, new_instance, report_outcomes, Held, Link};
use crate::session::channel::Handed;

/// What the stand-in's agent does with a message the extension sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Agent {
    /// Takes it as a user message.
    Takes,
    /// Refuses it, for this reason.
    Refuses(&'static str),
    /// Takes the line and never says what came of it.
    Silent,
}

/// A running stand-in extension, with the helper it started.
pub(crate) struct Extension {
    /// The messages the helper printed for the extension, in order.
    sent: mpsc::Receiver<Handed>,
    /// The extension's lines to the helper's standard input.
    lines: Option<mpsc::Sender<String>>,
    ended: Arc<AtomicBool>,
    polling: Option<JoinHandle<()>>,
    reporting: Option<JoinHandle<()>>,
}

impl Extension {
    /// Starts the helper for `session` against the daemon or holder at `port` with `key`, as the
    /// session's channel helper does, with an agent that does `agent` with each message.
    pub(crate) fn start(port: u16, key: &str, session: &str, agent: Agent) -> Self {
        let link = Arc::new(Link {
            port,
            key: key.to_string(),
            session: session.to_string(),
            instance: new_instance(),
            retry_pause: Duration::from_millis(20),
        });
        let held = Held::default();
        let (lines, read_lines) = mpsc::channel::<String>();
        let (printed, sent) = mpsc::channel();
        let ended = Arc::new(AtomicBool::new(false));

        let reporting = {
            let (link, held) = (link.clone(), held.clone());
            std::thread::spawn(move || {
                report_outcomes(&link, read_lines.into_iter().map(Ok), &held);
            })
        };
        let polling = {
            let (ended, lines) = (ended.clone(), Mutex::new(lines.clone()));
            std::thread::spawn(move || {
                hand_over_messages(
                    &link,
                    &held,
                    &|| ended.load(Ordering::Acquire),
                    &mut |message| {
                        let outcome = match agent {
                            Agent::Takes => Some(serde_json::json!({
                                "id": message.id, "outcome": "taken",
                            })),
                            Agent::Refuses(reason) => Some(serde_json::json!({
                                "id": message.id, "outcome": "refused", "reason": reason,
                            })),
                            Agent::Silent => None,
                        };
                        if let Some(outcome) = outcome {
                            let lines = lines.lock().unwrap();
                            lines
                                .send(outcome.to_string())
                                .map_err(std::io::Error::other)?;
                        }
                        printed.send(message.clone()).map_err(std::io::Error::other)
                    },
                );
            })
        };
        Self {
            sent,
            lines: Some(lines),
            ended,
            polling: Some(polling),
            reporting: Some(reporting),
        }
    }

    /// The next message the extension was handed, waited for as long as the tests' patience.
    pub(crate) fn sent(&self) -> Handed {
        self.sent
            .recv_timeout(crate::test_support::PATIENCE)
            .expect("the extension was handed a message")
    }

    /// Whether the extension was handed a message it has not been asked for yet. Never waits.
    pub(crate) fn sent_more(&self) -> bool {
        self.sent.try_recv().is_ok()
    }

    /// The agent gave the message `id` back unsent, and nothing of it is left with it.
    pub(crate) fn give_back(&self, id: &str) {
        let line = serde_json::json!({ "id": id, "outcome": "returned" }).to_string();
        self.lines
            .as_ref()
            .expect("the extension runs")
            .send(line)
            .expect("the helper reads");
    }

    /// Whether the helper has ended by itself, waited for as long as the tests' patience.
    pub(crate) fn helper_ended(&self) -> bool {
        crate::session::wait::wait_for(crate::test_support::PATIENCE, || {
            self.polling
                .as_ref()
                .is_none_or(|polling| polling.is_finished())
        })
    }

    /// Ends the extension, as the agent's exit does: the helper stops polling once its poll in
    /// flight is answered, and its standard input closes.
    pub(crate) fn stop(&mut self) {
        self.ended.store(true, Ordering::Release);
        if let Some(polling) = self.polling.take() {
            polling.join().expect("the helper's polls ended");
        }
        self.lines.take();
        if let Some(reporting) = self.reporting.take() {
            reporting.join().expect("the helper's reports ended");
        }
    }
}

impl Drop for Extension {
    /// Lets the helper's threads end without waiting on them: a test that failed part-way must
    /// not hang on a poll.
    fn drop(&mut self) {
        self.ended.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::outbox::Drain;
    use crate::protocol::{AgentId, SessionStatus};
    use crate::session::channel::{Channel, Timing};
    use crate::session::term::{
        send_message_with_id, MessageTransport, RefusedByAgent, Route, UncertainSubmission,
    };
    use crate::state::test_support::insert_record;
    use crate::state::AppState;
    use crate::test_support::{
        app_state, idle_stand_in_of, output_containing, ScratchDir, StandIn, PATIENCE, TEST_KEY,
    };

    /// A session of the test-only provider whose messages go through its channel, served by a
    /// daemon's own router on a loopback port.
    struct Served {
        state: Arc<AppState>,
        channel: Arc<Channel>,
        live: StandIn,
        port: u16,
        _runtime: tokio::runtime::Runtime,
        _dir: ScratchDir,
    }

    /// Polls short enough for a stopped extension's helper to end soon; everything a stand-in
    /// answers inside is the tests' patience.
    fn timing() -> Timing {
        Timing {
            pickup: PATIENCE,
            outcome: PATIENCE,
            poll: Duration::from_millis(100),
            gap: PATIENCE,
        }
    }

    fn served(label: &str, timing: Timing) -> Served {
        let (state, dir) = app_state(label);
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let channel = Arc::new(Channel::with_timing(timing));
        let live = idle_stand_in_of("s", AgentId::TEST).with_transport(channel.clone());
        state.register_live(live.clone());
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("a runtime");
        let port = runtime.block_on(crate::server::test_support::serve_state(state.clone()));
        Served {
            state,
            channel,
            live,
            port,
            _runtime: runtime,
            _dir: dir,
        }
    }

    impl Served {
        /// An extension whose helper is connected: a poll of it waits.
        fn extension(&self, agent: Agent) -> Extension {
            let extension = Extension::start(self.port, TEST_KEY, "s", agent);
            assert!(
                crate::session::wait::wait_for(PATIENCE, || self.channel.waiting_polls() > 0),
                "the helper never polled"
            );
            extension
        }

        fn send(&self, text: &str, id: &str) -> Result<Route, crate::platform::term::PartialWrite> {
            send_message_with_id(&self.live, text, id, false, &|| false)
        }
    }

    /// A message goes to the extension through the session's channel, under the id its outcome is
    /// reported by, and is delivered once the extension reports the agent took it: sent directly
    /// and by the session's queue, which then holds nothing.
    #[test]
    fn a_message_goes_through_the_channel_and_is_confirmed_by_the_extensions_report() {
        let served = served("test-provider-channel-delivery", timing());
        let mut extension = served.extension(Agent::Takes);

        assert_eq!(
            served.send("sent directly", "id-1").expect("taken"),
            Route::Queued
        );
        assert_eq!(
            extension.sent(),
            Handed {
                id: "id-1".to_string(),
                text: "sent directly".to_string()
            }
        );

        served
            .state
            .queue_message("s", "sent by the queue")
            .unwrap();
        assert_eq!(
            served.state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(served.state.queued_count("s"), 0);
        assert_eq!(extension.sent().text, "sent by the queue");
        // Each was handed over once.
        extension.stop();
        assert!(!extension.sent_more());
    }

    /// A message the agent refuses fails as refused, with the extension's reason, and one the
    /// extension was handed and reported nothing of is uncertain: neither goes into the terminal.
    #[test]
    fn a_refusal_and_a_write_with_no_report_fail_without_reaching_the_terminal() {
        let served = served("test-provider-channel-refused", timing());
        let mut extension = served.extension(Agent::Refuses("not while compacting"));
        let refused = served.send("refused", "id-1").expect_err("refused");
        let reason = refused
            .error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<RefusedByAgent>())
            .expect("a refusal by the agent");
        assert_eq!(reason.0, "not while compacting");
        extension.stop();

        let served = served_silent();
        let mut extension = served.extension(Agent::Silent);
        let unreported = served.send("unreported", "id-2").expect_err("uncertain");
        assert!(unreported
            .error
            .get_ref()
            .is_some_and(|inner| inner.is::<UncertainSubmission>()));
        assert_eq!(extension.sent().text, "unreported");
        extension.stop();
    }

    /// A channel whose wait for an outcome runs out soon, for the extension that never reports.
    fn served_silent() -> Served {
        served(
            "test-provider-channel-unreported",
            Timing {
                outcome: Duration::from_millis(100),
                ..timing()
            },
        )
    }

    /// Once the extension has gone, the channel costs the session nothing but itself: the next
    /// message goes into the terminal.
    #[test]
    fn a_message_goes_into_the_terminal_once_the_extension_has_gone() {
        let served = served("test-provider-channel-gone", timing());
        let mut extension = served.extension(Agent::Takes);
        assert_eq!(
            served.send("while it ran", "id-1").expect("taken"),
            Route::Queued
        );
        assert_eq!(extension.sent().text, "while it ran");

        extension.stop();
        // The gap after the helper's last poll has run out.
        served.channel.forget_last_poll();
        assert_eq!(
            served.send("after it went", "id-2").expect("written"),
            Route::Terminal
        );
        let heard = output_containing(&served.live, "after it went");
        assert!(heard.contains("after it went"), "{heard}");
        assert!(!heard.contains("while it ran"), "{heard}");
    }

    /// A message the agent took and gave back unsent is written again by the session's queue,
    /// through the channel, after the next message.
    #[test]
    fn a_message_the_agent_gave_back_through_the_channel_is_written_again() {
        let served = served("test-provider-channel-returned", timing());
        let mut extension = served.extension(Agent::Takes);
        served
            .state
            .queue_message("s", "held for the turn")
            .unwrap();
        assert_eq!(
            served.state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        let first = extension.sent();
        assert_eq!(first.text, "held for the turn");

        extension.give_back(&first.id);
        assert!(
            crate::session::wait::wait_for(PATIENCE, || served.state.queued_count("s") == 1),
            "the message was not queued again"
        );
        served.state.queue_message("s", "the next one").unwrap();
        assert_eq!(
            served.state.flush_outbox("s", SessionStatus::Working),
            Drain::Clear
        );
        assert_eq!(extension.sent().text, "the next one");
        assert_eq!(extension.sent().text, "held for the turn");
        extension.stop();
    }

    /// A launch of the provider gets a channel only while the provider declares one for it.
    #[test]
    fn a_launch_gets_a_channel_only_while_the_provider_declares_one() {
        use crate::adapter::AgentProvider;
        use crate::session::channel::transport_of;

        let fixture = crate::adapter::tests::spec_fixture();
        let provider = crate::adapter::test_provider::TestProvider;
        let declared = || {
            let spec = fixture.spec();
            let plan = provider.plan(&spec).expect("a plan");
            provider.extension_channel(&spec, &plan)
        };
        assert!(!declared());
        let (transport, is_channel) = transport_of(None, declared());
        assert!(transport.is_none() && !is_channel);

        let _declaring = crate::test_support::test_provider::declaring_extension_channel();
        assert!(declared());
        let (transport, is_channel) = transport_of(None, declared());
        assert!(is_channel);
        assert!(transport.is_some_and(|transport| transport.channel().is_some()));
    }

    /// The helper ends by itself once the session's channel is stopped, as it is when the
    /// session's process has exited, and where the session has no channel at all: what a daemon
    /// or a holder that predates the channel answers too.
    #[test]
    fn the_helper_ends_once_the_session_has_no_channel() {
        let served = served("test-provider-channel-ended", timing());
        let extension = served.extension(Agent::Takes);
        served.channel.stop();
        assert!(
            extension.helper_ended(),
            "the helper polls a stopped channel"
        );

        let no_channel = Extension::start(served.port, TEST_KEY, "no-such-session", Agent::Takes);
        assert!(no_channel.helper_ended(), "the helper polls no channel");
    }
}
