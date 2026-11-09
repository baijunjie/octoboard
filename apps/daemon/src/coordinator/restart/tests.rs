use super::*;
use crate::protocol::{AgentId, Event, SessionKind};
use crate::test_support::{app_state, bare_session, ScratchDir};

fn fixture() -> (Arc<AppState>, ScratchDir, Session) {
    let (state, dir) = app_state("restore-launch-overrides");
    state
        .store
        .insert_console(&crate::coordinator::test_support::console(
            AgentId::CODEX,
            &dir,
        ))
        .unwrap();
    let mut before = bare_session("s", SessionKind::Console);
    before.agent = AgentId::CODEX;
    before.model = Some(None);
    before
        .compatibility
        .insert("claude_compatibility".into(), false);
    state.store.insert_session(&before).unwrap();
    (state, dir, before)
}

#[test]
fn restoring_launch_overrides_writes_and_publishes_the_complete_values_once() {
    let (state, dir, before) = fixture();
    state
        .store
        .set_session_launch(
            "s",
            &Some(Some("changed".into())),
            &Some(None),
            &[("claude_compatibility".into(), true)].into(),
        )
        .unwrap();
    let audit = rusqlite::Connection::open(dir.join("octoboard.db")).unwrap();
    audit
        .execute_batch(
            "CREATE TABLE launch_writes (id TEXT);
         CREATE TRIGGER launch_writes AFTER UPDATE ON sessions
         BEGIN INSERT INTO launch_writes VALUES (NEW.id); END;",
        )
        .unwrap();
    let mut events = state.subscribe();
    put_back_launch_overrides(&state, &before);
    let count: i64 = audit
        .query_row("SELECT COUNT(*) FROM launch_writes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
        panic!("expected session_upserted");
    };
    assert_eq!(session.model, before.model);
    assert_eq!(session.effort, before.effort);
    assert_eq!(session.compatibility, before.compatibility);
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[test]
fn unchanged_launch_overrides_need_no_rollback_publication() {
    let (state, _dir, before) = fixture();
    let mut events = state.subscribe();
    put_back_launch_overrides(&state, &before);
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}
