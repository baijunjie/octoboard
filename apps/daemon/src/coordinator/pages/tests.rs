use super::*;
use crate::coordinator::handle;
use crate::coordinator::test_support::{console, console_workdir_in};
use crate::protocol::{error_code, AgentId, Event, RequestBody, Role, SessionKind};
use crate::test_support::code_of;
use crate::test_support::{bare_session, idle_stand_in};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Two console sessions in one console each keep their own page history: a submission is
/// written into the console session that pushed the page and no other, and "the newest page"
/// is the newest of that console session, so a later page from the other one does not make an
/// earlier page read-only.
#[tokio::test]
async fn a_page_submission_reaches_the_console_session_that_pushed_it() {
    let (state, dir) = crate::test_support::app_state("coordinator-submit-page-routes-by-owner");
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(AgentId::CLAUDE, &workdir))
        .unwrap();
    let mut live = HashMap::new();
    for id in ["owner-a", "owner-b"] {
        state
            .store
            .insert_session(&bare_session(id, SessionKind::Console))
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
        .insert_session(&bare_session("worker", SessionKind::Project))
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
    let mut bound = bare_session("bound-worker", SessionKind::Project);
    bound.bound_to = Some("owner-a".to_string());
    bound.role = Role::Worker;
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
