//! OpenCode's interaction reader against real screens and plugin payloads of OpenCode 1.19.6
//! (`testdata/opencode-captures.md`, "Prompts").

use std::sync::LazyLock;

use serde_json::{json, Value};

use super::*;
use crate::terminal::snapshot::TerminalSnapshot;

const PAYLOADS: &str =
    include_str!("../../../../testdata/opencode_hook_payloads_prompts_1.19.6.jsonl");
const WRAPPED_PAYLOADS: &str =
    include_str!("../../../../testdata/opencode_hook_payloads_wrapped_1.19.6.jsonl");

/// What the terminal showed once the first `upto` bytes of `capture` were written (all of them
/// past its end), read as the watch reads a prompt: as drawn.
fn shown(capture: &[u8], upto: usize) -> String {
    let mut terminal = TerminalSnapshot::new(40, 120);
    terminal.process(&capture[..upto.min(capture.len())]);
    terminal.visible_text()
}

/// The screen of the raw capture `testdata/opencode_screen_<file>_120x40_1.19.6.bin` (the bytes
/// OpenCode wrote in one launch, `testdata/opencode-captures.md`, "Prompts") once `upto` of them
/// were written, the whole capture where none is given.
macro_rules! screen {
    ($name:ident, $file:literal) => {
        screen!($name, $file, usize::MAX);
    };
    ($name:ident, $file:literal, $upto:expr) => {
        static $name: LazyLock<String> = LazyLock::new(|| {
            shown(
                include_bytes!(concat!(
                    "../../../../testdata/opencode_screen_",
                    $file,
                    "_120x40_1.19.6.bin"
                )),
                $upto,
            )
        });
    };
}

screen!(BASH, "prompt_bash", 32737);
screen!(ALWAYS_STEP, "prompt_bash", 33588);
screen!(ANSWERED, "prompt_bash");
screen!(EDIT, "prompt_edit", 33504);
screen!(EDIT_ALWAYS_STEP, "prompt_edit");
screen!(EXTERNAL, "prompt_external");
screen!(SINGLE, "prompt_question", 31878);
screen!(OWN_OPEN, "prompt_question", 32666);
screen!(OWN_TYPED, "prompt_question");
screen!(TABS, "prompt_tabs", 33149);
screen!(TABS_SECOND, "prompt_tabs", 33563);
screen!(REVIEWED, "prompt_tabs");
screen!(SEVERAL, "prompt_several", 39580);
screen!(SEVERAL_CHECKED, "prompt_several");
screen!(FIRST_QUESTION, "prompt_question_alpha");
screen!(WRAPPED_QUESTION, "question_wrapped");
screen!(WRAPPED_SEVERAL, "question_multiple_wrapped");
screen!(WRAPPED_BASH, "permission_bash_wrapped");
screen!(EDIT_SUBDIRECTORY, "permission_edit_subdirectory");

/// The calls the captured `permission.asked` and `question.asked` payloads start, in order: the
/// `bash` call, the `edit` call, a lone question, two questions, a question that takes several.
fn captured_calls() -> Vec<CallSeen> {
    calls_of(PAYLOADS)
}

/// The calls of the captures of what wraps, taken with the plugin forwarding `always`, in order:
/// a long question, a long question that takes several, a long `bash` command, an `edit` of a
/// file in a subdirectory, and the `echo hi` call whose `Allow always` step was captured.
fn wrapped_calls() -> Vec<CallSeen> {
    calls_of(WRAPPED_PAYLOADS)
}

fn calls_of(payloads: &str) -> Vec<CallSeen> {
    payloads
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let payload: Value = serde_json::from_str(line).unwrap();
            let event = payload["hook_event_name"].as_str().unwrap().to_string();
            OpenCodeInteractions.call_started(&event, &payload)
        })
        .collect()
}

fn call(at: usize) -> CallSeen {
    captured_calls()[at].clone()
}

fn read(screen: &str, calls: &[CallSeen]) -> Reading {
    OpenCodeInteractions.read(screen, calls)
}

fn key(screen: &str, call: &CallSeen, answer: &Answer) -> NextKey {
    OpenCodeInteractions.next_key(screen, call, answer)
}

fn choice(id: &str) -> Answer {
    Answer::Choice {
        id: id.to_string(),
        text: None,
    }
}

fn press(input: Input) -> NextKey {
    NextKey::Press(input)
}

fn text(key: &str) -> NextKey {
    press(Input::Text(key.to_string()))
}

fn answers(answers: &[(&[u32], Option<&str>)]) -> Answer {
    Answer::Questions(
        answers
            .iter()
            .map(|(selected, text)| QuestionAnswer {
                selected: selected.to_vec(),
                text: text.map(str::to_string),
            })
            .collect(),
    )
}

fn permission(reading: Reading) -> (usize, PermissionPrompt) {
    match reading {
        Reading::Shown {
            call,
            prompt: InteractionPrompt::Permission(prompt),
        } => (call, prompt),
        other => panic!("not a permission prompt: {other:?}"),
    }
}

fn question(reading: Reading) -> (usize, QuestionPrompt) {
    match reading {
        Reading::Shown {
            call,
            prompt: InteractionPrompt::Question(prompt),
        } => (call, prompt),
        other => panic!("not a question: {other:?}"),
    }
}

/// Each prompt payload starts a call of its own: the call's id, the permission's name or
/// `question` as the tool, and what it asks as the input; no other event starts one.
#[test]
fn each_prompt_payload_starts_a_call_of_what_it_asks() {
    let calls = captured_calls();
    let tools: Vec<&str> = calls.iter().map(|call| call.tool.as_str()).collect();
    assert_eq!(tools, ["bash", "edit", "question", "question", "question"]);
    assert!(calls
        .iter()
        .all(|call| call.id.as_deref().is_some_and(|id| id.starts_with("call_"))));
    assert_eq!(
        calls[0].input,
        json!({"patterns": ["echo hi"], "always": ["echo *"], "command": "echo hi"})
    );
    assert_eq!(calls[1].input["filepath"], "/tmp/probe/cwd/made.txt");
    assert_eq!(calls[1].input["drawn_path"], "made.txt");
    assert_eq!(calls[1].input["always"], json!(["*"]));
    assert_eq!(calls[2].input["questions"][0]["question"], "Which?");
    for event in ["turn.started", "permission.replied", "session.idle"] {
        assert!(OpenCodeInteractions
            .call_started(event, &json!({"tool": "bash", "request_id": "per_1"}))
            .is_none());
    }
}

/// A `bash` prompt is published for its call, with its command as the title and the three answers
/// of OpenCode's row, `Allow always` remembering a rule.
#[test]
fn a_bash_prompt_is_published_with_its_three_answers() {
    let calls = captured_calls();
    let (at, prompt) = permission(read(&BASH, &calls));
    assert_eq!(at, 0);
    assert_eq!(prompt.tool, "bash");
    assert_eq!(prompt.category, Some(ToolCategory::Command));
    assert_eq!(prompt.title.as_deref(), Some("echo hi"));
    assert_eq!(prompt.input, calls[0].input);
    let choices: Vec<(&str, &str, ChoiceEffect, bool)> = prompt
        .choices
        .iter()
        .map(|choice| {
            (
                choice.id.as_str(),
                choice.label.as_str(),
                choice.effect,
                choice.text.is_some(),
            )
        })
        .collect();
    assert_eq!(
        choices,
        [
            ("once", "Allow once", ChoiceEffect::Allow, false),
            ("always", "Allow always", ChoiceEffect::AllowRemember, false),
            ("reject", "Reject", ChoiceEffect::Deny, false),
        ]
    );
}

/// An edit's prompt names its file; a prompt for a directory outside the working directory, drawn
/// by an earlier capture, is matched by its pattern and titled by its row under the title.
#[test]
fn an_edit_and_an_outside_directory_are_matched_by_their_patterns() {
    let calls = captured_calls();
    let (at, prompt) = permission(read(&EDIT, &calls));
    assert_eq!(at, 1);
    assert_eq!(prompt.category, Some(ToolCategory::FileEdit));
    assert_eq!(prompt.title.as_deref(), Some("/tmp/probe/cwd/made.txt"));

    let outside = CallSeen::new(
        Some("call_1".to_string()),
        "external_directory",
        &json!({"patterns": ["/etc/*"]}),
    );
    let (at, prompt) = permission(read(&EXTERNAL, &[calls[0].clone(), outside]));
    assert_eq!(at, 1);
    assert_eq!(prompt.category, Some(ToolCategory::FileRead));
    assert_eq!(
        prompt.title.as_deref(),
        Some("Access external directory /etc")
    );
}

/// A prompt whose patterns no call has is up but not published, as is the second step of
/// `Allow always`; the input box back is the prompt gone, and anything else is unknown.
#[test]
fn what_cannot_be_published_is_up_and_the_box_back_is_gone() {
    let calls = captured_calls();
    assert_eq!(read(&BASH, &calls[1..]), Reading::Up);
    assert_eq!(read(&ALWAYS_STEP, &calls), Reading::Up);
    assert_eq!(read(&EDIT_ALWAYS_STEP, &calls), Reading::Up);
    assert_eq!(read(&ANSWERED, &calls), Reading::Gone);
    assert_eq!(
        read("nothing of OpenCode's here\n", &calls),
        Reading::Unknown
    );
}

/// Every answer starts from the prompt as it opens, `Allow once` selected: an Enter; `l` and Enter
/// together for `Allow always`, whose second step an Enter confirms; Esc for `Reject`.
#[test]
fn a_permission_is_answered_from_the_prompt_as_it_opens() {
    let bash = call(0);
    assert_eq!(key(&BASH, &bash, &choice("once")), press(Input::Enter));
    assert_eq!(
        key(&BASH, &bash, &choice("always")),
        NextKey::PressAll(vec![Input::Text("l".to_string()), Input::Enter])
    );
    assert_eq!(
        key(&ALWAYS_STEP, &bash, &choice("always")),
        press(Input::Enter)
    );
    // The step of another call's `Allow always` is not pressed: the `edit` call's `always` is
    // `*`, which the `bash` call's step does not say.
    assert!(matches!(
        key(&ALWAYS_STEP, &call(1), &choice("always")),
        NextKey::Stuck(_)
    ));
    assert_eq!(key(&BASH, &bash, &choice("reject")), press(Input::Escape));
    assert!(matches!(
        key(&ALWAYS_STEP, &bash, &choice("once")),
        NextKey::Stuck(_)
    ));
    assert!(matches!(
        key(&ANSWERED, &bash, &choice("once")),
        NextKey::Stuck(_)
    ));
}

/// The second step of an `edit` call's `Allow always`, whose `always` is `*` alone, says it will
/// allow `edit`: its `Confirm` is pressed for that call, and for no call of another permission or
/// of none.
#[test]
fn an_edit_calls_always_step_is_confirmed_for_that_call_alone() {
    let edit = call(1);
    assert_eq!(
        key(&EDIT_ALWAYS_STEP, &edit, &choice("always")),
        press(Input::Enter)
    );
    assert!(matches!(
        key(&EDIT_ALWAYS_STEP, &call(0), &choice("always")),
        NextKey::Stuck(_)
    ));
    let mut no_always = edit.input.clone();
    no_always.as_object_mut().unwrap().remove("always");
    let without = CallSeen::new(Some("call_w".to_string()), "edit", &no_always);
    assert!(matches!(
        key(&EDIT_ALWAYS_STEP, &without, &choice("always")),
        NextKey::Stuck(_)
    ));
}

/// An `edit` prompt is the call's only where the file it draws is the call's `drawn_path` whole:
/// an earlier call for a file of the same name in a subdirectory does not claim a prompt showing
/// `made.txt`, nor a call that says nothing of how its file is drawn.
#[test]
fn an_edit_prompt_is_matched_by_the_whole_file_it_draws() {
    let calls = captured_calls();
    let sub = CallSeen::new(
        Some("call_sub".to_string()),
        "edit",
        &json!({"patterns": ["tmp/probe/cwd/sub/made.txt"], "always": ["*"],
                "filepath": "/tmp/probe/cwd/sub/made.txt", "drawn_path": "sub/made.txt"}),
    );
    assert_eq!(read(&EDIT, std::slice::from_ref(&sub)), Reading::Up);
    assert_eq!(permission(read(&EDIT, &[calls[1].clone(), sub])).0, 0);
    let mut undrawn = calls[1].input.clone();
    undrawn.as_object_mut().unwrap().remove("drawn_path");
    let undrawn = CallSeen::new(Some("call_u".to_string()), "edit", &undrawn);
    assert_eq!(read(&EDIT, &[undrawn]), Reading::Up);
}

/// A lone question is published from the call's own input, each option with its description, an
/// answer of one's own in place of the options, and dismissible.
#[test]
fn a_lone_question_is_published_from_what_the_call_asks() {
    let calls = captured_calls();
    let (at, prompt) = question(read(&SINGLE, &calls));
    assert_eq!(at, 2);
    assert!(prompt.dismissible);
    let [asked] = prompt.questions.as_slice() else {
        panic!("one question: {prompt:?}");
    };
    assert_eq!(asked.text, "Which?");
    assert_eq!(asked.header.as_deref(), Some("Which"));
    assert!(!asked.multi_select);
    assert_eq!(asked.free_text, Some(FreeText::Instead));
    let options: Vec<(&str, Option<&str>)> = asked
        .options
        .iter()
        .map(|option| (option.label.as_str(), option.description.as_deref()))
        .collect();
    assert_eq!(options, [("A", Some("a")), ("B", Some("b"))]);

    // The earlier capture's question, with a call made for it.
    let alpha = CallSeen::new(
        Some("call_q".to_string()),
        "question",
        &json!({"questions": [{"question": "Which one?", "options": [
            {"label": "Alpha", "description": "the first"},
            {"label": "Beta", "description": "the second"}
        ]}]}),
    );
    assert_eq!(question(read(&FIRST_QUESTION, &[alpha])).0, 0);
}

/// Of a tabbed prompt only its first page is published, with every question; the later pages, the
/// review, an open field and a checked option are up.
#[test]
fn only_the_untouched_first_page_is_published() {
    let calls = captured_calls();
    let (at, prompt) = question(read(&TABS, &calls));
    assert_eq!(at, 3);
    let texts: Vec<&str> = prompt.questions.iter().map(|q| q.text.as_str()).collect();
    assert_eq!(texts, ["First one?", "Second one?"]);
    let (at, prompt) = question(read(&SEVERAL, &calls));
    assert_eq!(at, 4);
    assert!(prompt.questions[0].multi_select);
    assert_eq!(prompt.questions[0].text, "Which ones?");
    assert_eq!(prompt.questions[0].free_text, Some(FreeText::With));
    for screen in [
        &TABS_SECOND,
        &REVIEWED,
        &OWN_OPEN,
        &OWN_TYPED,
        &SEVERAL_CHECKED,
    ] {
        let screen: &str = screen;
        assert_eq!(read(screen, &calls), Reading::Up, "{screen}");
    }
}

/// A lone question takes its option's digit; an answer of one's own opens the field with the digit
/// after the last option, is pasted into it empty, and sent by Enter; Esc dismisses.
#[test]
fn a_lone_question_is_answered_by_digit_or_its_own_field() {
    let asks = call(2);
    assert_eq!(key(&SINGLE, &asks, &answers(&[(&[1], None)])), text("2"));
    let own = answers(&[(&[], Some("my own answer"))]);
    assert_eq!(key(&SINGLE, &asks, &own), text("3"));
    assert_eq!(
        key(&OWN_OPEN, &asks, &own),
        press(Input::Paste("my own answer".to_string()))
    );
    assert_eq!(key(&OWN_TYPED, &asks, &own), press(Input::Enter));
    let other = answers(&[(&[], Some("something else"))]);
    assert!(matches!(key(&OWN_TYPED, &asks, &other), NextKey::Stuck(_)));
    assert_eq!(key(&SINGLE, &asks, &Answer::Dismiss), press(Input::Escape));
}

/// Each page of a tabbed prompt takes its own question's digit, and the review an Enter.
#[test]
fn a_tabbed_prompt_is_answered_page_by_page() {
    let asks = call(3);
    let both = answers(&[(&[0], None), (&[1], None)]);
    assert_eq!(key(&TABS, &asks, &both), text("1"));
    assert_eq!(key(&TABS_SECOND, &asks, &both), text("2"));
    assert_eq!(key(&REVIEWED, &asks, &both), press(Input::Enter));
}

/// A question that takes several has each option toggled by its digit until the boxes match, the
/// answer of one's own opened by its digit, and `l` moves on once all match.
#[test]
fn a_question_that_takes_several_is_toggled_then_left() {
    let asks = call(4);
    let first_and_own = answers(&[(&[0], Some("mine"))]);
    assert_eq!(key(&SEVERAL, &asks, &first_and_own), text("1"));
    assert_eq!(key(&SEVERAL_CHECKED, &asks, &first_and_own), text("4"));
    let first = answers(&[(&[0], None)]);
    assert_eq!(key(&SEVERAL_CHECKED, &asks, &first), text("l"));
    let third = answers(&[(&[2], None)]);
    assert_eq!(key(&SEVERAL_CHECKED, &asks, &third), text("1"));
}

/// A prompt is the call's only by the detail OpenCode draws for it, the whole of it and over the
/// rows it wraps to: a `bash` command that wraps, an edited file drawn relative to the working
/// directory while its pattern is relative to the root (outside a repository).
#[test]
fn a_permission_is_matched_by_its_whole_detail_over_the_rows_it_wraps_to() {
    let calls = wrapped_calls();
    let (at, prompt) = permission(read(&WRAPPED_BASH, &calls));
    assert_eq!(at, 2);
    assert_eq!(prompt.title.as_deref(), calls[2].input["command"].as_str());
    let (at, prompt) = permission(read(&EDIT_SUBDIRECTORY, &calls));
    assert_eq!(at, 3);
    assert_eq!(
        prompt.title.as_deref(),
        Some("/tmp/probe/cwd/sub/dir/made.txt")
    );
    assert_eq!(
        calls[3].input["patterns"][0],
        "tmp/probe/cwd/sub/dir/made.txt"
    );
}

/// A call is no prompt's where its detail is only a part of what the prompt draws, where it has no
/// patterns, or where the prompt is another call's: a command that is the start of the one
/// shown, the same file in another directory, a call of no patterns.
#[test]
fn a_call_whose_detail_is_only_part_of_the_prompt_is_not_its_call() {
    let start = CallSeen::new(
        Some("call_s".to_string()),
        "bash",
        &json!({"patterns": ["echo the first part of a long command"],
                "command": "echo the first part of a long command"}),
    );
    assert_eq!(
        read(&WRAPPED_BASH, std::slice::from_ref(&start)),
        Reading::Up
    );
    let echo = call(0);
    assert_eq!(read(&WRAPPED_BASH, &[echo]), Reading::Up);
    let elsewhere = CallSeen::new(
        Some("call_e".to_string()),
        "edit",
        &json!({"patterns": ["made.txt"], "filepath": "/tmp/probe/cwd/other/dir/made.txt"}),
    );
    assert_eq!(read(&EDIT_SUBDIRECTORY, &[elsewhere]), Reading::Up);
    let bare = CallSeen::new(Some("call_b".to_string()), "read", &json!({}));
    assert_eq!(read(&EXTERNAL, std::slice::from_ref(&bare)), Reading::Up);
    let no_patterns = CallSeen::new(
        Some("call_n".to_string()),
        "bash",
        &json!({"patterns": [], "command": "echo hi"}),
    );
    assert_eq!(read(&BASH, &[no_patterns]), Reading::Up);
    assert!(matches!(
        key(&WRAPPED_BASH, &start, &choice("once")),
        NextKey::Stuck(_)
    ));
}

/// Two calls asking other things that the prompt both shows leave it up but unpublished; the
/// same call asked twice is the newest.
#[test]
fn calls_asking_other_things_that_both_fit_leave_the_prompt_unpublished() {
    let first = CallSeen::new(
        Some("call_1".to_string()),
        "read",
        &json!({"patterns": ["/etc"]}),
    );
    let second = CallSeen::new(
        Some("call_2".to_string()),
        "read",
        &json!({"patterns": ["/etc/*"]}),
    );
    let outside = CallSeen::new(
        Some("call_3".to_string()),
        "glob",
        &json!({"patterns": ["etc"]}),
    );
    // Neither is an `external_directory` call, so each is matched by what the prompt holds.
    assert_eq!(read(&EXTERNAL, &[first, second, outside]), Reading::Up);
    let echo = call(0);
    let again = CallSeen::new(Some("call_again".to_string()), "bash", &echo.input);
    assert_eq!(permission(read(&BASH, &[echo, again])).0, 1);
}

/// A question and an option's label that wrap are read over their rows: the long question is
/// published, and is answered by its digits.
#[test]
fn a_question_that_wraps_is_published_and_answered() {
    let calls = wrapped_calls();
    let (at, prompt) = question(read(&WRAPPED_QUESTION, &calls));
    assert_eq!(at, 0);
    assert_eq!(
        prompt.questions[0].options[0].label,
        calls[0].input["questions"][0]["options"][0]["label"]
    );
    assert_eq!(
        key(&WRAPPED_QUESTION, &calls[0], &answers(&[(&[0], None)])),
        text("1")
    );
    let (at, prompt) = question(read(&WRAPPED_SEVERAL, &calls));
    assert_eq!(at, 1);
    assert!(prompt.questions[0].multi_select);
    assert_eq!(
        key(&WRAPPED_SEVERAL, &calls[1], &answers(&[(&[1], None)])),
        text("2")
    );
}

/// A question whose text or option is only part of what the page draws is not the page's.
#[test]
fn a_question_that_is_only_part_of_the_page_is_not_its_call() {
    let calls = wrapped_calls();
    let mut shorter = calls[0].input.clone();
    shorter["questions"][0]["question"] = json!("When the build finishes");
    let mut option = calls[0].input.clone();
    option["questions"][0]["options"][0]["label"] = json!("The stable channel");
    for input in [shorter, option] {
        let other = CallSeen::new(Some("call_q".to_string()), "question", &input);
        assert_eq!(
            read(&WRAPPED_QUESTION, std::slice::from_ref(&other)),
            Reading::Up
        );
        assert!(matches!(
            key(&WRAPPED_QUESTION, &other, &answers(&[(&[0], None)])),
            NextKey::Stuck(_)
        ));
    }
}

/// OpenCode's prompt cannot be read once a key went into it: its selection is colour alone.
#[test]
fn its_prompt_is_unreadable_after_input() {
    assert!(OpenCodeInteractions.prompt_unreadable_after_input());
}

#[test]
fn v2_permission_prompt_offers_the_captured_allow_actions() {
    let capture =
        include_bytes!("../../../../testdata/opencode_screen_permission_120x40_2.4.1.bin");
    let calls = calls_of(include_str!(
        "../../../../testdata/opencode_hook_payloads_permission_2.4.1.jsonl"
    ));
    let screen = shown(capture, usize::MAX);
    let (at, prompt) = permission(read(&screen, &calls));
    assert_eq!(prompt.title.as_deref(), Some("echo M35_EXECUTED"));
    assert_eq!(
        prompt
            .choices
            .iter()
            .map(|choice| choice.effect)
            .collect::<Vec<_>>(),
        [
            ChoiceEffect::Allow,
            ChoiceEffect::AllowRemember,
            ChoiceEffect::Deny
        ]
    );
    assert!(matches!(
        key(&screen, &calls[at], &choice(ALWAYS)),
        NextKey::PressAll(inputs) if inputs == [Input::Text("l".into()), Input::Enter]
    ));
}

#[test]
fn v2_question_prompt_matches_its_captured_form() {
    let capture = include_bytes!("../../../../testdata/opencode_screen_question_120x40_2.4.1.bin");
    let calls = calls_of(include_str!(
        "../../../../testdata/opencode_hook_payloads_question_2.4.1.jsonl"
    ));
    let (_, prompt) = question(read(&shown(capture, usize::MAX), &calls));
    assert_eq!(prompt.questions[0].text, "Pick a probe value");
    assert_eq!(prompt.questions[0].options[1].label, "B");
}
