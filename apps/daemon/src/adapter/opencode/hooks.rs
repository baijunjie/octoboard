//! How the events of OpenCode's plugin are read.
//!
//! OpenCode has no hooks: Octoboard's plugin (`plugin.js` beside this file) runs inside its
//! process and forwards a few of its bus events through the session's hook script. The payloads
//! are therefore Octoboard's own format, not OpenCode's, and the plugin has already done what only
//! it can: it reports a turn's start once, on the first `busy` of a session that was idle, with
//! the prompt of the message that started it; it reports the idle that ends a turn with the text
//! of the turn's last answer, and drops the idle OpenCode repeats after an error; it tells an
//! interrupt (`turn.cancelled`) from a failure (`session.error`), which OpenCode reports as the
//! same event; it marks the events of a child session as a subagent's; and it reports a
//! subagent's permission prompt or question under the session's own id, since the same user
//! answers it in the same terminal; it reports a prompt only once it has stayed open a moment, so
//! one OpenCode answers itself under `--auto` raises no hand; it forwards an answer to one only
//! while the session's turn is open, so an answer never puts a session whose turn is over back at
//! work; and of the conversations one terminal can hold (`/new`, the session list) it forwards
//! the events of the one a prompt was last submitted in, which is the one the terminal shows.
//!
//! OpenCode creates a session only at its first prompt, so no event says the agent has started:
//! that is read off its terminal title (`screen.rs`).

use serde_json::Value;

use crate::hooks::{text_field, CallShape, HookInterpreter, ToolCallChange, TurnEnd, TurnOutcome};
use crate::protocol::SessionStatus;

/// A root session exists: the first event of a conversation, which carries its id and says
/// nothing else.
#[cfg(test)]
const SESSION_CREATED: &str = "session.created";
const TURN_STARTED: &str = "turn.started";
const TURN_CANCELLED: &str = "turn.cancelled";
const SESSION_IDLE: &str = "session.idle";
const SESSION_ERROR: &str = "session.error";
const PERMISSION_ASKED: &str = "permission.asked";
const PERMISSION_REPLIED: &str = "permission.replied";
const QUESTION_ASKED: &str = "question.asked";
const QUESTION_REPLIED: &str = "question.replied";
const QUESTION_REJECTED: &str = "question.rejected";

pub(super) struct OpenCodeHooks;

/// The tools that write a file, each with the field of its arguments that names what it writes
/// (1.19.6): `write` and `edit` a path, `apply_patch`, which a GPT model gets in their place, a
/// patch whose headers name the files. The plugin asks the cross-project guard about a call of
/// these alone (`plugin.js`, `tool.execute.before`).
pub(super) const EDIT_TOOLS: [(&str, &str); 3] = [
    ("write", "filePath"),
    ("edit", "filePath"),
    ("apply_patch", "patchText"),
];
/// The tool whose argument is a patch rather than a path.
const PATCH_TOOL: &str = "apply_patch";

/// The id of the permission prompt or question the payload is about.
fn request_id(payload: &Value) -> Option<&str> {
    payload
        .get("request_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

impl HookInterpreter for OpenCodeHooks {
    /// The files a call the plugin asks the cross-project guard about would write. The payload is
    /// the plugin's own, `{format, tool, args, cwd}`: the tool's name and arguments as OpenCode
    /// hands them to `tool.execute.before`, and the session's directory, which a relative path is
    /// taken from.
    fn edit_targets(&self, payload: &Value) -> (Vec<String>, Option<std::path::PathBuf>) {
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .filter(|cwd| !cwd.is_empty())
            .map(std::path::PathBuf::from);
        let tool = payload.get("tool").and_then(Value::as_str);
        let Some((name, field)) = EDIT_TOOLS.iter().find(|(name, _)| Some(*name) == tool) else {
            return (Vec::new(), cwd);
        };
        let Some(value) = payload
            .get("args")
            .and_then(|args| args.get(field))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        else {
            return (Vec::new(), cwd);
        };
        let targets = if *name == PATCH_TOOL {
            crate::adapter::patch::patch_targets(value)
        } else {
            vec![value.to_owned()]
        };
        (targets, cwd)
    }

    /// The refusal the plugin reads: it throws an error of `reason` from `tool.execute.before`,
    /// which refuses the call and hands the model the reason as the tool's result (1.19.6).
    fn deny_edit(&self, reason: &str) -> Option<Value> {
        Some(serde_json::json!({ "decision": "deny", "reason": reason }))
    }

    /// OpenCode shows no confirmation at start, and its plugin reports nothing before the first
    /// prompt.
    fn past_startup(&self, _event: &str, _payload: &Value) -> bool {
        true
    }

    fn starts_conversation(&self, event: &str, _payload: &Value) -> bool {
        event == TURN_STARTED
    }

    fn status(&self, event: &str, _payload: &Value) -> Option<SessionStatus> {
        match event {
            TURN_STARTED | PERMISSION_REPLIED | QUESTION_REPLIED | QUESTION_REJECTED => {
                Some(SessionStatus::Working)
            }
            PERMISSION_ASKED | QUESTION_ASKED => Some(SessionStatus::WaitingUser),
            SESSION_IDLE | SESSION_ERROR | TURN_CANCELLED => Some(SessionStatus::Idle),
            // `session.created` comes with the first prompt, just before that turn's start.
            _ => None,
        }
    }

    /// The id of the message that started the turn.
    fn turn_key(&self, payload: &Value) -> Option<String> {
        let key = payload.get("turn_id").and_then(Value::as_str)?;
        (!key.is_empty()).then(|| key.to_string())
    }

    /// The text of the message that started the turn, which OpenCode (1.19.6) records as typed.
    fn submitted_prompt<'a>(&self, payload: &'a Value) -> Option<&'a str> {
        payload.get("prompt")?.as_str()
    }

    fn is_subagent(&self, payload: &Value) -> bool {
        payload.get("subagent").and_then(Value::as_bool) == Some(true)
    }

    /// The session a turn's start names. OpenCode (1.19.6) has no event for the user moving to
    /// another conversation in the same terminal (`/new`, the session list); a prompt can only
    /// be submitted in the conversation shown, so the id recorded moves with a turn's start and
    /// with nothing else: not with a session's creation, nor with the end of a turn.
    fn agent_session_id(&self, payload: &Value) -> Option<String> {
        (crate::hooks::event_name(payload).as_deref() == Some(TURN_STARTED))
            .then(|| crate::hooks::agent_session_id(payload))
            .flatten()
    }

    fn turn_cancelled(&self, event: &str) -> bool {
        event == TURN_CANCELLED
    }

    fn turn_outcome(&self, event: &str, _payload: &Value) -> Option<TurnOutcome> {
        match event {
            TURN_CANCELLED | SESSION_ERROR => Some(TurnOutcome::Abnormal),
            SESSION_IDLE => Some(TurnOutcome::Normal),
            _ => None,
        }
    }

    fn turn_end(&self, event: &str, payload: &Value) -> Option<TurnEnd> {
        match event {
            SESSION_IDLE => Some(TurnEnd {
                last_assistant_message: text_field(payload, "last_assistant_message"),
                failed: false,
                backstop: false,
            }),
            SESSION_ERROR => Some(TurnEnd {
                last_assistant_message: text_field(payload, "error"),
                failed: true,
                backstop: false,
            }),
            _ => None,
        }
    }

    /// A permission prompt or a question waits on the user from the moment it is asked, and its
    /// reply names it, so one prompt answered leaves the hand up for another still open.
    fn tool_call_change<'a>(&self, event: &str, payload: &'a Value) -> Option<ToolCallChange<'a>> {
        match event {
            PERMISSION_ASKED | QUESTION_ASKED => Some(ToolCallChange::Started {
                id: request_id(payload),
                call: CallShape {
                    tool: payload.get("tool").and_then(Value::as_str),
                    input: None,
                },
                waits_on_user: true,
            }),
            PERMISSION_REPLIED | QUESTION_REPLIED | QUESTION_REJECTED => {
                Some(ToolCallChange::Ended {
                    id: request_id(payload),
                })
            }
            SESSION_IDLE | SESSION_ERROR | TURN_CANCELLED => Some(ToolCallChange::AllEnded),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The interpreter names the events the plugin forwards, so the two files cannot drift apart
    /// unnoticed: every event name read here is one the plugin's source writes.
    #[test]
    fn every_event_read_is_one_the_plugin_forwards() {
        let plugin = include_str!("plugin.js");
        for event in [
            SESSION_CREATED,
            TURN_STARTED,
            TURN_CANCELLED,
            SESSION_IDLE,
            SESSION_ERROR,
            PERMISSION_ASKED,
            PERMISSION_REPLIED,
            QUESTION_ASKED,
            QUESTION_REPLIED,
            QUESTION_REJECTED,
        ] {
            assert!(plugin.contains(&format!("\"{event}\"")), "{event}");
        }
    }

    /// Every event name the plugin forwards.
    const ALL_EVENTS: [&str; 10] = [
        SESSION_CREATED,
        TURN_STARTED,
        TURN_CANCELLED,
        SESSION_IDLE,
        SESSION_ERROR,
        PERMISSION_ASKED,
        PERMISSION_REPLIED,
        QUESTION_ASKED,
        QUESTION_REPLIED,
        QUESTION_REJECTED,
    ];

    /// What the committed plugin forwarded under OpenCode 1.19.6, one payload a line
    /// (`testdata/opencode-captures.md`, "Hook payloads"): seven launches in a row, each a new
    /// session.
    const CAPTURE: &str = include_str!("../../../testdata/opencode_hook_payloads_1.19.6.jsonl");

    const ANSWERED: usize = 0;
    const PERMISSION_ALLOWED: usize = 1;
    const PERMISSION_REJECTED: usize = 2;
    const QUESTION_ANSWERED: usize = 3;
    const SUBAGENT: usize = 4;
    const CANCELLED: usize = 5;
    const FAILED: usize = 6;

    /// The payloads of the capture, one list a launch: a root session's `session.created` starts the
    /// next.
    fn scenarios() -> Vec<Vec<Value>> {
        let mut scenarios: Vec<Vec<Value>> = Vec::new();
        for line in CAPTURE.lines() {
            let payload: Value = serde_json::from_str(line).expect("a payload");
            let creates_root = crate::hooks::event_name(&payload).as_deref()
                == Some(SESSION_CREATED)
                && !OpenCodeHooks.is_subagent(&payload);
            if creates_root {
                scenarios.push(Vec::new());
            }
            scenarios
                .last_mut()
                .expect("the first line creates")
                .push(payload);
        }
        scenarios
    }

    fn name(payload: &Value) -> String {
        crate::hooks::event_name(payload).expect("every payload names its event")
    }

    fn statuses_of(scenario: &[Value]) -> Vec<Option<SessionStatus>> {
        scenario
            .iter()
            .map(|payload| OpenCodeHooks.status(&name(payload), payload))
            .collect()
    }

    fn root_lines(scenario: &[Value]) -> impl Iterator<Item = &Value> {
        scenario
            .iter()
            .filter(|payload| !OpenCodeHooks.is_subagent(payload))
    }

    #[test]
    fn the_capture_is_the_seven_launches_it_is_documented_to_be() {
        let scenarios = scenarios();
        assert_eq!(scenarios.len(), 7);
        assert_eq!(scenarios.iter().map(Vec::len).sum::<usize>(), 30);
        assert_eq!(
            scenarios.iter().map(Vec::len).collect::<Vec<_>>(),
            [3, 5, 5, 5, 6, 3, 3]
        );
    }

    /// Every launch's lines move the status as the turn does: the creation says nothing, the
    /// start is working, a prompt waits on the user and its answer puts the turn back to work,
    /// and the idle, a cancel or an error ends it.
    #[test]
    fn a_launchs_lines_move_the_status_as_the_turn_does() {
        use SessionStatus::{Idle, WaitingUser, Working};
        let scenarios = scenarios();
        let with_prompt = vec![
            None,
            Some(Working),
            Some(WaitingUser),
            Some(Working),
            Some(Idle),
        ];
        assert_eq!(
            statuses_of(&scenarios[ANSWERED]),
            [None, Some(Working), Some(Idle)]
        );
        assert_eq!(statuses_of(&scenarios[PERMISSION_ALLOWED]), with_prompt);
        assert_eq!(statuses_of(&scenarios[PERMISSION_REJECTED]), with_prompt);
        assert_eq!(statuses_of(&scenarios[QUESTION_ANSWERED]), with_prompt);
        assert_eq!(
            statuses_of(&scenarios[CANCELLED]),
            [None, Some(Working), Some(Idle)]
        );
        assert_eq!(
            statuses_of(&scenarios[FAILED]),
            [None, Some(Working), Some(Idle)]
        );
    }

    /// The events the capture does not hold, by their names alone: no payload is read for a
    /// status, and a reply or a dismissal puts a turn back to work.
    #[test]
    fn each_event_moves_the_status_as_the_turn_does() {
        let table = [
            (SESSION_CREATED, None),
            (TURN_STARTED, Some(SessionStatus::Working)),
            (PERMISSION_ASKED, Some(SessionStatus::WaitingUser)),
            (QUESTION_ASKED, Some(SessionStatus::WaitingUser)),
            (PERMISSION_REPLIED, Some(SessionStatus::Working)),
            (QUESTION_REPLIED, Some(SessionStatus::Working)),
            (QUESTION_REJECTED, Some(SessionStatus::Working)),
            (SESSION_IDLE, Some(SessionStatus::Idle)),
            (SESSION_ERROR, Some(SessionStatus::Idle)),
            (TURN_CANCELLED, Some(SessionStatus::Idle)),
            ("message.part.updated", None),
        ];
        for (event, status) in table {
            assert_eq!(OpenCodeHooks.status(event, &json!({})), status, "{event}");
        }
    }

    /// The conversation starts with a turn's start, which is the first `busy` of an idle session,
    /// and with no other line: not with the session's creation, which only names the session.
    #[test]
    fn only_a_turns_start_starts_the_conversation() {
        for scenario in scenarios() {
            for payload in root_lines(&scenario) {
                assert_eq!(
                    OpenCodeHooks.starts_conversation(&name(payload), payload),
                    name(payload) == TURN_STARTED,
                    "{payload}"
                );
            }
        }
        for event in ALL_EVENTS {
            assert_eq!(
                OpenCodeHooks.starts_conversation(event, &json!({})),
                event == TURN_STARTED,
                "{event}"
            );
        }
    }

    /// No event says the agent has started: its session starts at its first prompt, so the start is
    /// read off the terminal title and a hook announces none.
    #[test]
    fn no_event_announces_the_start_of_the_session() {
        for scenario in scenarios() {
            for payload in &scenario {
                assert!(!OpenCodeHooks.announces_session_start(&name(payload), payload));
            }
        }
    }

    /// The plugin reports nothing before the first prompt and OpenCode asks nothing at start, so
    /// every event is one past its startup.
    #[test]
    fn every_event_is_past_the_startup() {
        for event in ALL_EVENTS {
            assert!(OpenCodeHooks.past_startup(event, &json!({})), "{event}");
        }
    }

    /// The id of the session a payload is about, whatever its event.
    fn named(payload: &Value) -> Option<String> {
        crate::hooks::agent_session_id(payload)
    }

    /// Every line of a launch's root session is about the session it created, and only its
    /// turn's start names it as the session's own; a subagent's lines are about its child session.
    #[test]
    fn only_a_turns_start_names_the_session() {
        for scenario in scenarios() {
            let root = named(&scenario[0]).expect("the creation is about the session");
            assert!(root.starts_with("ses_"), "{root}");
            for payload in root_lines(&scenario) {
                assert_eq!(named(payload), Some(root.clone()));
                assert_eq!(
                    OpenCodeHooks.agent_session_id(payload),
                    (name(payload) == TURN_STARTED).then(|| root.clone()),
                    "{payload}"
                );
            }
            for payload in scenario.iter().filter(|p| OpenCodeHooks.is_subagent(p)) {
                assert_ne!(named(payload).expect("a child id"), root);
            }
        }
    }

    /// Hand-built, since every captured payload names its session: a payload without one, or with
    /// a blank one, names none, a turn's start included.
    #[test]
    fn a_payload_without_a_session_id_names_none() {
        for payload in [
            json!({ "format": 1, "hook_event_name": TURN_STARTED }),
            json!({ "hook_event_name": TURN_STARTED, "session_id": "" }),
            json!({ "session_id": "ses_1" }),
        ] {
            assert_eq!(OpenCodeHooks.agent_session_id(&payload), None, "{payload}");
        }
    }

    fn lines_of(capture: &str) -> Vec<Value> {
        capture
            .lines()
            .map(|line| serde_json::from_str(line).expect("a payload"))
            .collect()
    }

    fn names_of(lines: &[Value]) -> Vec<String> {
        lines.iter().map(name).collect()
    }

    /// The session ids a capture's lines would record, in order, each once per change.
    fn recorded(lines: &[Value]) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for id in lines
            .iter()
            .filter_map(|p| OpenCodeHooks.agent_session_id(p))
        {
            if ids.last() != Some(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// Under `--auto` with a rule that asks, OpenCode asked for two commands and answered both
    /// itself within the same moment (`testdata/opencode-captures.md`, "Sessions and prompts"):
    /// the plugin forwards no prompt, so no line reads as waiting on the user, and the two
    /// answers read as the turn working.
    #[test]
    fn a_prompt_opencode_answers_itself_waits_on_nobody() {
        let lines = lines_of(include_str!(
            "../../../testdata/opencode_hook_payloads_auto_1.19.6.jsonl"
        ));
        assert_eq!(
            names_of(&lines),
            [
                SESSION_CREATED,
                TURN_STARTED,
                PERMISSION_REPLIED,
                PERMISSION_REPLIED,
                SESSION_IDLE
            ]
        );
        use SessionStatus::{Idle, Working};
        assert_eq!(
            statuses_of(&lines),
            [
                None,
                Some(Working),
                Some(Working),
                Some(Working),
                Some(Idle)
            ]
        );
        for payload in &lines {
            let waits = matches!(
                OpenCodeHooks.tool_call_change(&name(payload), payload),
                Some(ToolCallChange::Started { .. })
            );
            assert!(!waits, "{payload}");
        }
    }

    /// A question and a permission prompt open at once, each answered by the user: both are told
    /// of, each waits on the user under its own id, and each answer names its own.
    #[test]
    fn two_prompts_open_at_once_each_wait_and_end_under_their_own_id() {
        let lines = lines_of(include_str!(
            "../../../testdata/opencode_hook_payloads_two_prompts_1.19.6.jsonl"
        ));
        assert_eq!(
            names_of(&lines),
            [
                SESSION_CREATED,
                TURN_STARTED,
                QUESTION_ASKED,
                PERMISSION_ASKED,
                PERMISSION_REPLIED,
                QUESTION_REPLIED,
                SESSION_IDLE
            ]
        );
        let id = |at: usize| request_id(&lines[at]).expect("a request id");
        assert_ne!(id(2), id(3));
        assert_eq!(id(4), id(3), "the permission is answered first");
        assert_eq!(id(5), id(2));
        for at in [2, 3] {
            assert!(matches!(
                OpenCodeHooks.tool_call_change(&name(&lines[at]), &lines[at]),
                Some(ToolCallChange::Started {
                    waits_on_user: true,
                    ..
                })
            ));
        }
    }

    /// `/new` while a turn runs, a prompt in the new conversation, then back to the first through
    /// the session list and a prompt there: the id recorded follows the prompts, first to second
    /// and back, and the first conversation's permission prompt and the end of its turn, which
    /// came while the terminal showed the second, are not among the lines.
    #[test]
    fn the_recorded_session_follows_the_conversation_a_prompt_is_submitted_in() {
        let lines = lines_of(include_str!(
            "../../../testdata/opencode_hook_payloads_new_session_1.19.6.jsonl"
        ));
        assert_eq!(
            names_of(&lines),
            [
                SESSION_CREATED,
                TURN_STARTED,
                SESSION_CREATED,
                TURN_STARTED,
                SESSION_IDLE,
                TURN_STARTED,
                SESSION_IDLE
            ]
        );
        let first = named(&lines[0]).expect("the first conversation");
        let second = named(&lines[2]).expect("the second conversation");
        assert_ne!(first, second);
        assert_eq!(
            recorded(&lines),
            [first.clone(), second.clone(), first.clone()]
        );
        // Every line between two starts is about the conversation of the start before it.
        assert_eq!(named(&lines[4]), Some(second));
        assert_eq!(named(&lines[6]), Some(first));
        assert_eq!(
            OpenCodeHooks.submitted_prompt(&lines[5]),
            Some("third hello")
        );
    }

    /// Back in a conversation whose turn still runs, by a prompt that joins that turn: the plugin
    /// reports the turn's start again, under the turn's own key and with the prompt just
    /// submitted, so the id recorded is the first conversation's again and its idle ends the turn.
    #[test]
    fn a_prompt_joining_a_turn_left_running_brings_the_session_back_to_it() {
        let lines = lines_of(include_str!(
            "../../../testdata/opencode_hook_payloads_back_in_a_session_1.19.6.jsonl"
        ));
        assert_eq!(
            names_of(&lines),
            [
                SESSION_CREATED,
                TURN_STARTED,
                SESSION_CREATED,
                TURN_STARTED,
                SESSION_IDLE,
                TURN_STARTED,
                SESSION_IDLE
            ]
        );
        let first = named(&lines[0]).expect("the first conversation");
        let second = named(&lines[2]).expect("the second conversation");
        assert_eq!(recorded(&lines), [first.clone(), second, first]);
        assert_eq!(
            OpenCodeHooks.turn_key(&lines[5]),
            OpenCodeHooks.turn_key(&lines[1]),
            "the turn it joined"
        );
        assert_eq!(
            OpenCodeHooks.turn_key(&lines[6]),
            OpenCodeHooks.turn_key(&lines[1])
        );
        assert_eq!(
            OpenCodeHooks.submitted_prompt(&lines[5]),
            Some("joining now")
        );
    }

    /// A turn is keyed by the message that started it, the same at its start and at its end,
    /// however it ends.
    #[test]
    fn a_turn_is_keyed_by_the_message_that_started_it() {
        for scenario in scenarios() {
            let root: Vec<&Value> = root_lines(&scenario).collect();
            let started: Vec<_> = root
                .iter()
                .filter(|payload| name(payload) == TURN_STARTED)
                .collect();
            assert_eq!(started.len(), 1);
            let key = OpenCodeHooks.turn_key(started[0]).expect("a turn id");
            assert!(key.starts_with("msg_"), "{key}");
            let end = root.last().expect("lines");
            assert_eq!(OpenCodeHooks.turn_key(end), Some(key));
            assert_eq!(
                OpenCodeHooks.turn_key(root[0]),
                None,
                "the creation has none"
            );
        }
        assert_eq!(OpenCodeHooks.turn_key(&json!({ "turn_id": "" })), None);
        assert_eq!(OpenCodeHooks.turn_key(&json!({})), None);
    }

    /// The submitted prompt is the text the plugin read for the turn, as typed.
    #[test]
    fn the_submitted_prompt_is_the_text_the_plugin_read() {
        let scenarios = scenarios();
        let prompt_of = |at: usize| {
            let start = scenarios[at]
                .iter()
                .find(|p| name(p) == TURN_STARTED)
                .expect("a start");
            OpenCodeHooks.submitted_prompt(start)
        };
        assert_eq!(prompt_of(ANSWERED), Some("hello"));
        assert_eq!(prompt_of(PERMISSION_ALLOWED), Some("run BASH now"));
        assert_eq!(prompt_of(PERMISSION_REJECTED), Some("run BASH now"));
        assert_eq!(prompt_of(QUESTION_ANSWERED), Some("please ASKQ me"));
        assert_eq!(prompt_of(SUBAGENT), Some("do a SUBTASK"));
        assert_eq!(prompt_of(CANCELLED), Some("a SLOW one"));
        assert_eq!(prompt_of(FAILED), Some("please DENY this"));
        // The end of a turn carries no prompt; hand-built, a prompt that is no text is none.
        assert_eq!(
            OpenCodeHooks.submitted_prompt(&scenarios[ANSWERED][2]),
            None
        );
        assert_eq!(
            OpenCodeHooks.submitted_prompt(&json!({ "prompt": 3 })),
            None
        );
    }

    /// Only the lines the plugin marked as a subagent's are one, the child session's three; the
    /// marker is a boolean. The odd values are hand-built, the capture holding only `true`.
    #[test]
    fn only_a_payload_marked_as_a_subagents_is_one() {
        for (at, scenario) in scenarios().iter().enumerate() {
            let marked = scenario
                .iter()
                .filter(|p| OpenCodeHooks.is_subagent(p))
                .count();
            assert_eq!(marked, if at == SUBAGENT { 3 } else { 0 }, "launch {at}");
        }
        for payload in [
            json!({ "session_id": "ses_1", "subagent": false }),
            json!({ "session_id": "ses_1", "subagent": "true" }),
            json!({ "session_id": "ses_1", "subagent": null }),
        ] {
            assert!(!OpenCodeHooks.is_subagent(&payload), "{payload}");
        }
    }

    /// The subagent's own creation, start and idle are the child's, framed by the root's start and
    /// idle, and the root's turn is the one named by its own turn id.
    #[test]
    fn a_subagent_lies_inside_the_root_turn_and_has_its_own() {
        let scenario = &scenarios()[SUBAGENT];
        let events: Vec<(String, bool)> = scenario
            .iter()
            .map(|p| (name(p), OpenCodeHooks.is_subagent(p)))
            .collect();
        assert_eq!(
            events,
            [
                (SESSION_CREATED.to_string(), false),
                (TURN_STARTED.to_string(), false),
                (SESSION_CREATED.to_string(), true),
                (TURN_STARTED.to_string(), true),
                (SESSION_IDLE.to_string(), true),
                (SESSION_IDLE.to_string(), false),
            ]
        );
        assert_ne!(
            OpenCodeHooks.turn_key(&scenario[1]),
            OpenCodeHooks.turn_key(&scenario[3])
        );
    }

    /// Only the cancel line is a cancel.
    #[test]
    fn only_the_cancel_event_is_a_cancel() {
        for scenario in scenarios() {
            for payload in &scenario {
                assert_eq!(
                    OpenCodeHooks.turn_cancelled(&name(payload)),
                    name(payload) == TURN_CANCELLED
                );
            }
        }
        for event in ALL_EVENTS {
            assert_eq!(OpenCodeHooks.turn_cancelled(event), event == TURN_CANCELLED);
        }
    }

    /// A turn that ends in an idle ends normally; one cut short by an interrupt or a failure
    /// ends abnormally, which is what shows the session paused; anything else ends no turn.
    #[test]
    fn a_turn_ends_normally_in_an_idle_and_abnormally_in_a_cancel_or_an_error() {
        let scenarios = scenarios();
        let outcome = |payload: &Value| OpenCodeHooks.turn_outcome(&name(payload), payload);
        for at in [
            ANSWERED,
            PERMISSION_ALLOWED,
            PERMISSION_REJECTED,
            QUESTION_ANSWERED,
        ] {
            assert_eq!(
                outcome(scenarios[at].last().unwrap()),
                Some(TurnOutcome::Normal)
            );
        }
        assert_eq!(
            outcome(scenarios[CANCELLED].last().unwrap()),
            Some(TurnOutcome::Abnormal)
        );
        assert_eq!(
            outcome(scenarios[FAILED].last().unwrap()),
            Some(TurnOutcome::Abnormal)
        );
        for scenario in &scenarios {
            for payload in scenario {
                let ends = [SESSION_IDLE, TURN_CANCELLED, SESSION_ERROR];
                assert_eq!(
                    outcome(payload).is_some(),
                    ends.contains(&name(payload).as_str())
                );
            }
        }
    }

    /// The idle that ends a turn carries the turn's last answer as its report, none where the turn
    /// had no answer (a rejected permission); a failure carries the error's text, flagged as no
    /// model output; no other line ends a turn.
    #[test]
    fn the_end_of_a_turn_reports_the_last_answer_or_the_failure() {
        let scenarios = scenarios();
        let end = |at: usize| {
            let last = scenarios[at].last().unwrap();
            OpenCodeHooks
                .turn_end(&name(last), last)
                .expect("the turn ends")
        };
        for at in [ANSWERED, PERMISSION_ALLOWED, QUESTION_ANSWERED, SUBAGENT] {
            let end = end(at);
            assert_eq!(
                end.last_assistant_message.as_deref(),
                Some("Final answer from stub.")
            );
            assert!(!end.failed);
        }
        let rejected = end(PERMISSION_REJECTED);
        assert_eq!(rejected.last_assistant_message, None);
        assert!(!rejected.failed);

        let failed = end(FAILED);
        assert!(failed.failed);
        assert_eq!(
            failed.last_assistant_message.as_deref(),
            Some("stub says no")
        );

        for scenario in &scenarios {
            for payload in scenario {
                let ends = [SESSION_IDLE, SESSION_ERROR];
                assert_eq!(
                    OpenCodeHooks.turn_end(&name(payload), payload).is_some(),
                    ends.contains(&name(payload).as_str()),
                    "{payload}"
                );
            }
        }
        // Hand-built, the captured answers being whole: blank text is no answer, and text is trimmed.
        let idle = |text: &str| {
            OpenCodeHooks
                .turn_end(SESSION_IDLE, &json!({ "last_assistant_message": text }))
                .expect("an idle ends the turn")
                .last_assistant_message
        };
        assert_eq!(idle("  All done.\n").as_deref(), Some("All done."));
        assert_eq!(idle("  "), None);
    }

    /// A permission prompt and a question are calls that wait on the user from the moment they
    /// are asked, named by the request's id and by what is asked for; the reply names the same id.
    #[test]
    fn a_permission_or_a_question_waits_on_the_user_until_its_reply() {
        let scenarios = scenarios();
        for (at, tool) in [
            (PERMISSION_ALLOWED, "bash"),
            (PERMISSION_REJECTED, "bash"),
            (QUESTION_ANSWERED, "question"),
        ] {
            let scenario = &scenarios[at];
            let asked = &scenario[2];
            let replied = &scenario[3];
            let id = asked["request_id"].as_str().expect("a request id");
            assert_eq!(
                OpenCodeHooks.tool_call_change(&name(asked), asked),
                Some(ToolCallChange::Started {
                    id: Some(id),
                    call: CallShape {
                        tool: Some(tool),
                        input: None
                    },
                    waits_on_user: true,
                }),
                "launch {at}"
            );
            assert_eq!(
                OpenCodeHooks.tool_call_change(&name(replied), replied),
                Some(ToolCallChange::Ended { id: Some(id) }),
                "launch {at}"
            );
        }
    }

    /// Hand-built, the capture holding no dismissal and no request without an id: a dismissed
    /// question ends its call as a reply does, and a request with no id is still a call waiting
    /// on the user, only not one a reply can name.
    #[test]
    fn a_dismissed_question_and_a_request_without_an_id() {
        assert_eq!(
            OpenCodeHooks.tool_call_change(QUESTION_REJECTED, &json!({ "request_id": "que_1" })),
            Some(ToolCallChange::Ended { id: Some("que_1") })
        );
        for payload in [
            json!({ "tool": "edit" }),
            json!({ "request_id": "", "tool": "edit" }),
        ] {
            assert_eq!(
                OpenCodeHooks.tool_call_change(PERMISSION_ASKED, &payload),
                Some(ToolCallChange::Started {
                    id: None,
                    call: CallShape {
                        tool: Some("edit"),
                        input: None
                    },
                    waits_on_user: true,
                }),
                "{payload}"
            );
            assert_eq!(
                OpenCodeHooks.tool_call_change(PERMISSION_REPLIED, &payload),
                Some(ToolCallChange::Ended { id: None }),
                "{payload}"
            );
        }
    }

    /// The end of the turn, however it ends, ends every call of it; its start and creation touch
    /// none.
    #[test]
    fn the_end_of_a_turn_ends_every_call_of_it() {
        for scenario in scenarios() {
            for payload in &scenario {
                let change = OpenCodeHooks.tool_call_change(&name(payload), payload);
                match name(payload).as_str() {
                    SESSION_IDLE | SESSION_ERROR | TURN_CANCELLED => {
                        assert_eq!(change, Some(ToolCallChange::AllEnded), "{payload}")
                    }
                    SESSION_CREATED | TURN_STARTED => assert_eq!(change, None, "{payload}"),
                    _ => {}
                }
            }
        }
    }

    /// A guard payload as the plugin writes it, with a tool's arguments as OpenCode 1.19.6 handed
    /// them to `tool.execute.before` in the probe of this milestone.
    fn guard_payload(tool: &str, args: Value) -> Value {
        json!({ "format": 1, "tool": tool, "args": args, "cwd": "/work/project" })
    }

    /// `write` and `edit` name their file by `filePath`, `apply_patch` in the headers of its
    /// patch; the session's directory comes with them for a relative path.
    #[test]
    fn a_file_writing_call_names_its_files() {
        let cwd = Some(std::path::PathBuf::from("/work/project"));
        assert_eq!(
            OpenCodeHooks.edit_targets(&guard_payload(
                "write",
                json!({ "filePath": "/other/made.txt", "content": "made\n" })
            )),
            (vec!["/other/made.txt".to_string()], cwd.clone())
        );
        assert_eq!(
            OpenCodeHooks.edit_targets(&guard_payload(
                "edit",
                json!({ "filePath": "src/a.rs", "oldString": "old", "newString": "new" })
            )),
            (vec!["src/a.rs".to_string()], cwd.clone())
        );
        let patch = "*** Begin Patch\n*** Add File: /other/patched.txt\n+patched\n\
                     *** Update File: b.txt\n*** Move to: /other/c.txt\n\
                     *** Delete File: /other/d.txt\n*** End Patch";
        assert_eq!(
            OpenCodeHooks
                .edit_targets(&guard_payload("apply_patch", json!({ "patchText": patch }))),
            (
                vec![
                    "/other/patched.txt".to_string(),
                    "b.txt".to_string(),
                    "/other/c.txt".to_string(),
                    "/other/d.txt".to_string(),
                ],
                cwd
            )
        );
    }

    /// A call of another tool, or one whose arguments lack the field, names no file, so the guard
    /// lets it through.
    #[test]
    fn another_call_names_no_file() {
        for payload in [
            guard_payload("bash", json!({ "command": "echo > /other/x" })),
            guard_payload("read", json!({ "filePath": "/other/x" })),
            guard_payload("write", json!({ "content": "x" })),
            guard_payload("write", json!({ "filePath": "" })),
            guard_payload("apply_patch", json!({ "patchText": "no headers here" })),
            json!({ "format": 1 }),
        ] {
            assert!(
                OpenCodeHooks.edit_targets(&payload).0.is_empty(),
                "{payload}"
            );
        }
        assert_eq!(
            OpenCodeHooks.edit_targets(&json!({ "tool": "write" })).1,
            None
        );
    }

    /// The refusal is a decision the plugin reads, with the reason it throws.
    #[test]
    fn the_refusal_carries_its_reason() {
        assert_eq!(
            OpenCodeHooks.deny_edit("Blocked: not yours"),
            Some(json!({ "decision": "deny", "reason": "Blocked: not yours" }))
        );
    }
}
