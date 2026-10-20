//! The model and reasoning effort an agent's own records show a session running with. Only the two
//! values are kept. This module holds each agent's knowledge of which record says them and in which
//! field; reading the records is the collector's business (`usage.rs`).

use serde_json::Value;

/// What the records have shown so far, as the agent spells it. The last record that names a value
/// wins, and a record that names none leaves the last one in place, except where `effort_unset`
/// says otherwise.
#[derive(Default)]
pub struct Observed {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The last record named the model and, by naming no effort, said that none is set, so an
    /// effort shown earlier no longer holds. Only Codex's records say this.
    pub effort_unset: bool,
}

impl Observed {
    /// Takes an assistant turn of Claude's main transcript. A subagent's turns are marked as a
    /// sidechain and run other models, and a `<synthetic>` message is the client's own notice
    /// rather than a model's reply.
    pub fn fold_claude(&mut self, line: &Value) {
        if line.get("type").and_then(Value::as_str) != Some("assistant")
            || line.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            return;
        }
        let Some(model) = line.pointer("/message/model").and_then(Value::as_str) else {
            return;
        };
        if model != "<synthetic>" {
            self.fold(Some(model.to_owned()), text(line, "effort"));
        }
    }

    /// Takes a line of a Codex rollout, which names the model and effort a turn started with. A turn
    /// context carries the effort its whole configuration resolved to and leaves it out when no
    /// layer sets one, so the latest one is the whole truth about the effort: one that names a
    /// model and no effort unsets an effort an earlier turn named.
    pub fn fold_codex(&mut self, line: &str) {
        if !line.contains("\"turn_context\"") {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if value.get("type").and_then(Value::as_str) != Some("turn_context") {
            return;
        }
        if let Some(payload) = value.get("payload") {
            let (model, effort) = (text(payload, "model"), text(payload, "effort"));
            if model.is_some() && effort.is_none() {
                self.model = model;
                self.effort = None;
                self.effort_unset = true;
            } else {
                self.fold(model, effort);
            }
        }
    }

    /// Takes Grok's summary file, which names the configured model and effort rather than the id a
    /// request was served by.
    pub fn fold_grok(&mut self, summary: &Value) {
        self.fold(
            text(summary, "current_model_id"),
            text(summary, "reasoning_effort"),
        );
    }

    fn fold(&mut self, model: Option<String>, effort: Option<String>) {
        self.model = model.or(self.model.take());
        if effort.is_some() {
            self.effort = effort;
            self.effort_unset = false;
        }
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
