//! The session composer's queue as clients manage it, and the sending of its messages: by the
//! daemon once the session is idle (`when_idle`), or at once by the user (send now), resuming a
//! session with no process. Every message is handed to the session's message delivery as the
//! user's own message (`crate::messaging::delivery::write_queued_off_runtime`), under its holds
//! and refusals; what delivery makes of it is placed back in the queue by
//! `crate::state::composer`. The rules are in "The composer queue" in `apps/daemon/PROTOCOL.md`.

use std::future::Future;
use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use super::messages::{checked_command, MAX_IMAGES};
use crate::messaging::delivery;
use crate::protocol::{
    error_code, CodedError, ProviderOperation, QueuedMessage, QueuedMessageFailureKind,
    QueuedMessageState, QueuedMode, SendMode, SessionStatus,
};
use crate::session::queued_files;
use crate::session::term::{self, Shape};
use crate::state::composer::{failure_of, How};
use crate::state::AppState;
use crate::time::now_millis;

/// What a message holds, as a client gives it.
pub(super) struct Content {
    pub text: String,
    pub images: Vec<String>,
    pub files: Vec<String>,
    pub command: bool,
}

/// `create_queued_message`: puts the message at the end of the session's queue, in `mode` or the
/// session's default, and hands it over at once for `now`. Answers its id.
pub(super) async fn create(
    state: &Arc<AppState>,
    session: &str,
    content: Content,
    mode: Option<SendMode>,
) -> Result<String> {
    let record = state.session_record(session)?;
    refuse_archived(record.status, session)?;
    let (content, planned) = checked(state, session, content).await?;
    let mode = mode.unwrap_or(record.send_mode);
    let message = QueuedMessage {
        id: Uuid::new_v4().to_string(),
        text: content.text,
        images: planned.images.clone(),
        files: planned.files.clone(),
        command: content.command,
        // A message sent now that is refused waits for the user, never going by itself later.
        mode: match mode {
            SendMode::WhenIdle => QueuedMode::WhenIdle,
            SendMode::Now | SendMode::Manual => QueuedMode::Manual,
        },
        state: QueuedMessageState::Waiting,
        failure: None,
        created_at: now_millis(),
    };
    let id = message.id.clone();
    // Its files are in place before the queue shows it, and go back where they were if the queue
    // refuses it.
    let moved = commit(planned).await?;
    if let Err(err) = state.composer_add(session, message, moved.paths()) {
        undo(moved).await;
        return Err(err);
    }
    if mode == SendMode::Now {
        let (state, session, id) = (state.clone(), session.to_string(), id.clone());
        tokio::spawn(async move {
            // Its refusal is on the message.
            let _ = send(&state, &session, &id).await;
        });
    } else {
        look_at_queue(state, session);
    }
    Ok(id)
}

/// `update_queued_message`: changes the waiting message, each field given replacing its own.
/// Files its own create or update moved in that it no longer names are removed; new ones are
/// moved in only once the change is made.
pub(super) async fn update(
    state: &Arc<AppState>,
    session: &str,
    id: &str,
    text: Option<String>,
    images: Option<Vec<String>>,
    files: Option<Vec<String>>,
    command: Option<bool>,
) -> Result<()> {
    let record = state.session_record(session)?;
    refuse_archived(record.status, session)?;
    let before = state
        .queued_messages(session)?
        .into_iter()
        .find(|message| message.id == id)
        .ok_or_else(|| crate::state::composer::not_found(session, id))?;
    if before.state == QueuedMessageState::Sending {
        return Err(crate::state::composer::sending(session, id));
    }
    // Only what is given is written: a field left out keeps what a concurrent edit gave it.
    let given_text = text.is_some();
    let given_command = command.is_some();
    let given_images = images.is_some();
    let given_files = files.is_some();
    let (content, planned) = checked(
        state,
        session,
        Content {
            text: text.unwrap_or_else(|| before.text.clone()),
            images: images.unwrap_or_else(|| before.images.clone()),
            files: files.unwrap_or_else(|| before.files.clone()),
            command: command.unwrap_or(before.command),
        },
    )
    .await?;
    let (images, files) = (planned.images.clone(), planned.files.clone());
    let moved = commit(planned).await?;
    let changed = state.composer_update_files(session, id, moved.paths(), |message| {
        if given_text {
            message.text = content.text;
        }
        if given_images {
            message.images = images;
        }
        if given_files {
            message.files = files;
        }
        if given_command {
            message.command = content.command;
        }
        // The checks above read the message before this change; a concurrent edit may have
        // changed what this one leaves alone, so what a command must be is checked again on the
        // message as it now stands, and a command's text written normalised.
        if message.command {
            if !message.images.is_empty() || !message.files.is_empty() {
                return Err(command_not_alone());
            }
            message.text = checked_command(state, session, &message.text)?;
        }
        Ok(())
    });
    let dropped = match changed {
        Ok(dropped) => dropped,
        Err(err) => {
            undo(moved).await;
            return Err(err);
        }
    };
    state.composer_remove_files(session, &dropped);
    Ok(())
}

/// The refusal of a slash command given images or files: it is sent alone.
fn command_not_alone() -> anyhow::Error {
    CodedError::raised(
        error_code::FIELD_REQUIRED,
        "a slash command is sent alone, with no images or files",
        &[("field", "text")],
    )
}

/// Moves the planned files onto their claimed names, off the runtime ([`queued_files::Planned`]).
async fn commit(planned: queued_files::Planned) -> Result<queued_files::Moved> {
    tokio::task::spawn_blocking(move || planned.commit()).await?
}

/// Moves files back for a queue change that was refused, off the runtime.
async fn undo(moved: queued_files::Moved) {
    let _ = tokio::task::spawn_blocking(move || moved.undo()).await;
}

/// `set_queued_message_mode`: clears the message's failure, as any change to it does.
pub(super) fn set_message_mode(
    state: &Arc<AppState>,
    session: &str,
    id: &str,
    mode: QueuedMode,
) -> Result<()> {
    state.composer_update(session, id, |message| {
        message.mode = mode;
        Ok(())
    })?;
    look_at_queue(state, session);
    Ok(())
}

/// `move_queued_message`.
pub(super) fn move_to(state: &Arc<AppState>, session: &str, id: &str, position: u32) -> Result<()> {
    state.composer_move(session, id, position as usize)?;
    look_at_queue(state, session);
    Ok(())
}

/// `withdraw_queued_message`: takes the message off the queue, waiting or waiting on a resume,
/// or out of delivery where it holds it unwritten, and removes the files its own create or update
/// moved in unless it was written already. Refused with `queued_message_sending` while it is being
/// handed over or written.
pub(super) fn withdraw(state: &Arc<AppState>, session: &str, id: &str) -> Result<()> {
    let (_, removable) = state.composer_withdraw(session, id, |ticket| {
        state.outbox.withdraw_queued(session, ticket)
    })?;
    state.composer_remove_files(session, &removable);
    Ok(())
}

/// `set_session_send_mode`.
pub(super) fn set_send_mode(state: &Arc<AppState>, session: &str, mode: SendMode) -> Result<()> {
    if !state.store.set_session_send_mode(session, mode)? {
        return Err(CodedError::unknown_session(session));
    }
    state.publish_session_by_id(session);
    Ok(())
}

/// `send_queued_message`: sends the waiting message now, resuming the session first where it has
/// no process.
pub(super) async fn send(state: &Arc<AppState>, session: &str, id: &str) -> Result<()> {
    send_with(state, session, id, |state, session| async move {
        super::resume::resume_session(&state, &session, None, None).await
    })
    .await
}

/// [`send`] with the resume of a session with no process supplied by the caller, so a test needs
/// no agent launched. The message is handed over once the resumed session can take one
/// ([`after_drain`]).
pub(super) async fn send_with<F, R>(
    state: &Arc<AppState>,
    session: &str,
    id: &str,
    resume: R,
) -> Result<()>
where
    R: FnOnce(Arc<AppState>, String) -> F,
    F: Future<Output = Result<()>>,
{
    let record = state.session_record(session)?;
    refuse_archived(record.status, session)?;
    // A session still starting or ending reads as running with no process registered: delivery
    // answers for it as for any message of the user's.
    if record.status.has_no_process() {
        state.composer_begin_send(session, id, How::Now, true)?;
        if let Err(err) = resume(state.clone(), session.to_string()).await {
            put_back(state, session, id, How::Now, &err);
            return Err(err);
        }
        // The resumed session ending again without taking it puts it back; one that has ended
        // already is put back now.
        if state.composer_resumed(id)
            && state
                .session_record(session)
                .is_ok_and(|record| record.status.has_no_process())
        {
            state.composer_resume_ended(session);
        }
        return Ok(());
    }
    let message = state.composer_begin_send(session, id, How::Now, false)?;
    hand_over(state, session, message, How::Now).await
}

/// Hands the message, begun as sending, to the session's delivery; a refusal puts it back as
/// `how` says, and is answered.
async fn hand_over(
    state: &Arc<AppState>,
    session: &str,
    message: QueuedMessage,
    how: How,
) -> Result<()> {
    let id = message.id.clone();
    let (text, shape) = written(&message);
    match delivery::write_queued_off_runtime(state, session, text, shape, id.clone()).await {
        Ok(_) => Ok(()),
        Err(err) => {
            put_back(state, session, &id, how, &err);
            Err(err)
        }
    }
}

/// Puts the message back after its hand-over failed with `err`, before delivery held it, unless
/// what delivery reported of it placed it already. A send now keeps its mode. The daemon's own
/// dispatch leaves it waiting, as it was, where the refusal may pass by the next idle; any other
/// refusal of it is set as its failure and switches it to `manual`, so it is not sent again by
/// itself.
fn put_back(state: &Arc<AppState>, session: &str, id: &str, how: How, err: &anyhow::Error) {
    let coded = err.downcast_ref::<CodedError>();
    let passing = coded.is_some_and(|coded| {
        [
            error_code::SESSION_WAITING_FOR_USER,
            error_code::SESSION_TRUST_PENDING,
            error_code::SESSION_NOT_RUNNING,
            error_code::DAEMON_SHUTTING_DOWN,
        ]
        .contains(&coded.code)
    });
    match (how, passing) {
        (How::Idle, true) => state.composer_back(session, id, None, None),
        (how, _) => {
            let failure = coded.map_or_else(
                || crate::protocol::QueuedMessageFailure {
                    kind: QueuedMessageFailureKind::Refused,
                    code: error_code::INTERNAL_ERROR.as_str().to_string(),
                    params: [("detail".to_string(), err.to_string())].into(),
                    message: err.to_string(),
                },
                |coded| failure_of(QueuedMessageFailureKind::Refused, coded),
            );
            let mode = (how == How::Idle).then_some(QueuedMode::Manual);
            state.composer_back(session, id, Some(failure), mode);
        }
    }
}

/// The text a message is written as, its files after it on a line of their own, each bare or
/// quoted as a POSIX shell quotes it, and how it is written besides its text.
fn written(message: &QueuedMessage) -> (String, Option<Shape>) {
    if message.command {
        return (message.text.clone(), Some(Shape::Command));
    }
    let paths = message
        .files
        .iter()
        .map(|path| quoted(path))
        .collect::<Vec<_>>()
        .join(" ");
    let text = match (message.text.is_empty(), paths.is_empty()) {
        (_, true) => message.text.clone(),
        (true, false) => paths,
        (false, false) => format!("{}\n{paths}", message.text),
    };
    let shape = (!message.images.is_empty()).then(|| Shape::Images(message.images.clone()));
    (text, shape)
}

/// `path` bare, or in single quotes where it holds whitespace, a single quote inside written
/// `'\''`, as a dropped file's path is pasted (the UI's `pastedPath`, which an empty path
/// passes bare too).
fn quoted(path: &str) -> String {
    if path.chars().any(char::is_whitespace) {
        format!("'{}'", path.replace('\'', "'\\''"))
    } else {
        path.to_string()
    }
}

/// The content checked as a queued message of the session takes it, with the plan of its files'
/// moves into the session's composer directory: nothing is moved yet.
async fn checked(
    state: &Arc<AppState>,
    session: &str,
    content: Content,
) -> Result<(Content, queued_files::Planned)> {
    let agent = state.session_record(session)?.agent;
    let capabilities = agent.descriptor().capabilities;
    let Content {
        mut text,
        images,
        files,
        command,
    } = content;
    if command {
        if !images.is_empty() || !files.is_empty() {
            return Err(command_not_alone());
        }
        if !capabilities.slash_commands {
            return Err(CodedError::provider_unsupported(
                agent,
                ProviderOperation::SlashCommands,
            ));
        }
        text = checked_command(state, session, &text)?;
    } else if term::is_blank_once_stripped(&text) && images.is_empty() && files.is_empty() {
        return Err(delivery::blank_text());
    }
    if !images.is_empty() && !capabilities.message_images {
        return Err(CodedError::provider_unsupported(
            agent,
            ProviderOperation::MessageImages,
        ));
    }
    if images.len() > MAX_IMAGES {
        return Err(CodedError::raised(
            error_code::UNREADABLE_REQUEST,
            format!("a message takes at most {MAX_IMAGES} images"),
            &[("field", "images")],
        ));
    }
    let (planning, id) = (state.clone(), session.to_string());
    let (planned, images, files) = tokio::task::spawn_blocking(move || -> Result<_> {
        let planned = queued_files::plan(&planning, &id, &images, &files)?;
        Ok((planned, images, files))
    })
    .await??;
    Ok((
        Content {
            text,
            images,
            files,
            command,
        },
        planned,
    ))
}

fn refuse_archived(status: SessionStatus, session: &str) -> Result<()> {
    if status == SessionStatus::Archived {
        return Err(CodedError::raised(
            error_code::SESSION_ARCHIVED,
            "this session is archived",
            &[("session", session)],
        ));
    }
    Ok(())
}

/// Looks at the session's queue after a change to it, as a drain does: a message that can go now
/// goes.
fn look_at_queue(state: &Arc<AppState>, session: &str) {
    if state.composer_has_work(session) {
        after_drain(state, session);
    }
}

/// What the composer queue does once delivery has drained the session `id`'s queue
/// (`AppState::flush_outbox`): hands over the messages a send now waits to give a resumed session
/// once it can take one, and, while the session is idle with nothing held or queued, the first
/// `when_idle` message, one per idle. Started on the runtime, never waited for: the drain that
/// calls it may be the hand-over's own.
pub(crate) fn after_drain(state: &Arc<AppState>, id: &str) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let Ok(record) = state.session_record(id) else {
        return;
    };
    let Some(live) = state.live_session(id) else {
        return;
    };
    let status = state.agent_status(&record);
    let takes = matches!(status, SessionStatus::Working | SessionStatus::Idle)
        && !live.trust.holds_writes()
        && !record.status.has_no_process();
    if !takes {
        return;
    }
    for message in state.composer_take_after_resume(id) {
        let (state, id) = (state.clone(), id.to_string());
        runtime.spawn(async move {
            let _ = hand_over(&state, &id, message, How::Now).await;
        });
    }
    let idle = status == SessionStatus::Idle
        && state.write_hold(id).is_none()
        && state.outbox.len(id) == 0;
    if idle {
        if let Some(message) = state.composer_take_idle(id) {
            let (state, id) = (state.clone(), id.to_string());
            runtime.spawn(async move {
                let _ = hand_over(&state, &id, message, How::Idle).await;
            });
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
