//! The background work of an Antigravity CLI session, read from its hooks (1.5.14;
//! `docs/agent-cli-reference.md`, "Antigravity CLI", "Capabilities measured on 1.5.14", and the
//! captures `testdata/antigravity_hooks_background_command.jsonl`,
//! `testdata/antigravity_hooks_subagent.jsonl`, `testdata/antigravity_hooks_schedule_timer.jsonl`,
//! `testdata/antigravity_hooks_schedule_cron.jsonl` and
//! `testdata/antigravity_hooks_schedule_cancelled.jsonl`).
//!
//! No event lists the work, but each start and end shows in the hooks:
//!
//! - **A command that goes on as a background task.** A `run_command` that outlasts its wait
//!   ends the model's call with the task's id, and the turn ends with it running: its
//!   `PreToolUse` fired, its `PostToolUse` has not, and `Stop` says `fullyIdle: false`. Its
//!   `PostToolUse` (the same call id) fires only once the command has ended, and the CLI then
//!   runs a turn of its own on the task's result. So a command a turn's `Stop` finds started and
//!   not ended is listed, by its call id, and its `PostToolUse` takes it off.
//! - **A timer or a cron job.** `schedule` returns at once and its timer runs as a background task
//!   of the CLI's (`4s timer running`, `cron (every minute) running` in the panel under the box):
//!   the turn ends with its `PreToolUse` fired and `Stop` saying `fullyIdle: false`, and its
//!   `PostToolUse` (the same call id) fires once it is over, when the timer fires or the cron job
//!   has run its last time, before the turn the CLI runs on it. A cron job's runs before its last
//!   are each a turn of their own that ends with `fullyIdle: false`. One cancelled through
//!   `manage_task` (`kill`) fires its `PostToolUse` too, right after the `manage_task` call's own.
//!   So it is listed and taken off
//!   as a background command is, described as the CLI draws it (`4s: check the build`,
//!   `* * * * *: cron fired`).
//! - **A subagent.** `invoke_subagent` returns at once and the subagent runs a conversation of its
//!   own whose events come through the same hooks, each naming the subagent's own conversation
//!   and an empty `lastUserInput` ([`is_subagent`]): its first event lists it, and its `Stop`
//!   with `fullyIdle: true` takes it off.
//! - **Nothing left.** `Stop` with `fullyIdle: true` of the session's own conversation comes only
//!   once none of its work runs (the subagent's own `Stop` came before it), so it empties the
//!   list.
//!
//! A command whose permission prompt was declined, or whose turn was cancelled, fires its
//! `PreToolUse` and no other event, no `Stop` either; it is dropped once a turn on another
//! message begins, so that a later `Stop` does not take it for background work.
//!
//! A turn is told by its message alone, which is a limit: none of the events read names the turn
//! (`Stop`'s `executionNum` was 0 for every turn, and a call's `stepIdx` counts the conversation's
//! steps, not its turns). So where the next turn answers the same text, a command declined before
//! it is still kept, and if that turn leaves a command running, its `Stop` lists the declined one
//! too, until the `Stop` with `fullyIdle: true` that comes once the running one has ended. Only
//! `PreInvocation` (`invocationNum` 0 at each turn's first model request) would tell the turns
//! apart, at the cost of one more hook run on every model request, which is not registered.

use serde_json::Value;

use super::hooks::{POST_TOOL_USE, PRE_TOOL_USE, RUN_COMMAND, STOP};
use crate::hooks::{BackgroundTasksChange, HookSession};
use crate::protocol::BackgroundTask;
use crate::records::locate::plausible_id;

/// The tool that starts a subagent, whose `Role` is the subagent's description.
const INVOKE_SUBAGENT: &str = "invoke_subagent";
/// The tool that starts a timer (`DurationSeconds`) or a cron job (`CronExpression`), which
/// sends the model its `Prompt` when it fires.
const SCHEDULE: &str = "schedule";

/// The kinds a task is listed under: a timer or cron job by the name of the tool that starts it,
/// as the CLI names no kind of its own for it.
const SHELL_TASK: &str = "shell";
const SUBAGENT_TASK: &str = "subagent";
const SCHEDULE_TASK: &str = "schedule";

/// Whether `payload` is an event of a subagent's conversation rather than the session's own.
/// A subagent's conversation gets its task as a message of the CLI's, not from the user, so every
/// one of its events names `lastUserInput` empty (1.5.14), where an event of the session's own
/// conversation names the message its turn answers: the CLI sends no empty message, and a message
/// of attached images alone is not sent (an Enter on a box holding only an attachment sent
/// nothing).
pub(super) fn is_subagent(payload: &Value) -> bool {
    payload.get("lastUserInput").and_then(Value::as_str) == Some("")
}

/// What `event` changes in the session's background work.
pub(super) fn changes(
    session: &HookSession<'_>,
    event: &str,
    payload: &Value,
) -> Vec<BackgroundTasksChange> {
    session.carry(|kept: &mut Kept| {
        if is_subagent(payload) {
            kept.subagent_event(event, payload)
        } else {
            kept.own_event(event, payload)
        }
    })
}

/// What is kept of the session's work from one event to the next ([`HookSession::carry`]).
/// Losing it (a restart of the daemon) costs what a later event would have changed: a `Stop` with
/// `fullyIdle: true` empties the list whatever was kept.
#[derive(Default)]
pub(super) struct Kept {
    /// The message the current turn of the session's own conversation answers.
    turn: Option<String>,
    /// The commands, timers and cron jobs started and not ended, oldest first.
    commands: Vec<Command>,
    /// The roles of the subagents invoked and not yet seen starting, oldest first.
    roles: Vec<String>,
    /// The subagents seen, with the role each was invoked with where it is known.
    subagents: Vec<(String, Option<String>)>,
}

/// A tool call that may go on past its turn: a `run_command`, or a `schedule`.
struct Command {
    call: String,
    kind: &'static str,
    description: Option<String>,
    /// Whether a `Stop` found it running, so that it is listed.
    listed: bool,
}

impl Command {
    fn task(&self) -> BackgroundTask {
        BackgroundTask {
            id: self.call.clone(),
            kind: Some(self.kind.to_string()),
            description: self.description.clone(),
        }
    }
}

impl Kept {
    fn own_event(&mut self, event: &str, payload: &Value) -> Vec<BackgroundTasksChange> {
        let turn = payload.get("lastUserInput").and_then(Value::as_str);
        if turn.is_some() && turn != self.turn.as_deref() {
            self.turn = turn.map(str::to_string);
            self.commands.retain(|command| command.listed);
        }
        let call = payload.get("toolCall");
        let tool = call
            .and_then(|call| call.get("name"))
            .and_then(Value::as_str);
        let call_id = call
            .and_then(|call| call.get("id"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        match (event, tool, call_id) {
            (PRE_TOOL_USE, Some(tool @ (RUN_COMMAND | SCHEDULE)), Some(id)) => {
                if !self.commands.iter().any(|command| command.call == id) {
                    let (kind, description) = if tool == SCHEDULE {
                        (SCHEDULE_TASK, schedule_description(call))
                    } else {
                        (SHELL_TASK, argument(call, "CommandLine"))
                    };
                    self.commands.push(Command {
                        call: id.to_string(),
                        kind,
                        description,
                        listed: false,
                    });
                }
                Vec::new()
            }
            (PRE_TOOL_USE, Some(INVOKE_SUBAGENT), _) => {
                if let Some(role) = argument(call, "Role") {
                    self.roles.push(role);
                }
                Vec::new()
            }
            (POST_TOOL_USE, _, Some(id)) => {
                let Some(at) = self.commands.iter().position(|command| command.call == id) else {
                    return Vec::new();
                };
                if self.commands.remove(at).listed {
                    vec![BackgroundTasksChange::Finished(vec![id.to_string()])]
                } else {
                    Vec::new()
                }
            }
            (STOP, _, _) => match payload.get("fullyIdle").and_then(Value::as_bool) {
                Some(true) => {
                    *self = Kept {
                        turn: self.turn.take(),
                        ..Kept::default()
                    };
                    vec![BackgroundTasksChange::Replace(Vec::new())]
                }
                Some(false) => {
                    let started: Vec<BackgroundTask> = self
                        .commands
                        .iter_mut()
                        .filter(|command| !command.listed)
                        .map(|command| {
                            command.listed = true;
                            command.task()
                        })
                        .collect();
                    if started.is_empty() {
                        Vec::new()
                    } else {
                        vec![BackgroundTasksChange::Started(started)]
                    }
                }
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// A subagent's event: its `Stop` with `fullyIdle: true` ends it, and any other lists it,
    /// since it runs. It is described by the role it was invoked with, where one invocation
    /// waited to be paired with it: of several started at once, which is which is not told, and
    /// none is described.
    fn subagent_event(&mut self, event: &str, payload: &Value) -> Vec<BackgroundTasksChange> {
        let Some(id) = payload
            .get("conversationId")
            .and_then(Value::as_str)
            .filter(|id| plausible_id(id))
        else {
            return Vec::new();
        };
        if event == STOP {
            return match payload.get("fullyIdle").and_then(Value::as_bool) {
                Some(true) => vec![BackgroundTasksChange::Finished(vec![id.to_string()])],
                _ => Vec::new(),
            };
        }
        let description = match self.subagents.iter().find(|(seen, _)| seen == id) {
            Some((_, role)) => role.clone(),
            None => {
                let role = if self.roles.len() == 1 {
                    self.roles.pop()
                } else {
                    self.roles.clear();
                    None
                };
                self.subagents.push((id.to_string(), role.clone()));
                role
            }
        };
        vec![BackgroundTasksChange::Started(vec![BackgroundTask {
            id: id.to_string(),
            kind: Some(SUBAGENT_TASK.to_string()),
            description,
        }])]
    }
}

/// A `schedule` call as the CLI describes it (`Scheduling 1 timer (4s: check the build)`,
/// `Scheduling 1 cron (* * * * *: cron fired)`, 1.5.14): when it fires, then its prompt; either
/// alone where the call has only that.
fn schedule_description(call: Option<&Value>) -> Option<String> {
    let seconds = call
        .and_then(|call| call.get("args"))
        .and_then(|args| args.get("DurationSeconds"))
        .and_then(Value::as_u64)
        .map(|seconds| format!("{seconds}s"));
    let when = seconds.or_else(|| argument(call, "CronExpression"));
    match (when, argument(call, "Prompt")) {
        (Some(when), Some(prompt)) => Some(format!("{when}: {prompt}")),
        (when, prompt) => when.or(prompt),
    }
}

/// The text argument `name` of the tool call `call`, where it has one that is not empty.
fn argument(call: Option<&Value>, name: &str) -> Option<String> {
    call?
        .get("args")?
        .get(name)?
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookCarry;

    /// The hooks of a turn whose `run_command` (`sleep 6; echo finished-bg`, waiting 500 ms) went
    /// on as a background task (1.5.14, the stand-in): `SessionStart`, the first model call with
    /// the command's `PreToolUse`, the second, `Stop` with `fullyIdle: false`; then, when the
    /// command ended, its `PostToolUse`, the CLI's own turn on its result and `Stop` with
    /// `fullyIdle: true`.
    const COMMAND: &str =
        include_str!("../../../testdata/antigravity_hooks_background_command.jsonl");

    /// The hooks of a turn that defined a subagent and invoked it (role `Probe Sleeper`), whose
    /// one model request took 8 s (1.5.14, the stand-in): the session's events and the
    /// subagent's, interleaved as they came, the subagent's naming its own conversation.
    const SUBAGENT: &str = include_str!("../../../testdata/antigravity_hooks_subagent.jsonl");

    /// The hooks of a `schedule` timer (`DurationSeconds` 4, `Prompt` `check the build`; 1.5.14,
    /// the stand-in), through the hooks as the launch registers them: `SessionStart`, the call's
    /// `PreToolUse`, `Stop` with `fullyIdle: false`; 4 s later its `PostToolUse`, then the `Stop`
    /// of the CLI's own turn on it, `fullyIdle: true`.
    const TIMER: &str = include_str!("../../../testdata/antigravity_hooks_schedule_timer.jsonl");

    /// The same for a cron job (`* * * * *`, `MaxIterations` 2, `Prompt` `cron fired`): a second
    /// `Stop` with `fullyIdle: false` after its first run, and its `PostToolUse` at its last.
    const CRON: &str = include_str!("../../../testdata/antigravity_hooks_schedule_cron.jsonl");

    /// A timer (`DurationSeconds` 900) and a cron job (`0 3 * * *`) scheduled in one answer, then
    /// the timer cancelled by a `manage_task` `kill` in the next turn (1.5.14, the stand-in):
    /// `SessionStart`, both calls' `PreToolUse`, `Stop` with `fullyIdle: false`; then the
    /// `manage_task` call's `PreToolUse` and `PostToolUse`, the timer's `PostToolUse` (its call
    /// id), and that turn's `Stop` with `fullyIdle: false`, the cron job still running.
    const CANCELLED: &str =
        include_str!("../../../testdata/antigravity_hooks_schedule_cancelled.jsonl");

    fn events(capture: &str) -> Vec<Value> {
        capture
            .lines()
            .map(|line| serde_json::from_str(line).expect("a payload"))
            .collect()
    }

    fn event_of(payload: &Value) -> &str {
        payload["hookEventName"].as_str().expect("an event name")
    }

    /// What each event of `payloads` changes, applied one after the other to one session.
    fn replayed(payloads: &[Value]) -> Vec<Vec<BackgroundTasksChange>> {
        let carry = HookCarry::default();
        let session = HookSession::new("session", None, None, &carry);
        payloads
            .iter()
            .map(|payload| changes(&session, event_of(payload), payload))
            .collect()
    }

    fn shell(id: &str, command_line: &str) -> BackgroundTask {
        BackgroundTask {
            id: id.to_string(),
            kind: Some("shell".to_string()),
            description: Some(command_line.to_string()),
        }
    }

    /// The command is listed at the `Stop` that finds it running, by its call id with its command
    /// line; its `PostToolUse` takes it off, and the `Stop` of the CLI's own turn on its result
    /// says nothing is left. No other event changes anything.
    #[test]
    fn a_command_left_running_at_the_turns_end_is_listed_until_it_ends() {
        let payloads = events(COMMAND);
        let call = payloads[2]["toolCall"]["id"].as_str().expect("a call id");
        let changes = replayed(&payloads);
        let expected: Vec<(usize, Vec<BackgroundTasksChange>)> = vec![
            (
                6,
                vec![BackgroundTasksChange::Started(vec![shell(
                    call,
                    "sleep 6; echo finished-bg",
                )])],
            ),
            (
                7,
                vec![BackgroundTasksChange::Finished(vec![call.to_string()])],
            ),
            (10, vec![BackgroundTasksChange::Replace(Vec::new())]),
        ];
        for (at, change) in changes.iter().enumerate() {
            let want = expected
                .iter()
                .find(|(index, _)| *index == at)
                .map(|(_, change)| change.clone())
                .unwrap_or_default();
            assert_eq!(change, &want, "event {at}: {}", event_of(&payloads[at]));
        }
    }

    /// Each event's change in `payloads`, against the changes expected at the indexes given and
    /// none at the others.
    fn assert_changes(payloads: &[Value], expected: &[(usize, Vec<BackgroundTasksChange>)]) {
        for (at, change) in replayed(payloads).iter().enumerate() {
            let want = expected
                .iter()
                .find(|(index, _)| *index == at)
                .map(|(_, change)| change.clone())
                .unwrap_or_default();
            assert_eq!(change, &want, "event {at}: {}", event_of(&payloads[at]));
        }
    }

    fn scheduled(id: &str, description: &str) -> BackgroundTask {
        BackgroundTask {
            id: id.to_string(),
            kind: Some("schedule".to_string()),
            description: Some(description.to_string()),
        }
    }

    /// A timer is listed at the `Stop` that finds it running, by its call id, described as the CLI
    /// draws it; its `PostToolUse` when it fires takes it off, and the `Stop` of the CLI's turn on
    /// it says nothing is left.
    #[test]
    fn a_timer_is_listed_until_it_fires() {
        let payloads = events(TIMER);
        assert_eq!(payloads[1]["toolCall"]["name"], "schedule");
        let call = payloads[1]["toolCall"]["id"].as_str().expect("a call id");
        assert_changes(
            &payloads,
            &[
                (
                    2,
                    vec![BackgroundTasksChange::Started(vec![scheduled(
                        call,
                        "4s: check the build",
                    )])],
                ),
                (
                    3,
                    vec![BackgroundTasksChange::Finished(vec![call.to_string()])],
                ),
                (4, vec![BackgroundTasksChange::Replace(Vec::new())]),
            ],
        );
    }

    /// A timer cancelled through `manage_task` is taken off by its own `PostToolUse`, which the
    /// cancel fires; the cron job scheduled beside it stays listed.
    #[test]
    fn a_timer_cancelled_through_manage_task_is_taken_off() {
        let payloads = events(CANCELLED);
        assert_eq!(payloads[4]["toolCall"]["name"], "manage_task");
        assert_eq!(payloads[4]["toolCall"]["args"]["Action"], "kill");
        let timer = payloads[1]["toolCall"]["id"].as_str().expect("a call id");
        let cron = payloads[2]["toolCall"]["id"].as_str().expect("a call id");
        assert_eq!(payloads[6]["toolCall"]["id"], timer);
        assert_changes(
            &payloads,
            &[
                (
                    3,
                    vec![BackgroundTasksChange::Started(vec![
                        scheduled(timer, "900s: timer fired"),
                        scheduled(cron, "0 3 * * *: cron fired"),
                    ])],
                ),
                (
                    6,
                    vec![BackgroundTasksChange::Finished(vec![timer.to_string()])],
                ),
            ],
        );
    }

    /// A cron job is listed at the first `Stop` and stays listed through the `Stop` of a run that
    /// is not its last, which lists nothing again; its `PostToolUse` at its last run takes it off.
    #[test]
    fn a_cron_job_is_listed_until_its_last_run() {
        let payloads = events(CRON);
        assert_eq!(payloads[1]["toolCall"]["name"], "schedule");
        assert_eq!(event_of(&payloads[3]), "Stop");
        assert_eq!(payloads[3]["fullyIdle"], false);
        let call = payloads[1]["toolCall"]["id"].as_str().expect("a call id");
        assert_changes(
            &payloads,
            &[
                (
                    2,
                    vec![BackgroundTasksChange::Started(vec![scheduled(
                        call,
                        "* * * * *: cron fired",
                    )])],
                ),
                (
                    4,
                    vec![BackgroundTasksChange::Finished(vec![call.to_string()])],
                ),
                (5, vec![BackgroundTasksChange::Replace(Vec::new())]),
            ],
        );
    }

    /// A `schedule` call is described by when it fires and its prompt, or by whichever of the two
    /// it has; arguments built by hand on the captured call, since the stand-in was asked for the
    /// two captured forms only.
    #[test]
    fn a_schedule_is_described_by_when_it_fires_and_its_prompt() {
        let captured = events(TIMER)[1]["toolCall"].clone();
        let with_args = |args: Value| {
            let mut call = captured.clone();
            call["args"] = args;
            schedule_description(Some(&call))
        };
        assert_eq!(
            with_args(serde_json::json!({"DurationSeconds": 90, "Prompt": "look again"})),
            Some("90s: look again".to_string())
        );
        assert_eq!(
            with_args(serde_json::json!({"CronExpression": "*/5 * * * *", "Prompt": "poll"})),
            Some("*/5 * * * *: poll".to_string())
        );
        assert_eq!(
            with_args(serde_json::json!({"Prompt": "later"})),
            Some("later".to_string())
        );
        assert_eq!(
            with_args(serde_json::json!({"DurationSeconds": 5, "Prompt": " "})),
            Some("5s".to_string())
        );
        assert_eq!(with_args(serde_json::json!({})), None);
    }

    /// A command that ends inside its turn (its `PostToolUse` before the `Stop`) is never
    /// listed, and neither is one whose turn ended without a `Stop` (a declined prompt) once a
    /// turn on another message begins.
    #[test]
    fn a_command_that_ends_in_its_turn_or_never_ran_is_not_listed() {
        let payloads = events(COMMAND);
        let [start, pre_tool, post_tool, stop_running, stop_idle] =
            [0, 2, 7, 6, 10].map(|at| payloads[at].clone());
        let ended_in_turn = replayed(&[
            start.clone(),
            pre_tool.clone(),
            post_tool,
            stop_running.clone(),
            stop_idle,
        ]);
        assert!(ended_in_turn[3].is_empty(), "{:?}", ended_in_turn[3]);

        let mut next_turn = stop_running;
        next_turn["lastUserInput"] = Value::from("another message");
        let declined = replayed(&[start, pre_tool, next_turn]);
        assert!(declined[2].is_empty(), "{:?}", declined[2]);
    }

    /// A subagent is listed from its first event under its own conversation's id, described by
    /// the role it was invoked with, and taken off by its own `Stop`; the session's `Stop` with
    /// `fullyIdle: false` leaves it, and the one with `fullyIdle: true` empties the list.
    #[test]
    fn a_subagent_is_listed_from_its_start_to_its_stop() {
        let payloads = events(SUBAGENT);
        let own = payloads[0]["conversationId"].as_str().expect("an id");
        let changes = replayed(&payloads);
        let subagent = |payload: &Value| payload["conversationId"].as_str() != Some(own);
        let first = payloads
            .iter()
            .position(subagent)
            .expect("a subagent's event");
        let id = payloads[first]["conversationId"]
            .as_str()
            .expect("an id")
            .to_string();
        assert_eq!(event_of(&payloads[first]), "SessionStart");
        assert_eq!(
            changes[first],
            vec![BackgroundTasksChange::Started(vec![BackgroundTask {
                id: id.clone(),
                kind: Some("subagent".to_string()),
                description: Some("Probe Sleeper".to_string()),
            }])]
        );
        for (at, payload) in payloads.iter().enumerate() {
            let change = &changes[at];
            match (
                subagent(payload),
                event_of(payload),
                payload["fullyIdle"].as_bool(),
            ) {
                (true, "Stop", Some(true)) => {
                    assert_eq!(
                        change,
                        &vec![BackgroundTasksChange::Finished(vec![id.clone()])]
                    )
                }
                (true, _, _) => assert!(
                    matches!(change.as_slice(), [BackgroundTasksChange::Started(tasks)] if tasks[0].id == id),
                    "event {at}: {change:?}"
                ),
                (false, "Stop", Some(true)) => {
                    assert_eq!(change, &vec![BackgroundTasksChange::Replace(Vec::new())])
                }
                (false, _, _) => assert!(change.is_empty(), "event {at}: {change:?}"),
            }
        }
    }

    /// Only the subagent's events count as a subagent's: every event of the session's own
    /// conversation names the message its turn answers.
    #[test]
    fn a_subagents_event_is_told_by_its_empty_message() {
        let payloads = events(SUBAGENT);
        let own = &payloads[0]["conversationId"];
        assert!(payloads
            .iter()
            .any(|payload| payload["conversationId"] != *own));
        for payload in &payloads {
            assert_eq!(
                is_subagent(payload),
                payload["conversationId"] != *own,
                "{payload}"
            );
        }
        for payload in events(COMMAND) {
            assert!(!is_subagent(&payload), "{payload}");
        }
    }

    /// Two subagents invoked before either starts cannot be told apart, so neither is described.
    #[test]
    fn subagents_started_together_are_not_described() {
        let payloads = events(SUBAGENT);
        let invoke = payloads
            .iter()
            .find(|payload| {
                event_of(payload) == "PreToolUse"
                    && payload["toolCall"]["name"] == "invoke_subagent"
            })
            .expect("the invocation")
            .clone();
        let start = payloads
            .iter()
            .find(|payload| is_subagent(payload))
            .expect("the subagent's start")
            .clone();
        let changes = replayed(&[invoke.clone(), invoke, start]);
        assert!(
            matches!(changes[2].as_slice(), [BackgroundTasksChange::Started(tasks)] if tasks[0].description.is_none()),
            "{:?}",
            changes[2]
        );
    }
}
