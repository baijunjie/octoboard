//! A session's pages on the report panel, apart from the tools with which it pushes, lists and
//! deletes them: the panel's list of a session's pages and its form submission, which marks the
//! page submitted before it delivers and takes the mark back only when the delivery certainly
//! failed, withdrawing every page of a session that has become a worker, the sweep at daemon start
//! of the pages left on workers, and telling the panel which pages are gone.

use std::sync::Arc;

use anyhow::Result;

use crate::messaging::delivery;
use crate::messaging::texts;
use crate::protocol::{error_code, CodedError, Event};
use crate::state::AppState;
use crate::store::PageMark;

/// Withdraws every page `session_id` pushed, as `delete_pages` with `all` does, for a session that
/// has just lost the report panel's tools by becoming a worker: its pages would stay
/// on the panel, and be submittable, with no way left for it to list or delete them.
pub(crate) fn withdraw_pages(state: &Arc<AppState>, session_id: &str) -> Result<()> {
    // With `None` the store names no id, so its `Err(id)` arm, a named id that is not the
    // session's, cannot happen.
    if let Ok(deleted) = state.store.delete_pages(session_id, None)? {
        announce_deleted(state, session_id, &deleted);
    }
    Ok(())
}

/// Withdraws the pages of every session that has the role of a worker now: those a binding
/// change could not withdraw (a failed withdrawal, a page stored while the binding was being
/// written) and those pushed before pages were withdrawn on binding. Run once when the daemon
/// starts. One session's failure is logged and does not stop the others.
pub(crate) fn withdraw_stray_pages(state: &Arc<AppState>) -> Result<()> {
    for session_id in state.store.session_ids_with_pages_and_worker_role()? {
        if let Err(err) = withdraw_pages(state, &session_id) {
            tracing::warn!(
                session = %session_id, %err, "withdrawing the pages of a worker session failed"
            );
        }
    }
    Ok(())
}

/// Tells the panel which of a session's pages the store removed; nothing when none was.
pub(crate) fn announce_deleted(state: &Arc<AppState>, session_id: &str, deleted: &[String]) {
    if !deleted.is_empty() {
        state.broadcast(Event::PagesDeleted {
            session_id: session_id.to_string(),
            pages: deleted.to_vec(),
        });
    }
}

/// A report panel form submission. Refused unless `page_id` names the newest page of the session
/// that pushed it, a project session's included — history pages are read-only, and this is where
/// that is actually enforced; a panel that disables its own submit button on a history page is
/// only reflecting the rule, not the source of it. Refused too when the session that pushed it
/// has the role of a worker and so has no tool to read the submission through; a bound coordinator,
/// though it reports to an owner too, keeps its pages.
///
/// A page takes one submission. It is marked submitted, in the store and in every window
/// (`page_submission`), before the message is delivered, so that of several submissions racing for
/// the page exactly one is delivered and the others are refused with `page_already_submitted`. A
/// delivery that certainly failed takes the mark off again, so the form can be submitted again; one
/// whose acceptance is unconfirmed, or that may have been written, keeps it, as the message may
/// already be there.
pub(super) async fn submit_page(
    state: &Arc<AppState>,
    page_id: &str,
    data: serde_json::Value,
) -> Result<()> {
    let page = state.store.get_page(page_id)?.ok_or_else(|| {
        CodedError::raised(
            error_code::UNKNOWN_PAGE,
            format!("unknown page {page_id}"),
            &[("page", page_id)],
        )
    })?;
    let newest = state.store.newest_page_id(&page.session_id)?;
    if newest.as_deref() != Some(page.id.as_str()) {
        return Err(not_current());
    }
    // A worker has no tool to read what the page sent; its pages are withdrawn when it
    // becomes one, so this holds for one left behind by a failed withdrawal, until the daemon's
    // next start sweeps it.
    let pusher = state.store.get_session(&page.session_id)?;
    if pusher.is_some_and(|pusher| pusher.is_worker()) {
        return Err(CodedError::raised(
            error_code::UNKNOWN_PAGE,
            format!(
                "unknown page {page_id}: its session has the role of a worker, which \
                 pushes no pages"
            ),
            &[("page", page_id)],
        ));
    }
    let submission = page.content.check_submission(&data).map_err(|reason| {
        CodedError::raised(
            error_code::UNREADABLE_REQUEST,
            format!("the submission does not fit the page: {reason}"),
            &[],
        )
    })?;
    let revision = match state.store.mark_page_submitted(&page.id, &submission)? {
        PageMark::Marked(revision) => revision,
        PageMark::Unknown => {
            return Err(CodedError::raised(
                error_code::UNKNOWN_PAGE,
                format!("unknown page {page_id}"),
                &[("page", page_id)],
            ))
        }
        PageMark::NotCurrent => return Err(not_current()),
        PageMark::AlreadySubmitted => {
            return Err(CodedError::raised(
                error_code::PAGE_ALREADY_SUBMITTED,
                "this page's form was already submitted",
                &[],
            ))
        }
    };
    state.broadcast(Event::PageSubmission {
        session_id: page.session_id.clone(),
        page: page.id.clone(),
        submission: Some(submission.clone()),
        submission_revision: revision,
    });
    let message = texts::render_page_submission(&page.id, &submission);
    // The write blocks on the PTY. Queued rather than refused: the user is the one submitting, so
    // there is nobody to tell to answer a prompt first, and the submission must not be dropped.
    let delivered = delivery::write_message_off_runtime(
        state,
        &page.session_id,
        message,
        delivery::WhenBlocked::Queue,
    )
    .await;
    let Err(err) = delivered else {
        return Ok(());
    };
    if !may_have_written(&err) {
        release_submission(state, &page.session_id, &page.id, revision);
    }
    Err(err)
}

fn not_current() -> anyhow::Error {
    CodedError::raised(
        error_code::PAGE_NOT_CURRENT,
        "this page is no longer current; its form can no longer be submitted",
        &[],
    )
}

/// Whether the message may have reached the session though the write failed: its agent's own queue
/// did not confirm taking it, or the blocking task that wrote it did not come back, and may have
/// written it before it did not.
fn may_have_written(err: &anyhow::Error) -> bool {
    delivery::acceptance_uncertain(err) || err.downcast_ref::<tokio::task::JoinError>().is_some()
}

/// Takes the mark set at `revision` off a page whose submission certainly was not delivered, and
/// tells the windows, so the form can be submitted again. Only that mark: the page may have been
/// deleted, or its submission changed, since. The announcement carries the revision the store
/// gave the clearing, so a window that hears of a later mark first keeps that one. A store that
/// cannot clear it leaves the page marked, which refuses a retry rather than risking a second
/// delivery.
fn release_submission(state: &Arc<AppState>, session_id: &str, page_id: &str, revision: i64) {
    match state.store.clear_page_submission(page_id, revision) {
        Ok(Some(cleared)) => state.broadcast(Event::PageSubmission {
            session_id: session_id.to_string(),
            page: page_id.to_string(),
            submission: None,
            submission_revision: cleared,
        }),
        Ok(None) => {}
        Err(err) => {
            tracing::warn!(page = %page_id, %err, "taking the submission mark off a page failed")
        }
    }
}

/// Answers a `list_pages` request with the pages `session` has pushed. A project session can have
/// pages too; only a session that does not exist is unknown.
pub(super) fn list_pages(
    state: &Arc<AppState>,
    request_id: Option<String>,
    session: String,
) -> Result<Option<Event>> {
    state.session_record(&session)?;
    let pages = state.store.list_pages(&session)?;
    Ok(Some(Event::PageList {
        id: request_id,
        session_id: session,
        pages,
    }))
}

#[cfg(test)]
mod tests;
