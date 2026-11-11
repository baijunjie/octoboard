use super::*;
use crate::coordinator::handle;
use crate::coordinator::test_support::{console, console_workdir_in};
use crate::protocol::{error_code, AgentId, Event, RequestBody, Role, SessionKind};
use crate::test_support::code_of;
use crate::test_support::{bare_session, idle_stand_in, output_containing};
use std::collections::HashMap;

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
        form_page("a-old", "owner-a", 1),
        form_page("a-new", "owner-a", 3),
        form_page("b-new", "owner-b", 4),
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
        .insert_page(&form_page("bw-1", "bound-worker", 6))
        .unwrap();
    let refused = submit_page(&state, "bw-1", serde_json::json!({}))
        .await
        .expect_err("a page of a session that has no page tools");
    assert_eq!(code_of(&refused), error_code::UNKNOWN_PAGE);

    submit_page(&state, "a-new", serde_json::json!({}))
        .await
        .expect("owner-a's newest page, though owner-b has a later one");

    assert!(output_containing(&live["owner-a"], "a-new").contains("a-new"));
    // Read once, after owner-a's delivery has landed. A mistaken write to owner-b would most
    // likely have arrived by now; this cannot rule out one that is still on its way.
    assert!(!live["owner-b"]
        .recent_output(8 * 1024)
        .windows(5)
        .any(|window| window == b"a-new"));
}

/// A form with a name, a choice and a button that sends it.
fn form_page(id: &str, session: &str, created_at: i64) -> crate::protocol::Page {
    let content = crate::protocol::PageContent::parse(&serde_json::json!({
        "body": [
            { "type": "text", "name": "who", "label": "Who" },
            { "type": "checkboxes", "name": "topics", "label": "Topics",
              "options": [{ "value": "rust" }, { "value": "sql" }] },
            { "type": "toggle", "name": "more", "label": "More" },
            { "type": "choices", "name": "action", "options": [{ "value": "ok" }] },
        ],
    }))
    .expect("a valid form");
    crate::protocol::Page {
        content,
        ..crate::test_support::test_page(id, session, created_at)
    }
}

fn answer() -> serde_json::Value {
    serde_json::json!({ "who": "Ada", "topics": ["rust", "sql"], "more": true, "action": "ok" })
}

/// A console session `owner` of agent `agent` in a state where its page can be submitted, running
/// as a stand-in when `running`.
fn submitting_state(
    name: &str,
    agent: AgentId,
    running: bool,
) -> (
    std::sync::Arc<crate::state::AppState>,
    crate::test_support::ScratchDir,
    crate::test_support::StandIn,
) {
    let (state, dir) = crate::test_support::app_state(name);
    let workdir = console_workdir_in(&dir);
    state
        .store
        .insert_console(&console(AgentId::CLAUDE, &workdir))
        .unwrap();
    let mut session = bare_session("owner", SessionKind::Console);
    session.agent = agent;
    state.store.insert_session(&session).unwrap();
    let live = crate::test_support::idle_stand_in_of("owner", agent);
    if running {
        state.register_live(live.clone());
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
    }
    state
        .store
        .insert_page(&form_page("form", "owner", 1))
        .unwrap();
    (state, dir, live)
}

/// The `page_submission` events received so far, each the submission and its revision.
fn submission_events(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
) -> Vec<(Option<serde_json::Value>, i64)> {
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::PageSubmission {
            session_id,
            page,
            submission,
            submission_revision,
        } = event
        {
            assert_eq!((session_id.as_str(), page.as_str()), ("owner", "form"));
            seen.push((
                submission.map(serde_json::Value::Object),
                submission_revision,
            ));
        }
    }
    seen
}

/// A form is submitted once: the submission is stored with the page and announced, the session
/// gets one message with the fields in order and an array joined, and a second submission, with
/// the same data or other, is refused as already submitted and delivers nothing.
#[tokio::test]
async fn a_page_takes_one_submission() {
    let (state, _dir, live) = submitting_state("submit-once", AgentId::CLAUDE, true);
    let mut events = state.subscribe();

    submit_page(&state, "form", answer()).await.unwrap();

    let stored = state.store.get_page("form").unwrap().unwrap();
    assert_eq!(
        stored.submission.map(serde_json::Value::Object),
        Some(answer())
    );
    assert_eq!(submission_events(&mut events), [(Some(answer()), 1)]);
    let heard = output_containing(&live, "action: ok");
    assert!(heard.contains("page form"), "{heard}");
    assert!(
        heard.contains("who: Ada")
            && heard.contains("topics: rust, sql")
            && heard.contains("more: true"),
        "{heard}"
    );
    let writes = live.input_writes();

    for data in [answer(), serde_json::json!({ "who": "Grace" })] {
        let refused = submit_page(&state, "form", data)
            .await
            .expect_err("already submitted");
        assert_eq!(code_of(&refused), error_code::PAGE_ALREADY_SUBMITTED);
    }
    assert_eq!(live.input_writes(), writes);
    assert!(submission_events(&mut events).is_empty());
    assert_eq!(
        state
            .store
            .get_page("form")
            .unwrap()
            .unwrap()
            .submission
            .map(serde_json::Value::Object),
        Some(answer()),
        "the first submission stays"
    );
}

/// Of several submissions racing for one page exactly one is delivered, and every other is
/// refused. All of them are started before any is awaited, and held at a barrier so that they
/// enter the submission together.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_submissions_deliver_one_message() {
    const RACERS: usize = 4;
    let (state, _dir, live) = submitting_state("submit-race", AgentId::CLAUDE, true);
    let mut events = state.subscribe();
    let start = std::sync::Arc::new(tokio::sync::Barrier::new(RACERS));
    let racers: Vec<_> = (0..RACERS)
        .map(|n| {
            let (state, start) = (state.clone(), start.clone());
            tokio::spawn(async move {
                start.wait().await;
                submit_page(
                    &state,
                    "form",
                    serde_json::json!({ "who": format!("racer{n}") }),
                )
                .await
            })
        })
        .collect();
    let mut results = Vec::new();
    for racer in racers {
        results.push(racer.await.unwrap());
    }
    let (delivered, refused): (Vec<_>, Vec<_>) = results.iter().partition(|result| result.is_ok());
    assert_eq!(
        (delivered.len(), refused.len()),
        (1, RACERS - 1),
        "{results:?}"
    );
    for refusal in refused {
        assert_eq!(
            code_of(refusal.as_ref().unwrap_err()),
            error_code::PAGE_ALREADY_SUBMITTED
        );
    }
    let winner = state
        .store
        .get_page("form")
        .unwrap()
        .unwrap()
        .submission
        .unwrap()["who"]
        .as_str()
        .unwrap()
        .to_string();
    let heard = output_containing(&live, &format!("who: {winner}"));
    assert_eq!(heard.matches("form submission").count(), 1, "{heard}");
    assert_eq!(submission_events(&mut events).len(), 1);
}

/// A submission that certainly did not reach the session (it is not running) takes the mark off
/// again and tells the windows, and the form can then be submitted. Each change of the submission
/// is announced with a higher revision than the one before.
#[tokio::test]
async fn a_certain_delivery_failure_unlocks_the_form_for_a_retry() {
    let (state, _dir, live) = submitting_state("submit-retry", AgentId::CLAUDE, false);
    let mut events = state.subscribe();

    let refused = submit_page(&state, "form", answer())
        .await
        .expect_err("the session is not running");
    assert_eq!(code_of(&refused), error_code::SESSION_NOT_RUNNING);
    assert!(state
        .store
        .get_page("form")
        .unwrap()
        .unwrap()
        .submission
        .is_none());
    assert_eq!(
        submission_events(&mut events),
        [(Some(answer()), 1), (None, 2)]
    );

    state.register_live(live.clone());
    crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
    submit_page(&state, "form", answer())
        .await
        .expect("a retry");
    assert!(state
        .store
        .get_page("form")
        .unwrap()
        .unwrap()
        .submission
        .is_some());
    assert_eq!(submission_events(&mut events), [(Some(answer()), 3)]);
    output_containing(&live, "who: Ada");
}

/// A submission its agent's own queue may have taken keeps the mark, announces nothing more, and is
/// refused as submitted if tried again, since the message may already be there.
#[tokio::test]
async fn an_unconfirmed_acceptance_keeps_the_page_submitted() {
    let (state, _dir, live) = submitting_state("submit-uncertain", AgentId::CODEX, false);
    let (transport, _server) = crate::adapter::codex::transport::transport_losing_an_add(|text| {
        assert!(text.contains("who: Ada"), "{text}")
    });
    let live = live.with_transport(std::sync::Arc::new(transport));
    state.register_live(live.clone());
    let mut events = state.subscribe();

    let unconfirmed = submit_page(&state, "form", answer())
        .await
        .expect_err("the queue did not confirm");
    assert_eq!(
        code_of(&unconfirmed),
        error_code::MESSAGE_ACCEPTANCE_UNCONFIRMED
    );
    assert_eq!(
        state
            .store
            .get_page("form")
            .unwrap()
            .unwrap()
            .submission
            .map(serde_json::Value::Object),
        Some(answer())
    );
    assert_eq!(submission_events(&mut events), [(Some(answer()), 1)]);

    let again = submit_page(&state, "form", answer())
        .await
        .expect_err("already submitted");
    assert_eq!(code_of(&again), error_code::PAGE_ALREADY_SUBMITTED);
}

/// A page with no submitter has no form: a submission of it is refused, and nothing is stored,
/// announced or written.
#[tokio::test]
async fn a_page_without_a_form_takes_no_submission() {
    let (state, _dir, live) = submitting_state("submit-no-form", AgentId::CLAUDE, true);
    state
        .store
        .insert_page(&crate::test_support::test_page("report", "owner", 2))
        .unwrap();
    let mut events = state.subscribe();
    let refused = submit_page(&state, "report", serde_json::json!({}))
        .await
        .expect_err("a report has no form");
    assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
    assert!(refused.to_string().contains("no form"), "{refused}");
    assert!(state
        .store
        .get_page("report")
        .unwrap()
        .unwrap()
        .submission
        .is_none());
    assert!(events.try_recv().is_err());
    assert_eq!(live.input_writes(), 0);
}

/// Data that is not the page's fields, or not of the shapes a field takes, is refused before the
/// page is marked: nothing is stored, announced or written.
#[tokio::test]
async fn invalid_submission_data_is_refused_before_the_page_is_marked() {
    let (state, _dir, live) = submitting_state("submit-invalid", AgentId::CLAUDE, true);
    let mut events = state.subscribe();
    for data in [
        serde_json::json!({ "nope": "x" }),
        serde_json::json!({ "who": { "first": "Ada" } }),
        serde_json::json!({ "topics": [["rust"]] }),
        serde_json::json!(["Ada"]),
        serde_json::json!("Ada"),
    ] {
        let refused = submit_page(&state, "form", data.clone())
            .await
            .expect_err(&data.to_string());
        assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST, "{data}");
    }
    assert!(state
        .store
        .get_page("form")
        .unwrap()
        .unwrap()
        .submission
        .is_none());
    assert!(submission_events(&mut events).is_empty());
    assert_eq!(live.input_writes(), 0);
    submit_page(&state, "form", answer())
        .await
        .expect("a valid one still goes");
}
