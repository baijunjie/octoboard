//! How Qwen Code's hook events are read. Every field read here was measured on 0.28.0
//! (`docs/agent-cli-reference.md`, section "Qwen Code"; the payloads are in
//! `testdata/qwen_hooks.jsonl`). Its event names are Claude Code's, its payloads and their traps
//! are its own:
//!
//! - **`UserPromptSubmit` fires again inside a turn**, with an empty `prompt`, each time the model
//!   is called back with a tool's result. That one is no prompt: it starts no turn
//!   ([`HookInterpreter::starts_conversation`]) and brings no status.
//! - **A wait on the user is not read from hooks.** `PermissionRequest` does fire, for a
//!   permission prompt and for `ask_user_question` alike, but declining either fires nothing a
//!   turn's end could be told by, so a hand raised by a hook would stay up. Qwen Code's `blocked`
//!   reports say the wait and the report after it its end (`AgentProvider::reported_signals`),
//!   its title's glyph where no report was made (`super::screen`), and the hooks keep saying
//!   `Working` under it.
//! - **`Notification`'s `idle_prompt` says nothing.** It fires after a turn, and also right after
//!   a `PreToolUse` while the turn runs, with and without a `prompt_id` in both places.
//! - **A cancelled turn fires no hook**, and neither does a declined prompt. A cancel is closed
//!   at once by Qwen Code's own report of it (`idle` after `working`, declared in
//!   `AgentProvider::reported_signals`). A declined prompt is reported `done`, as the gap between
//!   two model calls is, so it is closed from the terminal's activity (the reports, else the
//!   glyph of the title, `ScreenReader::title_cancel`) having stayed at the prompt, by the watch
//!   every move into `Working` arms ([`HookInterpreter::record_watch`]).
//! - **No tool call is reported open** (`HookInterpreter::tool_call_change`): a declined
//!   `ask_user_question` fires its `PreToolUse` and no `PostToolUse`, and a call left open would
//!   keep that watch from ever closing the turn.
//! - **A turn the agent starts itself**, on a background agent's completion, fires no
//!   `UserPromptSubmit`: only its `Stop`, under a `prompt_id` of its own.
//! - `Stop` and `SubagentStop` list `background_tasks` with a `status` each, finished ones
//!   included; only the running ones are work left running.

use serde_json::Value;

use crate::hooks::{
    any_field_set, text_field, BackgroundTasksChange, HookInterpreter, RecordWatch, TurnEnd,
    TurnOutcome,
};
use crate::protocol::{BackgroundTask, SessionStatus};

pub(super) struct QwenHooks;

/// Whether the payload is the `UserPromptSubmit` Qwen Code fires when it calls the model back
/// inside a turn: one whose `prompt` is empty.
fn is_turn_continuation(payload: &Value) -> bool {
    payload.get("hook_event_name").and_then(Value::as_str) == Some("UserPromptSubmit")
        && payload
            .get("prompt")
            .and_then(Value::as_str)
            .is_some_and(str::is_empty)
}

impl HookInterpreter for QwenHooks {
    fn context_report(
        &self,
        event: &str,
        payload: &Value,
    ) -> Option<crate::records::context::ContextReport> {
        (event == "Stop")
            .then(|| payload["context_limit"].as_u64())
            .flatten()
            .filter(|window| *window > 0)
            .map(crate::records::context::ContextReport::Window)
    }

    /// The file a file-writing call (`edit`, `write_file`: `file_path`; `notebook_edit`:
    /// `notebook_path`, 0.28.3's tool schemas) would write, with the payload's working directory.
    fn edit_targets(&self, payload: &Value) -> (Vec<String>, Option<std::path::PathBuf>) {
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(std::path::PathBuf::from);
        let Some(input) = payload.get("tool_input") else {
            return (Vec::new(), cwd);
        };
        let paths = ["file_path", "notebook_path"]
            .into_iter()
            .filter_map(|key| input.get(key)?.as_str())
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .collect();
        (paths, cwd)
    }

    /// The refusal a `PreToolUse` hook prints: measured on 0.28.3, the call is refused with no
    /// prompt, its row shows the reason, and the model is called again with the refusal.
    fn deny_edit(&self, reason: &str) -> Option<Value> {
        Some(serde_json::json!({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }}))
    }

    /// `SessionStart`, which runs before the trust dialog is answered, is not registered: the first
    /// hook comes with the first prompt.
    fn past_startup(&self, _event: &str, _payload: &Value) -> bool {
        true
    }

    /// A `UserPromptSubmit` that submits a prompt: the one fired for a model call inside a turn
    /// starts nothing.
    fn starts_conversation(&self, event: &str, payload: &Value) -> bool {
        event == "UserPromptSubmit" && !is_turn_continuation(payload)
    }

    fn status(&self, event: &str, payload: &Value) -> Option<SessionStatus> {
        match event {
            // Inside a turn, where it says nothing new: a hand the reports raised stays as it is.
            "UserPromptSubmit" if is_turn_continuation(payload) => None,
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => {
                Some(SessionStatus::Working)
            }
            // Idle even when held on background work: the tasks are tracked beside the status.
            "Stop" | "StopFailure" => Some(SessionStatus::Idle),
            _ => None,
        }
    }

    /// Its `prompt_id`, `<session id>########<n>`, the same for every event of a turn.
    fn turn_key(&self, payload: &Value) -> Option<String> {
        text_field(payload, "prompt_id")
    }

    /// The `prompt` as submitted; a paste arrives as its text, with no wrapper.
    fn submitted_prompt<'a>(&self, payload: &'a Value) -> Option<&'a str> {
        payload
            .get("prompt")?
            .as_str()
            .filter(|prompt| !prompt.is_empty())
    }

    /// A subagent's events carry its `agent_id` and `agent_type`.
    fn is_subagent(&self, payload: &Value) -> bool {
        any_field_set(payload, &["agent_id", "agent_type"])
    }

    fn agent_session_id(&self, payload: &Value) -> Option<String> {
        crate::hooks::agent_session_id(payload)
    }

    fn turn_outcome(&self, event: &str, _payload: &Value) -> Option<TurnOutcome> {
        match event {
            "StopFailure" => Some(TurnOutcome::Abnormal),
            "Stop" => Some(TurnOutcome::Normal),
            _ => None,
        }
    }

    fn turn_end(&self, event: &str, payload: &Value) -> Option<TurnEnd> {
        match event {
            // Held on work still running: its completion wakes a turn of the agent's own, whose
            // `Stop` is the one to report.
            "Stop" if !running_tasks(payload).is_empty() => None,
            "Stop" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "last_assistant_message"),
                failed: false,
                backstop: false,
            }),
            // Its `last_assistant_message` is the error as shown to the user.
            "StopFailure" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "last_assistant_message"),
                failed: true,
                backstop: false,
            }),
            _ => None,
        }
    }

    fn background_tasks_change(
        &self,
        event: &str,
        payload: &Value,
    ) -> Option<BackgroundTasksChange> {
        match event {
            "SubagentStart" => Some(BackgroundTasksChange::SubagentStarted(text_field(
                payload, "agent_id",
            )?)),
            "SubagentStop" => Some(BackgroundTasksChange::SubagentStopped {
                id: text_field(payload, "agent_id")?,
                tasks: running_tasks(payload),
            }),
            // Only the session's own: this is also asked of a subagent's payloads.
            "Stop" if !self.is_subagent(payload) => {
                Some(BackgroundTasksChange::Replace(running_tasks(payload)))
            }
            _ => None,
        }
    }

    fn watch_reads_previous_status(&self) -> bool {
        true
    }

    /// From the prompt the terminal's activity is watched for the turn going back to its prompt
    /// with no hook: a prompt declined, or a cancel where Qwen Code made no report of it. A wait on
    /// the user is read off the reports and changes no status here, so the one watch lasts the
    /// turn. It is armed again by an event that finds the session out of `Working` inside a turn,
    /// which is a turn the watch closed and that went on after all.
    fn record_watch<'a>(
        &self,
        event: &str,
        payload: &'a Value,
        previous: Option<SessionStatus>,
        status: SessionStatus,
    ) -> Option<RecordWatch<'a>> {
        (status == SessionStatus::Working
            && (self.starts_conversation(event, payload)
                || previous.is_some_and(|previous| previous != SessionStatus::Working)))
        .then_some(RecordWatch::Interrupt {
            record: None,
            turn_id: None,
        })
    }
}

/// The tasks a `Stop` or `SubagentStop` lists as still running; an entry without an id is not one.
fn running_tasks(payload: &Value) -> Vec<BackgroundTask> {
    payload
        .get("background_tasks")
        .and_then(Value::as_array)
        .map(|tasks| tasks.iter().filter_map(running_task).collect())
        .unwrap_or_default()
}

fn running_task(entry: &Value) -> Option<BackgroundTask> {
    if entry.get("status").and_then(Value::as_str) != Some("running") {
        return None;
    }
    Some(BackgroundTask {
        id: text_field(entry, "id")?,
        kind: text_field(entry, "agent_type"),
        description: text_field(entry, "description"),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::super::settings::HOOK_EVENTS;
    use super::is_turn_continuation;
    use crate::adapter::provider;
    use crate::hooks::{BackgroundTasksChange, HookInterpreter, RecordWatch, TurnOutcome};
    use crate::protocol::{AgentId, BackgroundTask, SessionStatus};

    /// The payloads Qwen Code 0.28.0 wrote to a command hook's stdin, one a line, in the order
    /// `testdata/qwen-captures.md` lists them.
    const CAPTURE: &str = include_str!("../../../testdata/qwen_hooks.jsonl");

    const SESSION: &str = "11111111-2222-4333-8444-000000000002";

    // Qwen Code 0.28.3, see `testdata/qwen-captures.md`: a message submitted while a tool call ran
    // and held, steered with Enter and queued with Ctrl+Q, through to the turn that took it.
    const STEERED: &str = include_str!("../../../testdata/qwen_hooks_steered.jsonl");
    const QUEUED: &str = include_str!("../../../testdata/qwen_hooks_queued.jsonl");

    /// The payloads of `capture`, one a line.
    fn payloads(capture: &str) -> Vec<Value> {
        capture
            .lines()
            .map(|line| serde_json::from_str(line).expect("a captured payload"))
            .collect()
    }

    /// The prompts `capture` reports sent, each with the turn it is reported under.
    fn prompts_sent(capture: &str) -> Vec<(String, String)> {
        payloads(capture)
            .iter()
            .filter(|payload| payload["hook_event_name"] == "UserPromptSubmit")
            .filter_map(|payload| {
                let prompt = interpreter().submitted_prompt(payload)?;
                let turn = interpreter().turn_key(payload).expect("a turn key");
                let turn = turn.rsplit('#').next().expect("a turn number").to_string();
                Some((prompt.to_string(), turn))
            })
            .collect()
    }

    /// A held message the turn takes is reported as a prompt sent, with its text, which is what
    /// lets a cancel's dropped messages be told from the ones already taken: a steered one by the
    /// `UserPromptSubmit` of the model call that takes it, under the running turn's key and in
    /// place of the empty one, and a queued one by the prompt of the turn it runs as afterwards.
    #[test]
    fn a_held_message_the_turn_takes_is_reported_as_a_prompt_sent_with_its_text() {
        assert_eq!(
            prompts_sent(STEERED),
            [
                ("RUNSHELL:perl -e 'sleep 8';".to_string(), "0".to_string()),
                ("held message one".to_string(), "0".to_string()),
            ]
        );
        // The model call that took it fired no empty-prompt `UserPromptSubmit` beside it.
        assert!(!payloads(STEERED).iter().any(is_turn_continuation));

        assert_eq!(
            prompts_sent(QUEUED),
            [
                ("RUNSHELL:perl -e 'sleep 8';".to_string(), "0".to_string()),
                ("held message one".to_string(), "1".to_string()),
            ]
        );
        // The turn it waited for went on without it: its model call after the tool was the empty
        // one, and its `Stop` came before the queued message's prompt.
        let queued = payloads(QUEUED);
        let at = |wanted: &dyn Fn(&Value) -> bool| queued.iter().position(wanted).expect("there");
        let continuation = at(&is_turn_continuation);
        let first_stop = at(&|payload| payload["hook_event_name"] == "Stop");
        let taken = at(&|payload| payload["prompt"] == "held message one");
        assert!(continuation < first_stop && first_stop < taken);
    }

    fn interpreter() -> &'static dyn HookInterpreter {
        provider(AgentId::QWEN).hooks()
    }

    /// Every captured payload with its event name, in the capture's order.
    fn captured() -> Vec<(String, Value)> {
        CAPTURE
            .lines()
            .map(|line| {
                let payload: Value = serde_json::from_str(line).expect("a captured payload");
                let event = payload["hook_event_name"]
                    .as_str()
                    .expect("the payload names its event")
                    .to_string();
                (event, payload)
            })
            .collect()
    }

    /// The captured payload on `line`, counting from 1 as `qwen-captures.md` does, with its event.
    fn line(number: usize) -> (String, Value) {
        captured().swap_remove(number - 1)
    }

    fn running_agent() -> BackgroundTask {
        BackgroundTask {
            id: "general-purpose-call_2".to_string(),
            kind: Some("general-purpose".to_string()),
            description: Some("probe sub".to_string()),
        }
    }

    #[test]
    fn the_capture_is_the_twenty_two_payloads_it_is_described_as() {
        assert_eq!(captured().len(), 22);
    }

    /// What each captured event says about the session's status: a prompt, a tool call and its
    /// result are the turn working, and a stop of either kind is idle. Nothing else says anything,
    /// including the `UserPromptSubmit` the agent fires with an empty prompt inside a turn, the
    /// permission events (a wait is read from the terminal title) and the notifications.
    #[test]
    fn each_captured_event_has_the_status_it_says() {
        use SessionStatus::{Idle, Working};
        let expected = [
            None,          // 1 SessionStart
            Some(Working), // 2 UserPromptSubmit
            Some(Idle),    // 3 Stop
            None,          // 4 Notification idle_prompt
            Some(Working), // 5 PreToolUse
            None,          // 6 Notification idle_prompt inside the turn
            Some(Working), // 7 PostToolUse
            None,          // 8 UserPromptSubmit with an empty prompt
            None,          // 9 PermissionRequest
            None,          // 10 Notification permission_prompt
            None,          // 11 Notification idle_prompt after a decline
            Some(Working), // 12 PreToolUse of ask_user_question
            Some(Working), // 13 PostToolUse of the answered question
            Some(Idle),    // 14 StopFailure
            None,          // 15 SessionEnd
            Some(Working), // 16 PreToolUse of an MCP tool
            Some(Working), // 17 PostToolUse of the same call
            None,          // 18 SubagentStart
            None,          // 19 SubagentStop
            Some(Idle),    // 20 Stop
            Some(Idle),    // 21 Stop of a turn the agent started itself
            Some(Idle),    // 22 Stop holding a running background agent
        ];
        for (at, (event, payload)) in captured().iter().enumerate() {
            assert_eq!(
                interpreter().status(event, payload),
                expected[at],
                "line {} {event}",
                at + 1
            );
        }
    }

    #[test]
    fn a_failed_tool_call_is_the_turn_working_too() {
        let payload = json!({ "hook_event_name": "PostToolUseFailure", "prompt_id": "s########1" });
        assert_eq!(
            interpreter().status("PostToolUseFailure", &payload),
            Some(SessionStatus::Working)
        );
    }

    /// Line 2 is a typed prompt and line 8 the empty `UserPromptSubmit` the agent fires inside a
    /// turn when it calls the model back with a tool's result: only the first starts the
    /// conversation, and only the first carries a prompt.
    #[test]
    fn only_a_real_prompt_starts_the_conversation_and_carries_text() {
        for (at, (event, payload)) in captured().iter().enumerate() {
            let number = at + 1;
            assert_eq!(
                interpreter().starts_conversation(event, payload),
                number == 2,
                "line {number} {event}"
            );
            assert_eq!(
                interpreter().submitted_prompt(payload),
                (number == 2).then_some("hello there"),
                "line {number} {event}"
            );
        }
    }

    /// The empty-prompt payload is the turn's own and no subagent's.
    #[test]
    fn the_empty_prompt_payload_is_not_a_subagents() {
        let (_, payload) = line(8);
        assert_eq!(payload["prompt"], "");
        assert!(!interpreter().is_subagent(&payload));
    }

    #[test]
    fn only_a_payload_naming_an_agent_is_a_subagents() {
        for (at, (event, payload)) in captured().iter().enumerate() {
            let number = at + 1;
            assert_eq!(
                interpreter().is_subagent(payload),
                number == 18 || number == 19,
                "line {number} {event}"
            );
        }
        for payload in [
            json!({ "hook_event_name": "PreToolUse", "agent_id": "a-1" }),
            json!({ "hook_event_name": "PreToolUse", "agent_type": "general-purpose" }),
        ] {
            assert!(interpreter().is_subagent(&payload), "{payload}");
        }
    }

    #[test]
    fn the_turn_key_is_the_prompt_id_and_absent_where_a_payload_has_none() {
        let key = |number| interpreter().turn_key(&line(number).1);
        assert_eq!(key(2), Some(format!("{SESSION}########0")));
        assert_eq!(key(3), key(2), "the prompt and its stop are one turn");
        assert_eq!(key(5), Some(format!("{SESSION}########2")));
        assert_eq!(
            key(8),
            key(5),
            "the empty prompt is inside the turn it continues"
        );
        // A notification and the answered question carry no `prompt_id`.
        assert_eq!(key(4), None);
        assert_eq!(key(11), None);
        assert_eq!(key(13), None);
    }

    #[test]
    fn the_agents_session_id_is_the_payloads() {
        assert_eq!(
            interpreter().agent_session_id(&line(2).1).as_deref(),
            Some(SESSION)
        );
    }

    /// A `Stop` ends the turn with its final message, unless it is held on a background task still
    /// running; a `StopFailure` ends it as failed, with the error as shown.
    #[test]
    fn a_stop_ends_the_turn_unless_it_is_held_on_running_work() {
        let ends = |number| {
            let (event, payload) = line(number);
            interpreter().turn_end(&event, &payload)
        };
        let answered = ends(3).expect("a Stop that only answered ends the turn");
        assert_eq!(
            answered.last_assistant_message.as_deref(),
            Some("Hello from stub.")
        );
        assert!(!answered.failed);

        let failed = ends(14).expect("a StopFailure ends the turn");
        assert_eq!(
            failed.last_assistant_message.as_deref(),
            Some("[API Error: 500 stub failure]")
        );
        assert!(failed.failed);

        // Its tasks are all completed, so nothing holds it.
        for (number, message) in [(20, "Tool finished: done."), (21, "Hello from stub.")] {
            let end = ends(number).unwrap_or_else(|| panic!("line {number} ends the turn"));
            assert!(!end.failed, "line {number}");
            assert_eq!(
                end.last_assistant_message.as_deref(),
                Some(message),
                "line {number}"
            );
        }
        // A blank message is none.
        let (event, mut payload) = line(3);
        payload["last_assistant_message"] = json!("  ");
        let end = interpreter()
            .turn_end(&event, &payload)
            .expect("still a turn end");
        assert_eq!(end.last_assistant_message, None);
        // Held on a running agent: the agent's own turn on its completion carries the `Stop` to
        // report.
        assert!(ends(22).is_none());
        for number in (1..=22).filter(|number| ![3, 14, 20, 21].contains(number)) {
            assert!(ends(number).is_none(), "line {number}");
        }
    }

    #[test]
    fn a_stop_is_a_normal_outcome_even_when_held_and_a_stop_failure_is_abnormal() {
        let outcome = |number| {
            let (event, payload) = line(number);
            interpreter().turn_outcome(&event, &payload)
        };
        for number in [3, 20, 21, 22] {
            assert_eq!(outcome(number), Some(TurnOutcome::Normal), "line {number}");
        }
        assert_eq!(outcome(14), Some(TurnOutcome::Abnormal));
        for number in (1..=22).filter(|number| ![3, 14, 20, 21, 22].contains(number)) {
            assert_eq!(outcome(number), None, "line {number}");
        }
    }

    /// A subagent's start and stop change the background work and name the running tasks the
    /// session's `Stop` lists; completed ones are not work left running.
    #[test]
    fn subagent_events_and_stops_change_the_background_tasks() {
        let change = |number| {
            let (event, payload) = line(number);
            interpreter().background_tasks_change(&event, &payload)
        };
        assert_eq!(
            change(18),
            Some(BackgroundTasksChange::SubagentStarted(
                "general-purpose-call_2".to_string()
            ))
        );
        // The stopping agent is still listed as running in its own `SubagentStop`.
        assert_eq!(
            change(19),
            Some(BackgroundTasksChange::SubagentStopped {
                id: "general-purpose-call_2".to_string(),
                tasks: vec![running_agent()],
            })
        );
        // Completed tasks are filtered out, whether or not they are the only ones.
        for number in [3, 20, 21] {
            assert_eq!(
                change(number),
                Some(BackgroundTasksChange::Replace(Vec::new())),
                "line {number}"
            );
        }
        assert_eq!(
            change(22),
            Some(BackgroundTasksChange::Replace(vec![running_agent()]))
        );
        for number in (1..=22).filter(|number| ![3, 18, 19, 20, 21, 22].contains(number)) {
            assert_eq!(change(number), None, "line {number}");
        }
    }

    #[test]
    fn only_running_tasks_with_an_id_are_work_left_running() {
        let (event, mut payload) = line(22);
        payload["background_tasks"] = json!([
            { "id": "done", "status": "completed", "agent_type": "general-purpose" },
            { "status": "running", "agent_type": "general-purpose" },
            { "id": "bare", "status": "running" },
            { "id": "shell-1", "status": "running", "agent_type": "shell", "description": "tail log" },
        ]);
        assert_eq!(
            interpreter().background_tasks_change(&event, &payload),
            Some(BackgroundTasksChange::Replace(vec![
                BackgroundTask {
                    id: "bare".to_string(),
                    kind: None,
                    description: None
                },
                BackgroundTask {
                    id: "shell-1".to_string(),
                    kind: Some("shell".to_string()),
                    description: Some("tail log".to_string()),
                },
            ]))
        );
        // Without a list at all nothing is running.
        let (event, mut payload) = line(22);
        payload
            .as_object_mut()
            .expect("object")
            .remove("background_tasks");
        assert_eq!(
            interpreter().background_tasks_change(&event, &payload),
            Some(BackgroundTasksChange::Replace(Vec::new()))
        );
    }

    /// A subagent's own `Stop` lists the session's tasks too, but it is no change of the
    /// session's; and a subagent event that names no agent changes nothing.
    #[test]
    fn a_stop_of_a_subagent_replaces_nothing() {
        let (event, mut payload) = line(22);
        payload["agent_id"] = json!("general-purpose-call_2");
        assert_eq!(
            interpreter().background_tasks_change(&event, &payload),
            None
        );

        for event in ["SubagentStart", "SubagentStop"] {
            let payload = json!({ "hook_event_name": event, "agent_type": "general-purpose" });
            assert_eq!(
                interpreter().background_tasks_change(event, &payload),
                None,
                "{event}"
            );
        }
    }

    /// What a wait on the user and a cancel are read from is the terminal (the reports, else the
    /// title), so no hook announces a session start, cancels a turn, raises a tool call's wait or
    /// marks an error that leaves a prompt.
    #[test]
    fn no_captured_event_says_what_the_terminal_title_is_read_for() {
        for (at, (event, payload)) in captured().iter().enumerate() {
            let number = at + 1;
            assert!(
                !interpreter().announces_session_start(event, payload),
                "line {number}"
            );
            assert!(!interpreter().turn_cancelled(event), "line {number}");
            assert!(
                !interpreter().stopped_on_agent_error(event, payload),
                "line {number}"
            );
            assert!(
                interpreter().tool_call_change(event, payload).is_none(),
                "line {number}"
            );
        }
    }

    /// Every captured event the interpreter reads for something is one the launch registers its
    /// hook for, and `SessionStart` (which runs before the trust dialog is answered) and the
    /// events whose waits the title shows are not.
    #[test]
    fn the_launch_registers_the_events_that_are_read_and_not_the_others() {
        for (at, (event, payload)) in captured().iter().enumerate() {
            let read = interpreter().status(event, payload).is_some()
                || interpreter().turn_end(event, payload).is_some()
                || interpreter().turn_outcome(event, payload).is_some()
                || interpreter()
                    .background_tasks_change(event, payload)
                    .is_some()
                || interpreter().starts_conversation(event, payload);
            if read {
                assert!(
                    HOOK_EVENTS.contains(&event.as_str()),
                    "line {} {event}",
                    at + 1
                );
            }
        }
        for event in [
            "SessionStart",
            "SessionEnd",
            "PermissionRequest",
            "Notification",
        ] {
            assert!(!HOOK_EVENTS.contains(&event), "{event}");
        }
    }

    const STATUSES: [SessionStatus; 5] = [
        SessionStatus::Starting,
        SessionStatus::Working,
        SessionStatus::WaitingUser,
        SessionStatus::Idle,
        SessionStatus::Interrupted,
    ];

    fn watch_of(
        number: usize,
        previous: Option<SessionStatus>,
        status: SessionStatus,
    ) -> Option<RecordWatch<'static>> {
        let (event, payload) = line(number);
        // The watch only borrows from the payload for a record path, which Qwen Code's has none
        // of.
        interpreter()
            .record_watch(&event, &payload, previous, status)
            .map(|watch| match watch {
                RecordWatch::Interrupt {
                    record: None,
                    turn_id: None,
                } => RecordWatch::Interrupt {
                    record: None,
                    turn_id: None,
                },
                other => panic!("an unexpected watch: {other:?}"),
            })
    }

    const INTERRUPT: Option<RecordWatch<'static>> = Some(RecordWatch::Interrupt {
        record: None,
        turn_id: None,
    });

    /// The previous status is asked for, since a move into `Working` from another status inside a
    /// turn is what arms the watch again.
    #[test]
    fn the_status_before_an_event_is_read() {
        assert!(interpreter().watch_reads_previous_status());
    }

    /// A prompt arms the watch for the turn whatever the session was before it.
    #[test]
    fn a_prompt_arms_the_title_watch_whatever_the_session_was() {
        for previous in std::iter::once(None).chain(STATUSES.map(Some)) {
            assert_eq!(
                watch_of(2, previous, SessionStatus::Working),
                INTERRUPT,
                "{previous:?}"
            );
        }
    }

    /// An event inside the turn arms it again only when it finds the session out of `Working`: a
    /// turn the title closed that went on after all.
    #[test]
    fn an_event_inside_a_turn_arms_the_watch_only_after_a_move_into_working() {
        // PreToolUse, PostToolUse, an MCP tool's PreToolUse and the empty-prompt continuation.
        for number in [5, 7, 12, 16] {
            assert_eq!(
                watch_of(number, None, SessionStatus::Working),
                None,
                "line {number}"
            );
            assert_eq!(
                watch_of(number, Some(SessionStatus::Working), SessionStatus::Working),
                None,
                "line {number}"
            );
            for previous in STATUSES
                .into_iter()
                .filter(|status| *status != SessionStatus::Working)
            {
                assert_eq!(
                    watch_of(number, Some(previous), SessionStatus::Working),
                    INTERRUPT,
                    "line {number} after {previous:?}"
                );
            }
        }
    }

    /// The empty-prompt `UserPromptSubmit` is no prompt, so it does not arm the watch by itself,
    /// though it does find a session a title closed.
    #[test]
    fn the_empty_prompt_continuation_is_not_a_prompt_to_the_watch() {
        assert_eq!(watch_of(8, None, SessionStatus::Working), None);
        assert_eq!(
            watch_of(8, Some(SessionStatus::Working), SessionStatus::Working),
            None
        );
        assert_eq!(
            watch_of(8, Some(SessionStatus::Idle), SessionStatus::Working),
            INTERRUPT
        );
    }

    /// A status that is not `Working` arms nothing, a stop included.
    #[test]
    fn no_other_status_arms_the_watch() {
        for number in [2, 3, 14, 20, 22] {
            for status in STATUSES
                .into_iter()
                .filter(|status| *status != SessionStatus::Working)
            {
                for previous in std::iter::once(None).chain(STATUSES.map(Some)) {
                    assert_eq!(
                        watch_of(number, previous, status),
                        None,
                        "line {number} {status:?} after {previous:?}"
                    );
                }
            }
        }
    }

    /// `SessionStart` is not registered, so the first hook of a session comes with its first
    /// prompt, after the trust dialog: every event shows the agent past its startup.
    #[test]
    fn every_event_shows_the_agent_past_its_startup() {
        for (at, (event, payload)) in captured().iter().enumerate() {
            assert!(
                interpreter().past_startup(event, payload),
                "line {} {event}",
                at + 1
            );
        }
    }

    /// The `PreToolUse` payloads of the file-writing calls the guard probe made (0.28.3,
    /// `testdata/qwen-captures.md`): a `write_file` to another folder, one inside the working
    /// directory, and an `edit`.
    const GUARD_CAPTURE: &str = include_str!("../../../testdata/qwen_hooks_guard.jsonl");

    /// What each captured file-writing call would write, with the payload's working directory.
    #[test]
    fn a_file_writing_call_names_the_file_it_writes() {
        let hooks = &super::QwenHooks;
        let targets: Vec<(Vec<String>, Option<std::path::PathBuf>)> = GUARD_CAPTURE
            .lines()
            .map(|line| hooks.edit_targets(&serde_json::from_str(line).expect("payload")))
            .collect();
        let cwd = Some(std::path::PathBuf::from("/scratch/work"));
        assert_eq!(
            targets,
            [
                (
                    vec!["/private/tmp/DENYME-other/x.txt".to_string()],
                    cwd.clone()
                ),
                (vec!["/scratch/work/ok.txt".to_string()], cwd.clone()),
                (vec!["/scratch/work/a.txt".to_string()], cwd.clone()),
            ]
        );
        let notebook = json!({
            "cwd": "/w",
            "tool_name": "notebook_edit",
            "tool_input": { "notebook_path": "/w/n.ipynb", "new_source": "x" },
        });
        assert_eq!(hooks.edit_targets(&notebook).0, ["/w/n.ipynb"]);
        let shell = json!({ "cwd": "/w", "tool_input": { "command": "ls" } });
        assert!(hooks.edit_targets(&shell).0.is_empty());
        assert!(hooks.edit_targets(&json!({ "cwd": "/w" })).0.is_empty());
    }

    /// The refusal is the `PreToolUse` decision Qwen Code 0.28.3 was measured to obey: a `deny`
    /// with the reason, which it shows in the call's row.
    #[test]
    fn the_refusal_is_a_pre_tool_use_deny_with_its_reason() {
        let refusal = super::QwenHooks
            .deny_edit("another project")
            .expect("a refusal");
        assert_eq!(
            refusal,
            json!({"hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": "another project",
            }})
        );
    }
}
