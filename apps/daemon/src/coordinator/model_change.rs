//! Changing the model and effort of a session, restarting it where the agent cannot change them
//! in place.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use crate::accounts::relocate;
use crate::accounts::switch_continue;
use crate::agents::launch_settings::normalize_session_launch_setting;
use crate::env_shell;
use crate::protocol::{error_code, CodedError, Session, SessionStatus, TerminalAppearance};
use crate::session::term;
use crate::state::{session_archived, AppState};

use super::account_choice::account_dir;
use super::bindings::refuse_unless_still_askers;
use super::launch::{
    check_launch_settings, relaunch_session_with, session_scope, wait_until_down,
    wait_until_down_within, LaunchEdit, SettingsChecked, SWITCH_STOP_TIMEOUT,
};
use super::lifecycle::refuse_if_archived;
use super::restart::{
    came_up_under_claim, give_up_what_a_restart_held, record_needs_shell_env,
    stop_carrying_processes,
};
use super::resume::resumable_agent_session_id;
use super::validate::field_required;

/// What [`set_session_model_with`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelChange {
    /// The session already had that model and effort as its own: nothing was written or restarted.
    Unchanged,
    /// The session had no process: the model and effort were recorded for its next resume.
    Recorded,
    /// The session's process was ended and relaunched with them, resuming its conversation.
    /// `continued` says whether it was sent [`switch_continue::CONTINUE_PROMPT`] to carry on with
    /// a turn the restart cut off or a usage limit had stopped.
    Restarted { continued: bool },
}

/// Changes the model and reasoning effort of a session that is not archived, as the user asks from
/// the window: see [`set_session_model_with`]. `model` and `effort` are as `open_session` takes
/// them, each absent to keep what the session has.
pub async fn set_session_model(
    state: &Arc<AppState>,
    id: &str,
    model: Option<Option<String>>,
    effort: Option<Option<String>>,
    terminal_appearance: Option<TerminalAppearance>,
) -> Result<ModelChange> {
    set_session_model_with(
        state,
        id,
        LaunchEdit { model, effort },
        ModelAsker::User,
        terminal_appearance,
        env_shell::launch_snapshot,
        term::launch,
    )
    .await
}

/// Who asks [`set_session_model_with`] for a change.
#[derive(Clone, Copy)]
pub(crate) enum ModelAsker<'a> {
    /// The user, from the window, who owns nothing to check and can see what the session waits on.
    User,
    /// The owner whose id this is, over MCP: the session must still be bound to it once the change
    /// holds its claim, and a session waiting for the user or dormant is refused
    /// (`session_waiting_for_user`), since the restart would end the prompt the user alone answers
    /// or the background-agents view they are in — except where its only wait is a usage-limit
    /// dialog, which the restart is free to end, as an automatic account switch does
    /// (`AppState::status_for_switch`).
    Owner(&'a str),
}

/// Changes the model and reasoning effort `edit` gives a session, which become its own choices,
/// checked as opening a session checks them against what its agent offers under its own account
/// (only the values that change).
/// One mechanism serves every agent, the account switch's without the move: a session with a
/// running process has the process ended the way archiving ends it (archiving nothing), the new
/// values recorded, and is relaunched on the same account resuming its conversation, as a resume
/// does. What the process started (a dev server, say) is carried over to the relaunch as for a
/// switch, and kept among the processes of the session's project when the restart fails after it
/// ended the process (see [`keep_what_a_failed_restart_carried`]). A turn the restart cut off, or
/// one a usage limit had stopped, is sent [`switch_continue::CONTINUE_PROMPT`] as the relaunch's
/// instruction, as after a switch; otherwise the session comes back awaiting instructions. A
/// session waiting on a prompt of its agent's has that prompt ended with the process, unless an
/// owner asks (see [`ModelAsker::Owner`]).
///
/// Every client is shown a running session `starting` from the moment the change holds its claim,
/// before anything is checked that takes time, until it is over (`AppState::mark_model_change`),
/// and the model and effort the agent's records showed are forgotten once the process is down,
/// for what it records after the relaunch to replace them.
///
/// Values the session already has are answered [`ModelChange::Unchanged`] with nothing done,
/// before anything else can refuse them, except while another change of its model is under way,
/// which claims the session (`session_already_starting`). An interrupted session only has the
/// values recorded, for its next resume. Refused, with nothing changed: no value given
/// (`field_required`), an archived session (`session_archived`; reopening one with another model is
/// `resume_session`'s), one being launched, resumed, switched, handed off or otherwise claimed
/// (`session_already_starting`) or shown starting, a value the agent does not offer, and a change
/// of its account's sharing under way. Refused after the process was asked to end, with nothing
/// recorded: `model_change_did_not_stop`. Once the process is down, a relaunch the launch rules
/// refuse, or one whose process ends at once (`model_change_did_not_come_up`), leaves the session
/// interrupted with the model and effort it had.
///
/// The shell snapshot the default account is resolved from comes from `snapshot`, and the process
/// is started by `launch`: `env_shell::launch_snapshot` and `term::launch` outside the tests.
///
/// [`keep_what_a_failed_restart_carried`]: super::restart::keep_what_a_failed_restart_carried
pub(crate) async fn set_session_model_with(
    state: &Arc<AppState>,
    id: &str,
    edit: LaunchEdit,
    asker: ModelAsker<'_>,
    terminal_appearance: Option<TerminalAppearance>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl FnOnce(term::LaunchRequest) -> Result<term::Launch> + Send + 'static,
) -> Result<ModelChange> {
    if edit.model.is_none() && edit.effort.is_none() {
        return Err(field_required("model"));
    }
    let (model, effort) = (
        normalize_session_launch_setting(edit.model),
        normalize_session_launch_setting(edit.effort),
    );
    // The values the session would have, or `None` when it has them already: answered before the
    // claim, which another holder may have, since there is nothing to change, except while a
    // change of its model is under way, whose values it may yet put back.
    let changed = |session: &Session| -> Result<Option<_>> {
        if let ModelAsker::Owner(owner) = asker {
            refuse_unless_still_askers(session, Some(owner))?;
        }
        refuse_if_archived(session)?;
        let model = model.clone().or_else(|| session.model.clone());
        let effort = effort.clone().or_else(|| session.effort.clone());
        Ok(((&model, &effort) != (&session.model, &session.effort)).then_some((model, effort)))
    };
    if changed(&state.session_record(id)?)?.is_none() && !state.is_changing_model(id) {
        return Ok(ModelChange::Unchanged);
    }
    // Held until this function returns, as a switch holds its own: nothing else launches,
    // resumes, switches or deletes the session while its process is down or being watched.
    let claim = state.begin_switch(id)?;
    let before = state.session_record(id)?;
    let Some((model, effort)) = changed(&before)? else {
        return Ok(ModelChange::Unchanged);
    };
    if state.status_for_model_change(&before) == SessionStatus::Starting {
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_STARTING,
            "this session is already being started",
            &[("session", id)],
        ));
    }
    refuse_if_owner_must_wait(state, asker, &before)?;
    // Shown restarting at once, as an account switch is shown switching once it has its claim, so
    // that no window shows the old status or offers another change while the checks run.
    if !before.status.has_no_process() {
        state.mark_model_change(id);
    }
    crate::accounts::shared_config::refuse_if_changing(state, before.account_id.as_deref())?;
    let (project, console) = session_scope(state, &before)?;
    check_launch_settings(
        state,
        before.agent,
        before.account_id.clone(),
        project.as_ref(),
        &console,
        model.clone(),
        effort.clone(),
        SettingsChecked::Changed {
            model: &before.model,
            effort: &before.effort,
        },
    )
    .await?;
    let current = state.session_record(id)?;
    if current.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }
    if current.status.has_no_process() {
        if !state
            .store
            .set_session_launch_overrides_unless_archived(id, &model, &effort, None)?
        {
            return Err(session_archived(id));
        }
        state.publish_session_by_id(id);
        return Ok(ModelChange::Recorded);
    }

    // The default account has no directory of its own: the snapshot resolves it for the record
    // read below and for the relaunch, which must agree.
    let shell_env = if record_needs_shell_env(&current) {
        Some(tokio::task::spawn_blocking(snapshot).await??)
    } else {
        None
    };
    let current = state.session_record(id)?;
    if current.status == SessionStatus::Archived {
        return Err(session_archived(id));
    }
    // A prompt may have come up while the snapshot was taken.
    refuse_if_owner_must_wait(state, asker, &current)?;
    // Committed to the restart from here: a quota recovery waiting to resume the session would
    // only be cut off by it.
    state.quota_recovery.cancel(id);
    state.mark_model_change(id);
    let (held_for_relaunch, ended_a_process) = stop_carrying_processes(state, id).await?;
    if let Err(err) = wait_until_down_within(state, id, stop_timeout(state)).await {
        let stopped = err
            .downcast_ref::<CodedError>()
            .is_some_and(|coded| coded.code == error_code::SESSION_DID_NOT_STOP);
        return Err(if stopped {
            CodedError::raised(
                error_code::MODEL_CHANGE_DID_NOT_STOP,
                "the session's process did not end in time, so its model was not changed",
                &[("session", id)],
            )
        } else {
            err
        });
    }
    // From here the process is down; a failure that follows is the restart's, whatever it was.
    let after_stop = async {
        // Archiving it meanwhile is the user's later word on the matter. Hooks may also have
        // learned the conversation's identity while validation or the shell snapshot was pending.
        let stopped = state.session_record(id)?;
        if stopped.status == SessionStatus::Archived {
            return Err(session_archived(id));
        }
        let reading_state = state.clone();
        let shell = shell_env.clone();
        let (carry_on, boundary) = tokio::task::spawn_blocking(move || {
            let boundary = crate::records::collector::observed_boundary(
                &reading_state,
                &stopped,
                shell.clone(),
            );
            let carry_on = ended_a_process
                && resumable_agent_session_id(&stopped).is_some_and(|agent_session_id| {
                    let dir =
                        account_dir(stopped.agent, stopped.config_dir.as_deref(), shell.as_ref());
                    relocate::locate(stopped.agent, &dir, &agent_session_id).is_ok_and(|record| {
                        switch_continue::should_continue(stopped.agent, &dir.join(record))
                    })
                });
            (carry_on, boundary)
        })
        .await?;

        let boundary = serde_json::to_string(&boundary)?;
        if !state.store.set_session_launch_overrides_unless_archived(
            id,
            &model,
            &effort,
            Some(&boundary),
        )? {
            return Err(session_archived(id));
        }
        state.publish_session_by_id(id);
        let put_back = || {
            if let Err(revert) =
                state.set_session_launch_overrides(id, &before.model, &before.effort)
            {
                tracing::warn!(
                    session = %id,
                    %revert,
                    "putting the session's model and effort back failed"
                );
            }
        };
        if let Err(err) = relaunch_session_with(
            state,
            &claim,
            id,
            carry_on.then_some(switch_continue::CONTINUE_PROMPT),
            false,
            shell_env,
            terminal_appearance,
            launch,
        )
        .await
        {
            put_back();
            return Err(err);
        }
        if !came_up_under_claim(state, id).await {
            // Once the exit watcher has recorded it interrupted, so the record put back is the one
            // every client is left with.
            if let Err(err) = wait_until_down(state, id).await {
                tracing::warn!(session = %id, %err, "the relaunched process was not recorded as ended");
            }
            put_back();
            return Err(CodedError::raised(
                error_code::MODEL_CHANGE_DID_NOT_COME_UP,
                "the session ended as soon as it was relaunched with the new model, so it keeps the \
                 model and effort it had",
                &[("session", id)],
            ));
        }
        Ok(carry_on)
    };
    let restarted = after_stop.await;
    give_up_what_a_restart_held(
        state,
        id,
        held_for_relaunch,
        restarted.is_err() && ended_a_process,
    )
    .await;
    let carry_on = restarted?;
    Ok(ModelChange::Restarted {
        continued: carry_on,
    })
}

/// How long a change of a session's model waits for the process it ended to be recorded down:
/// [`SWITCH_STOP_TIMEOUT`], shortened by a test.
#[cfg_attr(not(test), allow(unused_variables))]
fn stop_timeout(state: &AppState) -> std::time::Duration {
    #[cfg(test)]
    if let Some(shorter) = *state.model_change_stop_timeout.lock().unwrap() {
        return shorter;
    }
    SWITCH_STOP_TIMEOUT
}

/// Refuses a change of the model of `session` an owner asked for while the session waits for the
/// user, beyond a usage-limit dialog, which the restart is free to end, or shows a view of its
/// background agents, which the restart would end all the same (see [`ModelAsker::Owner`]).
fn refuse_if_owner_must_wait(
    state: &AppState,
    asker: ModelAsker<'_>,
    session: &Session,
) -> Result<()> {
    if !matches!(asker, ModelAsker::Owner(_)) {
        return Ok(());
    }
    let message = match state.status_for_model_change(session) {
        SessionStatus::WaitingUser => {
            "this session is waiting for the user, who alone can answer what it is showing; \
             restarting it would end that prompt"
        }
        SessionStatus::Dormant => {
            "the user is in this session's background-agents view; restarting it would close it"
        }
        _ => return Ok(()),
    };
    Err(CodedError::raised(
        error_code::SESSION_WAITING_FOR_USER,
        message,
        &[("session", &session.id)],
    ))
}

#[cfg(test)]
mod tests;
