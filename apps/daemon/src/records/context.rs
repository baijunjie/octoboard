//! The context size an agent's own records show a session at: how much the main conversation's
//! latest request carried, never a running total. Subagents and side calls do not count, and the
//! size falls again after a compaction or a revert. Which record says it, in which field, and how
//! the share used comes out is each provider's own knowledge, which folds a line into a [`Context`]
//! through the calls here (`RecordStore::fold_line`, `RecordStore::used_percent`); reading the
//! records is the collector's business (`collector.rs`).
//!
//! Claude Code's transcript records no window. Its status line input does, so Octoboard's injected
//! status line reports it (`crate::adapter::StatusLine`), and [`ReportedWindows`] holds the last
//! report of each stored session, which the collector applies on every pass.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use crate::protocol::SessionContext;
use crate::records::RecordStore;

/// A context size in tokens and the window limit beside it, where the record gives one.
pub type Figure = (u64, Option<u64>);

/// What the records have shown so far.
#[derive(Default)]
pub struct Context {
    /// The size in tokens the records last gave: `None` before any, `Some(None)` after a
    /// compaction left it unknown until the next request.
    tokens: Option<Option<u64>>,
    /// The context-window limit: with each figure where the record gives one, else from a source
    /// beside it (a file the provider keeps, or what its status line reported, `ReportedWindows`).
    window: Option<u64>,
    /// The size a compaction began from, which a trailing line may still repeat
    /// ([`Self::observe_total`]).
    stale: Option<u64>,
    /// For a record that continues an earlier file and holds only what came after it picked up,
    /// the figure the earlier file ends on, with the record it was read for: what shows until the
    /// record's own first figure.
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

    /// Takes the window limit a source beside the record gives.
    pub fn set_window(&mut self, window: Option<u64>) {
        self.window = window;
    }

    /// A compaction left the size unknown until the next request.
    pub fn compacted(&mut self) {
        self.tokens = Some(None);
    }

    /// The latest request carried `tokens`.
    pub fn observe(&mut self, tokens: u64) {
        self.tokens = Some(Some(tokens));
    }

    /// The latest request carried `tokens`, in a record that gives the window limit beside it.
    pub fn observe_with_window(&mut self, (tokens, window): Figure) {
        self.tokens = Some(Some(tokens));
        self.window = window;
    }

    /// A compaction left `after`, having begun from `before`, which a line after it may still
    /// repeat ([`Self::observe_total`]).
    pub fn compacted_to(&mut self, after: u64, before: Option<u64>) {
        self.tokens = Some(Some(after));
        self.stale = before;
    }

    /// The running total a line gives, skipped once when it repeats the size the last compaction
    /// began from.
    pub fn observe_total(&mut self, total: u64) {
        if self.stale != Some(total) {
            self.tokens = Some(Some(total));
            self.stale = None;
        }
    }

    /// The context the records show, in the form the store takes it: `None` when they have shown
    /// nothing, `Some(None)` when they say it is not known now. The share used is `store`'s own.
    pub fn context(&self, store: &dyn RecordStore) -> Option<Option<SessionContext>> {
        let (tokens, window) = match self.tokens {
            Some(Some(tokens)) => (tokens, self.window),
            Some(None) => return Some(None),
            None => self.base.as_ref()?.1?,
        };
        Some(Some(SessionContext {
            tokens,
            window,
            used_percent: window.and_then(|window| store.used_percent(tokens, window)),
        }))
    }
}

/// The context window each Claude Code session's status line last reported, by Octoboard session
/// id. In memory only: the collector stores it with the session's context, which is what a daemon
/// restart starts from, and a running Claude Code reports it again after its next reply.
#[derive(Default)]
pub struct ReportedWindows {
    windows: Mutex<HashMap<String, u64>>,
}

impl ReportedWindows {
    pub fn report(&self, session_id: &str, window: u64) {
        self.windows
            .lock()
            .expect("reported window lock poisoned")
            .insert(session_id.to_owned(), window);
    }

    /// Forgets the sessions not in `ids` and returns what is left.
    pub fn retain(&self, ids: &HashSet<&str>) -> HashMap<String, u64> {
        let mut windows = self.windows.lock().expect("reported window lock poisoned");
        windows.retain(|id, _| ids.contains(id.as_str()));
        windows.clone()
    }
}

/// The unsigned number `value` holds at `key`.
pub(crate) fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentId;
    use crate::records::observed::Observed;
    use serde_json::json;

    fn store(agent: AgentId) -> &'static dyn RecordStore {
        crate::adapter::provider(agent).records()
    }

    fn used_percent(agent: AgentId, tokens: u64, window: u64) -> Option<u8> {
        store(agent).used_percent(tokens, window)
    }

    fn codex_figure(line: &str) -> Option<Figure> {
        crate::adapter::codex::records::figure(line)
    }

    /// Each provider's own reading of a line into the context.
    trait Folds {
        fn fold_claude(&mut self, line: &Value);
        fn fold_codex(&mut self, line: &str);
        fn fold_grok(&mut self, line: &str);
    }

    impl Folds for Context {
        fn fold_claude(&mut self, line: &Value) {
            crate::adapter::claude::records::fold_context(self, line);
        }

        fn fold_codex(&mut self, line: &str) {
            store(AgentId::CODEX).fold_line(line, false, self, &mut Observed::default());
        }

        fn fold_grok(&mut self, line: &str) {
            store(AgentId::GROK).fold_line(line, false, self, &mut Observed::default());
        }
    }

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

    fn tokens_of(context: &Context, agent: AgentId) -> Option<Option<u64>> {
        context
            .context(store(agent))
            .map(|context| context.map(|context| context.tokens))
    }

    /// Captures contain native usage metadata only; their origins and redactions are described
    /// in testdata/record-captures.md. The old failed-turn capture is the native null near-miss.
    #[test]
    fn committed_native_captures_replay_through_the_context_readers() {
        let mut context = Context::default();
        let mut sizes = Vec::new();
        for line in include_str!("../../testdata/usage_claude_2.1.296.jsonl").lines() {
            context.fold_claude(&serde_json::from_str(line).unwrap());
            sizes.push(tokens_of(&context, AgentId::CLAUDE).unwrap().unwrap());
        }
        // Each request's input, cached input included and the reply left out.
        assert_eq!(sizes, [48_762, 48_762, 48_762, 52_051, 52_051]);
        let codex = include_str!("../../testdata/usage_codex_0.162.1.jsonl");
        assert_eq!(codex_figure(codex), Some((18_829, Some(475_000))));
        assert!(include_str!("../../testdata/rollout_running_turn.jsonl")
            .lines()
            .all(|line| codex_figure(line).is_none()));
        let mut grok = Context::default();
        for line in include_str!("../../testdata/context_grok_1.0.62_updates.jsonl").lines() {
            grok.fold_grok(line);
        }
        // The compaction left 25,032; the stale line repeating 205,620 was skipped.
        assert_eq!(tokens_of(&grok, AgentId::GROK), Some(Some(25_500)));
    }

    #[test]
    fn claudes_context_is_the_last_main_requests_input_and_nothing_else_counts() {
        let usage = json!({"input_tokens":2,"cache_read_input_tokens":40,
            "cache_creation_input_tokens":8,"output_tokens":10});
        let main = claude_line(json!({}), usage);
        assert_eq!(
            tokens_of(&claude(std::slice::from_ref(&main)), AgentId::CLAUDE),
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
                tokens_of(&claude(&[main.clone(), later]), AgentId::CLAUDE),
                Some(Some(50))
            );
        }
        let synthetic = json!({"type":"assistant","message":{"model":"<synthetic>","usage":big}});
        let empty = claude_line(
            json!({}),
            json!({"input_tokens":0,"cache_read_input_tokens":0,"output_tokens":5}),
        );
        assert_eq!(
            tokens_of(&claude(&[main.clone(), synthetic, empty]), AgentId::CLAUDE),
            Some(Some(50))
        );
        // The last line wins, repeated or not.
        let later = claude_line(json!({}), json!({"input_tokens":70,"output_tokens":1}));
        assert_eq!(
            tokens_of(&claude(&[main.clone(), main, later]), AgentId::CLAUDE),
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
            tokens_of(&claude(&[claude_line(json!({}), usage)]), AgentId::CLAUDE),
            Some(Some(50))
        );
        let unusable = json!({"input_tokens":7,"iterations":[{"type":"advisor_message"}]});
        assert_eq!(
            tokens_of(
                &claude(&[claude_line(json!({}), unusable)]),
                AgentId::CLAUDE
            ),
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
        assert_eq!(context.context(store(AgentId::CLAUDE)), Some(None));
        context.fold_claude(&next);
        let shown = context.context(store(AgentId::CLAUDE)).unwrap().unwrap();
        assert_eq!(
            (shown.tokens, shown.window, shown.used_percent),
            (9, None, None)
        );
        context.forget();
        assert_eq!(context.context(store(AgentId::CLAUDE)), None);
    }

    /// The window comes from beside the transcript, so a compaction leaves it for the next request,
    /// and the share is Claude Code's own, rounded.
    #[test]
    fn claudes_reported_window_outlasts_a_compaction_and_sizes_the_share_rounded() {
        let request = claude_line(json!({}), json!({"input_tokens":1_000,"output_tokens":1}));
        let mut context = claude(std::slice::from_ref(&request));
        context.set_window(Some(200_000));
        let shown = context.context(store(AgentId::CLAUDE)).unwrap().unwrap();
        assert_eq!(
            (shown.tokens, shown.window, shown.used_percent),
            (1_000, Some(200_000), Some(1))
        );
        context.fold_claude(&json!({"type":"system","subtype":"compact_boundary"}));
        assert_eq!(context.context(store(AgentId::CLAUDE)), Some(None));
        context.fold_claude(&claude_line(
            json!({}),
            json!({"input_tokens":2_000,"cache_read_input_tokens":96_000,"output_tokens":1}),
        ));
        let shown = context.context(store(AgentId::CLAUDE)).unwrap().unwrap();
        assert_eq!(
            (shown.tokens, shown.window, shown.used_percent),
            (98_000, Some(200_000), Some(49))
        );
        assert_eq!(used_percent(AgentId::CLAUDE, 0, 200_000), Some(0));
        assert_eq!(used_percent(AgentId::CLAUDE, 999, 200_000), Some(0));
        assert_eq!(used_percent(AgentId::CLAUDE, 1_001, 200_000), Some(1));
        assert_eq!(used_percent(AgentId::CLAUDE, 199_000, 200_000), Some(100));
        assert_eq!(used_percent(AgentId::CLAUDE, 900_000, 200_000), Some(100));
    }

    #[test]
    fn reported_windows_keep_the_last_report_of_the_sessions_still_there() {
        let windows = ReportedWindows::default();
        windows.report("a", 200_000);
        windows.report("a", 1_000_000);
        windows.report("b", 200_000);
        let kept = windows.retain(&HashSet::from(["a"]));
        assert_eq!(kept, HashMap::from([("a".to_owned(), 1_000_000)]));
        assert!(windows.retain(&HashSet::from(["b"])).is_empty());
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
        let shown = context.context(store(AgentId::CODEX)).unwrap().unwrap();
        assert_eq!(shown.tokens, 18_999);
        assert_eq!(shown.window, Some(475_000));
        assert_eq!(shown.used_percent, Some(2));
        assert_eq!(used_percent(AgentId::CODEX, 12_000, 475_000), Some(0));
        assert_eq!(used_percent(AgentId::CODEX, 0, 475_000), Some(0));
        assert_eq!(used_percent(AgentId::CODEX, 475_000, 475_000), Some(100));
        assert_eq!(used_percent(AgentId::CODEX, 900_000, 475_000), Some(100));
        // A window no larger than the baseline has nothing left, which Codex shows as 0% left.
        assert_eq!(used_percent(AgentId::CODEX, 5, 12_000), Some(100));
        assert_eq!(used_percent(AgentId::CODEX, 5, 8_000), Some(100));
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
        assert_eq!(tokens_of(&context, AgentId::GROK), Some(Some(200)));
        context.fold_grok(&compacted(200, 30));
        context.fold_grok(&chunk(200));
        assert_eq!(
            tokens_of(&context, AgentId::GROK),
            Some(Some(30)),
            "the stale line is skipped"
        );
        context.fold_grok(&chunk(35));
        assert_eq!(tokens_of(&context, AgentId::GROK), Some(Some(35)));
        context.fold_grok(&chunk(200));
        assert_eq!(
            tokens_of(&context, AgentId::GROK),
            Some(Some(200)),
            "the stale value counts only once"
        );
        context.fold_grok(&chunk(25_500));
        context.set_window(Some(256_000));
        let shown = context.context(store(AgentId::GROK)).unwrap().unwrap();
        assert_eq!((shown.window, shown.used_percent), (Some(256_000), Some(9)));
    }
}
