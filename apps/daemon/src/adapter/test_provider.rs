//! The behavior of the test-only provider (`crate::providers::test_provider`): the launch plan,
//! which every provider must have, and a record store that finds `<agent session id>.jsonl` files
//! under the records directory and declares a live feed reader and companion, so the generic feed
//! code is shown to follow what a provider declares; every other sub-interface is the trait's
//! default, a prompt reader of a made-up screen, so the generic interaction code is shown to
//! follow what a provider reads, and a screen reader of a made-up input box, so the generic
//! composer requests (images and slash commands) are shown to follow it.

use std::fs::FileType;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use super::{AgentProvider, LaunchPlan, LaunchSpec};
use crate::protocol::{ToolCategory, ToolStatus};
use crate::providers::{AgentId, ProviderDescriptor};
use crate::records::feed::{FeedOut, FeedReader, Pending, ToolCall, ToolOutcome};
use crate::records::RecordStore;
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};
use crate::session::screen::ScreenReader;

pub struct TestProvider;

impl AgentProvider for TestProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::TEST.descriptor()
    }

    fn records(&self) -> &'static dyn RecordStore {
        &TestRecords
    }

    fn interactions(&self) -> Option<&'static dyn InteractionReader> {
        Some(&TestPrompts)
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        &TestScreen
    }

    fn plan(&self, _spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        Ok(LaunchPlan {
            args: Vec::new(),
            private_server: None,
            env: Vec::new(),
            resolves_approvals_itself: false,
            notice: None,
            carried_trust: None,
            after_exit: None,
            adoption: None,
        })
    }
}

/// The test provider's records: a file `<agent session id>.jsonl` in the records directory, whose
/// lines are `{"user": text}`, `{"assistant": text}`, `{"call": id, "name": name}`,
/// `{"result": id, "output": text}`, `{"truncate": len}` and `{"panic": true}`, which panics;
/// beside it, `<id>.live` lines `{"pending": text}` (or none, which clears what was pending).
pub struct TestRecords;

impl RecordStore for TestRecords {
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        let id = name.strip_suffix(".jsonl")?;
        (leaf && kind.is_file()).then(|| (id.to_owned(), path.to_path_buf()))
    }

    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        let named =
            record.file_name().and_then(|name| name.to_str()) == Some(&format!("{agent_id}.jsonl"));
        if levels_below_dir == 1 && named {
            vec![record.to_path_buf()]
        } else {
            Vec::new()
        }
    }

    fn feed_reader(&self) -> Option<Box<dyn FeedReader>> {
        Some(Box::new(TestReader))
    }

    fn feed_companion(&self, record: &Path) -> Option<PathBuf> {
        Some(record.with_extension("live"))
    }
}

struct TestReader;

impl FeedReader for TestReader {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        if line["panic"].as_bool() == Some(true) {
            panic!("the test reader was told to panic");
        }
        if let Some(text) = line["user"].as_str() {
            out.user(text, None);
        } else if let Some(text) = line["assistant"].as_str() {
            out.assistant(text, None);
        } else if let Some(id) = line["call"].as_str() {
            out.tool_call(
                ToolCall {
                    call_id: id.to_owned(),
                    name: line["name"].as_str().unwrap_or("tool").to_owned(),
                    category: ToolCategory::Other,
                    title: None,
                    input: json!({}),
                    paths: Vec::new(),
                    status: ToolStatus::Running,
                    output: None,
                    exit_code: None,
                },
                None,
            );
        } else if let Some(id) = line["result"].as_str() {
            out.tool_result(
                id,
                ToolOutcome {
                    status: ToolStatus::Completed,
                    output: line["output"].as_str().map(str::to_owned),
                    exit_code: None,
                },
            );
        } else if let Some(len) = line["truncate"].as_u64() {
            out.truncate(len as usize);
        }
    }

    fn companion_line(&mut self, line: &Value, out: &mut FeedOut) {
        out.set_pending(
            line["pending"]
                .as_str()
                .map(|text| Pending::Assistant(text.to_owned()))
                .into_iter()
                .collect(),
        );
    }
}

/// Reads a made-up screen: a line `PROMPT <tool> <command>` is a permission prompt for the call of
/// that tool and input `{"command": <command>}`, offering `1. Yes` and `2. No`, each chosen by its
/// digit; a line `EDITING` is that prompt with a text field open, which cannot be published; a line
/// `INPUT>` is the input box, and anything else is unknown. Calls start on `PreToolUse`, as
/// `{"tool_use_id", "tool_name", "tool_input"}`.
pub struct TestPrompts;

impl InteractionReader for TestPrompts {
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        (event == "PreToolUse").then(|| {
            CallSeen::new(
                payload
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                payload
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                payload.get("tool_input").unwrap_or(&Value::Null),
            )
        })
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        use crate::protocol::{
            ChoiceEffect, ChoiceText, InteractionPrompt, PermissionChoice, PermissionPrompt,
        };
        if screen.lines().any(|line| line.trim() == "EDITING") {
            return Reading::Up;
        }
        let prompt = screen
            .lines()
            .find_map(|line| line.trim().strip_prefix("PROMPT "));
        if let Some(prompt) = prompt {
            let (tool, command) = prompt.split_once(' ').unwrap_or((prompt, ""));
            let found = calls.iter().rposition(|call| {
                call.tool == tool
                    && call.input.get("command").and_then(Value::as_str) == Some(command)
            });
            let Some(call) = found else {
                return Reading::Up;
            };
            let choice = |id: &str, label: &str, effect| PermissionChoice {
                id: id.to_string(),
                label: label.to_string(),
                effect,
                text: (id == "2").then_some(ChoiceText::Optional),
            };
            return Reading::Shown {
                call,
                prompt: InteractionPrompt::Permission(PermissionPrompt {
                    tool: tool.to_string(),
                    category: Some(ToolCategory::Command),
                    title: Some(command.to_string()),
                    detail: None,
                    input: calls[call].input.clone(),
                    input_truncated: calls[call].input_truncated,
                    choices: vec![
                        choice("1", "Yes", ChoiceEffect::Allow),
                        choice("2", "No", ChoiceEffect::Deny),
                    ],
                }),
            };
        }
        if screen
            .lines()
            .any(|line| line.trim_start().starts_with("INPUT>"))
        {
            Reading::Gone
        } else {
            Reading::Unknown
        }
    }

    fn next_key(&self, screen: &str, _call: &CallSeen, answer: &Answer) -> NextKey {
        use crate::platform::term::Input;
        if !screen
            .lines()
            .any(|line| line.trim().starts_with("PROMPT "))
        {
            return NextKey::Stuck("no prompt on the screen");
        }
        match answer {
            Answer::Choice { id, text: None } => NextKey::Press(Input::Text(id.clone())),
            _ => NextKey::Stuck("the test screen takes a plain choice"),
        }
    }
}

/// Reads a made-up input box: the line beginning `INPUT>`, whose rest is the box's text, which
/// shows each attached image as `[Image #n]`; a line `COMPLETE <name>` is the slash-command
/// completion offering the command `<name>`, a line `STASH` shows the draft stashed, and
/// `[Pasted text #n +m lines]` is the form of a long paste. Its Ctrl-U clears the box.
pub struct TestScreen;

impl ScreenReader for TestScreen {
    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        screen.lines().find_map(|line| {
            line.trim_start()
                .strip_prefix("INPUT>")
                .map(|text| vec![text.trim().to_string()])
        })
    }

    fn image_placeholders(&self, text: &str) -> usize {
        crate::session::screen::count_image_placeholders(text)
    }

    fn without_image_placeholders(&self, text: &str) -> String {
        crate::session::screen::strip_image_placeholders(text)
    }

    fn command_completion(&self, screen: &str) -> Option<String> {
        screen.lines().find_map(|line| {
            line.trim()
                .strip_prefix("COMPLETE ")
                .map(|name| name.trim().to_string())
        })
    }

    fn clears_input_box_with_ctrl_u(&self) -> bool {
        true
    }

    fn collapsed_paste(&self, text: &str) -> bool {
        text.starts_with("[Pasted text #") && text.ends_with(']')
    }

    fn stash_shown(&self, screen: &str) -> bool {
        screen.lines().any(|line| line.trim() == "STASH")
    }
}
