//! Whether a Grok Build session shows a turn an account switch is to carry on
//! (`crate::records::switch_continue`, `RecordStore::should_continue`).

use std::path::Path;

use serde_json::Value;

use crate::records::line_file;

/// The Grok Build decision on the session directory at `record`, read from its `updates.jsonl`:
/// the last prompt's `turn_completed`, or, with none, whether the turn had shown any output.
///
/// Grok writes no ending for a turn its process is ended in, and none either for a Ctrl+C before
/// the turn's first output (it puts the prompt back in the input box instead), so a turn with no
/// output is not continued. Once there is output, a Ctrl+C or a refused permission is
/// `stop_reason: "cancelled"`. A usage limit is an `error` whose text carries the HTTP status 402,
/// the only thing that tells it from other request errors; a 429 is `rate_limit`, not continued.
pub(super) fn should_continue(record: &Path, usage_limit_only: bool) -> std::io::Result<bool> {
    // What the last prompt's turn has shown so far: `None` before any prompt.
    let mut last: Option<GrokTurn> = None;
    line_file::read_from(&record.join(super::records::UPDATES_LOG), 0, |line| {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let update = &entry["params"]["update"];
        let Some(kind) = update["sessionUpdate"].as_str() else {
            return;
        };
        match (kind, &last) {
            ("user_message_chunk", _) => last = Some(GrokTurn::Prompted),
            (
                "agent_thought_chunk" | "agent_message_chunk" | "tool_call" | "tool_call_update",
                Some(GrokTurn::Prompted),
            ) => last = Some(GrokTurn::Working),
            ("turn_completed", Some(GrokTurn::Prompted | GrokTurn::Working)) => {
                let carry_on = update["stop_reason"] == "error"
                    && update["agent_result"]
                        .as_str()
                        .is_some_and(|text| text.contains("status 402"));
                last = Some(GrokTurn::Ended { carry_on });
            }
            _ => {}
        }
    })?;
    Ok(match last {
        Some(GrokTurn::Working) => !usage_limit_only,
        Some(GrokTurn::Ended { carry_on }) => carry_on,
        Some(GrokTurn::Prompted) | None => false,
    })
}

enum GrokTurn {
    /// The prompt, and nothing after it yet.
    Prompted,
    /// Output after the prompt, and no ending.
    Working,
    Ended {
        carry_on: bool,
    },
}
