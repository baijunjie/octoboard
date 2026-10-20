//! The context size an agent's own records show a session at: how much the main conversation's
//! latest request carried, never a running total. Subagents and side calls do not count, and the
//! size falls again after a compaction or a revert. This module holds each agent's knowledge of
//! which record says it, in which field, and how the window limit and the share used come out;
//! reading the records is the collector's business (`record_collector.rs`).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::protocol::{Agent, SessionContext};

/// A context size in tokens and the window limit beside it, where the record gives one.
pub type Figure = (u64, Option<u64>);

/// What the records have shown so far.
#[derive(Default)]
pub struct Context {
    /// The size in tokens the records last gave: `None` before any, `Some(None)` after a
    /// compaction left it unknown until the next request.
    tokens: Option<Option<u64>>,
    /// The context-window limit the records give: with each figure for Codex, from the signals
    /// file for Grok.
    window: Option<u64>,
    /// The size a Grok compaction began from, which a trailing line may still repeat.
    stale: Option<u64>,
    /// For a Codex head segment, which holds only the turns since the last revert, the figure its
    /// base ends on, with the head it was read for: what shows until the head's first turn.
    base: Option<(PathBuf, Option<Figure>)>,
}

impl Context {
    /// What the records showed is gone with the record that showed it; a base figure belongs to
    /// the head it was read for and stays.
    pub fn forget(&mut self) {
        self.tokens = None;
        self.window = None;
        self.stale = None;
    }

    /// Whether the record itself has shown anything yet.
    pub fn is_shown(&self) -> bool {
        self.tokens.is_some()
    }

    /// Whether the base figure was already read for `head`.
    pub fn has_base_for(&self, head: &Path) -> bool {
        self.base.as_ref().is_some_and(|(read, _)| read == head)
    }

    /// Takes the figure found where `head` picks up, `None` when there is none.
    pub fn set_base(&mut self, head: PathBuf, figure: Option<Figure>) {
        self.base = Some((head, figure));
    }

    /// Takes the window limit a separate file gives (Grok's signals file).
    pub fn set_window(&mut self, window: Option<u64>) {
        self.window = window;
    }

    /// Takes a line of Claude's main transcript. A subagent's lines are marked as a sidechain,
    /// `<synthetic>` is the client's own notice, and replies that were not metered, failed or
    /// carried nothing in say nothing. A request's size is what it sent in: ordinary, cache-read
    /// and cache-write input, without the reply (Claude's own status line counts it so); a message
    /// that carries `iterations` is sized by the last of its own calls. A compaction leaves the
    /// size unknown until the next request.
    pub fn fold_claude(&mut self, line: &Value) {
        let flag = |key: &str| line.get(key).and_then(Value::as_bool) == Some(true);
        match line.get("type").and_then(Value::as_str) {
            Some("system")
                if line.get("subtype").and_then(Value::as_str) == Some("compact_boundary") =>
            {
                self.tokens = Some(None);
            }
            Some("assistant") => {
                let Some(message) = line.get("message") else {
                    return;
                };
                if flag("isSidechain")
                    || flag("isUnmetered")
                    || flag("isApiErrorMessage")
                    || message.get("model").and_then(Value::as_str) == Some("<synthetic>")
                {
                    return;
                }
                let Some(usage) = message.get("usage") else {
                    return;
                };
                let usage = usage
                    .get("iterations")
                    .and_then(Value::as_array)
                    .and_then(|calls| {
                        calls.iter().rev().find(|call| {
                            matches!(
                                call.get("type").and_then(Value::as_str),
                                Some("message" | "fallback_message")
                            )
                        })
                    })
                    .unwrap_or(usage);
                let tokens = [
                    "input_tokens",
                    "cache_creation_input_tokens",
                    "cache_read_input_tokens",
                ]
                .iter()
                .fold(0u64, |sum, key| {
                    sum.saturating_add(number(usage, key).unwrap_or(0))
                });
                if tokens > 0 {
                    self.tokens = Some(Some(tokens));
                }
            }
            _ => {}
        }
    }

    /// Takes a line of a Codex rollout: a `token_count` record gives the last request's total and
    /// the window limit.
    pub fn fold_codex(&mut self, line: &str) {
        if let Some((tokens, window)) = codex_figure(line) {
            self.tokens = Some(Some(tokens));
            self.window = window;
        }
    }

    /// Takes a line of a Grok session's updates log. Every streamed update carries the session's
    /// running context size; a compaction reports the size it left, after which a trailing line may
    /// still repeat the size it began from.
    pub fn fold_grok(&mut self, line: &str) {
        let compacted = line.contains("auto_compact_completed");
        if !compacted && !line.contains("\"totalTokens\"") {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(params) = value.get("params") else {
            return;
        };
        let update = params.get("update");
        if compacted
            && update.and_then(|update| update.get("sessionUpdate")?.as_str())
                == Some("auto_compact_completed")
        {
            if let Some(after) = update.and_then(|update| number(update, "tokens_after")) {
                self.tokens = Some(Some(after));
                self.stale = update.and_then(|update| number(update, "tokens_before"));
            }
        } else if let Some(total) = params
            .get("_meta")
            .and_then(|meta| number(meta, "totalTokens"))
        {
            if self.stale != Some(total) {
                self.tokens = Some(Some(total));
                self.stale = None;
            }
        }
    }

    /// The context the records show, in the form the store takes it: `None` when they have shown
    /// nothing, `Some(None)` when they say it is not known now.
    pub fn context(&self, agent: Agent) -> Option<Option<SessionContext>> {
        let (tokens, window) = match self.tokens {
            Some(Some(tokens)) => (tokens, self.window),
            Some(None) => return Some(None),
            None => self.base.as_ref()?.1?,
        };
        Some(Some(SessionContext {
            tokens,
            window,
            used_percent: window.and_then(|window| used_percent(agent, tokens, window)),
        }))
    }
}

fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}

/// The size of the context the conversation's last request carried and the window limit, from a
/// `token_count` record. One whose `info` is null reports nothing yet.
pub fn codex_figure(line: &str) -> Option<Figure> {
    if !line.contains("\"token_count\"") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let info = payload.get("info").filter(|info| !info.is_null())?;
    Some((
        number(info.get("last_token_usage")?, "total_tokens")?,
        number(info, "model_context_window"),
    ))
}

/// How much of the window is used, in whole percent, by the agent's own formula. Claude records no
/// window.
fn used_percent(agent: Agent, tokens: u64, window: u64) -> Option<u8> {
    /// What Codex sets aside of every window before it counts anything as used or left.
    const CODEX_BASELINE: u64 = 12_000;
    let percent = match agent {
        // Codex's `/status` shows what is left of the window past the baseline, rounded; the
        // share used is what that leaves of the whole. A window no larger than the baseline has
        // nothing left.
        Agent::Codex => match window.saturating_sub(CODEX_BASELINE) {
            0 => 100,
            usable => {
                let used = tokens.saturating_sub(CODEX_BASELINE).min(usable);
                let left = ((usable - used) as f64 / usable as f64 * 100.0).round() as u64;
                100 - left
            }
        },
        // Grok's `contextWindowUsage` rounds down.
        Agent::Grok => (u128::from(tokens) * 100 / u128::from(window.max(1))).min(100) as u64,
        Agent::Claude => return None,
    };
    u8::try_from(percent).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude(lines: &[Value]) -> Context {
        let mut context = Context::default();
        for line in lines {
            context.fold_claude(line);
        }
        context
    }

    fn claude_line(extra: Value, usage: Value) -> Value {
        let mut line = json!({"type":"assistant","message":{"id":"m","model":"claude-opus-5",
            "usage":usage}});
        line.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        line
    }

    fn tokens_of(context: &Context, agent: Agent) -> Option<Option<u64>> {
        context
            .context(agent)
            .map(|context| context.map(|context| context.tokens))
    }

    /// Captures contain native usage metadata only; their origins and redactions are described
    /// in testdata/record-captures.md. The old failed-turn capture is the native null near-miss.
    #[test]
    fn committed_native_captures_replay_through_the_context_readers() {
        let mut context = Context::default();
        let mut sizes = Vec::new();
        for line in include_str!("../testdata/usage_claude_2.1.296.jsonl").lines() {
            context.fold_claude(&serde_json::from_str(line).unwrap());
            sizes.push(tokens_of(&context, Agent::Claude).unwrap().unwrap());
        }
        // Each request's input, cached input included and the reply left out.
        assert_eq!(sizes, [48_762, 48_762, 48_762, 52_051, 52_051]);
        let codex = include_str!("../testdata/usage_codex_0.162.1.jsonl");
        assert_eq!(codex_figure(codex), Some((18_829, Some(475_000))));
        assert!(include_str!("../testdata/rollout_running_turn.jsonl")
            .lines()
            .all(|line| codex_figure(line).is_none()));
        let mut grok = Context::default();
        for line in include_str!("../testdata/context_grok_1.0.62_updates.jsonl").lines() {
            grok.fold_grok(line);
        }
        // The compaction left 25,032; the stale line repeating 205,620 was skipped.
        assert_eq!(tokens_of(&grok, Agent::Grok), Some(Some(25_500)));
    }

    #[test]
    fn claudes_context_is_the_last_main_requests_input_and_nothing_else_counts() {
        let usage = json!({"input_tokens":2,"cache_read_input_tokens":40,
            "cache_creation_input_tokens":8,"output_tokens":10});
        let main = claude_line(json!({}), usage);
        assert_eq!(
            tokens_of(&claude(std::slice::from_ref(&main)), Agent::Claude),
            Some(Some(50))
        );
        // Later lines that are not the main conversation's own requests leave it as it was.
        let big = json!({"input_tokens":900,"output_tokens":1});
        for extra in [
            json!({"isSidechain":true}),
            json!({"isUnmetered":true}),
            json!({"isApiErrorMessage":true}),
        ] {
            let later = claude_line(extra, big.clone());
            assert_eq!(
                tokens_of(&claude(&[main.clone(), later]), Agent::Claude),
                Some(Some(50))
            );
        }
        let synthetic = json!({"type":"assistant","message":{"model":"<synthetic>","usage":big}});
        let empty = claude_line(
            json!({}),
            json!({"input_tokens":0,"cache_read_input_tokens":0,"output_tokens":5}),
        );
        assert_eq!(
            tokens_of(&claude(&[main.clone(), synthetic, empty]), Agent::Claude),
            Some(Some(50))
        );
        // The last line wins, repeated or not.
        let later = claude_line(json!({}), json!({"input_tokens":70,"output_tokens":1}));
        assert_eq!(
            tokens_of(&claude(&[main.clone(), main, later]), Agent::Claude),
            Some(Some(70))
        );
    }

    #[test]
    fn claude_sizes_a_message_by_the_last_of_its_own_calls() {
        let usage = json!({"input_tokens":1000,"output_tokens":5,"iterations":[
            {"type":"message","input_tokens":10,"cache_read_input_tokens":90},
            {"type":"advisor_message","input_tokens":5000},
            {"type":"fallback_message","input_tokens":20,"cache_creation_input_tokens":30}]});
        assert_eq!(
            tokens_of(&claude(&[claude_line(json!({}), usage)]), Agent::Claude),
            Some(Some(50))
        );
        let unusable = json!({"input_tokens":7,"iterations":[{"type":"advisor_message"}]});
        assert_eq!(
            tokens_of(&claude(&[claude_line(json!({}), unusable)]), Agent::Claude),
            Some(Some(7))
        );
    }

    #[test]
    fn a_claude_compaction_leaves_the_size_unknown_until_the_next_request() {
        let request = claude_line(json!({}), json!({"input_tokens":3,"output_tokens":1}));
        let boundary = json!({"type":"system","subtype":"compact_boundary",
            "compactMetadata":{"postTokens":123}});
        let next = claude_line(json!({}), json!({"input_tokens":9,"output_tokens":1}));
        let mut context = claude(&[request, boundary]);
        assert_eq!(context.context(Agent::Claude), Some(None));
        context.fold_claude(&next);
        let shown = context.context(Agent::Claude).unwrap().unwrap();
        assert_eq!(
            (shown.tokens, shown.window, shown.used_percent),
            (9, None, None)
        );
        context.forget();
        assert_eq!(context.context(Agent::Claude), None);
    }

    fn codex_count(info: Value) -> String {
        json!({"type":"event_msg","payload":{"type":"token_count","info":info}}).to_string()
    }

    #[test]
    fn codexs_context_is_the_last_requests_total_with_the_window_and_codexs_own_percent() {
        let count = |last: u64, window: u64| {
            codex_count(json!({"total_token_usage":{"total_tokens":last * 40},
                "last_token_usage":{"total_tokens":last},"model_context_window":window}))
        };
        let mut context = Context::default();
        context.fold_codex(&count(18_999, 475_000));
        // A record with no info yet leaves the figure as it was.
        context.fold_codex(&codex_count(Value::Null));
        // Codex's `/status` reads 98% left of a 475,000 window at 18,999 tokens.
        let shown = context.context(Agent::Codex).unwrap().unwrap();
        assert_eq!(shown.tokens, 18_999);
        assert_eq!(shown.window, Some(475_000));
        assert_eq!(shown.used_percent, Some(2));
        assert_eq!(used_percent(Agent::Codex, 12_000, 475_000), Some(0));
        assert_eq!(used_percent(Agent::Codex, 0, 475_000), Some(0));
        assert_eq!(used_percent(Agent::Codex, 475_000, 475_000), Some(100));
        assert_eq!(used_percent(Agent::Codex, 900_000, 475_000), Some(100));
        // A window no larger than the baseline has nothing left, which Codex shows as 0% left.
        assert_eq!(used_percent(Agent::Codex, 5, 12_000), Some(100));
        assert_eq!(used_percent(Agent::Codex, 5, 8_000), Some(100));
    }

    #[test]
    fn groks_context_follows_the_updates_log_and_its_compactions() {
        let chunk = |total: u64| {
            json!({"method":"session/update","params":{"update":{"sessionUpdate":"tool_call"},
                "_meta":{"totalTokens":total}}})
            .to_string()
        };
        let compacted = |before: u64, after: u64| {
            json!({"method":"_x.ai/session/update","params":{"update":{
                "sessionUpdate":"auto_compact_completed","tokens_before":before,
                "tokens_after":after}}})
            .to_string()
        };
        let turn_done = json!({"method":"_x.ai/session/update","params":{"update":{
            "sessionUpdate":"turn_completed","usage":{"totalTokens":999_999}}}})
        .to_string();
        let mut context = Context::default();
        for line in [chunk(100), turn_done, chunk(200)] {
            context.fold_grok(&line);
        }
        assert_eq!(tokens_of(&context, Agent::Grok), Some(Some(200)));
        context.fold_grok(&compacted(200, 30));
        context.fold_grok(&chunk(200));
        assert_eq!(
            tokens_of(&context, Agent::Grok),
            Some(Some(30)),
            "the stale line is skipped"
        );
        context.fold_grok(&chunk(35));
        assert_eq!(tokens_of(&context, Agent::Grok), Some(Some(35)));
        context.fold_grok(&chunk(200));
        assert_eq!(
            tokens_of(&context, Agent::Grok),
            Some(Some(200)),
            "the stale value counts only once"
        );
        context.fold_grok(&chunk(25_500));
        context.set_window(Some(256_000));
        let shown = context.context(Agent::Grok).unwrap().unwrap();
        assert_eq!((shown.window, shown.used_percent), (Some(256_000), Some(9)));
    }
}
