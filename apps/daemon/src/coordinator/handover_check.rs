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
//!
//! [`restart_impact`] reads the answer the way a client asks the user about it, for a command line
//! that has no client's view of the sessions (`POST /restart-check`, `octoboardd update` and
//! `octoboardd restart`). Its rule mirrors the UI's `restartConfirmation`
//! (`packages/ui/src/lifecycle/useAppExit.ts`), and the two must stay in step: a test of each runs
//! the cases in `apps/daemon/testdata/restart_impact_cases.json`.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::holder::compat::{Range, Verdict, Version};
use crate::protocol::{ShellTerminal, ShellTerminalState};
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

/// What a planned restart into a daemon of a given holder floor and version would do to what runs,
/// as a client asks the user about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartVerdict {
    /// Nothing that asks would end: every session and every busy shell terminal is carried over.
    /// An idle shell terminal at its prompt may still be closed, which does not ask on its own.
    Safe,
    /// The next daemon's range is known, and the sessions and shell terminals listed would end.
    Interrupts,
    /// The range is not known, so every running session and busy shell terminal may end.
    MayInterrupt,
}

/// A session that [`RestartImpact`] counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactSession {
    pub id: String,
    /// Its title; empty where its record could not be read.
    pub title: String,
    /// Its status as the protocol names it (`working`, `waiting_user`, `idle`...); empty where its
    /// record could not be read. A string, so that a command line of another build reads a status
    /// it does not know.
    pub status: String,
}

/// A shell terminal that [`RestartImpact`] counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactTerminal {
    pub id: String,
    /// What its tab shows; empty where the daemon no longer lists it.
    pub title: String,
}

/// What a planned restart would interrupt, read as a client reads [`HandoverCheck`]: the answer
/// `POST /restart-check` gives, with the reason a restart asked for now would be refused, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestartImpact {
    pub verdict: RestartVerdict,
    /// Whether the next daemon's range was given and read ([`HandoverCheck::known`]).
    pub known: bool,
    /// The sessions that would end, or may: with a known range those not carried over, whatever
    /// they are doing; without one every running session.
    pub sessions: Vec<ImpactSession>,
    /// The busy shell terminals that would end, or may. One the daemon lists as not carried but no
    /// longer finds counts as busy.
    pub shell_terminals: Vec<ImpactTerminal>,
    /// How many shell terminals of holders not linked to yet would not be carried over: whether
    /// one is busy is not known, so each counts.
    pub unreached_shell_terminals: usize,
    /// The code of the refusal a planned restart asked for now would meet
    /// (`crate::state::RestartRefusal::code`), with its message; `None` where none.
    #[serde(default)]
    pub refusal: Option<ImpactRefusal>,
}

/// A refusal [`RestartImpact`] foresees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactRefusal {
    pub reason: String,
    pub message: String,
}

/// [`check`] read as a client asks about it ([`RestartImpact`]), with the refusal a restart asked
/// for now would meet. **Blocks** briefly on the state's locks and the store.
pub fn restart_impact(
    state: &AppState,
    holder_floor: Option<&str>,
    target_version: Option<&str>,
) -> RestartImpact {
    let answer = check(state, holder_floor, target_version);
    let terminals = state.shell_terminals.list();
    let sessions: Vec<ImpactSession> = state
        .store
        .list_sessions()
        .unwrap_or_else(|err| {
            tracing::warn!(%err, "the sessions could not be read for the restart check");
            Vec::new()
        })
        .into_iter()
        .map(|session| ImpactSession {
            status: serde_json::to_value(state.shown_status(&session))
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default(),
            id: session.id,
            title: session.title,
        })
        .collect();
    let mut impact = impact_of(&answer, &sessions, &terminals);
    impact.refusal = state.restart_refusal().map(|refusal| ImpactRefusal {
        reason: refusal.code().to_owned(),
        message: refusal.message().to_owned(),
    });
    impact
}

/// The statuses of a session with a process, which a restart without a known range may
/// interrupt: the UI's `LIVE_STATUSES` (`packages/ui/src/sessions/sessionStatus.ts`).
const LIVE_STATUSES: [&str; 5] = ["starting", "working", "waiting_user", "dormant", "idle"];

/// The [`RestartImpact`] of `answer`, given every session the daemon records, with the status a
/// client is shown, and the shell `terminals` it lists, as the UI's `restartConfirmation` reads
/// them:
///
/// - with a known range, every session `answer` lists, whatever it is doing (named from its
///   record where there is one), and the busy shell terminals it lists, one no longer listed
///   counting as busy;
/// - without one, every session with a process (by its status, so one recorded and still being
///   launched too, as a window counts it), and every busy shell terminal;
/// - either way, each shell terminal of a holder not reached.
///
/// A listed terminal at its prompt is closed too, but, as in the UI, does not ask on its own.
/// The cases in `apps/daemon/testdata/restart_impact_cases.json` pin this rule and the UI's
/// together.
fn impact_of(
    answer: &HandoverCheck,
    sessions: &[ImpactSession],
    terminals: &[ShellTerminal],
) -> RestartImpact {
    let busy =
        |terminal: &ShellTerminal| terminal.state == ShellTerminalState::Running && terminal.busy;
    let (sessions, shell_terminals): (Vec<ImpactSession>, Vec<ImpactTerminal>) = if answer.known {
        let listed = answer
            .sessions
            .iter()
            .map(|id| {
                sessions
                    .iter()
                    .find(|session| &session.id == id)
                    .cloned()
                    .unwrap_or_else(|| ImpactSession {
                        id: id.clone(),
                        title: String::new(),
                        status: String::new(),
                    })
            })
            .collect();
        let terminals = answer
            .shell_terminals
            .iter()
            .filter_map(
                |id| match terminals.iter().find(|terminal| &terminal.id == id) {
                    Some(terminal) => busy(terminal).then(|| ImpactTerminal {
                        id: id.clone(),
                        title: terminal.title.clone(),
                    }),
                    None => Some(ImpactTerminal {
                        id: id.clone(),
                        title: String::new(),
                    }),
                },
            )
            .collect();
        (listed, terminals)
    } else {
        let live = sessions
            .iter()
            .filter(|session| LIVE_STATUSES.contains(&session.status.as_str()))
            .cloned()
            .collect();
        let terminals = terminals
            .iter()
            .filter(|terminal| busy(terminal))
            .map(|terminal| ImpactTerminal {
                id: terminal.id.clone(),
                title: terminal.title.clone(),
            })
            .collect();
        (live, terminals)
    };
    let unreached_shell_terminals = answer.unreached_shell_terminals.len();
    let verdict =
        if sessions.is_empty() && shell_terminals.is_empty() && unreached_shell_terminals == 0 {
            RestartVerdict::Safe
        } else if answer.known {
            RestartVerdict::Interrupts
        } else {
            RestartVerdict::MayInterrupt
        };
    RestartImpact {
        verdict,
        known: answer.known,
        sessions,
        shell_terminals,
        unreached_shell_terminals,
        refusal: None,
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

    fn terminal(id: &str, state: ShellTerminalState, busy: bool) -> ShellTerminal {
        ShellTerminal {
            id: id.to_string(),
            project_id: "p".to_string(),
            worktree: None,
            cwd: "/".to_string(),
            title: format!("shell {id}"),
            renamed: false,
            state,
            busy,
            created_at: 0,
            screen_inexact: false,
            holder_link: None,
        }
    }

    fn session(id: &str) -> ImpactSession {
        ImpactSession {
            id: id.to_string(),
            title: format!("session {id}"),
            status: "idle".to_string(),
        }
    }

    fn answer(known: bool, shells: &[&str], unreached: &[&str]) -> HandoverCheck {
        HandoverCheck {
            known,
            sessions: Vec::new(),
            shell_terminals: shells.iter().map(|id| id.to_string()).collect(),
            unreached_shell_terminals: unreached.iter().map(|id| id.to_string()).collect(),
        }
    }

    /// As the UI asks: any session listed, whatever it does, any busy shell terminal listed and any
    /// unreached one ask; a listed terminal at its prompt or exited does not on its own; one the
    /// daemon no longer finds counts as busy.
    #[test]
    fn the_verdict_asks_for_what_the_ui_asks_about() {
        let terminals = [
            terminal("busy", ShellTerminalState::Running, true),
            terminal("prompt", ShellTerminalState::Running, false),
            terminal("exited", ShellTerminalState::Exited, false),
        ];
        let quiet = impact_of(&answer(true, &["prompt", "exited"], &[]), &[], &terminals);
        assert_eq!(quiet.verdict, RestartVerdict::Safe);
        assert!(quiet.shell_terminals.is_empty());

        let busy = impact_of(
            &answer(true, &["busy", "prompt", "gone"], &[]),
            &[],
            &terminals,
        );
        assert_eq!(busy.verdict, RestartVerdict::Interrupts);
        assert_eq!(
            busy.shell_terminals,
            [
                ImpactTerminal {
                    id: "busy".into(),
                    title: "shell busy".into()
                },
                ImpactTerminal {
                    id: "gone".into(),
                    title: String::new()
                },
            ]
        );

        let mut listing = answer(true, &[], &[]);
        listing.sessions = vec!["s".into()];
        let idle_session = impact_of(&listing, &[session("s")], &terminals);
        assert_eq!(idle_session.verdict, RestartVerdict::Interrupts);

        let unreached = impact_of(&answer(true, &[], &["u"]), &[], &terminals);
        assert_eq!(unreached.verdict, RestartVerdict::Interrupts);
        assert_eq!(unreached.unreached_shell_terminals, 1);
    }

    /// Without a known range what is counted may end, not will; nothing running is still safe.
    #[test]
    fn an_unknown_range_may_interrupt_and_nothing_running_is_safe() {
        let terminals = [terminal("busy", ShellTerminalState::Running, true)];
        let may = impact_of(&answer(false, &["busy"], &[]), &[], &terminals);
        assert_eq!(may.verdict, RestartVerdict::MayInterrupt);
        assert!(!may.known);
        let nothing = impact_of(&answer(false, &[], &[]), &[], &[]);
        assert_eq!(nothing.verdict, RestartVerdict::Safe);
    }

    /// The cases the UI's restart confirmation runs too (`useAppExit.test.ts`): the two copies of
    /// the rule agree on each.
    #[test]
    fn the_shared_restart_cases_hold() {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Case {
            name: String,
            known: bool,
            sessions: Vec<SessionCase>,
            listed_sessions: Vec<String>,
            shell_terminals: Vec<TerminalCase>,
            listed_shell_terminals: Vec<String>,
            unreached_shell_terminals: usize,
            verdict: RestartVerdict,
            counted_sessions: Vec<String>,
            counted_shell_terminals: Vec<String>,
        }
        #[derive(serde::Deserialize)]
        struct SessionCase {
            id: String,
            status: String,
        }
        #[derive(serde::Deserialize)]
        struct TerminalCase {
            id: String,
            state: String,
            busy: bool,
        }
        #[derive(serde::Deserialize)]
        struct Cases {
            cases: Vec<Case>,
        }
        let cases: Cases =
            serde_json::from_str(include_str!("../../testdata/restart_impact_cases.json")).unwrap();
        assert!(!cases.cases.is_empty());
        for case in cases.cases {
            let sessions: Vec<ImpactSession> = case
                .sessions
                .iter()
                .map(|session| ImpactSession {
                    id: session.id.clone(),
                    title: String::new(),
                    status: session.status.clone(),
                })
                .collect();
            let terminals: Vec<ShellTerminal> = case
                .shell_terminals
                .iter()
                .map(|terminal| {
                    let state = match terminal.state.as_str() {
                        "running" => ShellTerminalState::Running,
                        "exited" => ShellTerminalState::Exited,
                        other => panic!("{}: unknown state {other}", case.name),
                    };
                    self::terminal(&terminal.id, state, terminal.busy)
                })
                .collect();
            let answer = HandoverCheck {
                known: case.known,
                sessions: case.listed_sessions.clone(),
                shell_terminals: case.listed_shell_terminals.clone(),
                unreached_shell_terminals: (0..case.unreached_shell_terminals)
                    .map(|n| format!("unreached-{n}"))
                    .collect(),
            };
            let impact = impact_of(&answer, &sessions, &terminals);
            assert_eq!(impact.verdict, case.verdict, "{}", case.name);
            assert_eq!(
                impact
                    .sessions
                    .iter()
                    .map(|s| s.id.clone())
                    .collect::<Vec<_>>(),
                case.counted_sessions,
                "{}",
                case.name
            );
            assert_eq!(
                impact
                    .shell_terminals
                    .iter()
                    .map(|t| t.id.clone())
                    .collect::<Vec<_>>(),
                case.counted_shell_terminals,
                "{}",
                case.name
            );
            assert_eq!(
                impact.unreached_shell_terminals, case.unreached_shell_terminals,
                "{}",
                case.name
            );
        }
    }

    /// The impact names the sessions it counts by their records, and carries the refusal a
    /// restart would meet now.
    #[tokio::test]
    async fn the_impact_names_the_sessions_and_foresees_a_refusal() {
        let (state, dir) = app_state("handover-impact");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());

        let impact = restart_impact(&state, Some("0.0.0"), Some("9.9.9"));
        assert_eq!(impact.verdict, RestartVerdict::Interrupts);
        assert_eq!(impact.sessions.len(), 1);
        assert_eq!(impact.sessions[0].id, "s");
        assert_eq!(impact.sessions[0].status, "working");
        assert_eq!(impact.refusal, None);

        state.set_follows_application();
        let refused = restart_impact(&state, None, None);
        assert_eq!(refused.verdict, RestartVerdict::MayInterrupt);
        assert_eq!(
            refused.refusal.map(|refusal| refusal.reason),
            Some("follows_application".to_string())
        );
    }

    /// Without a floor or a target version, or with one that does not read, nothing is known to be
    /// carried over: every running session may be interrupted, whatever it is doing, and the
    /// answer says so rather than that it will.
    #[tokio::test]
    async fn without_a_known_range_every_running_session_may_be_interrupted() {
        let (state, dir) = app_state("handover-impact-unknown");
        insert_record(&state, &dir, "a", SessionStatus::WaitingUser);
        insert_record(&state, &dir, "b", SessionStatus::Idle);
        let a = fake_live_session("a", AgentId::CLAUDE, 80, 24, "sleep 30");
        let b = fake_live_session("b", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(a.clone());
        state.register_live(b.clone());
        let title_of = |id: &str| state.store.get_session(id).unwrap().unwrap().title;

        for (floor, target) in [
            (None, None),
            (Some("0.0.0"), None),
            (None, Some("9.9.9")),
            (Some("not a version"), Some("9.9.9")),
            (Some("0.0.0"), Some("not a version")),
        ] {
            let impact = restart_impact(&state, floor, target);
            let case = format!("{floor:?} {target:?}");
            assert_eq!(impact.verdict, RestartVerdict::MayInterrupt, "{case}");
            assert!(!impact.known, "{case}");
            assert_eq!(
                impact.sessions,
                [
                    ImpactSession {
                        id: "a".into(),
                        title: title_of("a"),
                        status: "waiting_user".into(),
                    },
                    ImpactSession {
                        id: "b".into(),
                        title: title_of("b"),
                        status: "idle".into(),
                    },
                ],
                "{case}"
            );
        }
    }

    /// A session is counted and named by the status a client is shown, as the window's restart
    /// confirmation reads it, not the one stored: one recorded idle whose model is being changed
    /// is shown starting, and one recorded interrupted with a process registered is still shown
    /// interrupted, which is no process, so a restart without a known range does not count it.
    #[tokio::test]
    async fn sessions_are_counted_and_named_by_the_status_a_client_is_shown() {
        let (state, dir) = app_state("handover-impact-shown-status");
        insert_record(&state, &dir, "changing", SessionStatus::Idle);
        insert_record(&state, &dir, "down", SessionStatus::Interrupted);
        let changing = fake_live_session("changing", AgentId::CLAUDE, 80, 24, "sleep 30");
        let down = fake_live_session("down", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(changing.clone());
        state.register_live(down.clone());
        let claim = state.begin_switch("changing").unwrap();
        state.mark_model_change("changing");

        let unknown = restart_impact(&state, None, None);
        assert_eq!(unknown.verdict, RestartVerdict::MayInterrupt);
        assert_eq!(
            unknown
                .sessions
                .iter()
                .map(|s| (s.id.as_str(), s.status.as_str()))
                .collect::<Vec<_>>(),
            [("changing", "starting")]
        );

        let known = restart_impact(&state, Some("0.0.0"), Some("9.9.9"));
        let named = |id: &str| {
            known
                .sessions
                .iter()
                .find(|session| session.id == id)
                .map(|session| session.status.as_str())
        };
        assert_eq!(named("changing"), Some("starting"));
        drop(claim);
        assert_eq!(
            restart_impact(&state, None, None)
                .sessions
                .iter()
                .map(|s| (s.id.as_str(), s.status.as_str()))
                .collect::<Vec<_>>(),
            [("changing", "idle")],
            "once the change is over it is shown as recorded"
        );
    }

    /// With a known range every running session in the daemon itself is listed, whatever it is
    /// doing: an idle one asks as much as a working one.
    #[tokio::test]
    async fn a_known_range_lists_every_session_not_carried_whatever_it_is_doing() {
        let (state, dir) = app_state("handover-impact-known-idle");
        insert_record(&state, &dir, "idle", SessionStatus::Idle);
        let live = fake_live_session("idle", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());

        let impact = restart_impact(&state, Some("0.0.0"), Some("9.9.9"));
        assert!(impact.known);
        assert_eq!(impact.verdict, RestartVerdict::Interrupts);
        assert_eq!(
            impact
                .sessions
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["idle"]
        );
        assert_eq!(impact.sessions[0].status, "idle");
    }

    /// Nothing running is safe, whether or not the range is known, and nothing is foreseen to
    /// refuse a restart of a daemon at rest.
    #[tokio::test]
    async fn nothing_running_is_safe_and_foresees_no_refusal() {
        let (state, _dir) = app_state("handover-impact-nothing");
        for (floor, target) in [(Some("0.0.0"), Some("9.9.9")), (None, None)] {
            let impact = restart_impact(&state, floor, target);
            assert_eq!(impact.verdict, RestartVerdict::Safe);
            assert_eq!(impact.known, floor.is_some());
            assert!(impact.sessions.is_empty());
            assert!(impact.shell_terminals.is_empty());
            assert_eq!(impact.unreached_shell_terminals, 0);
            assert_eq!(impact.refusal, None);
        }
    }

    /// A running session whose record cannot be read is still listed, as one that counts, with its
    /// title and status left empty.
    #[tokio::test]
    async fn a_running_session_without_a_record_is_still_listed() {
        let (state, _dir) = app_state("handover-impact-no-record");
        let live = fake_live_session("ghost", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());

        let impact = restart_impact(&state, Some("0.0.0"), Some("9.9.9"));
        assert_eq!(impact.verdict, RestartVerdict::Interrupts);
        assert_eq!(
            impact.sessions,
            [ImpactSession {
                id: "ghost".into(),
                title: String::new(),
                status: String::new(),
            }]
        );
    }

    /// A stop asked for already is foreseen as the refusal of a restart, with a message, and the
    /// verdict is still given.
    #[tokio::test]
    async fn a_stop_under_way_is_foreseen_as_a_refusal() {
        let (state, _dir) = app_state("handover-impact-stopping");
        state.request_shutdown();
        let impact = restart_impact(&state, Some("0.0.0"), Some("9.9.9"));
        assert_eq!(impact.verdict, RestartVerdict::Safe);
        let refusal = impact.refusal.expect("a refusal");
        assert_eq!(refusal.reason, "stop_in_progress");
        assert!(!refusal.message.is_empty());
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
