//! The tool with which an owner moves process trees between sessions, `handover_processes`: from
//! itself to a session of its own, from one to itself, or from one of its sessions to another, as
//! `handover_branches` moves branches. The trees keep running; only the session whose end stops
//! them changes (`crate::process_ledger`).

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::process_ledger::{MoveRefused, ProcessTree};
use crate::protocol::{error_code, CodedError, Session, SessionStatus};
use crate::reporting::{self, WhenBlocked, NOTICE_PREFIX};
use crate::sharing::named_session;
use crate::state::AppState;

use super::args::optional_string_list;
use super::handover::{busy_phrase, party};

/// Moves the process trees `processes` names from `from` to `to`, each the caller or a session it
/// owns. Needs no approval: the caller owns every session but itself.
///
/// Refused, changing nothing, for a `from` that is another session and is in the middle of a turn,
/// for an archived `from` or `to` (what an archive did not stop is the user's), for a session with
/// no process running (the trees of an interrupted session were stopped when it ended, and one
/// with no process has nothing to stop them with), and for a tree that is not `from`'s. The
/// sessions are claimed against a launch, a switch or a handoff, and the messages of a `from` other
/// than the caller are held, while it is checked again and the trees move.
pub(super) async fn handover_processes(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let ids = tree_ids(arguments)?;
    let from_id = party(state, caller, arguments, "from")?.id;
    let to_id = party(state, caller, arguments, "to")?.id;
    if from_id == to_id {
        bail!("`from` and `to` are the same session, `{from_id}`; there is nothing to hand over");
    }

    // Read again under the claims, which keep a relaunch, a switch or a handoff off both sessions
    // while the trees move.
    let _from_claim = (from_id != caller.id)
        .then(|| state.begin_switch(&from_id))
        .transpose()?;
    let _to_claim = (to_id != caller.id)
        .then(|| state.begin_switch(&to_id))
        .transpose()?;
    // `from` takes no message that starts a turn while the checks are made again and the trees
    // move, as for `handover_branches`.
    let hold = (from_id != caller.id).then(|| state.hold_outbox(&from_id));
    let from = party(state, caller, arguments, "from")?;
    let to = party(state, caller, arguments, "to")?;
    for session in [&from, &to] {
        if session.status == SessionStatus::Archived {
            bail!(
                "{} is archived: the processes it started were stopped when it was archived, \
                 apart from those the user chose to keep, which are the user's, and it can take \
                 none. Reopen it first to give it processes",
                named_session(session)
            );
        }
        // A live process, since the claims taken above count as launches for `has_process`.
        if state.live_session(&session.id).is_none() {
            bail!(
                "{} has no process running, so it {}; the processes a session started are \
                 stopped when its process ends",
                named_session(session),
                if session.id == from.id {
                    "holds no process to hand over"
                } else {
                    "cannot take processes: resume it first"
                }
            );
        }
    }
    if from.id != caller.id {
        refuse_if_at_work(state, &from)?;
    }

    let moved = {
        let (state, ids, from, to) = (state.clone(), ids.clone(), from.id.clone(), to.id.clone());
        tokio::task::spawn_blocking(move || state.process_ledger.move_trees(&ids, &from, &to))
            .await?
    };
    let moved = match moved {
        Ok(moved) => moved,
        Err(MoveRefused::NotFrom(missing)) => bail!(
            "{} {} not a process tree {} holds now: it ended, or another session holds it. Name \
             trees by the `id` `get_session` gives in `processes` for that session; nothing was \
             handed over",
            quoted(&missing),
            if missing.len() == 1 { "is" } else { "are" },
            named_session(&from)
        ),
        Err(MoveRefused::NoLaunch) => bail!(
            "{} has no process running to take the processes; nothing was handed over",
            named_session(&to)
        ),
        Err(MoveRefused::Unreadable) => {
            bail!("the process table could not be read; nothing was handed over")
        }
    };

    let trees = {
        let (state, to) = (state.clone(), to.id.clone());
        tokio::task::spawn_blocking(move || {
            state
                .process_ledger
                .trees_of_sessions(&[to], Duration::ZERO)
        })
        .await?
    };
    let listed: Vec<&ProcessTree> = trees
        .iter()
        .filter(|tree| moved.contains(&tree.id))
        .collect();
    let untold = tell(state, caller, &from, &to, &moved).await;
    drop(hold);

    let words = Words::for_count(moved.len());
    let mut note = format!(
        "Handed over the process {} {} from {} to {}. {} {} running; {} now holds {}, so {} {} \
         stopped when its process ends (its archive, an interrupt, its agent exiting), and no \
         longer when that of {} does.",
        words.noun,
        quoted(&moved),
        named_session(&from),
        named_session(&to),
        words.they_capitalised,
        words.keep,
        named_session(&to),
        words.them,
        words.they,
        words.are,
        named_session(&from),
    );
    for session in &untold {
        note.push_str(&format!(
            " Octoboard could not send {} a message about the handover: tell it yourself once it \
             can take a message.",
            named_session(session)
        ));
    }
    Ok(json!({
        "from": from.id,
        "to": to.id,
        "processes": listed,
        "note": note,
    }))
}

/// The tree ids in `processes`, each trimmed and named once.
fn tree_ids(arguments: &Map<String, Value>) -> Result<Vec<String>> {
    let names = optional_string_list(arguments, "processes")?.unwrap_or_default();
    let mut ids: Vec<String> = Vec::new();
    for name in &names {
        let name = name.trim();
        if name.is_empty() {
            bail!("`processes` must not hold a blank id");
        }
        if !ids.iter().any(|id| id == name) {
            ids.push(name.to_string());
        }
    }
    if ids.is_empty() {
        return Err(anyhow!(
            "`processes` is required and must name at least one process tree"
        ));
    }
    Ok(ids)
}

fn quoted(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("`{id}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A session in the middle of a turn — working, starting, or held up for the user — may be using
/// the processes it started, so it is not asked to give one up. Both the record and the status
/// the agent is at count, as for `handover_branches`.
fn refuse_if_at_work(state: &Arc<AppState>, from: &Session) -> Result<()> {
    let Some(busy) = busy_phrase(state, from) else {
        return Ok(());
    };
    Err(CodedError::raised(
        error_code::SESSION_WORKING,
        format!(
            "{} is {busy}: a session in the middle of a turn may be using the processes it \
             started. Hand them over once it is awaiting instructions",
            named_session(from),
        ),
        &[("session", &from.id)],
    ))
}

/// The words that agree with how many process trees moved.
struct Words {
    noun: &'static str,
    they: &'static str,
    they_capitalised: &'static str,
    them: &'static str,
    are: &'static str,
    keep: &'static str,
    they_are: &'static str,
    they_are_capitalised: &'static str,
    they_keep: &'static str,
}

impl Words {
    fn for_count(count: usize) -> Self {
        if count == 1 {
            Self {
                noun: "tree",
                they: "it",
                they_capitalised: "It",
                them: "it",
                are: "is",
                keep: "keeps",
                they_are: "it is",
                they_are_capitalised: "It is",
                they_keep: "it keeps",
            }
        } else {
            Self {
                noun: "trees",
                they: "they",
                they_capitalised: "They",
                them: "them",
                are: "are",
                keep: "keep",
                they_are: "they are",
                they_are_capitalised: "They are",
                they_keep: "they keep",
            }
        }
    }

    /// "process tree" or "process trees".
    fn noun_phrase(&self) -> String {
        format!("process {}", self.noun)
    }
}

/// Tells `from` and `to` what moved, unless one is the caller, which knows from the result.
/// Returns the sessions that could not be told.
async fn tell(
    state: &Arc<AppState>,
    caller: &Session,
    from: &Session,
    to: &Session,
    moved: &[String],
) -> Vec<Session> {
    let owner = named_session(caller);
    let words = Words::for_count(moved.len());
    let named = quoted(moved);
    let messages = [
        (
            from,
            format!(
                "{NOTICE_PREFIX}: {owner}, your owner, handed the {} {named} you started over to \
                 {}. {} no longer yours: do not stop {} or rely on {}; {} running for {}.",
                words.noun_phrase(),
                named_session(to),
                words.they_are_capitalised,
                words.them,
                words.them,
                words.they_keep,
                named_session(to),
            ),
        ),
        (
            to,
            format!(
                "{NOTICE_PREFIX}: {owner}, your owner, handed the {} {named} over to you from {}. \
                 {} yours now and {} running: {} stopped when your process ends. `get_session` \
                 lists {} in `processes`. Your owner will tell you what to do with {}.",
                words.noun_phrase(),
                named_session(from),
                words.they_are_capitalised,
                words.keep,
                words.they_are,
                words.them,
                words.them,
            ),
        ),
    ];
    let mut untold = Vec::new();
    for (session, text) in messages {
        if session.id == caller.id {
            continue;
        }
        if let Err(err) =
            reporting::write_message_off_runtime(state, &session.id, text, WhenBlocked::Queue).await
        {
            tracing::warn!(
                session = %session.id,
                %err,
                "telling a session of a process handover failed"
            );
            untold.push(session.clone());
        }
    }
    untold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::tests::{arguments, project_session, shared_console_state};
    use crate::test_support::{
        assert_stops_running, fake_live_session, is_running, output_containing, perl_write,
        running, wait_for_pid, KillOnDrop, ScratchDir, StandIn,
    };

    /// `shared_console_state` plus sessions of the console session `a`: `kid` and `other-kid`,
    /// idle, `busy-kid` in the middle of a turn and `asleep-kid`, interrupted.
    fn fixture(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = shared_console_state(name);
        for (id, status) in [
            ("kid", SessionStatus::Idle),
            ("other-kid", SessionStatus::Idle),
            ("busy-kid", SessionStatus::Working),
            ("asleep-kid", SessionStatus::Interrupted),
        ] {
            state
                .store
                .insert_session(&Session {
                    status,
                    ..project_session(id, "console-1", Some("a"))
                })
                .unwrap();
        }
        (state, dir)
    }

    /// A process for the session `id` whose shell command leaves one process running in a
    /// terminal session of its own, registered with its output read. Answers the process and the
    /// id of that tree, once the ledger has it as the session's.
    fn running_leaving_one(state: &Arc<AppState>, dir: &ScratchDir, id: &str) -> (StandIn, String) {
        let pid_file = dir.join(format!("{id}-child"));
        let live = fake_live_session(
            id,
            crate::protocol::Agent::Claude,
            80,
            24,
            &format!(
                "/usr/bin/perl -MPOSIX -e 'POSIX::setsid(); {} exec \"/bin/sleep\", \"300\";' &\n\
                 exec sleep 300",
                perl_write(&pid_file, "$$")
            ),
        );
        state.register_live(live.clone());
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        let child = wait_for_pid(&pid_file);
        let trees = state
            .process_ledger
            .trees_of_sessions(&[id.to_string()], Duration::ZERO);
        assert_eq!(trees.len(), 1, "{trees:?}");
        assert!(trees[0].id.starts_with(&format!("{child}:")));
        (live, trees[0].id.clone())
    }

    fn pid_of(tree: &str) -> i32 {
        tree.split(':').next().unwrap().parse().unwrap()
    }

    async fn call(state: &Arc<AppState>, caller: &str, input: Value) -> Result<Value> {
        handover_processes(
            state,
            &state.session_record(caller).unwrap(),
            &arguments(input),
        )
        .await
    }

    async fn processes_in_get_session(state: &Arc<AppState>, id: &str) -> Vec<String> {
        let read = crate::mcp::reads::get_session(
            state,
            &state.session_record("a").unwrap(),
            &arguments(json!({ "session": id })),
        )
        .await
        .unwrap();
        read["processes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tree| tree["id"].as_str().unwrap().to_string())
            .collect()
    }

    /// The owner moves a tree between two sessions of its own: `get_session` lists it under the
    /// receiving one, the giving one is told, and the tree is stopped when the receiving
    /// session's launch ends, not the giving one's.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_owner_hands_a_process_tree_from_one_session_of_its_own_to_another() {
        let (state, dir) = fixture("handover-processes");
        let _caller = running(&state, "a");
        let (from, tree) = running_leaving_one(&state, &dir, "kid");
        let _guard = KillOnDrop(vec![pid_of(&tree)]);
        let to = running(&state, "other-kid");
        assert_eq!(
            processes_in_get_session(&state, "kid").await,
            std::slice::from_ref(&tree)
        );
        assert!(processes_in_get_session(&state, "asleep-kid")
            .await
            .is_empty());

        let result = call(
            &state,
            "a",
            json!({ "processes": [tree, format!(" {tree} ")], "from": "kid", "to": "other-kid" }),
        )
        .await
        .unwrap();

        assert_eq!(result["from"], "kid");
        assert_eq!(result["to"], "other-kid");
        assert_eq!(result["processes"][0]["id"], tree.as_str());
        assert_eq!(result["processes"][0]["session_id"], "other-kid");
        let note = result["note"].as_str().unwrap();
        assert!(
            note.contains("It keeps running") && note.contains("holds it, so it is stopped"),
            "{note}"
        );
        assert!(processes_in_get_session(&state, "kid").await.is_empty());
        assert_eq!(
            processes_in_get_session(&state, "other-kid").await,
            std::slice::from_ref(&tree)
        );
        let told = output_containing(&from, "no longer yours");
        assert!(told.contains(&tree), "{told}");
        output_containing(&to, "handed the process tree");

        state.process_ledger.end_launch(&from.launch_id);
        assert!(
            is_running(pid_of(&tree)),
            "stopped with the session that gave it"
        );
        state.process_ledger.end_launch(&to.launch_id);
        assert_stops_running(pid_of(&tree));
    }

    /// Everything that is refused moves nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_process_handover_moves_nothing() {
        let (state, dir) = fixture("handover-processes-refused");
        let _caller = running(&state, "a");
        let (_from, tree) = running_leaving_one(&state, &dir, "kid");
        let (_busy, busy_tree) = running_leaving_one(&state, &dir, "busy-kid");
        let (_theirs, their_tree) = running_leaving_one(&state, &dir, "of-b");
        let _guard = KillOnDrop(vec![pid_of(&tree), pid_of(&busy_tree), pid_of(&their_tree)]);
        let _to = running(&state, "other-kid");

        for (input, reason) in [
            (
                json!({ "processes": ["1:1"], "from": "kid", "to": "other-kid" }),
                "`1:1` is not a process tree",
            ),
            (
                json!({ "processes": [tree, "1:1"], "from": "kid", "to": "other-kid" }),
                "`1:1` is not a process tree",
            ),
            (
                json!({ "processes": [busy_tree], "from": "busy-kid", "to": "other-kid" }),
                "in the middle of a turn",
            ),
            (
                json!({ "processes": [tree], "from": "kid", "to": "asleep-kid" }),
                "has no process running, so it cannot take processes",
            ),
            (
                json!({ "processes": [tree], "from": "asleep-kid", "to": "other-kid" }),
                "has no process running, so it holds no process to hand over",
            ),
            (
                json!({ "processes": [tree], "from": "kid", "to": "archived-of-a" }),
                "is archived",
            ),
            (
                json!({ "processes": [their_tree], "from": "of-b", "to": "other-kid" }),
                "bound to another console session",
            ),
            (
                json!({ "processes": [tree], "from": "kid", "to": "kid" }),
                "are the same session",
            ),
            (
                json!({ "processes": [], "from": "kid", "to": "other-kid" }),
                "at least one process tree",
            ),
            (
                json!({ "processes": [" "], "from": "kid", "to": "other-kid" }),
                "blank id",
            ),
        ] {
            let err = call(&state, "a", input.clone()).await.expect_err("refused");
            assert!(err.to_string().contains(reason), "{input}: {err}");
        }
        for (session, held) in [
            ("kid", &tree),
            ("busy-kid", &busy_tree),
            ("of-b", &their_tree),
        ] {
            assert_eq!(
                processes_in_get_session(&state, session).await,
                std::slice::from_ref(held),
                "{session}"
            );
        }
        assert!(processes_in_get_session(&state, "other-kid")
            .await
            .is_empty());
    }

    /// A session whose model is being changed is restarted in place, holding what its process
    /// started for the relaunch: trees are moved neither from it nor to it meanwhile.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_session_under_a_model_change_is_neither_given_nor_handed_processes() {
        let (state, dir) = fixture("handover-processes-model-change");
        let _caller = running(&state, "a");
        let (_from, tree) = running_leaving_one(&state, &dir, "kid");
        let _guard = KillOnDrop(vec![pid_of(&tree)]);
        let _to = running(&state, "other-kid");
        let claim = state.begin_switch("kid").unwrap();
        state.mark_model_change("kid");

        let err = call(
            &state,
            "a",
            json!({ "processes": [tree], "from": "kid", "to": "other-kid" }),
        )
        .await
        .expect_err("refused while the model changes");
        assert!(err.to_string().contains("already being started"), "{err}");
        drop(claim);
        assert_eq!(
            processes_in_get_session(&state, "kid").await,
            std::slice::from_ref(&tree)
        );
    }
}
