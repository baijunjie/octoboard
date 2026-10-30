//! Deleting the archived sessions whose conversation the agent can no longer resume. Agents expire
//! their own conversation records, and an archived session whose record is gone can only fail
//! when reopened, so a daemon task scans the archive at start and about once a day after, and
//! deletes what is proven unresumable the way the user's own delete does. Whether a conversation
//! can be resumed is `agent_records.rs`'s business; this module decides what that lets it delete.
//! The user can turn the cleanup off in the settings (`Settings.auto_delete_unresumable_archives`).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::task::JoinHandle;

use crate::agent_records::{self, Locator, Resumability};
use crate::branch_origin;
use crate::coordinator;
use crate::hostfs;
use crate::protocol::{now_millis, Session, SessionStatus};
use crate::state::AppState;

/// How often the task wakes to see whether a scan is due. It is short against the scan period
/// because the clock that counts is the wall clock: a timer of the whole period does not advance
/// while the machine sleeps, so a machine that sleeps most nights would fire it days late.
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

/// How much wall-clock time passes between two scans.
const SCAN_EVERY_MILLIS: i64 = 24 * 60 * 60 * 1000;

/// Where a scan gets the locator that finds the agents' records.
type LocatorSource = Arc<dyn Fn() -> Locator + Send + Sync>;

/// How a scan moves an agent record to the Trash.
type TrashRecord = Arc<dyn Fn(&Path) -> Result<()> + Send + Sync>;

/// Starts the task that scans the archive: once now, which also catches up on the time the
/// machine was off, then whenever a day of wall-clock time has passed since the last scan. The
/// time of the last scan is not kept across restarts, as the scan at start covers them. It holds
/// the state weakly, so a test or a shutdown can release it.
pub fn spawn(state: &Arc<AppState>) {
    spawn_with(
        state,
        CHECK_EVERY,
        SCAN_EVERY_MILLIS,
        Arc::new(Locator::default),
        Arc::new(hostfs::move_to_trash),
    );
}

fn spawn_with(
    state: &Arc<AppState>,
    check_every: Duration,
    scan_every_millis: i64,
    locator: LocatorSource,
    trash: TrashRecord,
) -> JoinHandle<()> {
    let weak = Arc::downgrade(state);
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(check_every);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_scan = None;
        loop {
            ticks.tick().await;
            let Some(state) = weak.upgrade() else { return };
            let now = now_millis();
            if !scan_due(last_scan, now, scan_every_millis) {
                continue;
            }
            last_scan = Some(now);
            scan(&state, locator.clone(), trash.clone()).await;
        }
    })
}

/// Whether a scan is due at `now`. A clock set back past the last scan counts as due, as waiting
/// for it to catch up could take any length of time.
fn scan_due(last_scan: Option<i64>, now: i64, every_millis: i64) -> bool {
    last_scan.is_none_or(|last| !(last..last + every_millis).contains(&now))
}

/// One scan: deletes the archived sessions the agent has proven gone
/// ([`Resumability::Unresumable`]; unknown never counts), then removes the branches and worktrees
/// they bound with nothing forced and moves the agent's remaining records to the Trash. Nothing is
/// scanned while the user has turned the cleanup off, and a setting that cannot be read counts as
/// off, as the scan only destroys; it is read on each scan and again before each delete, so a
/// change needs no restart and takes hold mid-scan. What the branch removal leaves is logged and
/// no client is told, as nobody asked for the deletion. Pinned and locked sessions are skipped
/// before judging. A group (an archived session with the sessions bound to it, at both levels)
/// goes only whole: one member that is resumable, unknown, pinned, locked or not archived keeps
/// all of it, as a delete of its owner would take the member along. Failures are logged and never
/// stop the scan.
async fn scan(state: &Arc<AppState>, locator: LocatorSource, trash: TrashRecord) {
    match state.store.auto_delete_unresumable_archives() {
        Ok(true) => {}
        Ok(false) => return,
        Err(err) => {
            tracing::warn!(%err, "reading whether to delete unresumable archived sessions failed");
            return;
        }
    }
    let deleting = {
        let (state, locator) = (state.clone(), locator.clone());
        tokio::task::spawn_blocking(move || delete_unresumable_groups(&state, &locator()))
    };
    let deleted = match deleting.await {
        Ok(deleted) => deleted,
        Err(err) => {
            tracing::warn!(%err, "deleting the archived sessions that cannot be resumed did not finish");
            return;
        }
    };
    if deleted.is_empty() {
        return;
    }
    branch_origin::cleanup::remove_logging_left(state.clone(), deleted.clone()).await;
    coordinator::trash_agent_records_with(state, deleted, locator(), move |path| trash(path)).await;
}

/// Deletes each group of archived sessions that is wholly unresumable and returns every session
/// deleted. A group starts at an archived session whose owner is not archived, as the sessions
/// below an archived owner go with it. The groups after one that found the setting off are not
/// judged.
fn delete_unresumable_groups(state: &Arc<AppState>, locator: &Locator) -> Vec<Session> {
    let mut deleted = Vec::new();
    let sessions = match state.store.list_sessions() {
        Ok(sessions) => sessions,
        Err(err) => {
            tracing::warn!(%err, "listing the sessions to scan for unresumable ones failed");
            return deleted;
        }
    };
    let archived_ids: HashSet<&str> = sessions
        .iter()
        .filter(|session| session.status == SessionStatus::Archived)
        .map(|session| session.id.as_str())
        .collect();
    let roots = sessions.iter().filter(|session| {
        archived_ids.contains(session.id.as_str())
            && !session
                .bound_to
                .as_deref()
                .is_some_and(|owner| archived_ids.contains(owner))
    });
    for root in roots {
        match delete_group_if_unresumable(state, locator, &coordinator::group_in(&sessions, root)) {
            Ok(Judged::SwitchedOff) => break,
            Ok(Judged::Deleted(gone)) => {
                for session in &gone {
                    tracing::info!(
                        session = %session.id,
                        title = %session.title,
                        agent = ?session.agent,
                        agent_session_id = ?session.agent_session_id,
                        "deleted an archived session whose conversation is gone"
                    );
                }
                deleted.extend(gone);
            }
            Err(err) => {
                tracing::warn!(session = %root.id, %err, "deleting an unresumable archived session failed");
            }
        }
    }
    deleted
}

/// Whether `session` may be deleted by a scan at all: archived, and neither pinned nor locked.
fn deletable(session: &Session) -> bool {
    session.status == SessionStatus::Archived && !session.pinned && !session.locked
}

/// What [`delete_judged`] found.
enum Judged {
    /// The setting is off or cannot be read, so nothing was deleted and nothing more should be.
    SwitchedOff,
    /// The sessions deleted: none when the group no longer matched the judging.
    Deleted(Vec<Session>),
}

/// Deletes `group` (the root first, then the sessions below it) when every member is deletable and
/// unresumable, and returns the sessions deleted; nothing otherwise. The group comes from a list
/// read before the judging, so the root is read again first, and [`delete_judged`] checks the
/// whole group again at the delete.
fn delete_group_if_unresumable(
    state: &Arc<AppState>,
    locator: &Locator,
    group: &[&Session],
) -> Result<Judged> {
    let root = group[0];
    match state.store.get_session(&root.id)? {
        Some(current) if same_standing(&current, root) => {}
        _ => return Ok(Judged::Deleted(Vec::new())),
    }
    let unresumable = |session: &Session| {
        deletable(session)
            && agent_records::resumability(locator, session) == Resumability::Unresumable
    };
    if !group.iter().all(|session| unresumable(session)) {
        return Ok(Judged::Deleted(Vec::new()));
    }
    delete_judged(state, group)
}

/// Whether `a` and `b` are the same session in everything that decided its judgement.
fn same_standing(a: &Session, b: &Session) -> bool {
    (
        a.id.as_str(),
        a.status,
        a.pinned,
        a.locked,
        a.orchestration_role,
        a.has_conversation,
    ) == (
        b.id.as_str(),
        b.status,
        b.pinned,
        b.locked,
        b.orchestration_role,
        b.has_conversation,
    ) && a.agent == b.agent
        && a.agent_session_id == b.agent_session_id
        && a.config_dir == b.config_dir
        && a.bound_to == b.bound_to
}

/// Deletes the group that was `judged` (the root first) under the removal claim, and only if the
/// group now has the same members, each in the same standing: a session archived, reopened,
/// pinned, bound or unbound since the judging was never judged, and keeps the whole group, as does
/// a member that is launching or has a process. The claim keeps a lock from changing meanwhile.
/// The setting is read again under the claim, as a scan can run long after it read the setting:
/// turned off meanwhile, or unreadable, it deletes nothing and says so ([`Judged::SwitchedOff`]).
fn delete_judged(state: &Arc<AppState>, judged: &[&Session]) -> Result<Judged> {
    let _removal = state.claim_session_removal();
    match state.store.auto_delete_unresumable_archives() {
        Ok(true) => {}
        Ok(false) => return Ok(Judged::SwitchedOff),
        Err(err) => {
            tracing::warn!(%err, "reading whether to delete unresumable archived sessions failed");
            return Ok(Judged::SwitchedOff);
        }
    }
    let sessions = state.store.list_sessions()?;
    let Some(root) = sessions.iter().find(|session| session.id == judged[0].id) else {
        return Ok(Judged::Deleted(Vec::new()));
    };
    let current = coordinator::group_in(&sessions, root);
    let unchanged = current.len() == judged.len()
        && judged.iter().all(|before| {
            current
                .iter()
                .find(|now| now.id == before.id)
                .is_some_and(|now| same_standing(now, before) && deletable(now))
        });
    // A launch under way or a process still exiting leaves the group for a later scan, rather than
    // having the delete skip that member and take the rest.
    let busy = current.iter().any(|session| state.has_process(&session.id));
    if !unchanged || busy {
        return Ok(Judged::Deleted(Vec::new()));
    }
    let mut gone = Vec::new();
    // What was deleted before a failure still needs its branches and records cleaned up.
    if let Err(err) = coordinator::delete_archived_group(state, root, &mut gone) {
        tracing::warn!(session = %root.id, %err, "deleting an unresumable archived group stopped partway");
    }
    Ok(Judged::Deleted(gone))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Agent, Event, OrchestrationRole, Role};
    use crate::test_support::{
        console_with_project_and_accounts, project_session, session_ids, PATIENCE,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// An account whose records the tests read, with a transcript for each id in `kept`.
    struct Account {
        dir: PathBuf,
    }

    impl Account {
        fn new(dir: &Path, kept: &[&str]) -> Self {
            let project_dir = dir.join("claude-account/projects/-work");
            std::fs::create_dir_all(&project_dir).unwrap();
            for id in kept {
                std::fs::write(
                    project_dir.join(format!("{}.jsonl", Self::native(id))),
                    "{}\n",
                )
                .unwrap();
            }
            Self {
                dir: dir.to_path_buf(),
            }
        }

        /// The agent session id of `id`: its bytes in hex, so that no two ids share one.
        fn native(id: &str) -> String {
            assert!(id.len() <= 16, "{id} is too long to spell as an id");
            let hex: String = format!(
                "{:0>32}",
                id.bytes().map(|b| format!("{b:02x}")).collect::<String>()
            );
            format!(
                "{}-{}-{}-{}-{}",
                &hex[..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..]
            )
        }

        /// A subagent folder left without its transcript: the record the Trash is to take.
        fn leftover(&self, id: &str) -> PathBuf {
            let folder = self
                .dir
                .join("claude-account/projects/-work")
                .join(Self::native(id));
            std::fs::create_dir_all(folder.join("subagents")).unwrap();
            folder
        }

        /// An archived Claude project session of the account; `owner` binds it.
        fn session(&self, id: &str, owner: Option<&str>) -> Session {
            Session {
                agent_session_id: Some(Self::native(id)),
                config_dir: Some(
                    self.dir
                        .join("claude-account")
                        .to_string_lossy()
                        .into_owned(),
                ),
                has_conversation: true,
                bound_to: owner.map(str::to_owned),
                orchestration_role: if owner.is_some() {
                    OrchestrationRole::Worker
                } else {
                    OrchestrationRole::Coordinator
                },
                ..project_session(id, Agent::Claude, SessionStatus::Archived)
            }
        }

        /// An archived console session of the account, which is what a lead session binds to.
        fn console_session(&self, id: &str) -> Session {
            Session {
                role: Role::Console,
                project_id: None,
                ..self.session(id, None)
            }
        }

        /// A coordinator under the console session `hub`.
        fn lead(&self, id: &str, hub: &str) -> Session {
            Session {
                orchestration_role: OrchestrationRole::Coordinator,
                ..self.session(id, Some(hub))
            }
        }

        fn locator(&self) -> LocatorSource {
            let home = self.dir.join("home").to_string_lossy().into_owned();
            Arc::new(move || {
                Locator::with_shell(HashMap::from([("HOME".to_string(), home.clone())]))
            })
        }
    }

    type Trashed = Arc<std::sync::Mutex<Vec<PathBuf>>>;

    fn recording_trash() -> (Trashed, TrashRecord) {
        let trashed = Trashed::default();
        let record = trashed.clone();
        let trash: TrashRecord = Arc::new(move |path| {
            record.lock().unwrap().push(path.to_path_buf());
            Ok(())
        });
        (trashed, trash)
    }

    fn with(status: SessionStatus, session: Session) -> Session {
        Session { status, ..session }
    }

    fn grok(session: Session) -> Session {
        Session {
            agent: Agent::Grok,
            ..session
        }
    }

    /// The sessions a [`delete_judged`] deleted; it fails the test when the setting was found off.
    fn gone(judged: Judged) -> Vec<Session> {
        match judged {
            Judged::Deleted(gone) => gone,
            Judged::SwitchedOff => panic!("the setting was found off"),
        }
    }

    /// The ids of the `session_deleted` events waiting in `events`, sorted.
    fn deleted_in(events: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<String> {
        let mut deleted = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let Event::SessionDeleted { session } = event {
                deleted.push(session);
            }
        }
        deleted.sort();
        deleted
    }

    /// Only an archived session proven unresumable is deleted, with its agent records trashed:
    /// pinned, locked and resumable ones are kept, and so is one whose answer is unknown.
    #[tokio::test]
    async fn the_cleanup_deletes_only_unresumable_archived_sessions() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-lone");
        let account = Account::new(&dir, &["resumable"]);
        let leftover = account.leftover("gone");
        // Grok restores a missing session from its remote registry, so its absence proves nothing.
        for session in [
            account.session("gone", None),
            account.session("resumable", None),
            Session {
                pinned: true,
                ..account.session("pinned", None)
            },
            Session {
                locked: true,
                ..account.session("locked", None)
            },
            grok(account.session("unknown", None)),
            with(
                SessionStatus::Interrupted,
                account.session("not-archived", None),
            ),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let (trashed, trash) = recording_trash();
        scan(&state, account.locator(), trash).await;
        assert_eq!(
            session_ids(&state),
            ["locked", "not-archived", "pinned", "resumable", "unknown"]
        );
        assert_eq!(*trashed.lock().unwrap(), [leftover]);
    }

    /// With the setting off a scan deletes nothing, trashes nothing and does not even look for the
    /// agents' records; turning it back on lets the next scan delete as before.
    #[tokio::test]
    async fn the_cleanup_deletes_nothing_while_the_setting_is_off() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-off");
        let account = Account::new(&dir, &[]);
        let leftover = account.leftover("gone");
        state
            .store
            .insert_session(&account.session("gone", None))
            .unwrap();
        state
            .store
            .set_auto_delete_unresumable_archives(false)
            .unwrap();
        let (trashed, trash) = recording_trash();
        let consulted = Arc::new(AtomicUsize::new(0));
        let counting: LocatorSource = {
            let (consulted, locator) = (consulted.clone(), account.locator());
            Arc::new(move || {
                consulted.fetch_add(1, Ordering::SeqCst);
                locator()
            })
        };
        scan(&state, counting, trash.clone()).await;
        assert_eq!(consulted.load(Ordering::SeqCst), 0);
        assert_eq!(session_ids(&state), ["gone"]);
        assert!(trashed.lock().unwrap().is_empty());
        state
            .store
            .set_auto_delete_unresumable_archives(true)
            .unwrap();
        scan(&state, account.locator(), trash).await;
        assert!(session_ids(&state).is_empty());
        assert_eq!(*trashed.lock().unwrap(), [leftover]);
    }

    /// A group goes whole or not at all, and the sessions of every member go to the Trash. The
    /// hubs are console sessions, as a lead session binds to one in practice.
    #[tokio::test]
    async fn the_cleanup_deletes_a_group_only_when_every_member_is_unresumable() {
        use SessionStatus::{Idle, Interrupted};
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-groups");
        let account = Account::new(&dir, &["live-member", "live-owner", "live-leaf"]);
        let sessions = [
            // Wholly unresumable, over two levels.
            account.console_session("dead-hub"),
            account.lead("dead-lead", "dead-hub"),
            account.session("dead-leaf", Some("dead-lead")),
            // The only member that keeps a two-level group is the leaf: resumable, unknown, pinned.
            account.console_session("live-hub"),
            account.lead("live-lead", "live-hub"),
            account.session("live-leaf", Some("live-lead")),
            account.console_session("unk-hub"),
            account.lead("unk-lead", "unk-hub"),
            grok(account.session("unk-leaf", Some("unk-lead"))),
            account.console_session("pin-hub"),
            account.lead("pin-lead", "pin-hub"),
            Session {
                pinned: true,
                ..account.session("pin-leaf", Some("pin-lead"))
            },
            // A resumable member keeps the owner and the unresumable member beside it.
            account.console_session("mixed-hub"),
            account.session("live-member", Some("mixed-hub")),
            account.session("dead-member", Some("mixed-hub")),
            // A resumable owner keeps its unresumable member too.
            account.console_session("live-owner"),
            account.session("dead-under-live", Some("live-owner")),
            // An unknown member alone keeps a group whole.
            account.console_session("unknown-hub"),
            grok(account.session("unknown-member", Some("unknown-hub"))),
            // A member that is not archived keeps the group, at either level.
            account.console_session("open-hub"),
            with(
                Interrupted,
                account.session("open-member", Some("open-hub")),
            ),
            account.console_session("open2-hub"),
            account.lead("open2-lead", "open2-hub"),
            with(Idle, account.session("open2-leaf", Some("open2-lead"))),
            // The owner is not archived: a lead session is the root of its own group.
            with(Idle, account.console_session("running-hub")),
            account.lead("dead-root", "running-hub"),
            account.session("dead-root-leaf", Some("dead-root")),
            // And a bound session goes alone.
            account.session("dead-bound", Some("running-hub")),
        ];
        // Each session that would go leaves a folder only the Trash can take.
        let gone = [
            "dead-bound",
            "dead-hub",
            "dead-lead",
            "dead-leaf",
            "dead-root",
            "dead-root-leaf",
        ];
        let mut leftovers = Vec::new();
        for session in &sessions {
            state.store.insert_session(session).unwrap();
            if session.id.starts_with("dead-") {
                let leftover = account.leftover(&session.id);
                if gone.contains(&session.id.as_str()) {
                    leftovers.push(leftover);
                }
            }
        }
        let (trashed, trash) = recording_trash();
        let mut events = state.subscribe();
        scan(&state, account.locator(), trash).await;
        assert_eq!(deleted_in(&mut events), gone);
        let mut trashed = trashed.lock().unwrap().clone();
        trashed.sort();
        leftovers.sort();
        assert_eq!(trashed, leftovers);
    }

    /// A locked member keeps its group, and the scan goes on to the others.
    #[tokio::test]
    async fn the_cleanup_goes_on_after_a_group_it_keeps() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-continues");
        let account = Account::new(&dir, &[]);
        for session in [
            account.console_session("a-hub"),
            Session {
                locked: true,
                ..account.session("a-locked", Some("a-hub"))
            },
            account.session("b-solo", None),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let (_trashed, trash) = recording_trash();
        scan(&state, account.locator(), trash).await;
        assert_eq!(session_ids(&state), ["a-hub", "a-locked"]);
    }

    /// A session that stopped being what was judged, between the judging and the delete, keeps its
    /// group: archived and reopened, or pinned, since.
    #[test]
    fn a_group_that_changed_after_judging_is_not_deleted() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-stale");
        let account = Account::new(&dir, &[]);
        for session in [
            account.console_session("hub"),
            account.session("leaf", Some("hub")),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let judged = state.store.list_sessions().unwrap();
        let judged_group = || {
            let hub = judged.iter().find(|session| session.id == "hub").unwrap();
            coordinator::group_in(&judged, hub)
        };
        let leaf = |change: &dyn Fn(&mut Session)| {
            let mut session = state.store.get_session("leaf").unwrap().unwrap();
            change(&mut session);
            state.store.delete_session("leaf").unwrap();
            state.store.insert_session(&session).unwrap();
        };
        leaf(&|session| session.status = SessionStatus::Idle);
        assert!(gone(delete_judged(&state, &judged_group()).unwrap()).is_empty());
        leaf(&|session| {
            session.status = SessionStatus::Archived;
            session.pinned = true;
        });
        assert!(gone(delete_judged(&state, &judged_group()).unwrap()).is_empty());
        assert_eq!(session_ids(&state), ["hub", "leaf"]);
        leaf(&|session| session.pinned = false);
        assert_eq!(
            gone(delete_judged(&state, &judged_group()).unwrap()).len(),
            2
        );
        assert!(session_ids(&state).is_empty());
    }

    /// A session archived and bound to the group after it was judged was never judged, so the group
    /// stays whole.
    #[test]
    fn a_session_bound_to_the_group_after_judging_keeps_it() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-late-member");
        let account = Account::new(&dir, &[]);
        for session in [
            account.console_session("hub"),
            account.session("leaf", Some("hub")),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let judged = state.store.list_sessions().unwrap();
        let hub = judged.iter().find(|session| session.id == "hub").unwrap();
        let group = coordinator::group_in(&judged, hub);
        state
            .store
            .insert_session(&account.session("late", Some("hub")))
            .unwrap();
        assert!(gone(delete_judged(&state, &group).unwrap()).is_empty());
        assert_eq!(session_ids(&state), ["hub", "late", "leaf"]);
    }

    /// The setting turned off between the judging and the delete stops the delete, and turned back
    /// on lets the same judged group go.
    #[test]
    fn a_group_judged_before_the_setting_was_turned_off_is_not_deleted() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-turned-off");
        let account = Account::new(&dir, &[]);
        state
            .store
            .insert_session(&account.session("lone", None))
            .unwrap();
        let judged = state.store.list_sessions().unwrap();
        let group: Vec<&Session> = judged.iter().collect();
        state
            .store
            .set_auto_delete_unresumable_archives(false)
            .unwrap();
        assert!(matches!(
            delete_judged(&state, &group).unwrap(),
            Judged::SwitchedOff
        ));
        assert_eq!(session_ids(&state), ["lone"]);
        state
            .store
            .set_auto_delete_unresumable_archives(true)
            .unwrap();
        assert_eq!(gone(delete_judged(&state, &group).unwrap()).len(), 1);
    }

    /// A pass that finds the setting off under the claim deletes nothing, however many groups it
    /// has.
    #[test]
    fn a_pass_deletes_nothing_while_the_setting_is_off() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-pass-stops");
        let account = Account::new(&dir, &[]);
        for id in ["first", "second"] {
            state
                .store
                .insert_session(&account.session(id, None))
                .unwrap();
        }
        state
            .store
            .set_auto_delete_unresumable_archives(false)
            .unwrap();
        assert!(delete_unresumable_groups(&state, &(account.locator())()).is_empty());
        assert_eq!(session_ids(&state), ["first", "second"]);
    }

    /// A member being launched, or with a process, keeps the group: the delete would refuse it and
    /// take the rest.
    #[test]
    fn a_group_with_a_launching_member_is_not_deleted() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-busy");
        let account = Account::new(&dir, &[]);
        for session in [
            account.console_session("hub"),
            account.session("leaf", Some("hub")),
        ] {
            state.store.insert_session(&session).unwrap();
        }
        let judged = state.store.list_sessions().unwrap();
        let hub = judged.iter().find(|session| session.id == "hub").unwrap();
        let group = coordinator::group_in(&judged, hub);
        let claim = state.begin_launch("leaf").unwrap();
        assert!(gone(delete_judged(&state, &group).unwrap()).is_empty());
        assert_eq!(session_ids(&state), ["hub", "leaf"]);
        drop(claim);
        assert_eq!(gone(delete_judged(&state, &group).unwrap()).len(), 2);
    }

    /// What the branch removal leaves is not told to any client: no error alert is broadcast.
    #[tokio::test]
    async fn the_cleanup_does_not_alert_about_a_branch_it_leaves() {
        let (state, dir) = crate::test_support::app_state("archive-cleanup-branches");
        state
            .store
            .insert_console(&crate::test_support::console(&dir))
            .unwrap();
        let repo = crate::test_support::repo_with("archive-cleanup-repo", &[(b"a.txt", b"one\n")]);
        crate::test_support::insert_project(&state, &repo);
        let worktrees = crate::test_support::ScratchDir::new("archive-cleanup-worktrees");
        crate::test_support::branch_made_by(&state, &repo, "work", "dead");
        crate::test_support::branch_made_by(&state, &repo, "spare", "dead");
        let (worktree, spare) = (worktrees.join("work"), worktrees.join("spare"));
        for (branch, path) in [("work", &worktree), ("spare", &spare)] {
            crate::test_support::git(
                &repo,
                &["worktree", "add", "-q", path.to_str().unwrap(), branch],
            );
        }
        // A locked worktree is not removed without forcing.
        crate::test_support::git(&repo, &["worktree", "lock", worktree.to_str().unwrap()]);
        let account = Account::new(&dir, &[]);
        state
            .store
            .insert_session(&Session {
                project_id: Some("p1".to_string()),
                ..account.session("dead", None)
            })
            .unwrap();
        let mut events = state.subscribe();
        let (_trashed, trash) = recording_trash();
        scan(&state, account.locator(), trash).await;
        assert!(session_ids(&state).is_empty());
        assert!(worktree.exists(), "the locked worktree was left");
        assert!(!spare.exists(), "the removable worktree was removed");
        let branches = crate::test_support::git(&repo, &["branch", "--list", "spare"]);
        assert!(branches.is_empty(), "the removable branch was removed");
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(event, Event::Error { .. }), "{event:?}");
        }
    }

    async fn next_deleted_session(events: &mut tokio::sync::broadcast::Receiver<Event>) -> String {
        tokio::time::timeout(PATIENCE, async {
            loop {
                if let Event::SessionDeleted { session } = events.recv().await.unwrap() {
                    return session;
                }
            }
        })
        .await
        .expect("a scan")
    }

    /// The task scans when it starts and again once the scan period has passed on the wall clock,
    /// and ends once the state is gone.
    #[tokio::test]
    async fn the_cleanup_task_scans_at_start_and_on_every_period() {
        let (state, dir) = console_with_project_and_accounts("archive-cleanup-task");
        let account = Account::new(&dir, &[]);
        state
            .store
            .insert_session(&account.session("first", None))
            .unwrap();
        let mut events = state.subscribe();
        let (_trashed, trash) = recording_trash();
        let task = spawn_with(
            &state,
            Duration::from_millis(10),
            100,
            account.locator(),
            trash,
        );
        assert_eq!(next_deleted_session(&mut events).await, "first");
        state
            .store
            .insert_session(&account.session("second", None))
            .unwrap();
        assert_eq!(next_deleted_session(&mut events).await, "second");
        drop(state);
        tokio::time::timeout(PATIENCE, task)
            .await
            .expect("the task ended")
            .unwrap();
    }

    #[test]
    fn a_scan_is_due_at_start_and_once_the_period_has_passed_on_the_wall_clock() {
        let day = 24 * 60 * 60 * 1000;
        assert!(scan_due(None, 5, day));
        assert!(!scan_due(Some(1000), 1000 + day - 1, day));
        assert!(scan_due(Some(1000), 1000 + day, day));
        assert!(scan_due(Some(1000), 999, day), "a clock set back");
    }
}
