//! The generic side of a session's prompts, with the test provider's reader of a made-up screen
//! (`crate::adapter::test_provider::TestPrompts`): what is published and when, the hand coming
//! down once a prompt leaves, and an answer pressed or refused.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::json;

use super::watch::Watch;
use super::*;
use crate::protocol::{
    error_code, AgentId, ChoiceEffect, CodedError, FreeText, PermissionChoice, PermissionPrompt,
    Question, QuestionOption, QuestionPrompt, SessionStatus,
};
use crate::session::LiveSession;
use crate::state::test_support::insert_record;
use crate::state::AppState;
use crate::test_support::{app_state, code_of, fake_live_session, param_of, StandIn, PATIENCE};

/// A test-provider session `s` at `status`, its process running `script`, and the call `ls` of the
/// tool `Bash` started in it.
fn session(
    name: &str,
    status: SessionStatus,
    script: &str,
) -> (Arc<AppState>, StandIn, crate::test_support::ScratchDir) {
    session_in(name, status, |_| script.to_string())
}

/// [`session`] with a script made once the session's scratch directory is known.
fn session_in(
    name: &str,
    status: SessionStatus,
    script: impl FnOnce(&std::path::Path) -> String,
) -> (Arc<AppState>, StandIn, crate::test_support::ScratchDir) {
    let (state, dir) = app_state(name);
    let script = script(&dir);
    let script = script.as_str();
    insert_record(&state, &dir, "s", status);
    let mut record = state.session_record("s").unwrap();
    record.agent = AgentId::TEST;
    state.save_session(&record).unwrap();
    let live = fake_live_session("s", AgentId::TEST, 80, 24, script);
    crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
    state.register_live(live.clone());
    live.interaction.call_started(CallSeen::new(
        Some("call-1".to_string()),
        "Bash",
        &json!({"command": "ls"}),
    ));
    (state, live, dir)
}

/// Draws `PROMPT Bash ls`, then shows the input box once a key comes in.
const PROMPT_THEN_INPUT: &str = "printf 'PROMPT Bash ls\\r\\n'; IFS= read -r -n 1 key; \
     printf '\\033[2J\\033[HINPUT> after %s\\r\\n' \"$key\"; sleep 30";

fn reader() -> &'static dyn InteractionReader {
    crate::adapter::provider(AgentId::TEST)
        .interactions()
        .expect("the test provider reads prompts")
}

/// Looks at the screen until `done` holds, for a stand-in that draws in its own time.
fn look_until(
    watch: &mut Watch,
    state: &Arc<AppState>,
    live: &Arc<LiveSession>,
    done: impl Fn() -> bool,
) {
    let deadline = std::time::Instant::now() + PATIENCE;
    while !done() {
        assert!(
            std::time::Instant::now() < deadline,
            "not seen in time: {:?} on {:?}, status {:?}",
            reader().read(&live.screen_text(), &live.interaction.calls()),
            live.screen_text(),
            state.shown_status(&state.session_record("s").unwrap()),
        );
        watch.look(state, live, reader());
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A prompt the provider matches to a call is published, on the session as a client receives it,
/// only while the session is shown waiting on the user.
#[test]
fn a_recognised_prompt_is_published_while_the_session_waits() {
    let (state, live, _dir) = session(
        "interaction-publish",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    let mut watch = Watch::default();
    look_until(&mut watch, &state, &live, || {
        live.interaction.interaction().is_some()
    });
    let published = live.interaction.interaction().unwrap();
    assert_eq!(published.call_id.as_deref(), Some("call-1"));
    let wire = state.wire_session(state.session_record("s").unwrap());
    assert_eq!(wire.interaction, Some(published.clone()));
    // Read again, the same prompt keeps its id.
    watch.look(&state, &live, reader());
    assert_eq!(live.interaction.interaction().unwrap().id, published.id);
    // Not waiting any more: withdrawn.
    state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    watch.look(&state, &live, reader());
    assert!(live.interaction.interaction().is_none());
    assert!(state
        .wire_session(state.session_record("s").unwrap())
        .interaction
        .is_none());
}

/// A prompt on the screen that no call of the turn explains is not published.
#[test]
fn a_prompt_matched_to_no_call_is_not_published() {
    let (state, live, _dir) = session(
        "interaction-unmatched",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    live.interaction.turn_started();
    wait_screen(&live, "PROMPT Bash ls");
    let mut watch = Watch::default();
    watch.look(&state, &live, reader());
    assert!(live.interaction.interaction().is_none());
}

/// Once a published prompt is seen gone, answered in the terminal here, it is withdrawn and the
/// session's hand comes down to working.
#[test]
fn the_hand_comes_down_once_a_published_prompt_leaves_the_screen() {
    let (state, live, _dir) = session(
        "interaction-left",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    let mut watch = Watch::default();
    look_until(&mut watch, &state, &live, || {
        live.interaction.interaction().is_some()
    });
    live.write_input(b"1").unwrap();
    look_until(&mut watch, &state, &live, || {
        live.interaction.interaction().is_none()
    });
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );
}

/// The answer is pressed with the provider's key, and acknowledged once the prompt is gone; the
/// hand is then down.
#[test]
fn an_answer_is_pressed_and_lowers_the_hand() {
    // An answer pauses the session's queue, whose release drains it on the runtime.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, _dir) = session(
        "interaction-press",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    let mut watch = Watch::default();
    look_until(&mut watch, &state, &live, || {
        live.interaction.interaction().is_some()
    });
    let id = live.interaction.interaction().unwrap().id;
    let fields = AnswerFields {
        choice: Some("1".to_string()),
        text: None,
        answers: None,
        dismiss: false,
    };
    press::answer(&state, &live, &id, fields).expect("pressed");
    assert!(live.screen_text().contains("INPUT> after 1"));
    assert!(live.interaction.interaction().is_none());
    assert_eq!(
        state.session_record("s").unwrap().status,
        SessionStatus::Working
    );
}

/// An answer naming an id that is not published, or given after someone typed into the terminal,
/// is refused as not waiting, and nothing is pressed.
#[test]
fn a_stale_answer_or_one_after_typing_is_not_waiting() {
    // An answer pauses the session's queue, whose release drains it on the runtime.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, _dir) = session(
        "interaction-stale",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    let mut watch = Watch::default();
    look_until(&mut watch, &state, &live, || {
        live.interaction.interaction().is_some()
    });
    let fields = || AnswerFields {
        choice: Some("1".to_string()),
        text: None,
        answers: None,
        dismiss: false,
    };
    let code = |result: anyhow::Result<()>| CodedError::of(&result.unwrap_err()).code;
    assert_eq!(
        code(press::answer(&state, &live, "another", fields())),
        error_code::INTERACTION_NOT_WAITING
    );
    let id = live.interaction.interaction().unwrap().id;
    // A key that is no answer, typed at the terminal.
    live.write_input(b"x").unwrap();
    assert_eq!(
        code(press::answer(&state, &live, &id, fields())),
        error_code::INTERACTION_NOT_WAITING
    );
}

/// A prompt the provider cannot answer as asked fails the press, which is told to every client as
/// a notice of the same code, and leaves the prompt up.
#[test]
fn a_press_the_provider_cannot_make_fails_and_is_told() {
    // An answer pauses the session's queue, whose release drains it on the runtime.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, _dir) = session(
        "interaction-stuck",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    let mut watch = Watch::default();
    look_until(&mut watch, &state, &live, || {
        live.interaction.interaction().is_some()
    });
    let id = live.interaction.interaction().unwrap().id;
    let mut events = state.subscribe();
    let fields = AnswerFields {
        choice: Some("2".to_string()),
        text: Some("try another way".to_string()),
        answers: None,
        dismiss: false,
    };
    let error = press::answer(&state, &live, &id, fields).unwrap_err();
    assert_eq!(
        CodedError::of(&error).code,
        error_code::INTERACTION_ANSWER_FAILED
    );
    let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(event, crate::protocol::Event::SessionNotice { code, .. }
            if code == crate::protocol::notice_code::INTERACTION_ANSWER_FAILED)
    });
    assert!(told);
    assert!(live.screen_text().contains("PROMPT Bash ls"));
}

fn permission() -> InteractionPrompt {
    InteractionPrompt::Permission(PermissionPrompt {
        tool: "Bash".to_string(),
        category: None,
        title: None,
        detail: None,
        input: json!({}),
        input_truncated: false,
        choices: vec![
            PermissionChoice {
                id: "yes".to_string(),
                label: "Yes".to_string(),
                effect: ChoiceEffect::Allow,
                text: None,
            },
            PermissionChoice {
                id: "no".to_string(),
                label: "No".to_string(),
                effect: ChoiceEffect::Deny,
                text: Some(crate::protocol::ChoiceText::Optional),
            },
        ],
    })
}

fn questions() -> InteractionPrompt {
    let question = |multi_select, free_text| Question {
        text: "Which?".to_string(),
        header: None,
        multi_select,
        options: vec![
            QuestionOption {
                label: "A".to_string(),
                description: None,
            },
            QuestionOption {
                label: "B".to_string(),
                description: None,
            },
        ],
        free_text,
    };
    InteractionPrompt::Question(QuestionPrompt {
        questions: vec![
            question(false, Some(FreeText::Instead)),
            question(true, Some(FreeText::With)),
            question(false, None),
        ],
        dismissible: false,
    })
}

fn fields(
    choice: Option<&str>,
    text: Option<&str>,
    answers: Option<Vec<QuestionAnswer>>,
    dismiss: bool,
) -> AnswerFields {
    AnswerFields {
        choice: choice.map(str::to_string),
        text: text.map(str::to_string),
        answers,
        dismiss,
    }
}

fn answer(selected: &[u32], text: Option<&str>) -> QuestionAnswer {
    QuestionAnswer {
        selected: selected.to_vec(),
        text: text.map(str::to_string),
    }
}

/// A permission prompt takes one of its choices, with text only where the choice takes it; blank
/// text is none.
#[test]
fn a_permission_answer_is_checked_against_its_choices() {
    let prompt = permission();
    assert_eq!(
        check_answer(&prompt, fields(Some("no"), Some(" not that "), None, false)),
        Ok(Answer::Choice {
            id: "no".to_string(),
            text: Some("not that".to_string())
        })
    );
    assert_eq!(
        check_answer(&prompt, fields(Some("yes"), Some("  "), None, false)),
        Ok(Answer::Choice {
            id: "yes".to_string(),
            text: None
        })
    );
    let misfit = |fields| check_answer(&prompt, fields).unwrap_err();
    assert_eq!(misfit(fields(None, None, None, false)).field, "choice");
    assert!(misfit(fields(None, None, None, false)).missing);
    assert_eq!(
        misfit(fields(Some("maybe"), None, None, false)).field,
        "choice"
    );
    assert_eq!(
        misfit(fields(Some("yes"), Some("why"), None, false)).field,
        "text"
    );
    assert_eq!(
        misfit(fields(Some("yes"), None, None, true)).field,
        "dismiss"
    );
}

/// Questions take one answer each, within what each question allows.
#[test]
fn question_answers_are_checked_against_their_questions() {
    let prompt = questions();
    let ok = vec![
        answer(&[], Some("my own")),
        answer(&[1, 0], Some("and more")),
        answer(&[1], None),
    ];
    let Ok(Answer::Questions(checked)) = check_answer(&prompt, fields(None, None, Some(ok), false))
    else {
        panic!("a fitting answer is taken");
    };
    assert_eq!(checked[1].selected, vec![0, 1]);
    let misfit = |answers: Vec<QuestionAnswer>| {
        check_answer(&prompt, fields(None, None, Some(answers), false)).unwrap_err()
    };
    let rest = || vec![answer(&[0], None), answer(&[0], None)];
    // Too few answers.
    assert_eq!(misfit(rest()).field, "answers");
    // Options and text where the text goes instead.
    assert!(!misfit([vec![answer(&[0], Some("both"))], rest()].concat()).missing);
    // Two options on a single-select question.
    assert!(!misfit([vec![answer(&[0, 1], None)], rest()].concat()).missing);
    // An option out of range.
    assert!(!misfit([vec![answer(&[2], None)], rest()].concat()).missing);
    // Text where none is taken.
    assert!(
        !misfit(vec![
            answer(&[0], None),
            answer(&[0], None),
            answer(&[0], Some("x"))
        ])
        .missing
    );
    // Nothing at all.
    assert!(misfit([vec![answer(&[], None)], rest()].concat()).missing);
    // Not dismissible.
    assert_eq!(
        check_answer(&prompt, fields(None, None, None, true))
            .unwrap_err()
            .field,
        "dismiss"
    );
}

/// Each string of a published prompt is cut to the limit at a character boundary, and a prompt
/// that would still be too large is not published.
#[test]
fn a_published_prompt_is_held_to_its_limits() {
    let long = "é".repeat(INTERACTION_TEXT_LIMIT);
    let mut prompt = permission();
    let InteractionPrompt::Permission(permission) = &mut prompt else {
        unreachable!()
    };
    permission.title = Some(long.clone());
    permission.input = json!({"content": long});
    let interaction = within_limits(PendingInteraction {
        id: "i".to_string(),
        call_id: None,
        prompt,
    })
    .expect("within the limit once cut");
    let InteractionPrompt::Permission(permission) = &interaction.prompt else {
        unreachable!()
    };
    assert!(permission.title.as_ref().unwrap().len() <= INTERACTION_TEXT_LIMIT);
    assert!(permission.input_truncated);
    let mut many = questions();
    let InteractionPrompt::Question(question) = &mut many else {
        unreachable!()
    };
    question.questions = (0..20).map(|_| question.questions[0].clone()).collect();
    for question in &mut question.questions {
        question.text = long.clone();
    }
    assert!(within_limits(PendingInteraction {
        id: "i".to_string(),
        call_id: None,
        prompt: many,
    })
    .is_none());
}

/// A stand-in that draws `first`, then on each key it is given logs the key to `keys`, clears the
/// screen and draws the next of `later`; it stays up after the last.
fn frames(keys: &std::path::Path, first: &str, later: &[&str]) -> String {
    let mut script = format!("printf '{first}\\r\\n'; ");
    for frame in later {
        script += &format!(
            "IFS= read -r -n 1 key; printf '%s' \"$key\" >> '{}'; \
             printf '\\033[2J\\033[H{frame}\\r\\n'; ",
            keys.display()
        );
    }
    script + "sleep 30"
}

/// The keys the stand-in of [`frames`] has read so far.
fn keys_read(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("keys")).unwrap_or_default()
}

/// A session at `status` whose stand-in is the [`frames`] of `first` and `later`.
fn framed(
    name: &str,
    status: SessionStatus,
    first: &str,
    later: &[&str],
) -> (Arc<AppState>, StandIn, crate::test_support::ScratchDir) {
    session_in(name, status, |dir| frames(&dir.join("keys"), first, later))
}

fn wait_screen(live: &LiveSession, text: &str) {
    assert!(
        crate::session::wait::wait_for(PATIENCE, || live.screen_text().contains(text)),
        "{text:?} not drawn: {:?}",
        live.screen_text()
    );
}

fn wait_keys(dir: &std::path::Path, count: usize) -> String {
    assert!(
        crate::session::wait::wait_for(PATIENCE, || keys_read(dir).len() >= count),
        "{count} keys not read: {:?}",
        keys_read(dir)
    );
    keys_read(dir)
}

/// Looks until the prompt is published, and returns it.
fn published(
    watch: &mut Watch,
    state: &Arc<AppState>,
    live: &Arc<LiveSession>,
) -> PendingInteraction {
    look_until(watch, state, live, || {
        live.interaction.interaction().is_some()
    });
    live.interaction.interaction().unwrap()
}

fn choice(id: &str) -> AnswerFields {
    fields(Some(id), None, None, false)
}

fn status_of(state: &AppState) -> SessionStatus {
    state.session_record("s").unwrap().status
}

fn shown_interaction(state: &AppState) -> Option<PendingInteraction> {
    state
        .wire_session(state.session_record("s").unwrap())
        .interaction
}

/// A prompt on the screen for a call the hooks reported later, though it reads as the same
/// prompt, is another prompt: it gets a new id, and a client is told.
#[test]
fn a_prompt_of_another_call_is_published_under_a_new_id() {
    let (state, live, _dir) = session(
        "interaction-other-call",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    let mut watch = Watch::default();
    let first = published(&mut watch, &state, &live);
    let mut events = state.subscribe();
    live.interaction.call_started(CallSeen::new(
        Some("call-2".to_string()),
        "Bash",
        &json!({"command": "ls"}),
    ));
    watch.look(&state, &live, reader());
    let second = live.interaction.interaction().unwrap();
    assert_ne!(second.id, first.id);
    assert_eq!(second.call_id.as_deref(), Some("call-2"));
    assert_eq!(shown_interaction(&state), Some(second.clone()));
    let told = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(event, crate::protocol::Event::SessionUpserted { session }
            if session.interaction.as_ref().map(|i| i.id.as_str()) == Some(second.id.as_str()))
    });
    assert!(told, "the new prompt was not published to the clients");
    // Read again, it is the same prompt.
    watch.look(&state, &live, reader());
    assert_eq!(live.interaction.interaction().unwrap().id, second.id);
}

/// A prompt the provider cannot publish as it stands (a text field open on it) keeps what was
/// published for as long as the session waits, and is withdrawn once it does not.
#[test]
fn a_prompt_in_the_middle_of_being_answered_keeps_what_is_published() {
    let (state, live, _dir) = framed(
        "interaction-up",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["EDITING"],
    );
    let mut watch = Watch::default();
    let first = published(&mut watch, &state, &live);
    live.write_input(b"1").unwrap();
    wait_screen(&live, "EDITING");
    let mut watch = Watch::default();
    for _ in 0..4 {
        assert!(watch.look(&state, &live, reader()));
        assert_eq!(live.interaction.interaction(), Some(first.clone()));
    }
    assert_eq!(shown_interaction(&state), Some(first));
    state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    watch.look(&state, &live, reader());
    assert!(live.interaction.interaction().is_none());
}

/// A screen that is neither a prompt nor the input box withdraws the prompt only when it is read
/// so twice running, and never while an answer is being pressed.
#[test]
fn an_unrecognised_screen_withdraws_the_prompt_on_the_second_reading_but_not_while_pressing() {
    let (state, live, _dir) = framed(
        "interaction-unknown",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["nothing recognisable"],
    );
    let mut watch = Watch::default();
    let first = published(&mut watch, &state, &live);
    live.write_input(b"1").unwrap();
    wait_screen(&live, "nothing recognisable");
    {
        let _pressing = live.interaction.claim_press().expect("no press is running");
        let mut watch = Watch::default();
        for _ in 0..4 {
            watch.look(&state, &live, reader());
            assert_eq!(live.interaction.interaction(), Some(first.clone()));
        }
    }
    let mut watch = Watch::default();
    assert!(watch.look(&state, &live, reader()));
    assert_eq!(
        live.interaction.interaction(),
        Some(first),
        "one reading is not enough"
    );
    watch.look(&state, &live, reader());
    assert!(live.interaction.interaction().is_none());
    assert!(shown_interaction(&state).is_none());
}

/// The input box seen once is not the prompt gone: only a second reading in a row withdraws it and
/// has the hand come down, whoever answered the prompt.
#[test]
fn the_prompt_is_gone_only_when_the_input_box_is_seen_twice_running() {
    let (state, live, _dir) = framed(
        "interaction-gone-twice",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["INPUT> "],
    );
    let mut watch = Watch::default();
    let first = published(&mut watch, &state, &live);
    live.write_input(b"1").unwrap();
    wait_screen(&live, "INPUT>");
    let mut watch = Watch::default();
    watch.look(&state, &live, reader());
    assert_eq!(live.interaction.interaction(), Some(first));
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);
    watch.look(&state, &live, reader());
    assert!(live.interaction.interaction().is_none());
    assert!(shown_interaction(&state).is_none());
    assert_eq!(status_of(&state), SessionStatus::Working);
    assert!(live.interaction.lowered());
}

/// A session whose agent has not drawn its prompt yet shows its input box: with nothing
/// published, that is no prompt leaving, and the hand stays up.
#[test]
fn an_input_box_with_no_prompt_published_lowers_no_hand() {
    let (state, live, _dir) = session(
        "interaction-box-only",
        SessionStatus::WaitingUser,
        "printf 'INPUT> \\r\\n'; sleep 30",
    );
    wait_screen(&live, "INPUT>");
    let mut watch = Watch::default();
    for _ in 0..5 {
        watch.look(&state, &live, reader());
    }
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);
    assert!(!live.interaction.lowered());
}

/// A process of the session that has been replaced is not watched by the watch of the one before:
/// that watch withdraws what it published and stops.
#[test]
fn the_watch_of_an_earlier_process_withdraws_and_stops() {
    let (state, live, _dir) = session(
        "interaction-replaced",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    let mut watch = Watch::default();
    published(&mut watch, &state, &live);
    let successor = crate::test_support::idle_stand_in_of("s", AgentId::TEST);
    state.register_live(successor.clone());
    assert!(!watch.look(&state, &live, reader()));
    assert!(live.interaction.interaction().is_none());
    assert!(successor.interaction.interaction().is_none());
    assert!(shown_interaction(&state).is_none());
}

/// A hook that puts a session at its prompt starts the watch of its screen; the watch ends once the
/// session no longer waits and nothing is left published.
#[tokio::test(start_paused = true)]
async fn the_watch_publishes_while_the_session_waits_and_ends_with_nothing_left() {
    let (state, live, _dir) = session(
        "interaction-ensure",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    wait_screen(&live, "PROMPT Bash ls");
    watch::ensure(&state, &live);
    assert!(live.interaction.watching.load(Ordering::Acquire));
    ticks(4).await;
    assert!(live.interaction.interaction().is_some());
    assert!(live.interaction.watching.load(Ordering::Acquire));
    state
        .apply_hook_status("s", SessionStatus::Working)
        .unwrap();
    ticks(4).await;
    assert!(live.interaction.interaction().is_none());
    assert!(!live.interaction.watching.load(Ordering::Acquire));
}

/// A session that is not waiting, with nothing published, is not watched for long.
#[tokio::test(start_paused = true)]
async fn the_watch_of_a_session_that_does_not_wait_ends_at_once() {
    let (state, live, _dir) = session(
        "interaction-ensure-idle",
        SessionStatus::Working,
        PROMPT_THEN_INPUT,
    );
    wait_screen(&live, "PROMPT Bash ls");
    watch::ensure(&state, &live);
    ticks(4).await;
    assert!(!live.interaction.watching.load(Ordering::Acquire));
    assert!(live.interaction.interaction().is_none());
}

/// Lets the watch's polls run on the paused clock.
async fn ticks(count: u32) {
    for _ in 0..count {
        tokio::time::sleep(watch::POLL).await;
        tokio::task::yield_now().await;
    }
}

/// A prompt is kept to the last [`CALLS_KEPT`] calls, the oldest going first, and a new turn
/// forgets them all.
#[test]
fn calls_are_kept_up_to_a_limit_and_a_turn_forgets_them() {
    let state = InteractionState::default();
    for index in 0..CALLS_KEPT + 3 {
        state.call_started(CallSeen::new(
            Some(format!("call-{index}")),
            "Bash",
            &json!({}),
        ));
    }
    let kept = state.calls();
    assert_eq!(kept.len(), CALLS_KEPT);
    assert_eq!(kept[0].id.as_deref(), Some("call-3"));
    assert_eq!(
        kept.last().unwrap().id.as_deref(),
        Some(format!("call-{}", CALLS_KEPT + 2).as_str())
    );
    state.turn_started();
    assert!(state.calls().is_empty());
}

/// Each string of a call's input is cut at the limit on a character boundary, with the cut said;
/// a string within the limit is left whole.
#[test]
fn a_calls_input_is_cut_at_a_character_boundary_and_says_so() {
    let at_the_limit = "a".repeat(INTERACTION_TEXT_LIMIT);
    let whole = CallSeen::new(None, "Write", &json!({"content": at_the_limit}));
    assert!(!whole.input_truncated);
    assert_eq!(whole.input["content"].as_str().unwrap().len(), 4096);
    // One byte then two-byte characters: the limit falls inside one.
    let long = format!("a{}", "é".repeat(INTERACTION_TEXT_LIMIT));
    let call = CallSeen::new(None, "Write", &json!({"content": long, "nested": [long]}));
    assert!(call.input_truncated);
    for cut in [&call.input["content"], &call.input["nested"][0]] {
        let cut = cut.as_str().unwrap();
        assert!(cut.len() <= INTERACTION_TEXT_LIMIT);
        assert!(cut.len() >= INTERACTION_TEXT_LIMIT - 3, "cut too short");
        assert!(long.starts_with(cut));
    }
}

/// An answer that does not fit the prompt is refused with the field at fault, and nothing is
/// pressed.
#[test]
fn an_answer_that_does_not_fit_is_refused_and_nothing_is_pressed() {
    let (state, live, dir) = framed(
        "interaction-misfit",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["PROMPT Bash ls"],
    );
    let mut watch = Watch::default();
    let id = published(&mut watch, &state, &live).id;
    let refused = |fields| press::answer(&state, &live, &id, fields).unwrap_err();
    let unknown = refused(choice("9"));
    assert_eq!(code_of(&unknown), error_code::UNREADABLE_REQUEST);
    assert_eq!(param_of(&unknown, "field"), "choice");
    let none = refused(fields(None, None, None, false));
    assert_eq!(code_of(&none), error_code::FIELD_REQUIRED);
    assert_eq!(param_of(&none, "field"), "choice");
    let questions = refused(fields(None, None, Some(vec![answer(&[0], None)]), false));
    assert_eq!(code_of(&questions), error_code::UNREADABLE_REQUEST);
    assert_eq!(param_of(&questions, "field"), "answers");
    // Nothing reached the agent before this key, which is the first it reads.
    live.write_input(b"z").unwrap();
    assert_eq!(wait_keys(&dir, 1), "z");
}

/// One answer is pressed at a time: a second is refused as not waiting and presses nothing; once
/// the first is over the prompt can be answered.
#[test]
fn only_one_answer_is_pressed_at_a_time() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, dir) = framed(
        "interaction-one-press",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["INPUT> "],
    );
    let mut watch = Watch::default();
    let id = published(&mut watch, &state, &live).id;
    {
        let _pressing = live.interaction.claim_press().expect("no press is running");
        let refused = press::answer(&state, &live, &id, choice("1")).unwrap_err();
        assert_eq!(code_of(&refused), error_code::INTERACTION_NOT_WAITING);
        assert!(keys_read(&dir).is_empty());
    }
    press::answer(&state, &live, &id, choice("1")).expect("pressed once the other is over");
    assert_eq!(keys_read(&dir), "1");
}

/// Something typed into the terminal since the prompt was published is the person answering it
/// there: the answer is refused as not waiting, and its key is not pressed after theirs.
#[test]
fn nothing_is_pressed_after_the_person_typed_into_the_terminal() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, dir) = framed(
        "interaction-typed",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["PROMPT Bash ls", "PROMPT Bash ls"],
    );
    let mut watch = Watch::default();
    let id = published(&mut watch, &state, &live).id;
    live.write_input(b"x").unwrap();
    assert_eq!(wait_keys(&dir, 1), "x");
    let refused = press::answer(&state, &live, &id, choice("1")).unwrap_err();
    assert_eq!(code_of(&refused), error_code::INTERACTION_NOT_WAITING);
    // Had the answer's key been written it would be read before this one.
    live.write_input(b"z").unwrap();
    assert_eq!(wait_keys(&dir, 2), "xz");
}

/// Restores the press's response timeout when a test that shortened it is over.
struct ShortResponse;

impl ShortResponse {
    fn of(timeout: std::time::Duration) -> Self {
        press::RESPONSE_OVERRIDE.with(|cell| cell.set(Some(timeout)));
        Self
    }
}

impl Drop for ShortResponse {
    fn drop(&mut self) {
        press::RESPONSE_OVERRIDE.with(|cell| cell.set(None));
    }
}

/// A key the agent does not respond to fails the press: the error and a notice of the same code,
/// about the session, are given, the prompt stays published and up for the terminal, and the hand
/// stays up.
#[test]
fn a_key_the_agent_does_not_respond_to_fails_the_answer_and_leaves_the_prompt() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let _short = ShortResponse::of(std::time::Duration::from_millis(600));
    let (state, live, dir) = framed(
        "interaction-no-response",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["PROMPT Bash ls"],
    );
    let mut watch = Watch::default();
    let first = published(&mut watch, &state, &live);
    let mut events = state.subscribe();
    let error = press::answer(&state, &live, &first.id, choice("1")).unwrap_err();
    assert_eq!(code_of(&error), error_code::INTERACTION_ANSWER_FAILED);
    assert_eq!(keys_read(&dir), "1", "the one key was pressed");
    let notice = std::iter::from_fn(|| events.try_recv().ok()).find_map(|event| match event {
        crate::protocol::Event::SessionNotice { session, code, .. } => Some((session, code)),
        _ => None,
    });
    assert_eq!(
        notice,
        Some((
            "s".to_string(),
            crate::protocol::notice_code::INTERACTION_ANSWER_FAILED
        ))
    );
    assert_eq!(live.interaction.interaction(), Some(first));
    assert_eq!(status_of(&state), SessionStatus::WaitingUser);
    assert!(live.screen_text().contains("PROMPT Bash ls"));
}

/// No reader takes a screen with neither a prompt nor its agent's input box for either: such a
/// screen is not recognised, so a view of another kind, or a frame half drawn, never lowers a hand
/// or publishes anything. And no reader starts a call from an event that carries none.
#[test]
fn no_reader_recognises_a_foreign_screen_or_starts_a_call_from_a_quiet_event() {
    let foreign = [
        "",
        "   \n\n   \n",
        "Loading...\n",
        "~/project $ ls\nCargo.toml  src\n~/project $ \n",
        "Press q to quit\n",
    ];
    for agent in crate::protocol::AgentId::ALL {
        let Some(reader) = crate::adapter::provider(agent).interactions() else {
            continue;
        };
        let calls = [CallSeen::new(None, "Bash", &json!({"command": "ls"}))];
        for screen in foreign {
            assert_eq!(
                reader.read(screen, &calls),
                Reading::Unknown,
                "{agent:?}: {screen:?}"
            );
        }
        for event in ["Stop", "UserPromptSubmit", "SessionStart", "PostToolUse"] {
            assert!(
                reader.call_started(event, &json!({})).is_none(),
                "{agent:?}: {event}"
            );
        }
    }
}

/// What a client is sent carries the prompt only while the session is shown waiting on the user,
/// whatever is still held for it.
#[test]
fn the_session_a_client_receives_carries_the_prompt_only_while_it_waits() {
    let (state, live, _dir) = session(
        "interaction-wire",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    let mut watch = Watch::default();
    let published = published(&mut watch, &state, &live);
    assert_eq!(shown_interaction(&state), Some(published.clone()));
    for status in [
        SessionStatus::Working,
        SessionStatus::Idle,
        SessionStatus::WaitingUser,
    ] {
        state.apply_hook_status("s", status).unwrap();
        assert_eq!(
            shown_interaction(&state),
            (status == SessionStatus::WaitingUser).then(|| published.clone()),
            "{status:?}"
        );
    }
}

/// A stand-in that draws `PROMPT Bash ls` and, on the first key, the input box, with a counter
/// running on its third row all the while (`animate_prompt` before the key, `animate_input` after
/// it), as an agent animates what it is doing. The counter stops after about half a minute.
fn animated(keys: &std::path::Path, animate_prompt: bool, animate_input: bool) -> String {
    let counter = "( for i in $(seq 1 1500); do printf '\\033[3;1Hframe %s ' \"$i\"; sleep 0.02; \
                   done ) &";
    let mut script = String::from("printf 'PROMPT Bash ls\\r\\n'; ");
    if animate_prompt {
        script += counter;
        script += " ";
    }
    script += &format!(
        "IFS= read -r -n 1 key; printf '%s' \"$key\" >> '{}'; \
         printf '\\033[2J\\033[HINPUT> \\r\\n'; ",
        keys.display()
    );
    if animate_input && !animate_prompt {
        script += counter;
        script += " ";
    }
    script + "sleep 30"
}

/// Presses the answer `1` to the stand-in of [`animated`], which animates before the key, after it,
/// or both.
fn press_while_animated(name: &str, animate_prompt: bool, animate_input: bool) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, dir) = session_in(name, SessionStatus::WaitingUser, |dir| {
        animated(&dir.join("keys"), animate_prompt, animate_input)
    });
    let mut watch = Watch::default();
    let id = published(&mut watch, &state, &live).id;
    press::answer(&state, &live, &id, choice("1")).expect("pressed");
    assert_eq!(keys_read(&dir), "1");
    assert!(live.interaction.interaction().is_none());
    assert_eq!(status_of(&state), SessionStatus::Working);
}

/// What settles a press is the prompt's reading, not the whole screen: rows that keep changing
/// beside a prompt that reads the same do not keep it from settling.
#[test]
fn a_press_settles_though_rows_beside_the_prompt_keep_changing() {
    press_while_animated("interaction-animated-prompt", true, false);
}

/// After the last key an agent that animates what it does, its input box back, is the prompt gone:
/// the answer is acknowledged, not failed as a screen that did not settle.
#[test]
fn an_answer_is_acknowledged_though_the_screen_animates_after_the_last_key() {
    press_while_animated("interaction-animated-input", false, true);
}

/// Once a key has been pressed, another call's prompt on the screen means the one answered is done
/// and the agent has gone on: the answer is acknowledged, and nothing is pressed on the new prompt.
#[test]
fn an_answer_is_acknowledged_once_the_agent_goes_on_to_another_calls_prompt() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, dir) = framed(
        "interaction-next-call",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["PROMPT Bash pwd", "PROMPT Bash pwd"],
    );
    let mut watch = Watch::default();
    let id = published(&mut watch, &state, &live).id;
    live.interaction.call_started(CallSeen::new(
        Some("call-2".to_string()),
        "Bash",
        &json!({"command": "pwd"}),
    ));
    press::answer(&state, &live, &id, choice("1")).expect("acknowledged");
    assert_eq!(keys_read(&dir), "1", "only the answered prompt's key");
    assert!(live.interaction.interaction().is_none());
    // Had a key been pressed on the new prompt it would be read before this one.
    live.write_input(b"z").unwrap();
    assert_eq!(wait_keys(&dir, 2), "1z");
}

/// Before any key, another call's prompt on the screen means the one answered is not up: the answer
/// is refused as not waiting and nothing is pressed.
#[test]
fn an_answer_is_not_waiting_when_another_calls_prompt_is_up_before_any_key() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, dir) = framed(
        "interaction-other-call-first",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["PROMPT Bash ls"],
    );
    let mut watch = Watch::default();
    let id = published(&mut watch, &state, &live).id;
    // The same prompt now belongs to a later call.
    live.interaction.call_started(CallSeen::new(
        Some("call-2".to_string()),
        "Bash",
        &json!({"command": "ls"}),
    ));
    let refused = press::answer(&state, &live, &id, choice("1")).unwrap_err();
    assert_eq!(code_of(&refused), error_code::INTERACTION_NOT_WAITING);
    assert!(keys_read(&dir).is_empty());
    live.write_input(b"z").unwrap();
    assert_eq!(wait_keys(&dir, 1), "z");
}

/// A prompt that someone typed into the terminal over and that still reads the same is published
/// again under a new id, with the writes made so far, and the answer that names the new id is
/// pressed; the old id is refused.
#[test]
fn a_prompt_typed_over_and_still_up_is_published_again_under_a_new_id() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
    let (state, live, dir) = framed(
        "interaction-typed-republish",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["PROMPT Bash ls", "INPUT> "],
    );
    let mut watch = Watch::default();
    let first = published(&mut watch, &state, &live);
    live.write_input(b"x").unwrap();
    assert_eq!(wait_keys(&dir, 1), "x");
    look_until(&mut watch, &state, &live, || {
        live.interaction
            .interaction()
            .is_some_and(|now| now.id != first.id)
    });
    let second = live.interaction.published().unwrap();
    assert_eq!(second.interaction.prompt, first.prompt);
    assert_eq!(second.interaction.call_id, first.call_id);
    assert_eq!(second.writes, live.input_writes());
    assert_eq!(shown_interaction(&state), Some(second.interaction.clone()));
    // Read again, it is the same prompt.
    watch.look(&state, &live, reader());
    assert_eq!(
        live.interaction.interaction(),
        Some(second.interaction.clone())
    );
    let refused = press::answer(&state, &live, &first.id, choice("1")).unwrap_err();
    assert_eq!(code_of(&refused), error_code::INTERACTION_NOT_WAITING);
    assert_eq!(keys_read(&dir), "x");
    press::answer(&state, &live, &second.interaction.id, choice("1")).expect("pressed");
    assert_eq!(keys_read(&dir), "x1");
}

/// A prompt is not published again for typing while an answer is being pressed: the press's own keys
/// move the writes.
#[test]
fn a_prompt_is_not_published_again_while_an_answer_is_pressed() {
    let (state, live, dir) = framed(
        "interaction-typed-pressing",
        SessionStatus::WaitingUser,
        "PROMPT Bash ls",
        &["PROMPT Bash ls"],
    );
    let mut watch = Watch::default();
    let first = published(&mut watch, &state, &live);
    live.write_input(b"x").unwrap();
    assert_eq!(wait_keys(&dir, 1), "x");
    {
        let _pressing = live.interaction.claim_press().expect("no press is running");
        for _ in 0..4 {
            assert!(watch.look(&state, &live, reader()));
            assert_eq!(live.interaction.interaction(), Some(first.clone()));
        }
    }
    look_until(&mut watch, &state, &live, || {
        live.interaction
            .interaction()
            .is_some_and(|now| now.id != first.id)
    });
}

/// A session that waits with no call of the turn known publishes nothing and is no longer watched,
/// unless a press runs; a call reported later starts the watch again.
#[test]
fn the_watch_stops_when_the_session_waits_with_no_call_known() {
    let (state, live, _dir) = session(
        "interaction-no-calls",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    live.interaction.turn_started();
    wait_screen(&live, "PROMPT Bash ls");
    let mut watch = Watch::default();
    assert!(!watch.look(&state, &live, reader()));
    assert!(live.interaction.interaction().is_none());
    {
        let _pressing = live.interaction.claim_press().expect("no press is running");
        assert!(watch.look(&state, &live, reader()), "a press is watched");
    }
    assert!(!watch.look(&state, &live, reader()));
    live.interaction.call_started(CallSeen::new(
        Some("call-1".to_string()),
        "Bash",
        &json!({"command": "ls"}),
    ));
    assert!(watch.look(&state, &live, reader()));
    assert!(live.interaction.interaction().is_some());
}

/// The running watch ends at once for a session that waits with no call known, and the next hook
/// that reports a call and raises the hand starts it again.
#[tokio::test(start_paused = true)]
async fn the_running_watch_ends_for_a_wait_with_no_call_known() {
    let (state, live, _dir) = session(
        "interaction-ensure-no-calls",
        SessionStatus::WaitingUser,
        PROMPT_THEN_INPUT,
    );
    live.interaction.turn_started();
    wait_screen(&live, "PROMPT Bash ls");
    watch::ensure(&state, &live);
    ticks(4).await;
    assert!(!live.interaction.watching.load(Ordering::Acquire));
    assert!(live.interaction.interaction().is_none());
    live.interaction.call_started(CallSeen::new(
        Some("call-1".to_string()),
        "Bash",
        &json!({"command": "ls"}),
    ));
    watch::ensure(&state, &live);
    ticks(4).await;
    assert!(live.interaction.interaction().is_some());
    assert!(live.interaction.watching.load(Ordering::Acquire));
}

/// A question prompt takes its text in its answers: `text` at the top level is refused with that
/// field named, and so is `choice`.
#[test]
fn a_question_refuses_a_top_level_text_or_choice() {
    let prompt = questions();
    let refused = |fields| check_answer(&prompt, fields).unwrap_err();
    let ok = || {
        vec![
            answer(&[], Some("mine")),
            answer(&[0], None),
            answer(&[0], None),
        ]
    };
    for with_answers in [None, Some(ok())] {
        let text = refused(fields(None, Some("typed"), with_answers.clone(), false));
        assert_eq!(text.field, "text");
        assert!(!text.missing);
        let picked = refused(fields(Some("1"), None, with_answers, false));
        assert_eq!(picked.field, "choice");
        assert!(!picked.missing);
    }
}
