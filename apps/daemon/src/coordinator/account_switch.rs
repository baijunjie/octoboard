//! Moving a session to another account: the plan, the claim and the waits around its restart, and
//! putting it back on its account when the move fails.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::accounts::relocate;
use crate::adapter;
use crate::messaging::texts;
use crate::protocol::{error_code, Agent, CodedError, Session, SessionStatus, TerminalAppearance};
use crate::records::switch_continue;
use crate::session::term;
use crate::state::refusals::refuse_if_archived;
use crate::state::{session_archived, AppState};

use super::accounts::refuse_unless_account_usable;
use super::bindings::refuse_unless_still_askers;
use super::launch::{checked_launch_overrides, relaunch_session_with, wait_until_down, LaunchEdit};
use super::restart::{self, came_up_under_claim};
use super::resume::resumable_agent_session_id;
use super::validate::field_required;
use crate::accounts::choice::{account_dir, checked_account_reference};

/// What a switch worked out before it ended anything.
struct SwitchPlan {
    /// The snapshot the default account was resolved from, handed on to the relaunch so the copy
    /// and the launch cannot disagree about where the default account is.
    shell_env: Option<HashMap<String, String>>,
    /// The conversation to copy. What the agent keeps beside it is listed only once the process is
    /// down, as it may still be creating some. None for a session with no conversation on the
    /// agent's side.
    relocation: Option<Relocation>,
}

/// A conversation to copy from the source account's directory to the target's.
#[derive(Debug, PartialEq)]
struct Relocation {
    from: PathBuf,
    to: PathBuf,
    /// The path of the record, relative to both directories.
    record: PathBuf,
    /// The agent session id the record belongs to.
    agent_session_id: String,
}

/// Everything a switch can refuse before it ends the session's process: the launch rule on the
/// target directory, and the conversation record being where it is expected. The default account
/// pins nothing, so each side of it resolves from one shell snapshot taken here, once, and kept
/// for the relaunch; so does a Codex switch, whose environment says where a home keeps its
/// database. `source` and `target` are the pinned directories, `None` for the default account.
/// Blocking.
fn prepare_switch(
    agent: Agent,
    resume_id: Option<&str>,
    source: Option<&str>,
    target: Option<&str>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<SwitchPlan> {
    // Before anything is copied: the copy would create the directory's `sessions/` and so make a
    // Grok home that was never initialized pass the same check at launch.
    adapter::refuse_unlaunchable_account(agent, target.map(Path::new))?;
    // Codex's environment also says where a home keeps its database, which the copy must find.
    let shell_env = if source.is_none() || target.is_none() || agent == Agent::Codex {
        Some(snapshot().context("snapshotting the user's shell environment")?)
    } else {
        None
    };
    let Some(resume_id) = resume_id else {
        return Ok(SwitchPlan {
            shell_env,
            relocation: None,
        });
    };
    let resolve = |pinned: Option<&str>| account_dir(agent, pinned, shell_env.as_ref());
    let (from, to) = (resolve(source), resolve(target));
    let record = relocate::locate(agent, &from, resume_id)?;
    Ok(SwitchPlan {
        shell_env,
        relocation: Some(Relocation {
            from,
            to,
            record,
            agent_session_id: resume_id.to_owned(),
        }),
    })
}

/// Moves a session to another account of its own agent: ends its process the way archiving does,
/// copies its conversation into the target account's directory, records the account and the
/// directory on the session, and relaunches it as a resume does — with a prompt to carry on where
/// the switch cut a turn off or a usage limit had stopped one (see
/// `crate::records::switch_continue`). The user or an owner asks for it, and so does Octoboard
/// itself for a session whose turn stopped on its account's usage limit, while
/// `auto_switch_on_usage_limit` is on (see `crate::coordinator::auto_switch`).
///
/// `account` is the target by id, `None` for the default account. Refused, with nothing recorded
/// and (where it can be told beforehand) the process left running: a launch, resume or switch of
/// the session under way, an archived session, the account it is on already, an account that is
/// not one of the session's agent, a disabled account, an account whose plan quota is used up, an
/// account the last quota reading found not logged in, a Grok target that is not an initialized
/// home, a conversation record that is not where it is expected, and a source or target whose
/// sharing is being changed (`account_sharing_in_progress`). Once the process has been ended, a
/// relocation that does not complete or a relaunch that is refused or does not come up leaves the
/// session interrupted on the account it had, with the conversation still in both directories.
///
/// Two windows break "always back on the account it had", neither harmful. A daemon that dies
/// between recording the new account and the relaunch leaves the session on the new account, which
/// already holds the copy, so the conversation resumes there. And a switch to the *default* Grok
/// account is not checked for being an initialized home (`refuse_unlaunchable_account` is a no-op
/// for an account that pins nothing, by design: that directory is the user's own setup), so an
/// uninitialized `~/.grok` gets the copy, and the relaunch either comes up on Grok's own sign-in,
/// which counts as a switch made like any target that is not signed in, or dies, which is reported
/// as a failed switch and put back.
///
/// It archives nothing: the process is ended by [`stop_process`], not by [`archive_session`], and
/// the status the session passes through is the interrupted one the exit watcher gives any
/// process that ends without the session having been archived.
///
/// [`archive_session`]: super::lifecycle::archive_session
/// [`stop_process`]: super::launch::stop_process
pub async fn switch_session_account(
    state: &Arc<AppState>,
    id: &str,
    account: Option<Option<String>>,
    terminal_appearance: Option<TerminalAppearance>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
) -> Result<()> {
    switch_session_account_with(
        state,
        id,
        account,
        None,
        terminal_appearance,
        LaunchEdit::default(),
        snapshot,
        term::launch,
    )
    .await
    .map(drop)
}

/// [`switch_session_account`] with the asker: `asked_by` is the asking owner's id, `None` for the
/// user. The session must still be bound to the owner once the switch holds its claim (the owner
/// checked before it), else the switch is refused with nothing changed. `launch_edit` is what the
/// user changed of the session's model and effort, checked with the rest and recorded just before
/// the relaunch that reads it, and put back with the account when the switch fails. `launch` is
/// `term::launch` outside the tests, which cannot start an agent. Answers whether the relaunched
/// session was sent the prompt to carry on with the turn the switch cut off or a usage limit
/// stopped (see `crate::records::switch_continue`). A session that was not is told what the switch
/// ended of what it started, if anything, once the switch has let go of its claim.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn switch_session_account_with(
    state: &Arc<AppState>,
    id: &str,
    account: Option<Option<String>>,
    asked_by: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    launch_edit: LaunchEdit,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<bool> {
    let switched = switch_session_account_if(
        state,
        id,
        account,
        asked_by,
        SwitchOnlyIf::default(),
        terminal_appearance,
        launch_edit,
        snapshot,
        launch,
    )
    .await
    .map_err(|failed| failed.error)?;
    if let Some(ended) = switched.ended {
        restart::tell_what_a_restart_ended(state, id, ended).await;
    }
    Ok(switched.continued)
}

/// What a switch did, once the session is relaunched and has come up.
#[derive(Debug)]
pub(crate) struct Switched {
    /// The relaunched session was sent the prompt to carry on.
    pub continued: bool,
    /// What the switch ended of what the session had started (see
    /// [`texts::processes_ended_by_restart`]), for a session that was not continued: one that was
    /// has it in its prompt. Left to the caller to tell, as the switch holds the session's claim
    /// and writing into the session is not done under it.
    pub ended: Option<String>,
}

/// What a switch decided on a record read before its claim still needs to find once it holds the
/// claim. Nothing is required by default.
#[derive(Clone, Copy, Default)]
pub(crate) struct SwitchOnlyIf<'a> {
    /// The failed turn and process whose verified quota recovery authorizes this restart.
    pub recovery: Option<&'a super::auto_switch::PendingRecovery>,
    /// The account the session must still be on (`Some(None)` the default account).
    pub on_account: Option<Option<&'a str>>,
    /// The session must not be shown as waiting for the user, by whatever route, nor as dormant
    /// over a view of its background agents: moving it would end the prompt it waits at or the
    /// view. Relaxed when `usage_limited` is set: the usage-limit dialog of an agent's screen is
    /// the point of that switch and does not count as a wait.
    pub not_waiting: bool,
    /// The session must have a process at its prompt or in a turn (shown `working` or `idle`),
    /// so one the user stopped, or whose process exited since the record was read, is not
    /// brought back.
    pub running: bool,
    /// The session's last turn must have stopped on the plan's usage limit, as its conversation
    /// record says (`switch_continue::stopped_on_usage_limit`), read once more under the claim
    /// before the process is ended. A caller that may need to wait for the record to say so does
    /// it ahead of the claim ([`wait_for_usage_limit`]). Also marks the switch as one made for the
    /// limit: once the process has come up, the account left is noted for the session
    /// (`AppState::note_left_for_limit`), under the claim.
    pub usage_limited: bool,
    /// Octoboard's own switch: the process it launches is watched under the claim, so the exit
    /// watcher does not tell the owner that it ended on its own (`LaunchClaim::watches_launch`).
    pub watch_launch: bool,
    /// Text put ahead of the prompt to carry on, in the same prompt, so that the session is told
    /// of the switch without a turn of its own. Not sent when the switch does not carry on: what
    /// the switch ended is then left to the caller too ([`Switched::ended`]), to put in its own
    /// message.
    pub note: Option<&'a str>,
}

/// Why [`switch_session_account_if`] failed: the error as every other caller sees it, untouched,
/// and whether the switch had ended the session's process and seen it down by then, which leaves
/// the session interrupted and not as it was (see [`restart::Failed`]).
pub(crate) type SwitchFailed = restart::Failed;

/// The switch was refused because what it was decided on no longer holds; nothing was changed.
#[derive(Debug)]
pub(crate) struct NoLongerDue(pub &'static str);

impl std::fmt::Display for NoLongerDue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for NoLongerDue {}

/// [`switch_session_account_with`] that also holds the session to `only_if`, checked under the
/// claim against the record read there: a session that no longer meets it is refused with
/// [`NoLongerDue`], nothing changed. A failure says, beside the error, whether this switch ended
/// the session's process and `wait_until_down` saw it down ([`SwitchFailed`]), the caller's cue
/// that the session is left interrupted; a failure before that, a process that did not stop in time
/// (`session_did_not_stop`) included, or after a process that had ended by itself, is not.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn switch_session_account_if(
    state: &Arc<AppState>,
    id: &str,
    account: Option<Option<String>>,
    asked_by: Option<&str>,
    only_if: SwitchOnlyIf<'_>,
    terminal_appearance: Option<TerminalAppearance>,
    launch_edit: LaunchEdit,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<Switched, SwitchFailed> {
    let account = account.ok_or_else(|| field_required("account"))?;
    // Held until this function returns, after the come-up verdict and any revert, so nothing else
    // can launch, resume, switch or delete the session while its process is down, while the
    // relaunched one is being watched, or while the account is put back; refused here when one
    // of those is under way.
    if only_if.recovery.is_none() {
        state.quota_recovery.cancel(id);
    }
    let claim = state.begin_account_switch(id, only_if.watch_launch)?;
    if only_if.recovery.is_some_and(|wait| !wait.still_due(state)) {
        return Err(NoLongerDue("the quota recovery no longer belongs to this turn").into());
    }
    let before = state.session_record(id)?;
    refuse_unless_still_askers(&before, asked_by)?;
    refuse_if_archived(&before)?;
    if only_if
        .on_account
        .is_some_and(|expected| expected != before.account_id.as_deref())
    {
        return Err(NoLongerDue("the session is on another account now").into());
    }
    // A turn the usage limit stopped may show the agent's limit dialog, which the switch ends.
    let shown = if only_if.usage_limited {
        state.status_for_switch(&before)
    } else {
        state.shown_status(&before)
    };
    if only_if.not_waiting && matches!(shown, SessionStatus::WaitingUser | SessionStatus::Dormant) {
        return Err(NoLongerDue("the session waits for the user now").into());
    }
    if only_if.running
        && !(matches!(shown, SessionStatus::Working | SessionStatus::Idle)
            && state.live_session(id).is_some())
    {
        return Err(NoLongerDue("the session has no running process now").into());
    }
    let target_account = checked_account_reference(state, before.agent, account)?;
    let same_account = before.account_id == target_account;
    if !only_if
        .recovery
        .is_some_and(super::auto_switch::PendingRecovery::permits_current_account)
    {
        refuse_if_on_account(&before, &target_account)?;
    }
    let target_dir = state.store.account_dir(&target_account)?;
    refuse_unless_account_usable(
        state,
        before.agent,
        target_account.as_deref(),
        target_dir.as_deref(),
        true,
    )?;
    // Asked under the claim, which a change of the source's sharing counts as a session starting.
    crate::accounts::shared_config::refuse_if_changing(state, before.account_id.as_deref())?;
    crate::accounts::shared_config::refuse_if_changing(state, target_account.as_deref())?;
    let overrides =
        checked_launch_overrides(state, &before, target_account.clone(), launch_edit).await?;

    let plan = {
        let (agent, resume_id) = (before.agent, resumable_agent_session_id(&before));
        let (source, target) = (before.config_dir.clone(), target_dir.clone());
        tokio::task::spawn_blocking(move || {
            prepare_switch(
                agent,
                resume_id.as_deref(),
                source.as_deref(),
                target.as_deref(),
                snapshot,
            )
        })
        .await??
    };

    if only_if.usage_limited {
        let stopped = match &plan.relocation {
            Some(relocation) => {
                let (agent, record) = (before.agent, relocation.from.join(&relocation.record));
                tokio::task::spawn_blocking(move || {
                    switch_continue::stopped_on_usage_limit(agent, &record)
                })
                .await?
            }
            None => false,
        };
        if !stopped {
            return Err(
                NoLongerDue("the session's last turn did not stop on a usage limit").into(),
            );
        }
    }

    if only_if.recovery.is_some_and(|wait| !wait.still_due(state)) {
        return Err(
            NoLongerDue("the quota recovery was cancelled while preparing the restart").into(),
        );
    }
    if !same_account {
        if let Some(wait) = only_if.recovery {
            if wait.own_account_recovered(state)? {
                return Err(
                    NoLongerDue("the held account recovered while preparing the switch").into(),
                );
            }
        }
    }
    // Borrowed by the restart below, so that it stays held until this function returns.
    let claim = &claim;
    // From the process being down, a failure that follows leaves the session interrupted when this
    // switch ended it, and is marked so; one whose process had ended before is not this switch's
    // to report.
    restart::run(
        state,
        id,
        |err| err,
        |ended_a_process, ended_trees| async move {
            // Archiving it meanwhile is the user's later word on the matter.
            if state.session_record(id)?.status == SessionStatus::Archived {
                return Err(session_archived(id));
            }
            // The process is down, so what the account it ran on holds now is final: a Codex
            // thread may have started a segment since the plan, and that head is the one carried.
            // Only a turn this switch ended, or that was waiting out a usage limit in the process
            // it ended, is carried on: a session whose process had ended before was left there by
            // something else.
            let (relocation, carry_on) = match plan.relocation {
                Some(mut relocation) => {
                    let agent = before.agent;
                    tokio::task::spawn_blocking(move || -> Result<_> {
                        if agent == Agent::Codex {
                            relocation.record = relocate::locate(
                                agent,
                                &relocation.from,
                                &relocation.agent_session_id,
                            )?;
                        }
                        let carry_on = restart::carry_on(
                            agent,
                            &relocation.from,
                            &relocation.record,
                            ended_a_process,
                        );
                        Ok((Some(relocation), carry_on))
                    })
                    .await??
                }
                None => (None, false),
            };

            // Held from before the copy until the session is recorded on the target, and given up
            // before the relaunch, which takes it itself: no change of sharing moves what the copy
            // writes.
            let copying = crate::accounts::shared_config::before_switch_copy(
                state,
                before.agent,
                target_account.as_deref(),
            )
            .await?;
            if let Some(relocation) =
                relocation.filter(|relocation| relocation.from != relocation.to)
            {
                let agent = before.agent;
                // The environment the relaunch will see, which with the target's `config.toml` says
                // where a Codex home keeps its database.
                let launch_env = plan.shell_env.clone().unwrap_or_default();
                tokio::task::spawn_blocking(move || {
                    let Relocation {
                        from,
                        to,
                        record,
                        agent_session_id,
                    } = relocation;
                    let companions = crate::records::carried::companions(
                        agent,
                        &from,
                        &record,
                        &agent_session_id,
                    );
                    relocate::copy(&from, &to, &record, &companions)?;
                    relocate::repoint_thread_row(
                        agent,
                        &to,
                        &record,
                        &agent_session_id,
                        &launch_env,
                    )
                })
                .await??;
            }
            state.set_session_account(id, target_account.as_deref(), target_dir.as_deref())?;
            drop(copying);
            if let Some((model, effort)) = &overrides {
                if let Err(err) = state.set_session_launch_overrides(id, model, effort) {
                    put_back_on_account(state, &before);
                    return Err(err);
                }
            }

            let ended = texts::processes_ended_by_restart(&ended_trees);
            let prompt_to_carry_on = match only_if.note {
                Some(note) => texts::continue_prompt_after(note, ended.as_deref()),
                None => texts::continue_prompt(ended.as_deref()),
            };
            if let Err(err) = relaunch_session_with(
                state,
                claim,
                id,
                carry_on.then_some(prompt_to_carry_on.as_str()),
                false,
                plan.shell_env,
                terminal_appearance,
                launch,
            )
            .await
            {
                put_back_on_account(state, &before);
                return Err(err);
            }
            if !came_up_under_claim(state, id).await {
                put_back_once_down(state, &before).await;
                return Err(CodedError::raised(
                    error_code::SWITCH_DID_NOT_COME_UP,
                    "the session ended as soon as it was relaunched, so it stays on the account it \
                     had",
                    &[("session", id)],
                ));
            }
            if only_if.usage_limited && !same_account {
                // Noted under the claim, so that a turn waiting for it to end never picks this
                // account.
                state.note_left_for_limit(id, before.account_id.as_deref());
            }
            Ok(Switched {
                continued: carry_on,
                ended: ended.filter(|_| !carry_on),
            })
        },
    )
    .await
}

/// How long a switch for a usage limit waits for the limit to show in the conversation record. An
/// agent may report a failed turn (its `StopFailure` hook) before it has written the entry that
/// says why, so a record that does not say so yet is read again for this long before the turn is
/// judged not to have stopped on the limit. The real ordering is not measured.
#[cfg(not(test))]
const USAGE_LIMIT_RECORD_WAIT: Duration = Duration::from_secs(4);
#[cfg(test)]
const USAGE_LIMIT_RECORD_WAIT: Duration = Duration::from_millis(1500);

/// Whether the conversation record of `session`, found the way [`prepare_switch`] finds it, says
/// the last turn stopped on the usage limit, read again for up to [`USAGE_LIMIT_RECORD_WAIT`] until
/// it does. Meant to run ahead of the switch's claim, so that the wait holds nothing: `shell_env`
/// is the snapshot when [`record_needs_shell_env`]. Refused as `prepare_switch` is when the record
/// is not where it is expected; a session with no conversation has not stopped on a limit.
///
/// [`record_needs_shell_env`]: super::restart::record_needs_shell_env
pub(crate) async fn wait_for_usage_limit(
    session: &Session,
    shell_env: Option<HashMap<String, String>>,
) -> Result<bool> {
    let Some(resume_id) = resumable_agent_session_id(session) else {
        return Ok(false);
    };
    let (agent, source) = (session.agent, session.config_dir.clone());
    let record = tokio::task::spawn_blocking(move || -> Result<PathBuf> {
        if source.is_none() && shell_env.is_none() {
            anyhow::bail!("the default account's directory needs the shell environment");
        }
        let from = account_dir(agent, source.as_deref(), shell_env.as_ref());
        Ok(from.join(relocate::locate(agent, &from, &resume_id)?))
    })
    .await??;
    let deadline = Instant::now() + USAGE_LIMIT_RECORD_WAIT;
    loop {
        let path = record.clone();
        let agent = session.agent;
        let stopped = tokio::task::spawn_blocking(move || {
            switch_continue::stopped_on_usage_limit(agent, &path)
        })
        .await?;
        if stopped || Instant::now() >= deadline {
            return Ok(stopped);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Brings a session a failed account switch left interrupted back up on the account it had
/// (`before` is its record from before the switch), as a resume does and without a prompt: it was
/// at its prompt before the switch, and its turn is not carried on, since the account's usage limit
/// would stop it again. Octoboard's own switch makes this, so that its failure leaves the session
/// as it was (see `crate::coordinator::auto_switch`). Refused, with nothing launched, when the
/// session is running or being started by then (somebody brought it back meanwhile) or archived;
/// fails when the process cannot be started or does not stay up. `target` is the account the
/// failed switch was to move it to (`None` the default account): a session found on neither that
/// nor `before`'s account was moved by somebody else meanwhile, and is left as they left it.
pub(crate) async fn restore_after_failed_switch(
    state: &Arc<AppState>,
    before: &Session,
    target: Option<&str>,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<()> {
    let id = before.id.as_str();
    // Under an account switch's claim, so that the process it starts is watched as the switch's
    // was. Another claim on the session (a transfer of branches, a binding change) is waited for
    // rather than given up on: the session is down and nobody else will bring it back.
    let deadline = Instant::now() + RESTORE_CLAIM_WAIT;
    let claim = loop {
        match state.begin_account_switch(id, true) {
            Ok(claim) => break claim,
            Err(err) if is_already_starting(&err) && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(err) => return Err(err),
        }
    };
    let now = state.session_record(id)?;
    if now.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }
    if !now.status.has_no_process() {
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_RUNNING,
            "this session is not interrupted or archived",
            &[("session", id)],
        ));
    }
    if now.account_id != before.account_id {
        if now.account_id.as_deref() != target {
            return Ok(());
        }
        // A revert that failed on the way is tried again.
        put_back_on_account(state, before);
    }
    relaunch_session_with(state, &claim, id, None, false, None, None, launch).await?;
    if !came_up_under_claim(state, id).await {
        return Err(CodedError::raised(
            error_code::SWITCH_DID_NOT_COME_UP,
            "the session ended as soon as it was relaunched on the account it had",
            &[("session", id)],
        ));
    }
    Ok(())
}

/// How long a restore waits for another claim on the session to end before it gives up.
const RESTORE_CLAIM_WAIT: Duration = Duration::from_secs(15);

/// Whether `err` says another launch or claim holds the session.
fn is_already_starting(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::SESSION_ALREADY_STARTING)
}

/// Records the account, directory, model and effort a switched session had before, `before`'s. A
/// revert that fails is logged, never allowed to hide the reason the switch failed.
fn put_back_on_account(state: &Arc<AppState>, before: &Session) {
    // The model and effort a switch recorded for its relaunch go back with the account.
    restart::put_back_model_and_effort(state, before);
    if let Err(revert) = state.set_session_account(
        &before.id,
        before.account_id.as_deref(),
        before.config_dir.as_deref(),
    ) {
        tracing::warn!(session = %before.id, %revert, "putting the session back on its account failed");
    }
}

/// [`put_back_on_account`] for a relaunched process that has already ended, once the exit watcher
/// has recorded the session interrupted. The watcher does that on its own schedule, so reverting
/// first would let every client see the session idle on its old account with no process until the
/// watcher caught up — after the reply saying the switch failed — and a watcher that read the
/// record just before the revert would publish it with the new account. Waiting first means the
/// restored record is broadcast before the caller replies.
async fn put_back_once_down(state: &Arc<AppState>, before: &Session) {
    if let Err(err) = wait_until_down(state, &before.id).await {
        tracing::warn!(session = %before.id, %err, "the relaunched process was not recorded as ended");
    }
    put_back_on_account(state, before);
}

/// Refuses a switch of `session` to the account it is on already (`None` for the default account).
pub(crate) fn refuse_if_on_account(session: &Session, account: &Option<String>) -> Result<()> {
    if *account == session.account_id {
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_ON_ACCOUNT,
            "this session is on that account already",
            &[("session", &session.id)],
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
