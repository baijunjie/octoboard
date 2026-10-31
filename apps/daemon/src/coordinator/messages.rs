//! The user's own writes into a session: a message, and the submission of a page's form.

use std::sync::Arc;

use anyhow::Result;

use crate::messaging::delivery;
use crate::messaging::texts;
use crate::protocol::{error_code, CodedError};
use crate::state::AppState;

/// The user's own write into a session. Refused rather than queued while the session is waiting for
/// them: they are the one who has to answer the prompt that is blocking it, and they can be told so.
/// Off the runtime, because the write blocks on the PTY and on the wait for the agent to send it.
pub(super) async fn send_message(state: &Arc<AppState>, id: &str, text: String) -> Result<()> {
    delivery::write_message_off_runtime(state, id, text, delivery::WhenBlocked::Refuse).await?;
    Ok(())
}

/// A report panel form submission. Refused unless `page_id` names its console session's newest page —
/// history pages are read-only, and this is where that is actually enforced; a panel that disables
/// its own submit button on a history page is only reflecting the rule, not the source of it.
/// Refused too when the session that pushed it has the role of a worker and so has no tool
/// to read the submission through; a lead session, though it reports to an owner too, keeps its
/// pages.
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
        return Err(CodedError::raised(
            error_code::PAGE_NOT_CURRENT,
            "this page is no longer current; its form can no longer be submitted",
            &[],
        ));
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
    let session_id = page.session_id;

    let message = texts::render_page_submission(&page.id, &data);
    let owned_state = state.clone();
    // The write blocks on the PTY. Queued rather than refused: the user is the one submitting, so
    // there is nobody to tell to answer a prompt first, and the submission must not be dropped.
    tokio::task::spawn_blocking(move || {
        delivery::write_message(
            &owned_state,
            &session_id,
            &message,
            delivery::WhenBlocked::Queue,
        )
    })
    .await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::handle;
    use crate::coordinator::test_support::{code_of, console, console_workdir};
    use crate::protocol::{error_code, Agent, Event, OrchestrationRole, RequestBody, Role};
    use crate::test_support::{bare_session, idle_stand_in};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    /// Two console sessions in one console each keep their own page history: a submission is
    /// written into the console session that pushed the page and no other, and "the newest page"
    /// is the newest of that console session, so a later page from the other one does not make an
    /// earlier page read-only.
    #[tokio::test]
    async fn a_page_submission_reaches_the_console_session_that_pushed_it() {
        let (state, dir) =
            crate::test_support::app_state("coordinator-submit-page-routes-by-owner");
        let workdir = console_workdir(&dir);
        state
            .store
            .insert_console(&console(Agent::Claude, &workdir))
            .unwrap();
        let mut live = HashMap::new();
        for id in ["owner-a", "owner-b"] {
            state
                .store
                .insert_session(&bare_session(id, Role::Console))
                .unwrap();
            let session = idle_stand_in(id);
            state.register_live(session.clone());
            crate::session::spawn_reader_thread(session.clone(), 8 * 1024);
            live.insert(id, session);
        }
        for page in [
            crate::test_support::test_page("a-old", "owner-a", 1),
            crate::test_support::test_page("a-new", "owner-a", 3),
            crate::test_support::test_page("b-new", "owner-b", 4),
        ] {
            state.store.insert_page(&page).unwrap();
        }

        // A project session that faces the user can have pages too; only a session that does not
        // exist is refused.
        state
            .store
            .insert_session(&bare_session("worker", Role::Project))
            .unwrap();
        state
            .store
            .insert_page(&crate::test_support::test_page("w-1", "worker", 5))
            .unwrap();
        let listing = |session: &str| {
            handle(
                &state,
                None,
                RequestBody::ListPages {
                    session: session.to_string(),
                },
            )
        };
        assert!(matches!(
            listing("owner-a").await.unwrap(),
            Some(Event::PageList { ref pages, .. }) if pages.len() == 2
        ));
        assert!(matches!(
            listing("worker").await.unwrap(),
            Some(Event::PageList { ref pages, .. }) if pages.len() == 1
        ));
        let refused = listing("nobody").await.expect_err("no such session");
        assert_eq!(code_of(&refused), error_code::UNKNOWN_SESSION);

        let refused = submit_page(&state, "a-old", serde_json::json!({}))
            .await
            .expect_err("a history page of its console session");
        assert_eq!(code_of(&refused), error_code::PAGE_NOT_CURRENT);

        // The newest page of a session that has since become a bound one is refused as no page at
        // all.
        let mut bound = bare_session("bound-worker", Role::Project);
        bound.bound_to = Some("owner-a".to_string());
        bound.orchestration_role = OrchestrationRole::Worker;
        state.store.insert_session(&bound).unwrap();
        state
            .store
            .insert_page(&crate::test_support::test_page("bw-1", "bound-worker", 6))
            .unwrap();
        let refused = submit_page(&state, "bw-1", serde_json::json!({}))
            .await
            .expect_err("a page of a session that has no page tools");
        assert_eq!(code_of(&refused), error_code::UNKNOWN_PAGE);

        submit_page(&state, "a-new", serde_json::json!({}))
            .await
            .expect("owner-a's newest page, though owner-b has a later one");

        let output_of = |id: &str| {
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                let output = live[id].recent_output(8 * 1024);
                if !output.is_empty() || Instant::now() >= deadline {
                    return String::from_utf8_lossy(&output).into_owned();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        assert!(output_of("owner-a").contains("a-new"));
        // Read once, after owner-a's delivery has landed. A mistaken write to owner-b would most
        // likely have arrived by now; this cannot rule out one that is still on its way.
        assert!(!live["owner-b"]
            .recent_output(8 * 1024)
            .windows(5)
            .any(|window| window == b"a-new"));
    }
}
