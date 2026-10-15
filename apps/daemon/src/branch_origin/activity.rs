//! What each session's hooks said it was doing: the tool calls it had running, and the working
//! directory its agent last reported. The tool-call signal and the worktree signal read it.
//!
//! In memory only. The daemon owns every agent process, so a restart begins with no session
//! running and no tool call missed by not having been there.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use serde_json::Value;

use super::CLOCK_SLACK_MS;
use crate::hooks::{self, ToolCall};
use crate::protocol::{Agent, Session};

/// How long before its start reached the daemon a tool call may already have been running. The
/// agents need not wait for the hook announcing a call before running it, and kill that hook after
/// `adapter::HOOK_TIMEOUT_SECS`, so a start can land that late, plus the clock's slack. A call's
/// end needs no such allowance: it can only land after the call is over.
pub(super) const HOOK_LATENCY_MS: i64 =
    crate::adapter::HOOK_TIMEOUT_SECS as i64 * 1000 + CLOCK_SLACK_MS;

/// How long a finished tool call is remembered. A branch is normally decided seconds after the
/// call that made it, since that call's own end prompts the look; one born longer ago than this
/// is past what the record can answer for.
pub(super) const CALL_MEMORY_MS: i64 = 10 * 60 * 1000;

/// The most finished tool calls remembered per session, so that a session calling tools in a tight
/// loop cannot grow the record without bound inside [`CALL_MEMORY_MS`].
const MAX_FINISHED_CALLS: usize = 1_024;

/// Every session's tool calls and working directory, as far as its hooks have told.
#[derive(Default)]
pub(super) struct SessionActivity {
    calls: HashMap<String, ToolCalls>,
    /// The working directory each session's latest own hook reported. Kept for the daemon's life,
    /// one path per session that ever ran: a session sitting idle in a worktree is still there.
    cwds: HashMap<String, PathBuf>,
    /// The latest end of a tool call no longer remembered. Whether a session was calling a tool
    /// at a time before it can no longer be told.
    forgotten_until: i64,
}

impl SessionActivity {
    /// Takes in what one hook of `session` says, arriving at `now` (milliseconds since the epoch).
    /// `agent` is `None` for a session with no running process, whose turn end is then not read:
    /// what ends a turn is the agent's to say.
    pub(super) fn heard(&mut self, session: &str, agent: Option<Agent>, payload: &Value, now: i64) {
        // A subagent's tool calls run in the session's name, but its working directory can be a
        // worktree of its own and its turn ending is not the session's.
        let subagent = hooks::is_subagent(payload);
        if !subagent {
            if let Some(cwd) = hooks::working_directory(payload) {
                self.cwds.insert(session.to_string(), PathBuf::from(cwd));
            }
        }
        if let Some(event) = hooks::event_name(payload) {
            let calls = self.calls.entry(session.to_string()).or_default();
            match hooks::tool_call(&event, payload) {
                Some(ToolCall::Started(id)) => calls.started(id, subagent, now),
                Some(ToolCall::Ended(id)) => calls.ended(id, now),
                None if !subagent && agent.is_some_and(|a| ends_turn(a, &event, payload)) => {
                    calls.close(now, |_| true);
                }
                None => {}
            }
        }
        self.prune(now);
    }

    /// Closes the calls `session` has open of its own, leaving its subagents': something other
    /// than a hook has shown that the session's turn is over, which a subagent running in the
    /// background outlives.
    pub(super) fn close_own(&mut self, session: &str, now: i64) {
        if let Some(calls) = self.calls.get_mut(session) {
            calls.close(now, |call| !call.by_subagent);
        }
    }

    /// The one session of `sessions` that had a tool call in flight at some moment between `from`
    /// and `to` (milliseconds since the epoch). `None` when none did, when several did, and when
    /// the record no longer reaches back to `from`.
    pub(super) fn alone_in_flight<'s>(
        &self,
        sessions: &'s [Session],
        from: i64,
        to: i64,
    ) -> Option<&'s Session> {
        if self.forgotten_until >= from {
            return None;
        }
        // A session that started after `to` was running nothing then, whatever an end it reported
        // with no start says.
        let mut busy = sessions.iter().filter(|session| {
            session.started_at - CLOCK_SLACK_MS <= to
                && self
                    .calls
                    .get(&session.id)
                    .is_some_and(|calls| calls.in_flight(from, to, session.ended_at))
        });
        match (busy.next(), busy.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    }

    /// The working directory the latest of `session`'s own hooks reported.
    pub(super) fn working_directory(&self, session: &str) -> Option<&PathBuf> {
        self.cwds.get(session)
    }

    fn prune(&mut self, now: i64) {
        let horizon = now - CALL_MEMORY_MS;
        let mut forgotten = self.forgotten_until;
        self.calls.retain(|_, calls| {
            while let Some(call) = calls.finished.front() {
                if call.end >= horizon && calls.finished.len() <= MAX_FINISHED_CALLS {
                    break;
                }
                forgotten = forgotten.max(call.end);
                calls.finished.pop_front();
            }
            !calls.is_empty()
        });
        self.forgotten_until = forgotten;
    }
}

/// Whether the event ends the session's turn, after which none of its tool calls is still running.
/// A Claude Code `Stop` that only pauses on background work does not, nor does Grok's
/// `idle_prompt`, which can fire while a long call is still running.
fn ends_turn(agent: Agent, event: &str, payload: &Value) -> bool {
    hooks::turn_cancelled(agent, event)
        || hooks::turn_end(agent, event, payload).is_some_and(|end| !end.backstop)
}

/// One session's tool calls, by when their start and end events arrived, in milliseconds since the
/// epoch.
///
/// The hooks are asynchronous on every agent, so the events of one call can arrive in either
/// order, and a call's end can arrive after its turn's. Whatever cannot be paired is kept on the
/// side of a call still running: reading a session as idle while its call was making the branch
/// would hand the branch to whichever other session was calling a tool, where reading it as busy
/// only leaves the branch unattributed.
#[derive(Default)]
struct ToolCalls {
    /// The calls whose start has arrived and whose end has not, by id.
    open: HashMap<String, OpenCall>,
    /// The earliest start that named no call, at most one of the session's own and one of its
    /// subagents'. No end can be paired with one, so only what closes calls wholesale closes it.
    unnamed: Vec<OpenCall>,
    finished: VecDeque<FinishedCall>,
}

#[derive(Clone, Copy)]
struct OpenCall {
    /// When its start arrived.
    start: i64,
    /// Whether a subagent's event started it.
    by_subagent: bool,
}

struct FinishedCall {
    id: Option<String>,
    /// When its start arrived; `None` while it has not, which leaves the call reaching back as far
    /// as the record does.
    start: Option<i64>,
    end: i64,
}

impl ToolCalls {
    fn is_empty(&self) -> bool {
        self.open.is_empty() && self.unnamed.is_empty() && self.finished.is_empty()
    }

    fn finished(&mut self, id: &str) -> Option<&mut FinishedCall> {
        self.finished
            .iter_mut()
            .find(|call| call.id.as_deref() == Some(id))
    }

    fn started(&mut self, id: Option<&str>, by_subagent: bool, at: i64) {
        let call = OpenCall {
            start: at,
            by_subagent,
        };
        let Some(id) = id else {
            if !self
                .unnamed
                .iter()
                .any(|open| open.by_subagent == by_subagent)
            {
                self.unnamed.push(call);
            }
            return;
        };
        match self.finished(id) {
            // Its end arrived first, or its turn's did.
            Some(call) => {
                call.start.get_or_insert(at);
            }
            None => {
                self.open.entry(id.to_string()).or_insert(call);
            }
        }
    }

    fn ended(&mut self, id: Option<&str>, at: i64) {
        let start = match id {
            Some(id) => match self.open.remove(id) {
                Some(call) => Some(call.start),
                // Already closed by its turn's end, which arrived first.
                None if self.finished(id).is_some() => return,
                None => None,
            },
            None => None,
        };
        self.finished.push_back(FinishedCall {
            id: id.map(str::to_string),
            start,
            end: at,
        });
    }

    /// Closes the open calls `which` picks, as ended `at`.
    fn close(&mut self, at: i64, which: impl Fn(&OpenCall) -> bool) {
        let ids: Vec<String> = self
            .open
            .iter()
            .filter(|(_, call)| which(call))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let call = self.open.remove(&id).expect("picked from the open calls");
            self.finished.push_back(FinishedCall {
                id: Some(id),
                start: Some(call.start),
                end: at,
            });
        }
        let (closed, kept): (Vec<OpenCall>, Vec<OpenCall>) =
            self.unnamed.drain(..).partition(|call| which(call));
        self.unnamed = kept;
        self.finished
            .extend(closed.into_iter().map(|call| FinishedCall {
                id: None,
                start: Some(call.start),
                end: at,
            }));
    }

    /// Whether a call was running at some moment between `from` and `to`. A session that has
    /// ended was running nothing after `ended_at`, whatever its record still holds open.
    fn in_flight(&self, from: i64, to: i64, ended_at: Option<i64>) -> bool {
        let started_by = |start: i64| start - HOOK_LATENCY_MS <= to;
        let open = ended_at.is_none_or(|ended| from <= ended)
            && self
                .open
                .values()
                .chain(&self.unnamed)
                .any(|call| started_by(call.start));
        open || self
            .finished
            .iter()
            .any(|call| from <= call.end && call.start.is_none_or(started_by))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_origin::tests::session_record;
    use serde_json::json;

    const T: i64 = 1_800_000_000_000;

    fn hook(event: &str, id: Option<&str>) -> Value {
        match id {
            Some(id) => json!({"hook_event_name": event, "tool_use_id": id}),
            None => json!({"hook_event_name": event}),
        }
    }

    /// Whether `session` alone, of a session started long before, was busy around `at`.
    fn busy(activity: &SessionActivity, at: i64) -> bool {
        let sessions = [session_record("s", T - 3_600_000, None)];
        activity.alone_in_flight(&sessions, at, at).is_some()
    }

    /// Every way the record pairs a call's two events, or fails to, errs towards a call still
    /// running.
    #[test]
    fn calls_are_paired_leaning_towards_still_running() {
        let claude = Some(Agent::Claude);
        let mut a = SessionActivity::default();

        // An end that arrived before its start reaches back until the start lands.
        a.heard("s", claude, &hook("PostToolUse", Some("c1")), T);
        assert!(busy(&a, T - 60_000), "an end alone reaches back");
        a.heard("s", claude, &hook("PreToolUse", Some("c1")), T + 1);
        assert!(!busy(&a, T - 60_000), "the late start bounds it");
        assert!(
            busy(&a, T - HOOK_LATENCY_MS + 1),
            "within the hook's latency"
        );

        // A turn's end closes what is open, and an end landing after it reopens nothing.
        a.heard("s", claude, &hook("PreToolUse", Some("c2")), T + 10_000);
        a.heard("s", claude, &hook("Stop", None), T + 20_000);
        a.heard("s", claude, &hook("PostToolUse", Some("c2")), T + 21_000);
        assert!(busy(&a, T + 15_000));
        assert!(!busy(&a, T + 30_000), "closed by the turn's end");
        assert!(!busy(&a, T - 60_000), "the late end did not reach back");

        // A start with no id stays open until what closes every call.
        a.heard("s", claude, &hook("PreToolUse", None), T + 40_000);
        a.heard("s", claude, &hook("PostToolUse", None), T + 41_000);
        assert!(busy(&a, T + 50_000), "an end with no id cannot close it");
        a.close_own("s", T + 60_000);
        assert!(!busy(&a, T + 70_000));

        // A subagent's calls count, but neither its directory nor its turn's end is the session's.
        let subagent = |event| json!({"hook_event_name": event, "agent_type": "x", "cwd": "/sub"});
        a.heard(
            "s",
            claude,
            &json!({"hook_event_name": "Stop", "cwd": "/own"}),
            T + 80_000,
        );
        a.heard("s", claude, &hook("PreToolUse", Some("c3")), T + 80_000);
        a.heard("s", claude, &subagent("Stop"), T + 85_000);
        assert!(busy(&a, T + 90_000), "a subagent's Stop closes nothing");
        assert_eq!(a.working_directory("s"), Some(&PathBuf::from("/own")));

        // A declined prompt ends the session's own calls, not a subagent's running on.
        let mut d = SessionActivity::default();
        let by_subagent =
            json!({"hook_event_name": "PreToolUse", "tool_use_id": "b", "agent_type": "x"});
        d.heard("s", claude, &hook("PreToolUse", Some("own")), T);
        d.heard("s", claude, &by_subagent, T);
        d.close_own("s", T + 10_000);
        assert!(busy(&d, T + 20_000), "the subagent's call is still running");
        d.heard("s", claude, &hook("Stop", None), T + 30_000);
        assert!(!busy(&d, T + 40_000), "the turn's end closes it");

        // A question to the user is not a call.
        let question = json!({"hook_event_name": "PreToolUse", "tool_use_id": "q", "tool_name": "AskUserQuestion"});
        let mut b = SessionActivity::default();
        b.heard("s", claude, &question, T);
        assert!(!busy(&b, T + 1_000));

        // An ended session ran nothing after its end, whatever it left open.
        let ended = [session_record("s", T - 3_600_000, Some(T + 100_000))];
        assert!(a.alone_in_flight(&ended, T + 90_000, T + 90_000).is_some());
        assert!(a
            .alone_in_flight(&ended, T + 200_000, T + 200_000)
            .is_none());

        // A session that started after the moment ran nothing then, even with an end alone.
        let mut c = SessionActivity::default();
        c.heard("s", claude, &hook("PostToolUse", Some("c4")), T);
        let late = [session_record("s", T - 1_000, None)];
        assert!(c.alone_in_flight(&late, T - 60_000, T - 60_000).is_none());
        assert!(c.alone_in_flight(&late, T - 500, T - 500).is_some());

        // Past the memory, nothing can be told about the time it covered, though a call still
        // open says it was running.
        c.heard("s", claude, &hook("PreToolUse", Some("c5")), T - 100);
        c.heard(
            "s",
            claude,
            &hook("Notification", None),
            T + CALL_MEMORY_MS + 1,
        );
        let early = [session_record("s", T - 3_600_000, None)];
        assert!(c.alone_in_flight(&early, T - 500, T - 500).is_none());
    }
}
