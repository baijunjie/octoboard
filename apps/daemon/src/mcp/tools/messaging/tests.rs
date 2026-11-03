use crate::coordinator;
use crate::mcp::dispatch::call;
use crate::mcp::test_support::{arguments, peer_state, shared_console_state, with_status};
use crate::protocol::{error_code, AgentId, CodedError, SessionStatus};
use crate::session_tools::catalogue::qualified_tool_name;
use crate::test_support::{output_containing, running};
use serde_json::json;

/// Information for an interrupted session is kept in the store for its resume (which the
/// coordinator's tests follow); its owner's copy is sent or kept by the owner's own state;
/// archiving the session drops what was kept, and an archived session is refused outright.
#[tokio::test]
async fn information_for_an_interrupted_session_is_kept_for_its_resume() {
    let (state, _dir) = peer_state("peer-share-interrupted");
    let console_a = running(&state, "a");
    with_status(&state, "of-a", SessionStatus::Interrupted);
    let share = |target: &'static str| {
        let state = state.clone();
        async move {
            call(
                &state,
                "loose",
                "inform_session",
                &arguments(json!({ "session": target, "text": "the port moved" })),
            )
            .await
        }
    };

    let result = share("of-a").await.unwrap();
    assert_eq!(result["delivered"], false);
    let note = result["note"].as_str().unwrap();
    assert!(
        note.contains("interrupted") && note.contains("resumed"),
        "{note}"
    );
    assert!(note.contains("A copy was delivered to its owner"), "{note}");
    assert_eq!(state.store.kept_messages("of-a").unwrap().len(), 1);
    assert!(output_containing(&console_a, "the port moved").contains("Copy of information"));

    // `get_session` counts it among the messages waiting for the session.
    let described = call(
        &state,
        "a",
        "get_session",
        &arguments(json!({ "session": "of-a" })),
    )
    .await
    .unwrap();
    assert_eq!(described["queued_messages"], 1);

    // Archived while information is kept for it: nothing is kept for a later reopen.
    with_status(&state, "of-b", SessionStatus::Interrupted);
    share("of-b").await.unwrap();
    assert_eq!(state.store.kept_messages("of-b").unwrap().len(), 1);
    coordinator::lifecycle::archive_session(&state, "of-b").unwrap();
    assert!(state.store.kept_messages("of-b").unwrap().is_empty());

    let err = share("archived-of-a").await.expect_err("archived");
    assert!(err.to_string().contains("archived"), "{err}");
    assert!(state
        .store
        .kept_messages("archived-of-a")
        .unwrap()
        .is_empty());
}

/// The copy for an interrupted owner is kept like the information itself.
#[tokio::test]
async fn the_copy_for_an_interrupted_owner_is_kept() {
    let (state, _dir) = peer_state("peer-share-interrupted-owner");
    let _of_a = running(&state, "of-a");
    with_status(&state, "a", SessionStatus::Interrupted);
    let result = call(
        &state,
        "loose",
        "inform_session",
        &arguments(json!({ "session": "of-a", "text": "heads up" })),
    )
    .await
    .unwrap();
    assert_eq!(result["delivered"], true);
    assert!(result["note"]
        .as_str()
        .unwrap()
        .contains("A copy is kept for its owner, which is interrupted"));
    assert_eq!(state.store.kept_messages("a").unwrap().len(), 1);
}

/// A message for an interrupted session of the caller's is refused as before, and the refusal
/// points at `resume_session`, whose `text` delivers it on relaunch.
#[tokio::test]
async fn a_message_for_an_interrupted_session_points_at_resume_session() {
    let (state, _dir) = shared_console_state("send-to-interrupted");
    with_status(&state, "of-a", SessionStatus::Interrupted);
    let err = call(
        &state,
        "a",
        "instruct_session",
        &arguments(json!({ "session": "of-a", "text": "go on" })),
    )
    .await
    .expect_err("refused");
    assert_eq!(
        err.downcast_ref::<CodedError>().map(|err| err.code),
        Some(error_code::SESSION_NOT_RUNNING),
        "{err}"
    );
    let message = err.to_string();
    assert!(message.contains("interrupted"), "{message}");
    assert!(
        message.contains(&qualified_tool_name(AgentId::CLAUDE, "resume_session")),
        "{message}"
    );
}

/// The same pointer, said for an archived session.
#[tokio::test]
async fn a_message_for_an_archived_session_points_at_resume_session() {
    let (state, _dir) = shared_console_state("send-to-archived");
    with_status(&state, "of-a", SessionStatus::Archived);
    let err = call(
        &state,
        "a",
        "instruct_session",
        &arguments(json!({ "session": "of-a", "text": "go on" })),
    )
    .await
    .expect_err("refused");
    assert_eq!(
        err.downcast_ref::<CodedError>().map(|err| err.code),
        Some(error_code::SESSION_NOT_RUNNING),
        "{err}"
    );
    let message = err.to_string();
    assert!(message.contains("archived"), "{message}");
    assert!(
        message.contains(&qualified_tool_name(AgentId::CLAUDE, "resume_session")),
        "{message}"
    );
}

/// The tool the refusal points at is named as the caller's own agent shows it: Grok's prefix
/// differs from Claude Code's.
#[tokio::test]
async fn the_pointer_to_resume_session_is_named_as_the_callers_agent_shows_it() {
    let (state, _dir) = shared_console_state("send-to-archived-grok-caller");
    with_status(&state, "of-a", SessionStatus::Archived);
    let mut caller = state.store.get_session("a").unwrap().unwrap();
    caller.agent = AgentId::GROK;
    state.store.update_session(&caller).unwrap();
    let err = call(
        &state,
        "a",
        "instruct_session",
        &arguments(json!({ "session": "of-a", "text": "go on" })),
    )
    .await
    .expect_err("refused");
    let message = err.to_string();
    assert!(message.contains("`octoboard__resume_session`"), "{message}");
    assert!(!message.contains("mcp__octoboard__"), "{message}");
}

/// An `open_items` that is not a list of strings is refused rather than read as none.
#[tokio::test]
async fn a_report_refuses_open_items_that_are_not_strings() {
    let (state, _dir) = shared_console_state("report-open-items");
    let err = call(
        &state,
        "of-a",
        "report",
        &arguments(json!({ "summary": "done", "status": "needs_decision", "open_items": [1] })),
    )
    .await
    .expect_err("not strings");
    assert_eq!(err.to_string(), "`open_items` must be an array of strings");
}

/// `owner_items` is read like `open_items`: one that is not a list of strings is refused.
#[tokio::test]
async fn a_report_refuses_owner_items_that_are_not_strings() {
    let (state, _dir) = shared_console_state("report-owner-items");
    let err = call(
        &state,
        "of-a",
        "report",
        &arguments(json!({ "summary": "done", "status": "done", "owner_items": "test it" })),
    )
    .await
    .expect_err("not a list");
    assert_eq!(err.to_string(), "`owner_items` must be an array of strings");
}

/// Information reaches any running session but the caller: one of another project or console,
/// and a console session, which can share information itself too.
#[tokio::test]
async fn information_reaches_any_running_session_but_the_caller() {
    let (state, _dir) = peer_state("peer-share-scope");
    let console_a = running(&state, "a");
    let foreign = running(&state, "foreign");
    let loose = running(&state, "loose");
    for (caller, target, live) in [
        ("mine", "a", &console_a),
        ("mine", "foreign", &foreign),
        ("a", "loose", &loose),
    ] {
        let result = call(
            &state,
            caller,
            "inform_session",
            &arguments(json!({ "session": target, "text": "the port moved" })),
        )
        .await
        .unwrap();
        assert_eq!(result["delivered"], true, "{caller} to {target}");
        let seen = output_containing(live, "the port moved");
        assert!(
            seen.contains("not your owner"),
            "{caller} to {target}: {seen}"
        );
    }
    let err = call(
        &state,
        "mine",
        "inform_session",
        &arguments(json!({ "session": "mine", "text": "hello" })),
    )
    .await
    .expect_err("the caller itself");
    assert!(err.to_string().contains("your own session"), "{err}");
}

/// Information arrives framed as information from a session that is not the receiver's owner,
/// and the receiver's owner, a console session or a project session, is sent a copy; when the
/// sender is the owner it is told so instead, and nobody is copied.
#[tokio::test]
async fn information_is_framed_and_copied_to_the_receivers_owner() {
    let (state, _dir) = peer_state("peer-share-delivery");
    let share = |caller: &'static str, target: &'static str| {
        let state = state.clone();
        async move {
            call(
                &state,
                caller,
                "inform_session",
                &arguments(json!({ "session": target, "text": "the port moved" })),
            )
            .await
        }
    };
    let loose = running(&state, "loose");
    let of_a = running(&state, "of-a");
    let console_a = running(&state, "a");
    let starter = running(&state, "starter");
    let mine = running(&state, "mine");

    // Unbound receiver: framed, and there is no owner to copy.
    let result = share("starter", "loose").await.unwrap();
    assert_eq!(result["note"], "The message was delivered to this session.");
    let seen = output_containing(&loose, "the port moved");
    assert!(
        seen.contains("Information from session starter — Work starter"),
        "{seen}"
    );
    assert!(seen.contains("not your owner"), "{seen}");

    // Receiver owned by a console session: that session is sent the copy.
    let result = share("loose", "of-a").await.unwrap();
    assert!(result["note"]
        .as_str()
        .unwrap()
        .contains("A copy was delivered to its owner"));
    assert!(output_containing(&of_a, "the port moved").contains("not your owner"));
    let copy = output_containing(&console_a, "the port moved");
    assert!(
        copy.contains("Copy of information shared with your session of-a"),
        "{copy}"
    );
    assert!(copy.contains("Sent by session loose"), "{copy}");

    // Receiver owned by a project session: likewise.
    share("loose", "mine").await.unwrap();
    assert!(output_containing(&mine, "the port moved").contains("not your owner"));
    let copy = output_containing(&starter, "Copy of information");
    assert!(copy.contains("shared with your session mine"), "{copy}");

    // From the owner itself: no "not your owner", and a note with no copy in it.
    let result = share("starter", "mine").await.unwrap();
    assert_eq!(result["note"], "The message was delivered to this session.");
    let seen = output_containing(&mine, "is your owner");
    assert!(seen.contains("Information from session starter"), "{seen}");
}

/// A copy that cannot be delivered, because the owner is not running, does not fail the
/// information: the receiver has it, and the sender is told the owner has not.
#[tokio::test]
async fn an_owner_that_cannot_be_copied_does_not_fail_the_delivery() {
    let (state, _dir) = peer_state("peer-share-copy-fails");
    let of_b = running(&state, "of-b");
    let result = call(
        &state,
        "loose",
        "inform_session",
        &arguments(json!({ "session": "of-b", "text": "heads up" })),
    )
    .await
    .unwrap();
    assert_eq!(result["delivered"], true);
    assert!(result["note"]
        .as_str()
        .unwrap()
        .contains("could not be sent a copy"));
    assert!(output_containing(&of_b, "heads up").contains("heads up"));

    // A receiver that is not running is the one refusal.
    let err = call(
        &state,
        "loose",
        "inform_session",
        &arguments(json!({ "session": "of-a", "text": "heads up" })),
    )
    .await
    .expect_err("not running");
    assert!(err.to_string().contains("not running"), "{err}");
}

/// What the sender of a message is told became of it and of its owner's copy, word for word: the
/// sender acts on it, and must not send again what is queued or kept.
#[test]
fn the_sender_is_told_what_became_of_a_message_word_for_word() {
    use super::{delivery_note, owner_copy_note};
    use crate::messaging::delivery::Delivery;
    use crate::messaging::sharing::OwnerCopy;

    assert_eq!(
        delivery_note(Delivery::Written),
        "The message was delivered to this session."
    );
    assert_eq!(
        delivery_note(Delivery::Queued),
        "The message is queued: this session cannot take a message right now. It will be \
         delivered as soon as this session can take one, so do not send it again."
    );
    assert_eq!(
        delivery_note(Delivery::Unsent),
        "The message was written into the input of this session, but this session's agent did \
         not send it to the conversation (it is in the input box, or went to a view of its \
         background agents), so this session has not read it. The user has been told to check \
         this session; do not send it again."
    );
    assert_eq!(
        delivery_note(Delivery::Kept),
        "The message is kept for this session, which is interrupted with no process running: it \
         will be delivered once this session is resumed, so do not send it again."
    );

    assert_eq!(owner_copy_note(&OwnerCopy::NotNeeded), "");
    assert_eq!(
        owner_copy_note(&OwnerCopy::Sent(Delivery::Written)),
        " A copy was delivered to its owner."
    );
    assert_eq!(
        owner_copy_note(&OwnerCopy::Sent(Delivery::Queued)),
        " A copy is queued: its owner cannot take a message right now. It will be delivered as \
         soon as its owner can take one."
    );
    assert_eq!(
        owner_copy_note(&OwnerCopy::Sent(Delivery::Kept)),
        " A copy is kept for its owner, which is interrupted with no process running: it will be \
         delivered once its owner is resumed."
    );
    assert_eq!(
        owner_copy_note(&OwnerCopy::Sent(Delivery::Unsent)),
        " A copy was written into the input of its owner, but its owner's agent did not send it \
         to the conversation (it is in the input box, or went to a view of its background \
         agents), so its owner has not read it. The user has been told to check its owner."
    );
    assert_eq!(
        owner_copy_note(&OwnerCopy::Failed("it is archived".to_string())),
        " Its owner could not be sent a copy (it is archived); the information itself was not \
         affected."
    );
}
