//! Copilot's transcript gives messages and the start and result of each tool invocation.

use serde_json::Value;

use super::records::{edit_paths, is_edit, main_event};
use crate::protocol::{ToolCategory, ToolStatus};
use crate::providers::AgentId;
use crate::records::conversation::text_of;
use crate::records::feed::{at_of, EntriesReader, FeedOut, FeedReader, ToolCall, ToolOutcome};

pub(super) struct CopilotFeed;

impl FeedReader for CopilotFeed {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        let Some((kind, data)) = main_event(line) else {
            return;
        };
        match kind {
            "tool.execution_start" => {
                if let Some(call) = tool_call(data) {
                    out.tool_call(call, at_of(line));
                }
            }
            "tool.execution_complete" => {
                if let Some((id, outcome)) = tool_result(data) {
                    out.tool_result(id, outcome);
                }
            }
            _ => EntriesReader::new(AgentId::COPILOT).line(line, out),
        }
    }
}

fn tool_call(data: &Value) -> Option<ToolCall> {
    let name = data.get("toolName")?.as_str()?;
    let input = &data["arguments"];
    let category = match name {
        "bash" | "powershell" | "read_bash" | "write_bash" => ToolCategory::Command,
        "view" => ToolCategory::FileRead,
        "glob" | "grep" => ToolCategory::Search,
        "web_fetch" | "web_search" => ToolCategory::Web,
        "task" => ToolCategory::Subagent,
        _ if is_edit(data) => ToolCategory::FileEdit,
        _ if data.get("mcpServerName").is_some() => ToolCategory::Mcp,
        _ => ToolCategory::Other,
    };
    let paths = if category == ToolCategory::FileEdit {
        edit_paths(input)
    } else {
        Vec::new()
    };
    Some(ToolCall {
        call_id: text_of(data, "toolCallId")?,
        name: name.to_owned(),
        category,
        title: text_of(data, "toolTitle"),
        input: input.clone(),
        paths,
        status: ToolStatus::Running,
        output: None,
        exit_code: None,
    })
}

fn tool_result(data: &Value) -> Option<(&str, ToolOutcome)> {
    let status = if data.get("success")?.as_bool()? {
        ToolStatus::Completed
    } else {
        match data.pointer("/error/code").and_then(Value::as_str) {
            Some("denied" | "permission_denied") => ToolStatus::Declined,
            _ => ToolStatus::Failed,
        }
    };
    let output = text_of(&data["result"], "content").or_else(|| text_of(&data["error"], "message"));
    Some((
        data.get("toolCallId")?.as_str()?,
        ToolOutcome {
            status,
            output,
            exit_code: None,
        },
    ))
}
