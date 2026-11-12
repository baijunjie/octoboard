use serde_json::{json, Value};

use super::*;
use crate::protocol::AgentId;

/// The hook interpreter of `agent`'s provider.
fn of(agent: AgentId) -> &'static dyn HookInterpreter {
    crate::adapter::provider(agent).hooks()
}

/// Whether `payload` is a subagent's, read as for a session of `agent`, or of nothing known.
fn is_subagent(agent: Option<AgentId>, payload: &Value) -> bool {
    agent.map_or_else(
        || is_named_subagent(payload),
        |agent| of(agent).is_subagent(payload),
    )
}

/// Which events end a turn abnormally or normally, per agent, and which say nothing of it.
#[test]
fn each_agents_events_say_how_a_turn_ended() {
    use TurnOutcome::{Abnormal, Normal};
    let none = json!({});
    let with_tasks = json!({ "background_tasks": [{ "id": "t", "type": "shell" }] });
    let shutdown = json!({ "reason": "shutdown" });
    let end_turn = json!({ "reason": "end_turn" });
    let idle_prompt = json!({ "notificationType": "idle_prompt" });
    for (agent, event, payload, expected) in [
        (AgentId::CLAUDE, "StopFailure", &none, Some(Abnormal)),
        (AgentId::CLAUDE, "Stop", &none, Some(Normal)),
        (
            AgentId::CLAUDE,
            "SessionStart",
            &json!({ "source": "clear" }),
            Some(Normal),
        ),
        (
            AgentId::CLAUDE,
            "SessionStart",
            &json!({ "source": "startup" }),
            Some(Normal),
        ),
        (
            AgentId::CLAUDE,
            "SessionStart",
            &json!({ "source": "resume" }),
            None,
        ),
        (
            AgentId::CLAUDE,
            "SessionStart",
            &json!({ "source": "compact" }),
            None,
        ),
        (AgentId::CLAUDE, "SessionStart", &none, None),
        (AgentId::CLAUDE, "Stop", &with_tasks, Some(Normal)),
        (AgentId::CLAUDE, "Notification", &idle_prompt, None),
        (AgentId::CLAUDE, "UserPromptSubmit", &none, None),
        (AgentId::CODEX, "Interrupt", &none, Some(Abnormal)),
        (AgentId::CODEX, "Stop", &none, Some(Normal)),
        (AgentId::CODEX, "UserPromptSubmit", &none, None),
        (AgentId::GROK, "StopCancelled", &none, Some(Abnormal)),
        (AgentId::GROK, "StopFailure", &none, Some(Abnormal)),
        (AgentId::GROK, "Stop", &end_turn, Some(Normal)),
        (AgentId::GROK, "Stop", &none, Some(Normal)),
        (AgentId::GROK, "Stop", &shutdown, None),
        (AgentId::GROK, "Notification", &idle_prompt, None),
        // Another agent's event name is no event of this one.
        (AgentId::CLAUDE, "Interrupt", &none, None),
        (AgentId::CODEX, "StopFailure", &none, None),
        (AgentId::CLAUDE, "StopCancelled", &none, None),
    ] {
        assert_eq!(
            of(agent).turn_outcome(event, payload),
            expected,
            "{agent:?} {event}"
        );
    }
}

/// Octoboard's messages reach Claude Code as pastes, and a long paste is handed to the hook
/// wrapped (Claude Code 2.1.295); a turn they start is only recognisable with the wrapper off.
#[test]
fn a_pasted_prompt_comes_through_without_its_wrapper() {
    let pasted = json!({
        "prompt": "\n\n<pasted_content id=\"7fb6\">\nMessage from Octoboard\n</pasted_content id=\"7fb6\">\n",
    });
    let prompt = of(AgentId::CLAUDE).submitted_prompt(&pasted).unwrap();
    assert!(prompt.starts_with("Message from Octoboard"), "{prompt}");
}

/// Captured from Grok Build 1.0.70: a long multiline paste reaches the hook verbatim.
#[test]
fn grok_reports_the_text_of_a_pasted_prompt() {
    let payload: Value =
        serde_json::from_str(include_str!("../../testdata/grok_user_prompt_submit.json")).unwrap();
    assert_eq!(event_name(&payload).as_deref(), Some("UserPromptSubmit"));
    let prompt = of(AgentId::GROK).submitted_prompt(&payload).unwrap();
    assert!(prompt.starts_with("USER_DRAFT_SENTINEL_"));
    assert!(prompt.contains("\n"));
    assert_eq!(prompt, payload["prompt"].as_str().unwrap());
}

#[test]
fn codex_reports_a_multiline_prompt_with_outer_whitespace_trimmed() {
    let payload: Value =
        serde_json::from_str(include_str!("../../testdata/codex_user_prompt_submit.json")).unwrap();
    let prompt = of(AgentId::CODEX).submitted_prompt(&payload).unwrap();
    assert_eq!(prompt, payload["prompt"].as_str().unwrap());
    assert_eq!(prompt, prompt.trim());
    assert!(prompt.contains('\n'));
}

/// Grok fires `Stop` twice per session and `idle_prompt` after every turn, so the events that
/// are *not* a reportable turn end are the ones worth pinning.
#[test]
fn only_a_real_turn_end_is_reportable() {
    assert!(of(AgentId::GROK)
        .turn_end("Stop", &json!({"reason": "shutdown"}))
        .is_none());
    assert!(of(AgentId::CLAUDE)
        .turn_end("Stop", &json!({"background_tasks": [{"id": "t1"}]}))
        .is_none());
    // A cancelled turn owes no report, but it is still a turn ending — the distinction is
    // `turn_cancelled`'s, not this function's.
    assert!(of(AgentId::CODEX)
        .turn_end("Interrupt", &json!({}))
        .is_none());
    assert!(of(AgentId::GROK)
        .turn_end("StopCancelled", &json!({}))
        .is_none());
}

/// A cancelled turn left open is what lets Grok's backstop invent a report for it a minute
/// later, so the cancellation has to be recognised as one.
#[test]
fn a_user_cancellation_is_recognised_where_the_agent_reports_one() {
    assert!(of(AgentId::CODEX).turn_cancelled("Interrupt"));
    assert!(of(AgentId::GROK).turn_cancelled("StopCancelled"));
    assert!(!of(AgentId::CODEX).turn_cancelled("Stop"));
    // Claude Code emits nothing at all on an interrupt.
    assert!(!of(AgentId::CLAUDE).turn_cancelled("Interrupt"));
}

/// Only the id-less signal may be attributed by the clock; everything else names its own turn.
/// Grok's backstop carries nothing but the fact that the turn is over.
#[test]
fn only_the_backstop_is_clock_attributed_and_it_carries_no_message() {
    let backstop = of(AgentId::GROK)
        .turn_end("Notification", &json!({"notificationType": "idle_prompt"}))
        .expect("a turn end");
    assert!(backstop.backstop);
    assert!(backstop.last_assistant_message.is_none());
    assert!(
        !of(AgentId::GROK)
            .turn_end("Stop", &json!({}))
            .expect("a turn end")
            .backstop
    );
}

#[test]
fn the_turns_last_message_comes_through_in_either_spelling() {
    let claude = of(AgentId::CLAUDE)
        .turn_end("Stop", &json!({"last_assistant_message": "finished"}))
        .expect("a turn end");
    assert_eq!(claude.last_assistant_message.as_deref(), Some("finished"));
    assert!(!claude.failed);

    let grok = of(AgentId::GROK)
        .turn_end("Stop", &json!({"lastAssistantMessage": "finished"}))
        .expect("a turn end");
    assert_eq!(grok.last_assistant_message.as_deref(), Some("finished"));
}

/// On `StopFailure` the message field is the user-facing error text rather than model output,
/// so a caller has to be able to tell the two apart.
#[test]
fn a_failed_turn_is_marked_as_one() {
    let end = of(AgentId::CLAUDE)
        .turn_end(
            "StopFailure",
            &json!({"last_assistant_message": "API error"}),
        )
        .expect("a turn end");
    assert!(end.failed);
}

/// Claude Code's auto mode blocks a tool call its classifier refuses without raising a prompt:
/// the call's `PreToolUse` is followed by the turn's `Stop`, with no `PermissionRequest` and no
/// `PostToolUse` between them. The session must come out of it idle and the turn reportable,
/// not left working or waiting.
#[test]
fn a_classifier_blocked_claude_tool_call_ends_the_turn_idle() {
    let pre = json!({"tool_name": "Bash", "permission_mode": "auto"});
    assert_eq!(
        of(AgentId::CLAUDE).status("PreToolUse", &pre),
        Some(SessionStatus::Working)
    );
    let stop = json!({"last_assistant_message": "That was blocked.", "permission_mode": "auto"});
    assert_eq!(
        of(AgentId::CLAUDE).status("Stop", &stop),
        Some(SessionStatus::Idle)
    );
    let end = of(AgentId::CLAUDE)
        .turn_end("Stop", &stop)
        .expect("a reportable turn end");
    assert!(!end.failed);
}

/// Codex fires a tool call's `PreToolUse` and `PermissionRequest` in the same instant and its
/// hooks are asynchronous, so a session whose approval is on screen reads as waiting for the
/// user whichever of the two is applied last.
#[test]
fn a_codex_approval_holds_the_hand_up_whichever_of_its_two_events_lands_last() {
    assert_eq!(of(AgentId::CODEX).status("PreToolUse", &json!({})), None);
    assert_eq!(
        of(AgentId::CODEX).status("PermissionRequest", &json!({})),
        Some(SessionStatus::WaitingUser)
    );
    // Granting the approval runs the tool, and that is what reports work again.
    assert_eq!(
        of(AgentId::CODEX).status("PostToolUse", &json!({})),
        Some(SessionStatus::Working)
    );
}

/// Codex defers `SessionStart` to the first prompt, where it arrives together with that
/// prompt's `UserPromptSubmit`; read as `Idle` it would race the turn it came with.
#[test]
fn codex_session_start_says_nothing_about_status() {
    assert_eq!(of(AgentId::CODEX).status("SessionStart", &json!({})), None);
    assert_eq!(
        of(AgentId::CODEX).status("UserPromptSubmit", &json!({})),
        Some(SessionStatus::Working)
    );
}

/// A normal Codex `Stop` is a turn end that did not fail; a failure comes from the rollout, not
/// from any hook event.
#[test]
fn a_codex_stop_is_a_turn_end_that_did_not_fail() {
    let end = of(AgentId::CODEX)
        .turn_end("Stop", &json!({"last_assistant_message": "done"}))
        .expect("a turn end");
    assert!(!end.failed);
    assert!(of(AgentId::CODEX)
        .turn_end("UserPromptSubmit", &json!({}))
        .is_none());
}

fn task(id: &str, kind: &str, description: &str) -> BackgroundTask {
    BackgroundTask {
        id: id.to_string(),
        kind: Some(kind.to_string()),
        description: Some(description.to_string()),
    }
}

/// A `Stop` held on background work leaves the session idle, not working: the `idle_prompt`
/// that follows it a minute later would otherwise flip it at an arbitrary moment. The tasks
/// are carried beside the status, and the turn is still not a reportable end.
#[test]
fn a_claude_stop_with_background_work_is_idle_with_its_tasks() {
    let stop = json!({"background_tasks": [
        {"id": "b1", "type": "shell", "status": "running", "description": "npm run dev"},
        {"id": "b2"},
        {"type": "shell"},
    ]});
    assert_eq!(
        of(AgentId::CLAUDE).status("Stop", &stop),
        Some(SessionStatus::Idle)
    );
    assert!(of(AgentId::CLAUDE).turn_end("Stop", &stop).is_none());
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("Stop", &stop),
        Some(BackgroundTasksChange::Replace(vec![
            task("b1", "shell", "npm run dev"),
            BackgroundTask {
                id: "b2".to_string(),
                kind: None,
                description: None,
            },
        ]))
    );
}

/// `idle_prompt` is the same idle whether or not tasks run, and says nothing about them.
#[test]
fn idle_prompt_leaves_the_background_tasks_alone() {
    let idle = json!({"notification_type": "idle_prompt"});
    assert_eq!(
        of(AgentId::CLAUDE).status("Notification", &idle),
        Some(SessionStatus::Idle)
    );
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("Notification", &idle),
        None
    );
}

/// A `Stop` with no list, or an empty one, and a `StopFailure` all leave nothing running.
#[test]
fn a_stop_without_tasks_and_a_failed_turn_clear_them() {
    let cleared = Some(BackgroundTasksChange::Replace(Vec::new()));
    for payload in [json!({}), json!({"background_tasks": []})] {
        assert_eq!(
            of(AgentId::CLAUDE).background_tasks_change("Stop", &payload),
            cleared
        );
    }
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("StopFailure", &json!({})),
        cleared
    );
}

/// A completion arrives as a prompt of its own, pasted or not, naming each finished task.
#[test]
fn a_task_notification_prompt_names_the_tasks_that_finished() {
    let prompt = json!({
        "prompt": "<task-notification>\n<task-id>b1</task-id>\n<status>completed</status>\n</task-notification>",
    });
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("UserPromptSubmit", &prompt),
        Some(BackgroundTasksChange::Finished(vec!["b1".to_string()]))
    );
    let pasted = json!({
        "prompt": "\n<pasted_content id=\"1\">\n<task-notification>\n<task-id>b1</task-id>\n<task-id>b2</task-id>\n",
    });
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("UserPromptSubmit", &pasted),
        Some(BackgroundTasksChange::Finished(vec![
            "b1".to_string(),
            "b2".to_string()
        ]))
    );
    let hand_back = json!({
        "prompt": "<agent-message from=\"abdbf855229f79d6b\">\n[Subagent hand-back] The text below \
                   is the final report.\n</agent-message>",
    });
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("UserPromptSubmit", &hand_back),
        None,
        "a hand-back names no task"
    );
    let typed = json!({"prompt": "run the tests"});
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("UserPromptSubmit", &typed),
        None
    );
}

/// Claude Code hands a background subagent's final report back as an `<agent-message from="…">`
/// prompt marked `[Subagent hand-back]` (2.1.322), sometimes after `Another Claude session sent a
/// message:`. That is a completion notice, like a `<task-notification>`; the same wrapper without
/// the marker is a peer's message, which is new work, though still sent by the agent itself, and
/// the marker without the wrapper is nothing special.
#[test]
fn a_subagent_hand_back_is_a_completion_notice_and_a_peers_message_is_not() {
    let hand_back = "<agent-message from=\"abdbf855229f79d6b\">\n[Subagent hand-back] The text \
                     below is the final report of a subagent this session delegated to.\n  Done.\n\
                     </agent-message>";
    let prefixed = format!("Another Claude session sent a message:\n{hand_back}");
    let peer = "Another Claude session sent a message:\n<agent-message from=\"peer-session\">\n\
                Can you look at this?\n</agent-message>";
    let marker_alone = "[Subagent hand-back] The text below is the final report.";
    let claude = of(AgentId::CLAUDE);
    // (prompt, completion notice, sent by the agent itself)
    for (prompt, completion, self_sent) in [
        (hand_back, true, true),
        (prefixed.as_str(), true, true),
        (peer, false, true),
        (marker_alone, false, false),
        (
            "<task-notification><task-id>b1</task-id></task-notification>",
            true,
            true,
        ),
        ("run the tests", false, false),
    ] {
        assert_eq!(claude.is_completion_notice(prompt), completion, "{prompt}");
        assert_eq!(claude.is_self_sent(prompt), self_sent, "{prompt}");
    }
    // Another agent's prompt is never read this way.
    for agent in AgentId::ALL
        .into_iter()
        .filter(|agent| *agent != AgentId::CLAUDE)
    {
        assert!(!of(agent).is_completion_notice(hand_back), "{agent:?}");
        assert!(!of(agent).is_self_sent(hand_back), "{agent:?}");
    }
}

#[test]
fn only_claude_codes_stop_carries_background_tasks() {
    let stop = json!({"backgroundTasks": [{"id": "g1"}], "background_tasks": [{"id": "g1"}]});
    assert_eq!(
        of(AgentId::GROK).background_tasks_change("Stop", &stop),
        None
    );
    assert_eq!(
        of(AgentId::CODEX).background_tasks_change("Stop", &stop),
        None
    );
}

/// On Claude Code any `agent_id` marks a subagent's payload, including the internal helper
/// agent's, which has no `agent_type`; for another agent only `agent_type` does.
#[test]
fn a_claude_payload_with_an_agent_id_is_a_subagents() {
    let claude = Some(AgentId::CLAUDE);
    assert!(is_subagent(claude, &json!({"agent_id": "a1"})));
    assert!(is_subagent(
        claude,
        &json!({"agent_id": "a1", "agent_type": ""})
    ));
    assert!(is_subagent(
        claude,
        &json!({"agent_type": "general-purpose"})
    ));
    assert!(!is_subagent(claude, &json!({"agent_id": ""})));
    assert!(!is_subagent(claude, &json!({"session_id": "s"})));
    for other in [Some(AgentId::CODEX), Some(AgentId::GROK), None] {
        assert!(!is_subagent(other, &json!({"agent_id": "a1"})));
        assert!(is_subagent(other, &json!({"agent_type": "worker"})));
    }
}

/// A named subagent's stop carries the registry, its own entry included;
/// its start adds it. A helper agent (no `agent_type`) changes nothing, and neither does a
/// subagent's own `Stop`-shaped payload.
#[test]
fn a_named_subagents_start_and_stop_change_the_tasks() {
    let stop = json!({
        "agent_id": "x", "agent_type": "general-purpose",
        "background_tasks": [
            {"id": "x", "type": "subagent", "description": "probe"},
            {"id": "b1", "type": "shell", "description": "npm run dev"},
        ],
    });
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("SubagentStop", &stop),
        Some(BackgroundTasksChange::SubagentStopped {
            id: "x".to_string(),
            tasks: vec![
                task("x", "subagent", "probe"),
                task("b1", "shell", "npm run dev")
            ],
        })
    );
    let start = json!({"agent_id": "x", "agent_type": "general-purpose"});
    assert_eq!(
        of(AgentId::CLAUDE).background_tasks_change("SubagentStart", &start),
        Some(BackgroundTasksChange::SubagentStarted("x".to_string()))
    );
    let helper = json!({"agent_id": "h", "background_tasks": [{"id": "h"}]});
    for event in ["SubagentStart", "SubagentStop", "Stop"] {
        assert_eq!(
            of(AgentId::CLAUDE).background_tasks_change(event, &helper),
            None
        );
    }
    // A named subagent's `Stop`-shaped payload is not the session's `Stop`.
    for event in ["Stop", "StopFailure", "UserPromptSubmit"] {
        assert_eq!(
            of(AgentId::CLAUDE).background_tasks_change(event, &stop),
            None
        );
    }
    assert_eq!(
        of(AgentId::CODEX).background_tasks_change("SubagentStop", &stop),
        None
    );
}

/// Each provider asks for the watch its hooks leave a gap for, and only on the events that open
/// one: Claude Code's on a move into waiting or working, Codex's on a prompt, Grok's on a prompt or
/// on the hand coming down.
#[test]
fn each_provider_asks_for_the_record_watch_its_hooks_leave_a_gap_for() {
    use SessionStatus::{Idle, WaitingUser, Working};
    let transcript = json!({"transcript_path": "/t.jsonl", "turn_id": "u1", "prompt_id": "p1"});
    let claude = of(AgentId::CLAUDE);
    assert!(claude.watch_reads_previous_status());
    assert_eq!(
        claude.record_watch("PermissionRequest", &transcript, Some(Working), WaitingUser),
        Some(RecordWatch::Decline {
            record: Some("/t.jsonl")
        })
    );
    assert_eq!(
        claude.record_watch("UserPromptSubmit", &transcript, Some(Working), Working),
        Some(RecordWatch::Interrupt {
            record: Some("/t.jsonl"),
            turn_id: Some("p1")
        })
    );
    assert_eq!(
        claude.record_watch("PostToolUse", &transcript, Some(WaitingUser), Working),
        Some(RecordWatch::Interrupt {
            record: Some("/t.jsonl"),
            turn_id: Some("p1")
        })
    );
    assert_eq!(
        claude.record_watch("PostToolUse", &transcript, Some(Working), Working),
        None
    );
    assert_eq!(
        claude.record_watch("Stop", &transcript, Some(Working), Idle),
        None
    );

    let codex = of(AgentId::CODEX);
    assert!(!codex.watch_reads_previous_status());
    assert_eq!(
        codex.record_watch("UserPromptSubmit", &transcript, None, Working),
        Some(RecordWatch::Failure {
            record: Some("/t.jsonl"),
            turn_id: Some("u1")
        })
    );
    assert_eq!(
        codex.record_watch("PostToolUse", &transcript, None, Working),
        None
    );

    let grok = of(AgentId::GROK);
    assert!(grok.watch_reads_previous_status());
    assert_eq!(
        grok.record_watch("UserPromptSubmit", &transcript, Some(Working), Working),
        Some(RecordWatch::Interrupt {
            record: None,
            turn_id: None
        })
    );
    assert_eq!(
        grok.record_watch("PostToolUse", &transcript, Some(WaitingUser), Working),
        Some(RecordWatch::Interrupt {
            record: None,
            turn_id: None
        })
    );
    assert_eq!(
        grok.record_watch("PostToolUse", &transcript, Some(Idle), Working),
        None
    );
}

/// Only Grok stops on an error with a modal the next write would answer.
#[test]
fn only_grok_stops_on_an_agent_error() {
    let error = json!({"notificationType": "agent_error"});
    assert!(of(AgentId::GROK).stopped_on_agent_error("StopFailure", &json!({})));
    assert!(of(AgentId::GROK).stopped_on_agent_error("Notification", &error));
    assert!(!of(AgentId::GROK).stopped_on_agent_error("Stop", &json!({})));
    for agent in [AgentId::CLAUDE, AgentId::CODEX] {
        assert!(!of(agent).stopped_on_agent_error("StopFailure", &json!({})));
    }
}

/// What `payload`'s call comes to as a [`CallShape`].
fn shape(payload: &Value) -> CallShape<'_> {
    CallShape {
        tool: payload["tool_name"].as_str(),
        input: payload.get("tool_input"),
    }
}

/// Claude Code's `PreToolUse` starts a call under its `tool_use_id`, and only its own
/// ask-the-user tool starts one that waits on the user.
#[test]
fn a_claude_tool_start_opens_a_call_and_only_a_question_waits() {
    let bash = json!({"tool_name": "Bash", "tool_input": {"command": "ls"}, "tool_use_id": "t1"});
    assert_eq!(
        of(AgentId::CLAUDE).tool_call_change("PreToolUse", &bash),
        Some(ToolCallChange::Started {
            id: Some("t1"),
            call: shape(&bash),
            waits_on_user: false,
        })
    );
    let question = json!({
        "tool_name": "AskUserQuestion",
        "tool_input": {"questions": []},
        "tool_use_id": "t2",
    });
    assert_eq!(
        of(AgentId::CLAUDE).tool_call_change("PreToolUse", &question),
        Some(ToolCallChange::Started {
            id: Some("t2"),
            call: shape(&question),
            waits_on_user: true,
        })
    );
    let unnamed = json!({});
    assert_eq!(
        of(AgentId::CLAUDE).tool_call_change("PreToolUse", &unnamed),
        Some(ToolCallChange::Started {
            id: None,
            call: CallShape {
                tool: None,
                input: None
            },
            waits_on_user: false,
        })
    );
}

/// A `PermissionRequest` names no call, so it carries the call's tool and input to be told by.
#[test]
fn a_claude_permission_request_waits_on_the_call_of_its_tool_and_input() {
    let request = json!({"tool_name": "Bash", "tool_input": {"command": "make"}});
    assert_eq!(
        of(AgentId::CLAUDE).tool_call_change("PermissionRequest", &request),
        Some(ToolCallChange::WaitsOnUser(shape(&request)))
    );
}

/// Either post event ends the call it names; one that names none ends an unnamed call.
#[test]
fn a_claude_post_event_ends_its_own_call() {
    let named = json!({"tool_name": "Bash", "tool_use_id": "t1"});
    for event in ["PostToolUse", "PostToolUseFailure"] {
        assert_eq!(
            of(AgentId::CLAUDE).tool_call_change(event, &named),
            Some(ToolCallChange::Ended { id: Some("t1") }),
            "{event}"
        );
        assert_eq!(
            of(AgentId::CLAUDE).tool_call_change(event, &json!({})),
            Some(ToolCallChange::Ended { id: None }),
            "{event}"
        );
    }
}

/// The end of a batch, a new prompt, the turn's end and a session's start leave no call open.
#[test]
fn a_claude_batch_prompt_or_turn_end_ends_every_call() {
    for event in [
        "PostToolBatch",
        "UserPromptSubmit",
        "Stop",
        "StopFailure",
        "SessionStart",
    ] {
        assert_eq!(
            of(AgentId::CLAUDE).tool_call_change(event, &json!({})),
            Some(ToolCallChange::AllEnded),
            "{event}"
        );
    }
}

/// An event that says nothing about tool calls says nothing about them.
#[test]
fn other_claude_events_say_nothing_of_tool_calls() {
    for event in [
        "Notification",
        "SubagentStart",
        "SubagentStop",
        "PreCompact",
    ] {
        assert_eq!(
            of(AgentId::CLAUDE).tool_call_change(event, &json!({})),
            None,
            "{event}"
        );
    }
}

/// Grok Build's tool events say what a session is doing: its ask-the-user tool's start raises the
/// hand, whichever spelling names the tool, any other start is work, and every tool event that
/// ends a call, or stops one that will never run, is work too.
#[test]
fn a_grok_tool_event_sets_waiting_for_a_question_and_working_otherwise() {
    for payload in [
        json!({"toolName": "ask_user_question"}),
        json!({"tool_name": "ask_user_question"}),
    ] {
        assert_eq!(
            of(AgentId::GROK).status("PreToolUse", &payload),
            Some(SessionStatus::WaitingUser),
            "{payload}"
        );
    }
    for payload in [
        json!({"toolName": "run_terminal_command"}),
        json!({"tool_name": "spawn_subagent"}),
        json!({}),
    ] {
        assert_eq!(
            of(AgentId::GROK).status("PreToolUse", &payload),
            Some(SessionStatus::Working),
            "{payload}"
        );
    }
    for event in ["PostToolUse", "PostToolUseFailure", "PermissionDenied"] {
        assert_eq!(
            of(AgentId::GROK).status(event, &json!({"toolName": "ask_user_question"})),
            Some(SessionStatus::Working),
            "{event}"
        );
    }
}

/// Grok Build's `PreToolUse` starts a call under its `toolUseId`, read in either spelling, with its
/// tool but not its input, and only its own ask-the-user tool starts one that waits on the user.
#[test]
fn a_grok_tool_start_opens_a_call_and_only_a_question_waits() {
    let shell = json!({
        "toolName": "run_terminal_command",
        "toolInput": {"command": "sleep 6"},
        "toolUseId": "t1",
    });
    assert_eq!(
        of(AgentId::GROK).tool_call_change("PreToolUse", &shell),
        Some(ToolCallChange::Started {
            id: Some("t1"),
            call: CallShape {
                tool: Some("run_terminal_command"),
                input: None,
            },
            waits_on_user: false,
        })
    );
    let question = json!({
        "tool_name": "ask_user_question",
        "tool_input": {"questions": []},
        "tool_use_id": "t2",
    });
    assert_eq!(
        of(AgentId::GROK).tool_call_change("PreToolUse", &question),
        Some(ToolCallChange::Started {
            id: Some("t2"),
            call: CallShape {
                tool: Some("ask_user_question"),
                input: None,
            },
            waits_on_user: true,
        })
    );
}

/// Either post event ends the call it names, and so does the denial of a call that will never
/// run; an event that names none ends an unnamed call.
#[test]
fn a_grok_post_event_or_denial_ends_its_own_call() {
    for event in ["PostToolUse", "PostToolUseFailure", "PermissionDenied"] {
        for payload in [json!({"toolUseId": "t1"}), json!({"tool_use_id": "t1"})] {
            assert_eq!(
                of(AgentId::GROK).tool_call_change(event, &payload),
                Some(ToolCallChange::Ended { id: Some("t1") }),
                "{event} {payload}"
            );
        }
        assert_eq!(
            of(AgentId::GROK).tool_call_change(event, &json!({})),
            Some(ToolCallChange::Ended { id: None }),
            "{event}"
        );
    }
}

/// A new prompt, any of the turn's three ends and a session's start leave no call open.
#[test]
fn a_grok_prompt_or_turn_end_ends_every_call() {
    for event in [
        "UserPromptSubmit",
        "Stop",
        "StopFailure",
        "StopCancelled",
        "SessionStart",
    ] {
        assert_eq!(
            of(AgentId::GROK).tool_call_change(event, &json!({})),
            Some(ToolCallChange::AllEnded),
            "{event}"
        );
    }
}

/// A permission prompt or a question's dialog names no call, and Grok's permission prompts hold
/// back every other call, so neither is a call waiting on the user; nor are the other events.
#[test]
fn other_grok_events_say_nothing_of_tool_calls() {
    for payload in [
        json!({"notificationType": "permission_prompt"}),
        json!({"notificationType": "elicitation_dialog"}),
        json!({"notificationType": "idle_prompt"}),
    ] {
        assert_eq!(
            of(AgentId::GROK).tool_call_change("Notification", &payload),
            None,
            "{payload}"
        );
    }
    for event in ["SubagentStart", "SubagentStop", "SessionEnd", "PreCompact"] {
        assert_eq!(
            of(AgentId::GROK).tool_call_change(event, &json!({})),
            None,
            "{event}"
        );
    }
}

/// Only Claude Code's and Grok Build's hooks tell calls apart; every other agent's events leave
/// its status mapping as it was.
#[test]
fn only_claude_code_and_grok_build_report_tool_call_changes() {
    let payload = json!({"tool_name": "Bash", "tool_input": {}, "tool_use_id": "t1"});
    for agent in [AgentId::CODEX, AgentId::TEST] {
        for event in [
            "PreToolUse",
            "PermissionRequest",
            "PostToolUse",
            "PostToolUseFailure",
            "PostToolBatch",
            "UserPromptSubmit",
            "Stop",
            "StopFailure",
            "SessionStart",
        ] {
            assert_eq!(
                of(agent).tool_call_change(event, &payload),
                None,
                "{agent:?} {event}"
            );
        }
    }
}
