//! How the events Octoboard's extension forwards from Pi are read. Pi reports status through
//! OSC 7501. Where the console host drops those reports, the extension forwards equivalent
//! run states to `crate::session::program_status` through the session's hook script. The script
//! also forwards:
//! `session_start`, with the id of the session Pi opened, `message_start`, with a prompt that has
//! entered the conversation, and `agent_settled`, with what the run that has just settled said.
//! It also runs the cross-project guard, whose payload and refusal are read here too.
//!
//! Pi fires `session_start` once its startup, its project-trust confirmation included, is behind
//! it (1.5.2), and again when `/new`, `/resume` or `/fork` replaces the session in the same
//! process. The extension gives the id of the session open then only where Pi has written that
//! session's file, which is the one a later resume can open: a session `/new` starts is written at
//! its first message, as is one `/fork` opens, so its id is not given then. The extension sends the
//! event again, with the id, once a run has ended with the file there (`reason: "saved"`), so a
//! later resume opens the conversation the user went on in.
//!
//! Pi fires `agent_settled` once a run is over and it will not run again by itself, which is when
//! it reports `done` or `error` (1.5.2). Those reports carry the session's name as their `msg`
//! once the session has one (`/name`), or the first line of the error, never what the model said,
//! so the turn's message is taken from the extension instead: the text of the run's last assistant
//! message, or the error it ended on, and nothing for a run that said nothing. The report's `msg`
//! never stands in for it ([`HookInterpreter::report_msg_is_turn_message`]): a turn the extension
//! gave no message for, or none in time, ends with none. A run the user cancelled is reported
//! `idle` without a turn message.
//!
//! The extension forwards each prompt as `message_start`, with its text as `prompt`: the prompt
//! that starts a run from Pi's `before_agent_start`, which Pi awaits before it reports `working`,
//! so that the prompt reaches the daemon before the report does (a turn that errors at once, or
//! that Esc cancels at once, would otherwise be closed before its prompt arrived and the prompt
//! open a turn of its own); and a steering message as it enters the conversation, which Pi takes
//! into a run once its current step is done (1.5.2). A steering message Esc puts back in the input
//! box never enters the conversation, so it is never reported sent. The text is what the model is
//! given: Pi trims what it takes from the box, and an image Octoboard pasted is taken out of it as
//! an attachment, so a message of images alone reports an empty prompt.
//!
//! Where the session is kept to its own project, the extension asks the guard about each call of
//! Pi's file-writing tools, `write` and `edit`, with a payload of its own:
//! `{"tool_name", "tool_input": {"path"}, "cwd"}`, the path resolved as Pi resolves it before it
//! writes. A refusal is Pi's own `tool_call` answer, `{"block": true, "reason"}`, which Pi hands
//! the model as the call's result (1.5.2).

use serde_json::Value;

use crate::hooks::{HookInterpreter, TurnMessage};
use crate::terminal::program_status::{BlockedKind, Record, ReportedState};

/// The names the extension gives the events it forwards, as Pi names them.
const SESSION_START: &str = "session_start";
const AGENT_SETTLED: &str = "agent_settled";
const MESSAGE_START: &str = "message_start";

/// Pi's tools that write a file, which the guard is asked about; the extension is given them
/// (`super::write_extension`).
pub(super) const FILE_WRITING_TOOLS: [&str; 2] = ["write", "edit"];

/// The field of an `agent_settled` payload holding what the run said, `null` where it said
/// nothing.
const LAST_ASSISTANT_MESSAGE: &str = "last_assistant_message";

pub(super) struct PiHooks;

impl HookInterpreter for PiHooks {
    fn program_status(&self, event: &str, payload: &Value) -> Option<Record> {
        let state = match (event, payload["turn_status"].as_str()?) {
            ("agent_status", "working") => ReportedState::Working,
            ("agent_status", "blocked") => ReportedState::Blocked,
            ("agent_status", "idle") => ReportedState::Idle,
            (AGENT_SETTLED, "done") => ReportedState::Done,
            (AGENT_SETTLED, "error") => ReportedState::Error,
            _ => return None,
        };
        Some(Record {
            state,
            kind: (state == ReportedState::Blocked).then_some(BlockedKind::Question),
            progress: None,
            app: Some("pi".to_owned()),
            title: None,
            msg: None,
        })
    }

    fn context_report(
        &self,
        event: &str,
        payload: &Value,
    ) -> Option<crate::records::context::ContextReport> {
        if event != "context_usage" {
            return None;
        }
        let tokens = payload.get("tokens")?;
        let window = payload["window"].as_u64().filter(|window| *window > 0);
        Some(crate::records::context::ContextReport::Usage {
            tokens: if tokens.is_null() {
                None
            } else {
                Some(tokens.as_u64()?)
            },
            window,
        })
    }

    /// The file a `write` or `edit` call names, which the extension has already resolved against
    /// the working directory it gives beside it.
    fn edit_targets(&self, payload: &Value) -> (Vec<String>, Option<std::path::PathBuf>) {
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(std::path::PathBuf::from);
        let writes = payload
            .get("tool_name")
            .and_then(Value::as_str)
            .is_some_and(|tool| FILE_WRITING_TOOLS.contains(&tool));
        let path = payload
            .get("tool_input")
            .and_then(|input| input.get("path"))
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty());
        match path {
            Some(path) if writes => (vec![path.to_owned()], cwd),
            _ => (Vec::new(), cwd),
        }
    }

    /// Pi's own answer to a `tool_call` it is to refuse, which the extension hands back to it.
    fn deny_edit(&self, reason: &str) -> Option<Value> {
        Some(serde_json::json!({"block": true, "reason": reason}))
    }

    /// Each user message that enters the conversation.
    fn starts_conversation(&self, event: &str, _payload: &Value) -> bool {
        event == MESSAGE_START
    }

    fn submitted_prompt<'a>(&self, payload: &'a Value) -> Option<&'a str> {
        payload.get("prompt")?.as_str()
    }

    /// The extension forwards a run's opening prompt from `before_agent_start`, which Pi awaits
    /// before it reports `working`, and waits for the hook script to exit, up to a bound. The
    /// script exits once the daemon has queued the event, which is applied on a task of its own
    /// beside the one that applies the reports, so the report can still be applied first.
    fn opening_prompt_may_follow_start(&self) -> bool {
        true
    }

    /// Pi fires `session_start`, the first event forwarded, once its project-trust confirmation
    /// is behind it.
    fn past_startup(&self, _event: &str, _payload: &Value) -> bool {
        true
    }

    fn announces_session_start(&self, event: &str, _payload: &Value) -> bool {
        event == SESSION_START
    }

    fn agent_session_id(&self, payload: &Value) -> Option<String> {
        crate::hooks::agent_session_id(payload)
    }

    /// Pi's `done` report says nothing of what the turn said, so its turn ends wait for the
    /// extension's word.
    fn supplies_turn_message(&self) -> bool {
        true
    }

    /// The `msg` of Pi's `done` report is the session's name once it has one (`/name`), so it is
    /// never told as what a turn said: not for a last message with no text, not for a manual
    /// `/compact`, which settles with no run, and not where the extension's hook failed.
    fn report_msg_is_turn_message(&self) -> bool {
        false
    }

    /// What the settled run said, which may be nothing: that ends the turn end's wait all the same.
    fn turn_message(&self, event: &str, payload: &Value) -> Option<TurnMessage> {
        (event == AGENT_SETTLED).then(|| {
            TurnMessage::Said(
                payload
                    .get(LAST_ASSISTANT_MESSAGE)
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::hooks::{HookInterpreter, TurnMessage};
    use crate::protocol::AgentId;

    /// What the extension forwarded as Pi 1.5.2 settled against a stand-in model
    /// (`testdata/pi-captures.md`): in a session named `Fix - the build`, after a turn that
    /// answered and after a request the model refused; and after a turn whose answer was empty.
    const SETTLED: &str = include_str!("../../../testdata/pi_settled_hooks.jsonl");

    /// The message the hooks read from an `agent_settled` payload: `None` where the event gives
    /// no word of the turn, `Some(None)` where it says the turn said nothing.
    fn said(event: &str, payload: &Value) -> Option<Option<String>> {
        match super::PiHooks.turn_message(event, payload)? {
            TurnMessage::Said(message) => Some(message),
            TurnMessage::Read(_) => panic!("Pi's message comes with the event"),
        }
    }

    /// A turn's message is what the extension forwards as Pi settles, not the `msg` of Pi's
    /// `done` report, which is the session's name: the model's last text, the error a refused
    /// request ended on, and nothing for a turn that said nothing, each of which ends the wait.
    #[test]
    fn the_turns_message_is_what_the_extension_forwards_as_pi_settles() {
        assert!(super::PiHooks.supplies_turn_message());
        let payloads: Vec<Value> = SETTLED
            .lines()
            .map(|line| serde_json::from_str(line).expect("a payload"))
            .collect();
        let messages: Vec<_> = payloads
            .iter()
            .map(|payload| {
                let event = crate::hooks::event_name(payload).expect("an event name");
                said(&event, payload).expect("a settled run gives its word")
            })
            .collect();
        assert_eq!(
            messages,
            [
                Some("ANSWER[9 users; last=named plain]".to_string()),
                Some(
                    r#"400: {"message":"standin refuses: bad request","type":"invalid_request_error"}"#
                        .to_string()
                ),
                None,
            ]
        );
        assert!(
            messages
                .iter()
                .flatten()
                .all(|message| !message.contains("Fix - the build")),
            "the session's name is no message"
        );
    }

    /// The name of the session the first two payloads of [`SETTLED`] were captured in.
    const NAME: &str = "Fix - the build";

    /// A status report as Pi makes it in a session named [`NAME`]: the root record, `app=pi`, and
    /// the name as its `msg`.
    fn named_report(state: &str) -> Vec<u8> {
        use base64::Engine;
        let msg = base64::engine::general_purpose::STANDARD.encode(NAME);
        format!("\x1b]7501;state={state}:app=pi:msg={msg}\x1b\\").into_bytes()
    }

    /// The `index`th payload of [`SETTLED`], read as the daemon reads the event the extension
    /// forwarded.
    fn settled(index: usize) -> TurnMessage {
        let payload: Value =
            serde_json::from_str(SETTLED.lines().nth(index).expect("a line")).expect("a payload");
        let event = crate::hooks::event_name(&payload).expect("an event name");
        super::PiHooks
            .turn_message(&event, &payload)
            .expect("a settled run gives its word")
    }

    /// A Pi session bound to the owner `hub`, at its prompt with a process whose reports give it
    /// its status, as Pi's do.
    fn pi_child(
        name: &str,
    ) -> (
        std::sync::Arc<crate::state::AppState>,
        crate::test_support::ScratchDir,
        crate::test_support::StandIn,
        crate::test_support::StandIn,
    ) {
        use crate::protocol::SessionStatus;
        let (state, dir, hub) = crate::test_support::owner_team::team_state(
            name,
            &[("s", "hub", false, SessionStatus::Idle)],
        );
        let mut record = state.session_record("s").unwrap();
        record.agent = AgentId::PI;
        record.has_conversation = false;
        state.store.update_session(&record).unwrap();
        let live = crate::test_support::fake_live_session("s", AgentId::PI, 80, 24, "sleep 30");
        state.register_live(live.clone());
        (state, dir, hub, live)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_launch_hook_drives_the_same_working_waiting_and_cancel_states() {
        use crate::hooks::queue::QueuedHook;
        use crate::protocol::SessionStatus;
        let (state, _dir, _hub, live) = pi_child("pi-hook-status");
        for (report, expected) in [
            ("working", SessionStatus::Working),
            ("blocked", SessionStatus::WaitingUser),
            ("working", SessionStatus::Working),
            ("idle", SessionStatus::Idle),
        ] {
            let payload = json!({"hook_event_name": "agent_status", "turn_status": report});
            state.hook_queues.push(
                "s",
                QueuedHook {
                    body: payload.to_string().into(),
                    launch: Some(live.launch_id.clone()),
                    event: None,
                },
            );
            crate::hooks::apply::drain_hook_queue(&state, "s");
            live.settle_program_status().await;
            assert_eq!(state.session_record("s").unwrap().status, expected);
        }
        assert!(!state.turn_is_open("s"));
        assert!(state.session_record("s").unwrap().paused);
    }

    /// Pi's `working` can be applied before the opening prompt the extension forwarded, since the
    /// two are applied on tasks of their own: the prompt is then that turn's own, and decides the
    /// credit its start spent rather than joining the turn as a steer.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_opening_prompt_applied_after_working_decides_its_turns_credit() {
        use crate::state::TurnClose;
        let (state, _dir, _hub, live) = pi_child("pi-late-opening-prompt");
        let prompt = |text: &str| {
            let payload = json!({"hook_event_name": "message_start", "prompt": text});
            state
                .hook_queues
                .push("s", bytes::Bytes::from(payload.to_string()));
            crate::hooks::apply::drain_hook_queue(&state, "s");
        };
        state.credit_turn_opened_by("s", "Message from Octoboard".to_string());

        live.on_output(&named_report("working"));
        live.settle_program_status().await;
        prompt("typed by the user");
        assert!(state.turn_is_open("s"));
        assert_eq!(state.close_turn("s", false), TurnClose::OwesReport);

        // The credit the user's turn spent on starting went back, for the turn it is for.
        live.on_output(&named_report("done"));
        live.on_output(&named_report("working"));
        live.settle_program_status().await;
        prompt("Message from Octoboard about it");
        assert_eq!(state.close_turn("s", false), TurnClose::Reported);
    }

    /// The `msg` of a named session's `done` report is the session's name, so a turn the extension
    /// said had no message ends with none, and the owner is not told the name as what the turn
    /// said. That one payload is what both a last assistant message with no text and a manual
    /// `/compact`, which settles with no run before it, forward.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_named_sessions_turn_given_no_message_is_not_told_as_the_sessions_name() {
        use crate::test_support::owner_team::hub_heard_of;
        assert!(!super::PiHooks.report_msg_is_turn_message());

        let (_state, _dir, hub, live) = pi_child("pi-named-turn-said-nothing");
        live.on_output(&named_report("working"));
        live.settle_program_status().await;
        live.supply_turn_message(settled(2));
        live.on_output(&named_report("done"));
        let heard = hub_heard_of(&hub, "it said nothing").await;
        assert!(heard.contains("it said nothing"), "{heard}");
        assert!(!heard.contains(NAME), "{heard}");
    }

    /// A turn whose hook failed is given nothing at all: once the wait for its message is over it
    /// ends with none, and the owner is not told the session's name as what it said.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_named_sessions_turn_whose_hook_failed_is_not_told_as_the_sessions_name() {
        use crate::test_support::owner_team::hub_heard_of;
        let (_state, _dir, hub, live) = pi_child("pi-named-turn-hook-failed");
        let _bound =
            crate::session::shorten_turn_message_bound(std::time::Duration::from_millis(50));
        live.on_output(&named_report("working"));
        live.settle_program_status().await;
        live.on_output(&named_report("done"));
        let heard = hub_heard_of(&hub, "it said nothing").await;
        assert!(heard.contains("it said nothing"), "{heard}");
        assert!(!heard.contains(NAME), "{heard}");
    }

    /// What the extension forwards for a turn that answered is the turn's message in a named
    /// session too, with the name nowhere in what the owner is told.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_named_sessions_turn_is_told_by_what_the_extension_forwarded() {
        use crate::test_support::owner_team::hub_heard_of;
        let (_state, _dir, hub, live) = pi_child("pi-named-turn-answered");
        live.on_output(&named_report("working"));
        live.settle_program_status().await;
        live.supply_turn_message(settled(0));
        live.on_output(&named_report("done"));
        let heard = hub_heard_of(&hub, "ANSWER[9 users; last=named plain]").await;
        assert!(
            heard.contains("ANSWER[9 users; last=named plain]"),
            "{heard}"
        );
        assert!(!heard.contains(NAME), "{heard}");
    }

    /// The event that names the session gives no word of a turn, whatever it carries.
    #[test]
    fn a_session_start_gives_no_turn_message() {
        for payload in [
            json!({"hook_event_name": "session_start", "session_id": "0199", "reason": "saved"}),
            json!({"hook_event_name": "session_start", "last_assistant_message": "text"}),
        ] {
            assert!(said("session_start", &payload).is_none(), "{payload}");
        }
        assert!(said("agent_end", &json!({"last_assistant_message": "text"})).is_none());
    }

    /// Pi says its session has started with the extension's `session_start` and no other event.
    #[test]
    fn only_session_start_announces_the_session_start() {
        crate::adapter::tests::only_announces_session_start(AgentId::PI, "session_start");
    }

    /// The extension names the session only where Pi has written its file, so a payload may carry
    /// the id or leave it out, and an empty one is no id.
    #[test]
    fn the_session_id_is_the_one_the_extension_reports_when_it_reports_one() {
        let hooks = super::PiHooks;
        assert_eq!(
            hooks.agent_session_id(&json!({
                "hook_event_name": "session_start",
                "session_id": "0199-pi-session",
                "reason": "startup",
            })),
            Some("0199-pi-session".to_string())
        );
        for payload in [
            json!({"hook_event_name": "session_start", "reason": "new"}),
            json!({"hook_event_name": "session_start", "session_id": "", "reason": "new"}),
            json!({"hook_event_name": "session_start", "session_id": null}),
            serde_json::Value::Null,
        ] {
            assert_eq!(hooks.agent_session_id(&payload), None, "{payload}");
        }
    }

    /// What the extension forwarded for each user message of one run of Pi 1.5.2 against a
    /// stand-in model (`testdata/pi-captures.md`): two prompts, a message of one image alone, one
    /// of two images and text, a prompt and the steering message Pi took into its run.
    const PROMPTS: &str = include_str!("../../../testdata/pi_prompt_hooks.jsonl");

    /// Each user message that entered the conversation is a prompt submitted, with the text the
    /// model was given: a steering message too, and an empty text for a message of images alone.
    #[test]
    fn each_prompt_the_extension_forwards_is_a_submitted_prompt() {
        let hooks = super::PiHooks;
        let prompts: Vec<Option<String>> = PROMPTS
            .lines()
            .map(|line| {
                let payload: Value = serde_json::from_str(line).expect("a payload");
                let event = crate::hooks::event_name(&payload).expect("an event name");
                assert!(hooks.starts_conversation(&event, &payload), "{line}");
                hooks.submitted_prompt(&payload).map(str::to_owned)
            })
            .collect();
        assert_eq!(
            prompts,
            [
                "CMD:two /private/tmp/nap25/other/x5.txt",
                "CMD:edit ~/../other/y.txt",
                "",
                "two images",
                "SLOW first",
                "steer me",
            ]
            .map(|prompt| Some(prompt.to_string()))
        );
    }

    /// No other event the extension forwards is a prompt, whatever it carries.
    #[test]
    fn only_a_message_start_is_a_submitted_prompt() {
        let hooks = super::PiHooks;
        for (event, payload) in [
            (
                "session_start",
                json!({"hook_event_name": "session_start", "prompt": "x"}),
            ),
            (
                "agent_settled",
                json!({"hook_event_name": "agent_settled", "prompt": "x"}),
            ),
            ("input", json!({"hook_event_name": "input", "prompt": "x"})),
        ] {
            assert!(!hooks.starts_conversation(event, &payload), "{event}");
        }
        assert_eq!(hooks.submitted_prompt(&json!({"prompt": 7})), None);
        assert_eq!(hooks.submitted_prompt(&Value::Null), None);
    }

    /// A run's opening prompt can still be applied after the `working` that opened its turn, so
    /// it is that turn's own, not a steer.
    #[test]
    fn an_opening_prompt_may_follow_its_turns_start() {
        assert!(super::PiHooks.opening_prompt_may_follow_start());
    }

    /// What the extension asked the guard in that run: a `write` into another project, a `write`
    /// in the session's own, and an `edit` given as `~/../other/y.txt`, which the extension
    /// resolved as Pi does.
    const GUARD_PAYLOADS: &str = include_str!("../../../testdata/pi_guard_payloads.jsonl");

    /// The guard is given the file each `write` or `edit` call names, with the working directory.
    #[test]
    fn the_guard_reads_the_file_each_write_or_edit_names() {
        let hooks = super::PiHooks;
        let read: Vec<_> = GUARD_PAYLOADS
            .lines()
            .map(|line| hooks.edit_targets(&serde_json::from_str(line).expect("a payload")))
            .collect();
        let cwd = Some(std::path::PathBuf::from("/private/tmp/nap25/proj"));
        assert_eq!(
            read,
            [
                (
                    vec!["/private/tmp/nap25/other/x5.txt".to_string()],
                    cwd.clone()
                ),
                (
                    vec!["/private/tmp/nap25/proj/local.txt".to_string()],
                    cwd.clone()
                ),
                (vec!["/private/tmp/nap25/other/y.txt".to_string()], cwd),
            ]
        );
    }

    /// A call of any other tool, or one with no path, names no file to guard.
    #[test]
    fn other_calls_name_no_file() {
        let hooks = super::PiHooks;
        for payload in [
            json!({"tool_name": "bash", "tool_input": {"path": "/x"}, "cwd": "/w"}),
            json!({"tool_name": "read", "tool_input": {"path": "/x"}, "cwd": "/w"}),
            json!({"tool_name": "write", "tool_input": {"path": ""}, "cwd": "/w"}),
            json!({"tool_name": "write", "tool_input": {"file_path": "/x"}, "cwd": "/w"}),
            json!({"tool_name": "edit", "cwd": "/w"}),
            Value::Null,
        ] {
            assert!(hooks.edit_targets(&payload).0.is_empty(), "{payload}");
        }
    }

    /// The refusal is Pi's own `tool_call` answer, with the guard's reason as it is.
    #[test]
    fn a_refusal_is_pis_own_block_answer() {
        assert_eq!(
            super::PiHooks.deny_edit("Blocked: \"other\""),
            Some(json!({"block": true, "reason": "Blocked: \"other\""}))
        );
    }
}
