//! Taking over what the holders kept running while no daemon was: at start, before anything
//! marks sessions interrupted, clears the run directory or reconciles the process ledger, the
//! terminals the holders run are matched with what the daemon has a record of.
//!
//! - A session's terminal, whose session is on record, not archived and not recorded as without a
//!   process (interrupted), and not running already, is adopted: the [`LiveSession`] is built again
//!   around the held terminal from what the launch left in the terminal's meta (see
//!   `crate::session::term::launch`), and watched as one the daemon started itself. The session
//!   keeps the status it was stored with. What the launch's plan and its private server gave the
//!   session is rebuilt by its provider (`AgentProvider::rebuild_launch`): the server the holder's
//!   welcome lists with the terminal, as the terminal's companion, is linked to again, and its
//!   messages go through it as before the restart; one the welcome lists exited is a server that
//!   is gone, and the terminal is ended for it as for a server that dies while the session runs. A
//!   terminal the welcome lists no companion with (an ordinary launch, or a holder that runs none)
//!   is adopted without one, its messages going to the terminal.
//! - A shell terminal whose project is still on record is put back among the shell terminals
//!   ([`crate::terminal::shell::restore`]).
//! - Any other terminal is ended: the program is killed, what it started is swept and the terminal
//!   released.
//! - A holder outside the compatible range (see `crate::holder::compat`) is not looked into: the
//!   holders' client ends all its terminals, and the sessions they ran are marked interrupted like
//!   any session no holder runs.
//! - A holder whose process runs but that cannot be linked to is not ended: what it runs stays
//!   claimed until it is linked to, when its terminals are taken over here as at start, or known to
//!   be gone (see [`super::unlinked`]).
//!
//! The sessions that no terminal was adopted for, and none of those holders keeps claimed, are
//! marked interrupted by the caller, as a start with no holders marks every one.
//!
//! A terminal handed over by the previous daemon's planned stop, whose screen that daemon saved
//! for it, gets its screen back exactly: the saved screen is restored and only the tail written
//! since the handover is replayed over it, as the program's output, with no redraw and no inexact
//! mark; a saved screen marked inexact is restored the same way but marked, and the program is made
//! to draw it again (see `crate::state::restart_screens`). The saved screen
//! is taken for good once the terminal's fate is settled, adopted or ended, and before its reader
//! starts, so that no later start replays that tail again; a screen that cannot be taken for good
//! is not used. Any other terminal's screen is rebuilt from the tail the holder kept, which holds
//! only the last of what the program drew: it is marked inexact, and a running program is made to
//! draw it again.
//!
//! A screen the previous daemon saved for a session whose holder is gone (the system shut down
//! after the planned stop) is kept as the preview the session is interrupted with, as an ordinary
//! stop would have frozen it ([`unlinked::keep_screens_as_previews`]).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use super::{exit_watch, unlinked};
use crate::adapter::AdoptedLaunch;
use crate::holder::client::{self, FoundCompanion, FoundTerminal, HeldAt};
use crate::protocol::{AgentId, SessionStatus};
use crate::session::{self, dialogs, trust, LiveSession, NewSession};
use crate::state::{AppState, ExactScreen};
use crate::terminal::backend::Started;
use crate::terminal::display::DisplaySeed;
use crate::terminal::pty;
use crate::terminal::server_host::{FoundServer, HeldServer, ServerEndpoint};
use crate::terminal::shell::{self, ShellMeta};

/// What a start took over from the holders.
#[derive(Debug, Default)]
pub struct Adopted {
    /// The sessions whose terminals were adopted.
    pub sessions: Vec<String>,
    /// The scratch directories of those sessions, which clearing the run directory has to leave.
    pub scratch_dirs: Vec<PathBuf>,
    /// The shell terminals put back.
    pub shells: Vec<String>,
    /// How many terminals were ended for being none of the above.
    pub ended: usize,
    /// The holders taken over.
    pub holders: Vec<String>,
    /// The sessions left claimed for a holder whose process runs but that could not be linked to,
    /// which are not interrupted: their scratch directories are among `scratch_dirs`.
    pub claimed: Vec<String>,
}

/// What a session terminal's meta holds; see `crate::session::term::launch`, which writes it.
/// It crosses builds: a daemon of an older build in the holder's compatible range writes it and a
/// newer one reads it, so a field added later needs `#[serde(default)]`, and it is kept readable
/// by every build in that range: `crate::holder::compat::FLOOR` only moves by the
/// previous-stable release rule.
#[derive(Deserialize)]
struct SessionMeta {
    session_id: String,
    launch_id: String,
    agent: AgentId,
    #[serde(default)]
    scratch: Option<String>,
    #[serde(default)]
    mcp_token: Option<String>,
    #[serde(default)]
    resolves_approvals_itself: bool,
    #[serde(default)]
    native_stash: bool,
    #[serde(default)]
    launch_prompt: Option<String>,
    #[serde(default)]
    adoption: Option<Value>,
}

/// What a terminal found in a holder is, by its meta and what the daemon has on record.
enum Kind {
    Session {
        meta: SessionMeta,
        /// The agent's own id of the session's conversation, as its record holds it.
        agent_session_id: Option<String>,
    },
    Shell(ShellMeta),
    /// None of the daemon's: the reason it is ended for.
    Unknown(&'static str),
}

/// Adopts what the holders of the registry left running, with `run_dir` the directory the
/// sessions' scratch directories are in, and claims what the holders it cannot link to run (see
/// [`super::unlinked`]). Does nothing where the terminals run in the daemon itself. Called inside
/// the runtime, before the daemon serves.
pub async fn adopt(state: &Arc<AppState>, run_dir: &Path) -> Adopted {
    let Some(holders) = client::current() else {
        state.unlinked_claimed(Vec::new(), false);
        return Adopted::default();
    };
    let on_resolved = unlinked::on_resolved(state, run_dir);
    let (state, run_dir) = (state.clone(), run_dir.to_path_buf());
    let failed = state.clone();
    let adopting = tokio::task::spawn_blocking(move || {
        let adoption = holders.adopt_existing(on_resolved);
        let holder_ids: Vec<String> = adoption
            .found
            .iter()
            .map(|holder| holder.holder_id.clone())
            .collect();
        let terminals = adoption.found.into_iter().flat_map(|holder| {
            tracing::info!(
                holder = %holder.holder_id,
                build = %holder.build_id,
                terminals = holder.terminals.len(),
                last_parting = ?holder.last_parting,
                "found a holder that outlived the previous daemon"
            );
            holder.terminals
        });
        let mut adopted = adopt_found(&state, terminals.collect(), &run_dir);
        // Every terminal of these holders is settled: a screen none of them took is of a terminal
        // its holder no longer runs.
        state.expire_restart_screens(|holder| holder_ids.iter().any(|id| id == holder));
        adopted.holders = holder_ids;
        unlinked::claim_at_start(
            &state,
            adoption.pending,
            adoption.ending,
            &run_dir,
            &mut adopted,
        );
        // Now that every holder that runs is known: the copies of builds nothing runs from go.
        holders.sweep_builds();
        adopted
    })
    .await;
    adopting.unwrap_or_else(|err| {
        tracing::error!(%err, "adopting the holders' terminals panicked");
        // Nothing is claimed, and what the holders resolve to later is not held up.
        failed.unlinked_claimed(Vec::new(), false);
        Adopted::default()
    })
}

/// Decides for each of `terminals` what it is and acts on it, holder by holder: every terminal of
/// a holder is decided on first, then the screens saved for them are taken for good in one go
/// (committed, see `crate::state::restart_screens`), and only then is each adopted or ended, so
/// that none of their readers starts while the file still holds its screen. A screen that could
/// not be taken for good is not restored from. **Blocks**, and registers what it adopts with the
/// runtime it is called inside.
pub(crate) fn adopt_found(
    state: &Arc<AppState>,
    terminals: Vec<FoundTerminal>,
    run_dir: &Path,
) -> Adopted {
    let mut adopted = Adopted::default();
    let mut taken = HashSet::new();
    for terminals in by_holder(terminals) {
        let decided: Vec<(FoundTerminal, Kind, Option<ExactScreen>)> = terminals
            .into_iter()
            .map(|found| {
                let kind = classify(state, &found.meta, &taken);
                taken.extend(match &kind {
                    Kind::Session { meta, .. } => Some(meta.session_id.clone()),
                    Kind::Shell(meta) => Some(meta.id.clone()),
                    Kind::Unknown(_) => None,
                });
                let exact = state.select_exact_screen(&found);
                if matches!(kind, Kind::Session { .. } | Kind::Shell(_)) {
                    match &exact {
                        Ok(_) => {
                            tracing::info!(terminal = %found.id, "restoring the screen saved at the handover")
                        }
                        Err(why) => {
                            tracing::info!(terminal = %found.id, why, "rebuilding the screen from the tail")
                        }
                    }
                }
                (found, kind, exact.ok())
            })
            .collect();
        let at: Vec<HeldAt> = decided
            .iter()
            .map(|(found, _, _)| HeldAt {
                holder_id: found.holder_id.clone(),
                terminal_id: found.id.clone(),
            })
            .collect();
        // A screen still in the file could be taken again by a start after a crash, which would
        // replay the tail since the handover, and answer its queries, a second time.
        let committed = state.commit_restart_screens(&at);
        if !committed {
            tracing::warn!(
                holder = %at[0].holder_id,
                "the saved screens could not be consumed for good; rebuilding the screens from the tails"
            );
        }
        for (found, kind, exact) in decided {
            let exact = exact.filter(|_| committed);
            settle(state, found, kind, exact, run_dir, &mut adopted);
        }
    }
    adopted
}

/// `terminals` grouped by their holder, in the order the holders first appear.
fn by_holder(terminals: Vec<FoundTerminal>) -> Vec<Vec<FoundTerminal>> {
    let mut groups: Vec<Vec<FoundTerminal>> = Vec::new();
    for found in terminals {
        match groups
            .iter_mut()
            .find(|group| group[0].holder_id == found.holder_id)
        {
            Some(group) => group.push(found),
            None => groups.push(vec![found]),
        }
    }
    groups
}

/// Acts on `found`, which is `kind`, its screen restored from `exact` where there is one: adopts
/// a session's or a shell's terminal, or ends it. **Blocks.**
fn settle(
    state: &Arc<AppState>,
    found: FoundTerminal,
    kind: Kind,
    exact: Option<ExactScreen>,
    run_dir: &Path,
    adopted: &mut Adopted,
) {
    let id = found.id.clone();
    match kind {
        Kind::Session {
            meta,
            agent_session_id,
        } => {
            // Claimed until the process is registered, so that no launch of the session can
            // begin in between and run beside it.
            let Some(claim) = state.claim_for_adoption(&meta.session_id) else {
                tracing::info!(terminal = %id, "ending a terminal of a session that runs or is being launched");
                found.held.end();
                adopted.ended += 1;
                return;
            };
            let scratch = adopt_session(
                state,
                meta,
                agent_session_id,
                found,
                exact,
                run_dir,
                adopted,
            );
            drop(claim);
            tracing::info!(terminal = %id, scratch = ?scratch, "adopted a session's terminal");
        }
        Kind::Shell(meta) => {
            // With the screen restored, only what the shell wrote since it was saved is
            // replayed.
            let tail = if exact.is_some() {
                found.since_handover().to_vec()
            } else {
                found.tail
            };
            shell::restore(state, &meta, found.held, found.exited, tail, exact);
            tracing::info!(terminal = %id, "put back a shell terminal");
            adopted.shells.push(meta.id);
        }
        Kind::Unknown(why) => {
            tracing::info!(terminal = %id, why, "ending a terminal the daemon has no use for");
            found.held.end();
            adopted.ended += 1;
        }
    }
}

/// What `meta` makes a terminal. `taken` holds the sessions and shell terminals an earlier terminal
/// was adopted for, which a second terminal of the same one never is, nor a shell terminal held
/// already. Whether a session runs or is being launched is decided with its claim, in
/// [`adopt_found`]: a holder linked to after the start may hold an old terminal of a session
/// resumed since.
fn classify(state: &AppState, meta: &Value, taken: &HashSet<String>) -> Kind {
    match meta.get("kind").and_then(Value::as_str) {
        Some("session") => {
            let Ok(meta) = serde_json::from_value::<SessionMeta>(meta.clone()) else {
                return Kind::Unknown("the session's meta does not read");
            };
            if meta.launch_id.is_empty() || taken.contains(&meta.session_id) {
                return Kind::Unknown("a second terminal of the session, or none of a launch");
            }
            match state.store.get_session(&meta.session_id) {
                Ok(Some(session)) if session.status == SessionStatus::Archived => {
                    Kind::Unknown("its session is archived")
                }
                // The record says it has no process, which one that runs contradicts.
                Ok(Some(session)) if session.status.has_no_process() => {
                    Kind::Unknown("its session is recorded as without a process")
                }
                Ok(Some(session)) => Kind::Session {
                    meta,
                    agent_session_id: session.agent_session_id,
                },
                Ok(None) => Kind::Unknown("its session is not on record"),
                Err(err) => {
                    tracing::warn!(session = %meta.session_id, %err, "reading the session failed");
                    Kind::Unknown("its session could not be read")
                }
            }
        }
        Some(ShellMeta::KIND) => {
            let Ok(meta) = serde_json::from_value::<ShellMeta>(meta.clone()) else {
                return Kind::Unknown("the shell terminal's meta does not read");
            };
            if taken.contains(&meta.id) || state.shell_terminals.process(&meta.id).is_some() {
                return Kind::Unknown("a second terminal of the shell terminal");
            }
            match state.store.get_project(&meta.project_id) {
                Ok(Some(_)) => Kind::Shell(meta),
                Ok(None) => Kind::Unknown("its project is not on record"),
                Err(err) => {
                    tracing::warn!(project = %meta.project_id, %err, "reading the project failed");
                    Kind::Unknown("its project could not be read")
                }
            }
        }
        _ => Kind::Unknown("it is none the daemon knows"),
    }
}

/// Builds the session of `meta` again around `found`, and has the daemon watch it as it does a
/// launch of its own, its screen restored from `exact`, the screen saved at the handover, when
/// there is one, and its messages going through the server the holder runs with the terminal where
/// the provider links to it again. Returns the scratch directory it keeps.
fn adopt_session(
    state: &Arc<AppState>,
    meta: SessionMeta,
    agent_session_id: Option<String>,
    found: FoundTerminal,
    exact: Option<ExactScreen>,
    run_dir: &Path,
    adopted: &mut Adopted,
) -> Option<PathBuf> {
    let since_mark = if exact.is_some() {
        found.since_handover().to_vec()
    } else {
        Vec::new()
    };
    let FoundTerminal {
        held,
        tail,
        exited,
        companion,
        ..
    } = found;
    // A saved screen the save marked inexact keeps its content but is drawn again, as a rebuilt
    // one is: what it left out (the character sets in use, say) is the program's to bring back.
    let exact_screen = exact.as_ref().is_some_and(|exact| !exact.saved.inexact);
    // Read before any output, as what the program wrote since the screen was saved; otherwise the
    // tail is history, taken in below.
    let tail = match exact {
        Some(_) => {
            held.replay_first(since_mark);
            Vec::new()
        }
        None => tail,
    };
    // Only what the welcome lists: a companion it lists exited is a server that is gone, which the
    // transport reports once the session is registered, and the terminal is ended for it.
    let server = companion.and_then(|companion| found_server(&meta.session_id, companion));
    let rebuilt = crate::adapter::provider(meta.agent).rebuild_launch(AdoptedLaunch {
        adoption: meta.adoption.as_ref(),
        launch_id: &meta.launch_id,
        agent_session_id: agent_session_id.as_deref(),
        server,
    });
    let (transport, after_exit) = (rebuilt.transport, rebuilt.after_exit);
    // Only the directory the daemon makes for the session is kept, and removed with its process:
    // a meta that names another place is not trusted with the removal.
    let scratch = meta
        .scratch
        .map(PathBuf::from)
        .filter(|scratch| *scratch == run_dir.join(&meta.session_id) && scratch.is_dir());
    let live = Arc::new(LiveSession::new(NewSession {
        id: meta.session_id.clone(),
        agent: meta.agent,
        launch_id: meta.launch_id,
        spawned: Started::Held(held),
        checks_composer: true,
        transport: transport.clone(),
        scratch_dir: scratch.clone(),
        resolves_approvals_itself: meta.resolves_approvals_itself,
        carried_trust: rebuilt.carried_trust,
        // The transport is stopped once the process is reaped; a server its holder runs is left to
        // the holder, which ends it with the terminal.
        before_scratch_removal: crate::session::term::before_scratch_removal(transport, after_exit),
        display: DisplaySeed::new(state.display_fallback.clone(), None),
    }));
    // Its agent ran its start hook, if it ever did, before this daemon was there to see it.
    live.trust.note_adopted();
    live.enable_native_stash(meta.native_stash);
    if let Some(prompt) = meta.launch_prompt {
        live.note_launch_prompt(prompt);
    }
    match exact {
        // The screen as the previous daemon saved it (marked inexact only when it was then), and
        // the tail the reader replays first taken in as output under the replay's rule.
        Some(ExactScreen { saved, rule }) => {
            live.restore_screen(&saved);
            live.begin_replay(rule);
        }
        // The screen and the replay are rebuilt from the raw tail the holder kept, which holds
        // only the last of what the program drew: the screen is marked inexact, and the program is
        // made to draw it again below.
        None => {
            live.mark_screen_inexact();
            live.seed_output(&tail);
        }
    }
    // What the program last reported about its turn, which it does not report again.
    live.note_restored_program_status(state);
    // This title belongs to the same held terminal, so its provider's startup signal remains
    // valid without replaying the old output's status changes. Hosts that render their own
    // titles still use the input box as their start signal.
    if live.has_title() {
        live.trust.note_title();
    }
    session::spawn_reader_thread(live.clone(), pty::READ_BUF);
    if let Some(token) = &meta.mcp_token {
        state.restore_mcp_token(&meta.session_id, token);
    }
    // Before the session is live: nothing may see it live with work not confirmed for its launch,
    // and a persist must not tag that work with the live launch.
    state.restore_background_work(&meta.session_id, &live.launch_id);
    // The ledger follows the launch from here, before the reconcile reads its file.
    state.process_ledger.containment_adopted(&live.launch_id);
    state.register_live(live.clone());
    state.forget_saved_output(&meta.session_id);
    exit_watch::watch_exit(state, live.clone());
    trust::supervise(state, &live);
    dialogs::watch(state, &live);
    // Last, so what the program draws in answer reaches everything that watches its output. A
    // screen restored exact is the program's own already.
    if !exited && !exact_screen {
        if let Err(err) = live.nudge_repaint() {
            tracing::warn!(session = %meta.session_id, %err, "asking the program to redraw its screen failed");
        }
    }

    adopted.scratch_dirs.extend(scratch.clone());
    adopted.sessions.push(meta.session_id);
    scratch
}

/// The private server `companion` is, for the provider to link to again; `None` for an endpoint of
/// a kind this build does not know, which leaves the messages of the session `session` to its
/// terminal.
fn found_server(session: &str, companion: FoundCompanion) -> Option<FoundServer> {
    let Some(endpoint) = ServerEndpoint::of_companion(&companion.endpoint) else {
        tracing::warn!(
            session = %session,
            "the server listed with an adopted terminal listens at an endpoint this daemon does \
             not know; the session's messages go to its terminal"
        );
        return None;
    };
    Some(FoundServer {
        endpoint,
        server: Box::new(HeldServer::new(companion.server)),
    })
}

#[cfg(test)]
mod tests;
