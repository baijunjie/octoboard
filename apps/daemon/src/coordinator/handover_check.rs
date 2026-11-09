//! `handover_check`: which running sessions and shell terminals a planned stop followed by a
//! daemon that adopts holders from one version up to another would not carry over. The desktop
//! application asks it before an update restart, to confirm only what the restart would end.
//!
//! A terminal is carried over when its holder's version lies within the next daemon's range, the
//! floor and the daemon's own version both inclusive (the holders that daemon cannot adopt are
//! ended, and their sessions interrupted: see `crate::holder::compat`, whose ordering this
//! uses). It is not when it runs in the daemon itself (no holders are installed, or its terminal
//! was not started in one, and a planned stop ends it), when it is claimed for a holder not linked
//! to and no record places it, or when its holder's version is unknown or cannot be read. A holder
//! still pending is not judged by this daemon's upper bound, so a version above the next daemon's
//! counts as not carried too.
//!
//! The answer is `known` only when the caller gave both ends of the range and both read; without
//! them nothing is known of what the next daemon adopts, and every running terminal is listed.
//!
//! The shell terminals of a holder still pending are not registered with the daemon: their holder
//! has not been reached, so whether one is busy is not known. Those that would not be carried
//! over are listed apart, as unreached.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::holder::compat::{Range, Verdict, Version};
use crate::protocol::ShellTerminalState;
use crate::state::AppState;
use crate::store::{HeldKind, HeldTerminalRecord};

/// The running sessions and shell terminals, by id, a planned stop would not carry over.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HandoverCheck {
    /// Whether the next daemon's range was given and read, so that the lists are a proven answer
    /// rather than everything running.
    pub known: bool,
    pub sessions: Vec<String>,
    /// The shell terminals registered with the daemon.
    pub shell_terminals: Vec<String>,
    /// The shell terminals of a pending holder, not registered, whose busy state is not known.
    pub unreached_shell_terminals: Vec<String>,
}

/// The versions the next daemon adopts holders of, or `None` unless both ends are given and read. Read
/// here, so that judging against it never logs a range that does not read.
fn next_range(floor: Option<&str>, target_version: Option<&str>) -> Option<Range> {
    let (floor, own) = (floor?, target_version?);
    Version::parse(floor)?;
    Version::parse(own)?;
    Some(Range {
        floor: floor.to_string(),
        own: own.to_string(),
    })
}

/// Answers `handover_check` for a next daemon whose build is `target_version` and whose oldest
/// adopted holder version is `holder_floor`; either unknown is `None`. **Blocks** briefly on the
/// state's locks.
pub fn check(
    state: &AppState,
    holder_floor: Option<&str>,
    target_version: Option<&str>,
) -> HandoverCheck {
    let range = next_range(holder_floor, target_version);
    // Every holder installed or pending gives its version; none installed means the terminals
    // run in the daemon, and no holder carries any of them.
    let holders = crate::holder::client::current();
    let versions = holders
        .map(|holders| holders.versions())
        .unwrap_or_default();
    let pending: HashSet<String> = holders
        .map(|holders| holders.pending_holders())
        .unwrap_or_default()
        .into_iter()
        .collect();

    // A session claimed for a holder that could not be linked to runs there without being live;
    // a live one is placed by its own terminal.
    let mut sessions: BTreeMap<String, Option<String>> =
        state.unlinked_claims().into_iter().collect();
    for live in state.live_sessions() {
        let holder = live.held_at().map(|at| at.holder_id);
        sessions.insert(live.id.clone(), holder);
    }
    // An exited shell runs nothing, so a planned stop loses nothing of it.
    let shell_terminals: BTreeMap<String, Option<String>> = state
        .shell_terminals
        .list()
        .into_iter()
        .filter(|terminal| terminal.state == ShellTerminalState::Running)
        .map(|terminal| {
            let holder = state
                .shell_terminals
                .process(&terminal.id)
                .and_then(|process| process.held_at())
                .map(|at| at.holder_id);
            (terminal.id, holder)
        })
        .collect();
    let unreached = if pending.is_empty() {
        BTreeMap::new()
    } else {
        let records = state.store.held_terminals().unwrap_or_else(|err| {
            tracing::warn!(%err, "the held terminals could not be read for the handover check");
            Vec::new()
        });
        unreached_shells(records, &pending, &shell_terminals)
    };

    HandoverCheck {
        known: range.is_some(),
        sessions: not_carried(&range, &versions, placed(&sessions)),
        shell_terminals: not_carried(&range, &versions, placed(&shell_terminals)),
        unreached_shell_terminals: not_carried(&range, &versions, placed(&unreached)),
    }
}

/// The shell terminals `records` place in a holder of `pending` that are not among the
/// `registered` ones, by id with the id of their holder.
fn unreached_shells(
    records: Vec<HeldTerminalRecord>,
    pending: &HashSet<String>,
    registered: &BTreeMap<String, Option<String>>,
) -> BTreeMap<String, Option<String>> {
    records
        .into_iter()
        .filter(|record| record.kind == HeldKind::ShellTerminal)
        .filter(|record| pending.contains(&record.holder_id))
        .filter(|record| !registered.contains_key(&record.owner_id))
        .map(|record| (record.owner_id, Some(record.holder_id)))
        .collect()
}

/// Each id of `running` with the id of the holder that runs it, in the order of the ids.
fn placed(
    running: &BTreeMap<String, Option<String>>,
) -> impl Iterator<Item = (&String, Option<&str>)> {
    running.iter().map(|(id, holder)| (id, holder.as_deref()))
}

/// The ids among `running` that a daemon adopting the holders within `range` would not carry
/// over, in the order given: each is paired with the id of the holder that runs it, `None` where
/// none is known. `versions` has each holder's version by id. Without a `range`, nothing is
/// known to be carried over.
fn not_carried<'a>(
    range: &Option<Range>,
    versions: &HashMap<String, String>,
    running: impl IntoIterator<Item = (&'a String, Option<&'a str>)>,
) -> Vec<String> {
    running
        .into_iter()
        .filter(|(_, holder)| {
            let carried = match (range, holder) {
                (Some(range), Some(holder)) => versions
                    .get(*holder)
                    .is_some_and(|version| range.judge(version) == Verdict::Adoptable),
                _ => false,
            };
            !carried
        })
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AgentId, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session};

    fn versions() -> HashMap<String, String> {
        [
            ("old", "0.9.0"),
            ("floor", "1.0.0"),
            ("new", "1.2.0-beta.1"),
            ("newer", "3.0.0"),
            ("odd", "x"),
        ]
        .into_iter()
        .map(|(id, version)| (id.to_string(), version.to_string()))
        .collect()
    }

    fn run(
        floor: Option<&str>,
        target: Option<&str>,
        running: &[(&str, Option<&str>)],
    ) -> Vec<String> {
        let ids: Vec<String> = running.iter().map(|(id, _)| id.to_string()).collect();
        not_carried(
            &next_range(floor, target),
            &versions(),
            ids.iter()
                .zip(running)
                .map(|(id, (_, holder))| (id, *holder)),
        )
    }

    #[test]
    fn a_holder_outside_the_range_or_unreadable_or_unplaced_is_not_carried() {
        let running = [
            ("below", Some("old")),
            ("at-floor", Some("floor")),
            ("above-floor", Some("new")),
            ("above-target", Some("newer")),
            ("unreadable", Some("odd")),
            ("unknown-holder", Some("missing")),
            ("in-daemon", None),
        ];
        assert_eq!(
            run(Some("1.0.0"), Some("2.0.0"), &running),
            [
                "below",
                "above-target",
                "unreadable",
                "unknown-holder",
                "in-daemon"
            ]
        );
    }

    /// Both ends are inclusive, as `holder::compat` adopts holders.
    #[test]
    fn a_holder_at_either_end_of_the_range_is_carried() {
        let running = [("a", Some("floor")), ("b", Some("newer"))];
        assert!(run(Some("1.0.0"), Some("3.0.0"), &running).is_empty());
    }

    #[test]
    fn a_prerelease_end_orders_as_semantic_versioning_does() {
        let running = [("a", Some("floor")), ("b", Some("new"))];
        assert_eq!(run(Some("1.2.0-alpha"), Some("2.0.0"), &running), ["a"]);
        assert_eq!(run(Some("1.2.0"), Some("2.0.0"), &running), ["a", "b"]);
        assert_eq!(run(Some("1.0.0"), Some("1.2.0-alpha"), &running), ["b"]);
    }

    /// No floor or target version, or one that does not read, is not known to carry anything
    /// over.
    #[test]
    fn an_unknown_range_counts_every_terminal() {
        let running = [("a", Some("new")), ("b", None)];
        assert_eq!(run(None, Some("2.0.0"), &running), ["a", "b"]);
        assert_eq!(run(Some("1.0.0"), None, &running), ["a", "b"]);
        assert_eq!(
            run(Some("not a version"), Some("2.0.0"), &running),
            ["a", "b"]
        );
        assert_eq!(
            run(Some("1.0.0"), Some("not a version"), &running),
            ["a", "b"]
        );
        assert!(next_range(Some("1.0.0"), Some("2.0.0")).is_some());
        assert!(next_range(Some("1.0.0"), None).is_none());
    }

    #[test]
    fn nothing_running_is_nothing_interrupted() {
        assert!(run(Some("1.0.0"), Some("2.0.0"), &[]).is_empty());
        assert!(run(None, None, &[]).is_empty());
    }

    fn held(kind: HeldKind, owner: &str, holder: &str) -> HeldTerminalRecord {
        HeldTerminalRecord {
            kind,
            owner_id: owner.to_string(),
            holder_id: holder.to_string(),
            terminal_id: format!("t-{owner}"),
            launch_id: format!("l-{owner}"),
        }
    }

    /// Only the shell terminals recorded for a pending holder count, and one the daemon registers
    /// already is not listed twice.
    #[test]
    fn the_unreached_shells_are_those_of_pending_holders_not_registered() {
        let records = vec![
            held(HeldKind::ShellTerminal, "pending-shell", "p"),
            held(HeldKind::ShellTerminal, "registered", "p"),
            held(HeldKind::ShellTerminal, "elsewhere", "q"),
            held(HeldKind::Session, "a-session", "p"),
        ];
        let pending: HashSet<String> = ["p".to_string()].into();
        let registered: BTreeMap<String, Option<String>> =
            [("registered".to_string(), Some("p".to_string()))].into();
        let unreached = unreached_shells(records, &pending, &registered);
        assert_eq!(
            unreached.into_iter().collect::<Vec<_>>(),
            [("pending-shell".to_string(), Some("p".to_string()))]
        );
    }

    /// Where the terminals run in the daemon itself, a planned stop ends every one of them
    /// whatever the floor.
    #[tokio::test]
    async fn terminals_running_in_the_daemon_are_not_carried() {
        let (state, dir) = app_state("handover-check-in-daemon");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());

        let check = check(&state, Some("0.0.0"), Some("9.9.9"));
        assert!(check.known);
        assert_eq!(check.sessions, ["s"]);
        assert!(check.shell_terminals.is_empty());
        assert!(check.unreached_shell_terminals.is_empty());
    }

    /// A session claimed for a holder not linked to is listed with the running ones, once, in the
    /// order of the ids.
    #[tokio::test]
    async fn claimed_sessions_are_listed_with_the_live_ones() {
        let (state, dir) = app_state("handover-check-claimed");
        for id in ["b", "a", "c"] {
            insert_record(&state, &dir, id, SessionStatus::Working);
        }
        let live = fake_live_session("b", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        state.claim_unlinked("a", Some(("h", "launch-a")));
        state.claim_unlinked("c", None);

        let unknown = check(&state, None, None);
        assert!(!unknown.known);
        assert_eq!(unknown.sessions, ["a", "b", "c"]);
    }
}
