//! The standing notices of each session's running launch (`Session::standing_notices`): kept
//! while their condition holds, served on the record a client receives, and dropped with the
//! launch's process or the record.
//!
//! A daemon that adopts the launch restores those its spawn had from the terminal's meta, which
//! the holder keeps as it was at the spawn. So a notice the meta has that is withdrawn later is
//! also written down in the launch's scratch directory, which the adopting daemon keeps
//! ([`withdrawn_in`]), and is not brought back: whatever withdrew it may never say so again.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::AppState;
use crate::protocol::{Code, Event, Notice, StandingNotice};

/// The file in a launch's scratch directory that lists its withdrawn notices ([`withdrawn_in`]).
const WITHDRAWN_FILE: &str = "withdrawn-notices.json";

/// Taken around reading and rewriting a [`WITHDRAWN_FILE`], so that two withdrawals written at
/// once both land.
static WITHDRAWN_WRITES: Mutex<()> = Mutex::new(());

/// A standing notice withdrawn from a launch: its code, its `detail` param where it has one, and
/// when it was raised, which together tell the very notice the terminal's meta keeps from another
/// of the code (the notices of a spawn share one time) and from one raised again later. A file
/// written without the detail reads as a notice without one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Withdrawn {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub at: i64,
}

impl Withdrawn {
    fn of(notice: &StandingNotice) -> Self {
        Self {
            code: notice.code.clone(),
            detail: detail_of(notice).map(str::to_string),
            at: notice.at,
        }
    }

    /// Whether `notice` is the one withdrawn.
    pub(crate) fn is(&self, notice: &StandingNotice) -> bool {
        self.code == notice.code
            && self.at == notice.at
            && self.detail.as_deref() == detail_of(notice)
    }
}

/// The `detail` param of `notice`, which tells it from another of its code.
fn detail_of(notice: &StandingNotice) -> Option<&str> {
    notice.params.get("detail").map(String::as_str)
}

/// The notices withdrawn from the launch whose scratch directory is `scratch`; none where nothing
/// was written down or the file cannot be read. **Blocks.**
pub(crate) fn withdrawn_in(scratch: &Path) -> Vec<Withdrawn> {
    std::fs::read(scratch.join(WITHDRAWN_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Adds `withdrawn` to those written down in `scratch`. A scratch directory gone with its process
/// leaves nothing to write into, and a failure only lets a later adoption restore the notice.
/// **Blocks.**
fn write_withdrawn(scratch: &Path, withdrawn: &[Withdrawn]) {
    let _writes = WITHDRAWN_WRITES
        .lock()
        .expect("withdrawn notices lock poisoned");
    if !scratch.is_dir() {
        return;
    }
    let mut all = withdrawn_in(scratch);
    all.extend_from_slice(withdrawn);
    let written = serde_json::to_vec(&all)
        .map_err(std::io::Error::other)
        .and_then(|bytes| {
            crate::hostfs::replace::replace_beside(
                &scratch.join(WITHDRAWN_FILE),
                &bytes,
                crate::hostfs::replace::Permissions::Private,
            )
        });
    if let Err(err) = written {
        tracing::warn!(scratch = %scratch.display(), %err, "writing down a withdrawn notice failed");
    }
}

/// The standing notices of every session that has some, by session id, each list in the order
/// raised. Held in memory only: they describe the running launch, so they go with its process,
/// and a daemon that adopts the launch restores those its spawn had from the terminal's meta.
#[derive(Default)]
pub(super) struct StandingNotices(Mutex<HashMap<String, Vec<StandingNotice>>>);

impl StandingNotices {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<StandingNotice>>> {
        self.0.lock().expect("standing notices lock poisoned")
    }
}

/// Whether `kept` is the notice of `code` with `detail`: a `detail` of `None` matches every
/// notice of the code.
fn matches(kept: &StandingNotice, code: &str, detail: Option<&str>) -> bool {
    kept.code == code && detail.is_none_or(|detail| detail_of(kept) == Some(detail))
}

impl AppState {
    /// Raises `notice` for session `session` as a standing notice raised at `at` (milliseconds
    /// since the epoch; a launch's notices take the time its terminal meta keeps for them, so a
    /// daemon that adopts the launch restores the very same notice): keeps it on the session's
    /// record, tells every client with a `session_notice` that says it stands, and publishes the
    /// session. A notice already kept with the same code and `detail` param is replaced, in its
    /// place in the order.
    pub fn raise_standing_notice(&self, session: &str, notice: Notice, at: i64) {
        let notice = notice.standing();
        let kept = notice.kept_at(at);
        {
            let mut all = self.standing_notices.lock();
            let list = all.entry(session.to_string()).or_default();
            match list
                .iter_mut()
                .find(|other| matches(other, &kept.code, notice.detail()))
            {
                Some(other) => *other = kept,
                None => list.push(kept),
            }
        }
        self.broadcast(notice.about(session));
        self.publish_session_by_id(session);
    }

    /// Withdraws session `session`'s standing notices of `code`, only the one whose `detail`
    /// param is `detail` where it is given: drops them, tells every client with
    /// `session_notice_withdrawn`, and publishes the session. Nothing is sent when none was kept.
    pub fn withdraw_standing_notice(&self, session: &str, code: Code, detail: Option<&str>) {
        let dropped: Vec<Withdrawn> = {
            let mut all = self.standing_notices.lock();
            let Some(list) = all.get_mut(session) else {
                return;
            };
            let dropped = list
                .iter()
                .filter(|kept| matches(kept, code.as_str(), detail))
                .map(Withdrawn::of)
                .collect();
            list.retain(|kept| !matches(kept, code.as_str(), detail));
            if list.is_empty() {
                all.remove(session);
            }
            dropped
        };
        if dropped.is_empty() {
            return;
        }
        self.write_down_withdrawn(session, dropped);
        self.broadcast(Event::SessionNoticeWithdrawn {
            session: session.to_string(),
            code,
            detail: detail.map(str::to_string),
        });
        self.publish_session_by_id(session);
    }

    /// Writes `withdrawn` down in the scratch directory of session `session`'s running launch
    /// ([`withdrawn_in`]), off the runtime's threads where there is a runtime.
    fn write_down_withdrawn(&self, session: &str, withdrawn: Vec<Withdrawn>) {
        let Some(scratch) = self
            .live_session(session)
            .and_then(|live| live.scratch_dir())
        else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn_blocking(move || write_withdrawn(&scratch, &withdrawn));
            }
            Err(_) => write_withdrawn(&scratch, &withdrawn),
        }
    }

    /// The sessions that have a standing notice of `code` kept, for a caller that withdraws it
    /// from those its condition no longer holds for.
    pub(crate) fn sessions_with_standing_notice(&self, code: Code) -> Vec<String> {
        self.standing_notices
            .lock()
            .iter()
            .filter(|(_, list)| list.iter().any(|kept| kept.code == code.as_str()))
            .map(|(session, _)| session.clone())
            .collect()
    }

    /// Puts back the standing notices `notices` an adopted launch of session `session` had at its
    /// spawn, as its terminal's meta kept them. Nothing is told: no client is connected yet.
    pub(crate) fn restore_standing_notices(&self, session: &str, notices: Vec<StandingNotice>) {
        if notices.is_empty() {
            return;
        }
        self.standing_notices
            .lock()
            .insert(session.to_string(), notices);
    }

    /// The standing notices kept for session `session`, in the order raised: none while it has no
    /// process.
    pub(crate) fn standing_notices_of(&self, session: &str) -> Vec<StandingNotice> {
        if self.live_session(session).is_none() {
            return Vec::new();
        }
        self.standing_notices
            .lock()
            .get(session)
            .cloned()
            .unwrap_or_default()
    }

    /// Drops session `session`'s standing notices, whose launch's process is gone or whose
    /// record is.
    pub(super) fn forget_standing_notices(&self, session: &str) {
        self.standing_notices.lock().remove(session);
    }
}

#[cfg(test)]
mod tests {
    /// A withdrawn notice is written down in the launch's scratch directory with the time it was
    /// raised, for a daemon that adopts the launch; a notice still standing is not.
    #[test]
    fn a_withdrawn_notice_is_written_down_in_the_scratch_directory() {
        let (state, dir) = app_state("standing-withdrawn-written-down");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let scratch = dir.join("scratch-s");
        std::fs::create_dir_all(&scratch).unwrap();
        let live = crate::test_support::fake_live_session_in(
            "s",
            crate::protocol::AgentId::TEST,
            "sleep 30",
            scratch.clone(),
        );
        state.register_live(live.clone());
        let notice = |detail: &str| {
            Notice::new(
                notice_code::HOOKS_NOT_RECEIVED,
                "No hooks came.",
                &[("detail", detail)],
            )
        };
        state.raise_standing_notice("s", notice("a"), 5);
        state.raise_standing_notice("s", notice("b"), 6);
        assert!(super::withdrawn_in(&scratch).is_empty());

        state.withdraw_standing_notice("s", notice_code::HOOKS_NOT_RECEIVED, Some("a"));
        assert_eq!(
            super::withdrawn_in(&scratch),
            [super::Withdrawn {
                code: notice_code::HOOKS_NOT_RECEIVED.as_str().to_string(),
                detail: Some("a".to_string()),
                at: 5,
            }]
        );
        live.end();
    }

    use crate::protocol::{notice_code, Event, Notice, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, idle_stand_in};

    fn degraded(detail: &str) -> Notice {
        Notice::new(
            notice_code::HOOKS_NOT_RECEIVED,
            "No hooks came.",
            &[("agent", "claude"), ("detail", detail)],
        )
    }

    /// A standing notice is on the record a client that connects afterwards receives, with the
    /// event that announced it marked standing; raising it again replaces it in place.
    #[test]
    fn a_standing_notice_is_served_on_the_record_and_replaced_in_place() {
        let (state, dir) = app_state("standing-served");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = idle_stand_in("s");
        state.register_live(live.clone());
        let mut events = state.subscribe();

        state.raise_standing_notice("s", degraded("a"), 1);
        state.raise_standing_notice("s", degraded("b"), 2);
        state.raise_standing_notice("s", degraded("a"), 3);

        let first = events.try_recv().unwrap();
        assert!(matches!(first, Event::SessionNotice { standing: true, .. }));
        let record = state.wire_session(state.session_record("s").unwrap());
        let kept: Vec<_> = record
            .standing_notices
            .iter()
            .map(|notice| (notice.params["detail"].as_str(), notice.at))
            .collect();
        assert_eq!(kept, [("a", 3), ("b", 2)]);
    }

    /// Withdrawn by code and detail, a notice leaves the record and clients are told; one never
    /// kept tells nothing.
    #[test]
    fn a_withdrawn_notice_leaves_the_record() {
        let (state, dir) = app_state("standing-withdrawn");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = idle_stand_in("s");
        state.register_live(live.clone());
        state.raise_standing_notice("s", degraded("a"), 1);
        state.raise_standing_notice("s", degraded("b"), 2);
        let mut events = state.subscribe();

        state.withdraw_standing_notice("s", notice_code::HOOKS_NOT_RECEIVED, Some("a"));

        assert!(matches!(
            events.try_recv().unwrap(),
            Event::SessionNoticeWithdrawn { detail: Some(ref detail), .. } if detail == "a"
        ));
        let record = state.wire_session(state.session_record("s").unwrap());
        assert_eq!(record.standing_notices.len(), 1);
        assert_eq!(record.standing_notices[0].params["detail"], "b");

        state.withdraw_standing_notice("s", notice_code::HOOKS_NOT_RECEIVED, None);
        while events.try_recv().is_ok() {}
        state.withdraw_standing_notice("s", notice_code::HOOKS_NOT_RECEIVED, None);
        assert!(events.try_recv().is_err());
        assert!(state
            .wire_session(state.session_record("s").unwrap())
            .standing_notices
            .is_empty());
    }

    /// The notices go with the launch's process: the record shows none once it has left, and a
    /// later launch starts with none.
    #[test]
    fn the_notices_go_with_the_process() {
        let (state, dir) = app_state("standing-process-gone");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = idle_stand_in("s");
        state.register_live(live.clone());
        state.raise_standing_notice("s", degraded("a"), 1);

        crate::coordinator::exit_watch::end_process_for_test(&state, "s");

        assert!(state
            .wire_session(state.session_record("s").unwrap())
            .standing_notices
            .is_empty());
        let later = idle_stand_in("s");
        state.register_live(later.clone());
        assert!(state
            .wire_session(state.session_record("s").unwrap())
            .standing_notices
            .is_empty());
    }

    /// The notices belong to the launch that raised them: a process another launch already
    /// replaced ending leaves the newer launch's notices on the record.
    #[test]
    fn a_replaced_launchs_end_leaves_the_newer_launchs_notices() {
        let (state, dir) = app_state("standing-replaced");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let older = idle_stand_in("s");
        state.register_live(older.clone());
        let newer = idle_stand_in("s");
        state.register_live(newer.clone());
        state.raise_standing_notice("s", degraded("newer"), 1);

        crate::coordinator::exit_watch::unregister_live(&state, &older);

        let record = state.wire_session(state.session_record("s").unwrap());
        assert_eq!(record.standing_notices.len(), 1);
        assert_eq!(record.standing_notices[0].params["detail"], "newer");
    }

    /// A session's notices go with its record, whichever way the record went, and what asks which
    /// sessions hold a notice of a code lists those and no others.
    #[test]
    fn a_deleted_sessions_notices_are_dropped_and_holders_of_a_code_are_listed() {
        let (state, dir) = app_state("standing-forgotten");
        for id in ["a", "b", "c"] {
            insert_record(&state, &dir, id, SessionStatus::Idle);
        }
        let lives: Vec<_> = ["a", "b", "c"].map(idle_stand_in).into();
        for live in &lives {
            state.register_live((**live).clone());
        }
        state.raise_standing_notice("a", degraded("x"), 1);
        state.raise_standing_notice(
            "b",
            Notice::new(
                notice_code::ACCOUNT_SIGN_IN_UNCONFIRMED,
                "Not confirmed.",
                &[("agent", "claude")],
            ),
            2,
        );
        state.raise_standing_notice("c", degraded("y"), 3);

        let mut holders = state.sessions_with_standing_notice(notice_code::HOOKS_NOT_RECEIVED);
        holders.sort();
        assert_eq!(holders, ["a", "c"]);

        state.forget_cascaded_sessions(&["a"]);

        let kept = |id: &str| state.standing_notices_of(id).len();
        assert_eq!((kept("a"), kept("b"), kept("c")), (0, 1, 1));
        assert_eq!(
            state.sessions_with_standing_notice(notice_code::HOOKS_NOT_RECEIVED),
            ["c"]
        );
    }

    /// An adopted launch gets back the notices its spawn had, as raised, in their order, with
    /// nobody told; an empty list restores nothing.
    #[test]
    fn restored_notices_are_served_in_their_order_and_tell_nobody() {
        let (state, dir) = app_state("standing-restored");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = idle_stand_in("s");
        state.register_live(live.clone());
        let kept: Vec<_> = [("a", 5), ("b", 6)]
            .map(|(detail, at)| degraded(detail).standing().kept_at(at))
            .into();
        let mut events = state.subscribe();

        state.restore_standing_notices("s", Vec::new());
        assert!(state.standing_notices_of("s").is_empty());
        state.restore_standing_notices("s", kept);

        assert!(events.try_recv().is_err(), "nobody is connected yet");
        let record = state.wire_session(state.session_record("s").unwrap());
        let shown: Vec<_> = record
            .standing_notices
            .iter()
            .map(|notice| (notice.params["detail"].as_str(), notice.at))
            .collect();
        assert_eq!(shown, [("a", 5), ("b", 6)]);
    }
}
