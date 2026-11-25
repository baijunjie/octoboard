use serde_json::{json, Value};

use super::*;
use crate::terminal::snapshot::TerminalSnapshot;

// Antigravity CLI 1.5.14 against the stand-in, `testdata/antigravity-captures.md`: the terminal
// output of a launch from its start to each prompt, replayed on a 120x40 terminal.
const PERMISSION_COMMAND: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_permission_command.bin");
const PERMISSION_ALL_ROWS: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_permission_command_all_rows.bin");
const PERMISSION_ON_NO: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_permission_command_on_no.bin");
const PERMISSION_WRITE: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_permission_write.bin");
const PERMISSION_EDIT: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_permission_edit.bin");
const PERMISSION_MCP: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_permission_mcp.bin");
const QUESTION: &[u8] = include_bytes!("../../../../testdata/antigravity_screen_question.bin");
const QUESTION_MULTI: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_question_multi.bin");
const QUESTION_MULTI_TICKED: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_question_multi_ticked.bin");
const QUESTION_LAST: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_question_last.bin");
const QUESTION_WRITE_IN: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_question_write_in.bin");
const INPUT_EMPTY: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_input_empty.bin");
const INPUT_DURING_TURN: &[u8] =
    include_bytes!("../../../../testdata/antigravity_screen_input_during_turn.bin");
const TRUST_SCREEN: &[u8] = include_bytes!("../../../../testdata/antigravity_trust_screen.bin");

/// The screen as the reader is given it ([`InteractionReader::prompt_screen`], as drawn).
fn screen(capture: &[u8]) -> String {
    let mut terminal = TerminalSnapshot::new(40, 120);
    terminal.process(capture);
    terminal.visible_text()
}

fn call(tool: &str, input: Value) -> CallSeen {
    CallSeen::new(Some(format!("{tool}-call")), tool, &input)
}

/// The calls the captured prompts were drawn for, as their `PreToolUse` gave them.
fn command(line: &str) -> CallSeen {
    call(
        "run_command",
        json!({ "CommandLine": line, "toolSummary": "make a file", "WaitMsBeforeAsync": 5000 }),
    )
}

fn write_file() -> CallSeen {
    call(
        "write_file",
        json!({ "Path": "/scratch/project/note.txt", "Content": "hi" }),
    )
}

fn edit_file() -> CallSeen {
    call(
        "edit_file",
        json!({ "Path": "/scratch/project/note.txt", "Target": "hi", "Replacement": "ho" }),
    )
}

fn mcp() -> CallSeen {
    call(
        "call_mcp_tool",
        json!({ "ServerName": "probe", "ToolName": "slow", "Arguments": { "seconds": 0.1 } }),
    )
}

fn one_question() -> CallSeen {
    call(
        "ask_question",
        json!({ "questions": [
            { "question": "Which target?", "options": ["Alpha", "Beta"], "is_multi_select": false }
        ] }),
    )
}

fn colors_then_size() -> CallSeen {
    call(
        "ask_question",
        json!({ "questions": [
            { "question": "Pick colors?", "options": ["Red", "Green", "Blue"], "is_multi_select": true },
            { "question": "Which size?", "options": ["Small", "Medium"] }
        ] }),
    )
}

fn three_questions() -> CallSeen {
    call(
        "ask_question",
        json!({ "questions": [
            { "question": "Which target?", "options": ["Alpha", "Beta"] },
            { "question": "Pick colors?", "options": ["Red", "Green", "Blue"], "is_multi_select": true },
            { "question": "Name?", "options": ["Xavier", "Yuri"] }
        ] }),
    )
}

fn read(capture: &[u8], calls: &[CallSeen]) -> Reading {
    AntigravityInteractions.read(&screen(capture), calls)
}

fn permission_of(reading: Reading) -> (usize, PermissionPrompt) {
    match reading {
        Reading::Shown {
            call,
            prompt: InteractionPrompt::Permission(prompt),
        } => (call, prompt),
        other => panic!("a permission prompt: {other:?}"),
    }
}

fn choices_of(prompt: &PermissionPrompt) -> Vec<(&str, ChoiceEffect)> {
    prompt
        .choices
        .iter()
        .map(|choice| (choice.id.as_str(), choice.effect))
        .collect()
}

/// A command's prompt is published for its call, among the turn's others, with every row it
/// shows: the four drawn at first, and all six once moved. A row that keeps a rule remembers the
/// answer, the one kept "(Persist to settings.json)" as well, under the CLI's own label. No row
/// takes text.
#[test]
fn a_command_prompt_offers_every_row_it_shows() {
    let calls = [
        one_question(),
        command("echo octo > made.txt"),
        write_file(),
    ];
    let (at, prompt) = permission_of(read(PERMISSION_COMMAND, &calls));
    assert_eq!(at, 1);
    assert_eq!(prompt.tool, "run_command");
    assert_eq!(prompt.category, Some(ToolCategory::Command));
    assert_eq!(prompt.title.as_deref(), Some("echo octo > made.txt"));
    assert_eq!(prompt.detail.as_deref(), Some("make a file"));
    assert_eq!(
        choices_of(&prompt),
        [
            ("1", ChoiceEffect::Allow),
            ("2", ChoiceEffect::AllowRemember),
            ("3", ChoiceEffect::AllowRemember),
            ("4", ChoiceEffect::Deny)
        ]
    );
    assert!(prompt.choices.iter().all(|choice| choice.text.is_none()));
    assert!(
        prompt.choices[2]
            .label
            .ends_with("(Persist to settings.json)"),
        "{:?}",
        prompt.choices[2]
    );

    let (_, prompt) = permission_of(read(PERMISSION_ALL_ROWS, &calls));
    assert_eq!(
        choices_of(&prompt),
        [
            ("1", ChoiceEffect::Allow),
            ("2", ChoiceEffect::AllowRemember),
            ("3", ChoiceEffect::AllowRemember),
            ("4", ChoiceEffect::Deny),
            ("5", ChoiceEffect::DenyRemember),
            ("6", ChoiceEffect::DenyRemember)
        ]
    );
    assert!(
        prompt.choices[5]
            .label
            .ends_with("(Persist to settings.json)"),
        "{:?}",
        prompt.choices[5]
    );
}

/// A file's creation and its edit each offer to allow and to refuse, for the call of their tool
/// that names the file; the MCP prompt offers all six of its rows.
#[test]
fn the_file_and_mcp_prompts_are_published_for_their_calls() {
    let calls = [write_file(), edit_file(), mcp()];
    for (capture, at) in [(PERMISSION_WRITE, 0), (PERMISSION_EDIT, 1)] {
        let (call, prompt) = permission_of(read(capture, &calls));
        assert_eq!(call, at);
        assert_eq!(prompt.category, Some(ToolCategory::FileEdit));
        assert_eq!(prompt.title.as_deref(), Some("/scratch/project/note.txt"));
        assert_eq!(
            choices_of(&prompt),
            [("1", ChoiceEffect::Allow), ("2", ChoiceEffect::Deny)]
        );
    }
    let (call, prompt) = permission_of(read(PERMISSION_MCP, &calls));
    assert_eq!(call, 2);
    assert_eq!(prompt.category, Some(ToolCategory::Mcp));
    assert_eq!(prompt.title.as_deref(), Some("probe/slow"));
    assert_eq!(
        choices_of(&prompt),
        [
            ("1", ChoiceEffect::Allow),
            ("2", ChoiceEffect::AllowRemember),
            ("3", ChoiceEffect::AllowRemember),
            ("4", ChoiceEffect::Deny),
            ("5", ChoiceEffect::DenyRemember),
            ("6", ChoiceEffect::DenyRemember)
        ]
    );
}

/// A prompt whose call is not among the turn's is up but not published: another command, the
/// same file under another tool, no calls at all.
#[test]
fn a_prompt_with_no_call_of_its_own_is_up_but_not_published() {
    assert_eq!(
        read(PERMISSION_COMMAND, &[command("echo other")]),
        Reading::Up
    );
    assert_eq!(read(PERMISSION_EDIT, &[write_file()]), Reading::Up);
    assert_eq!(read(PERMISSION_MCP, &[]), Reading::Up);
    assert_eq!(read(QUESTION, &[colors_then_size()]), Reading::Up);
}

/// The input box, idle or under a turn, is the prompt gone; the trust screen is neither.
#[test]
fn the_input_box_is_the_prompt_gone() {
    assert_eq!(read(INPUT_EMPTY, &[]), Reading::Gone);
    assert_eq!(read(INPUT_DURING_TURN, &[]), Reading::Gone);
    assert_eq!(read(TRUST_SCREEN, &[]), Reading::Unknown);
}

fn next_key(capture: &[u8], answer: &Answer) -> NextKey {
    AntigravityInteractions.next_key(&screen(capture), &write_file(), answer)
}

fn choice(id: &str) -> Answer {
    Answer::Choice {
        id: id.to_string(),
        text: None,
    }
}

/// A choice is its digit, wherever the highlight is; one the screen does not offer, or one with
/// text, is not pressed.
#[test]
fn a_choice_is_pressed_as_its_digit() {
    assert_eq!(
        next_key(PERMISSION_COMMAND, &choice("2")),
        NextKey::Press(Input::Text("2".to_string()))
    );
    assert_eq!(
        next_key(PERMISSION_ON_NO, &choice("1")),
        NextKey::Press(Input::Text("1".to_string()))
    );
    assert!(matches!(
        next_key(PERMISSION_WRITE, &choice("4")),
        NextKey::Stuck(_)
    ));
    let with_text = Answer::Choice {
        id: "1".to_string(),
        text: Some("why".to_string()),
    };
    assert!(matches!(
        next_key(PERMISSION_WRITE, &with_text),
        NextKey::Stuck(_)
    ));
    assert!(matches!(
        next_key(INPUT_EMPTY, &choice("1")),
        NextKey::Stuck(_)
    ));
}

fn questions_of(reading: Reading) -> (usize, QuestionPrompt) {
    match reading {
        Reading::Shown {
            call,
            prompt: InteractionPrompt::Question(prompt),
        } => (call, prompt),
        other => panic!("questions: {other:?}"),
    }
}

/// The form as the CLI drew it is published with every question of its call: a written answer in
/// place of the options on a question of one choice, none on one of several, and the questions
/// can be skipped.
#[test]
fn the_form_is_published_with_every_question_of_its_call() {
    let (at, prompt) = questions_of(read(QUESTION, &[write_file(), one_question()]));
    assert_eq!(at, 1);
    assert!(prompt.dismissible);
    let [question] = prompt.questions.as_slice() else {
        panic!("one question: {:?}", prompt.questions);
    };
    assert_eq!(question.text, "Which target?");
    assert!(!question.multi_select);
    assert_eq!(question.free_text, Some(FreeText::Instead));
    assert_eq!(
        question
            .options
            .iter()
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Beta"]
    );

    let (_, prompt) = questions_of(read(QUESTION_MULTI, &[colors_then_size()]));
    assert_eq!(
        prompt
            .questions
            .iter()
            .map(|question| (question.multi_select, question.free_text))
            .collect::<Vec<_>>(),
        [(true, None), (false, Some(FreeText::Instead))]
    );
}

/// A form already under way is left to the terminal: a later question on the screen, options
/// ticked, a field open.
#[test]
fn a_form_under_way_is_up_but_not_published() {
    let calls = [three_questions()];
    for capture in [QUESTION_MULTI_TICKED, QUESTION_LAST, QUESTION_WRITE_IN] {
        assert_eq!(read(capture, &calls), Reading::Up);
    }
}

fn picked(selected: &[u32]) -> QuestionAnswer {
    QuestionAnswer {
        selected: selected.to_vec(),
        text: None,
    }
}

fn written(text: &str) -> QuestionAnswer {
    QuestionAnswer {
        selected: Vec::new(),
        text: Some(text.to_string()),
    }
}

fn text(key: &str) -> NextKey {
    NextKey::Press(Input::Text(key.to_string()))
}

/// On a question of one choice the option's digit is pressed, and a written answer opens the
/// field with the digit of `Write-in...`, is pasted into it and sent with Enter.
#[test]
fn a_question_of_one_choice_takes_a_digit_or_a_written_answer() {
    let answers = |answer: QuestionAnswer| Answer::Questions(vec![answer]);
    assert_eq!(next_key(QUESTION, &answers(picked(&[1]))), text("2"));
    assert_eq!(next_key(QUESTION, &answers(written("mine"))), text("3"));

    let three = |last: QuestionAnswer| Answer::Questions(vec![picked(&[0]), picked(&[0, 2]), last]);
    assert_eq!(next_key(QUESTION_LAST, &three(picked(&[1]))), text("2"));
    assert_eq!(
        next_key(QUESTION_LAST, &three(written("custom answer"))),
        text("3")
    );
    assert_eq!(
        next_key(QUESTION_WRITE_IN, &three(written("custom answer"))),
        NextKey::Press(Input::Enter)
    );
    assert!(matches!(
        next_key(QUESTION_WRITE_IN, &three(picked(&[0]))),
        NextKey::Stuck(_)
    ));
}

/// On a question of several choices each option whose tick is not as answered gets its digit,
/// and Enter goes on once all are; a written answer is not pressed there.
#[test]
fn a_question_of_several_choices_ticks_by_digit_then_enter() {
    let second =
        |answer: QuestionAnswer| Answer::Questions(vec![picked(&[1]), answer, picked(&[0])]);
    // Red and Blue are ticked on the screen.
    assert_eq!(
        next_key(QUESTION_MULTI_TICKED, &second(picked(&[0, 2]))),
        NextKey::Press(Input::Enter)
    );
    assert_eq!(
        next_key(QUESTION_MULTI_TICKED, &second(picked(&[0]))),
        text("3")
    );
    assert_eq!(
        next_key(QUESTION_MULTI_TICKED, &second(picked(&[0, 1, 2]))),
        text("2")
    );
    assert!(matches!(
        next_key(QUESTION_MULTI_TICKED, &second(written("mine"))),
        NextKey::Stuck(_)
    ));
    // Answers that are not one a question are not pressed.
    assert!(matches!(
        next_key(
            QUESTION_MULTI_TICKED,
            &Answer::Questions(vec![picked(&[0])])
        ),
        NextKey::Stuck(_)
    ));
}

/// Skipping is Esc, question by question.
#[test]
fn the_questions_are_skipped_with_esc() {
    for capture in [QUESTION, QUESTION_MULTI_TICKED, QUESTION_WRITE_IN] {
        assert_eq!(
            next_key(capture, &Answer::Dismiss),
            NextKey::Press(Input::Escape)
        );
    }
    assert!(matches!(
        next_key(PERMISSION_COMMAND, &Answer::Dismiss),
        NextKey::Stuck(_)
    ));
}

/// Only the session's own calls are kept: a subagent's call, told by its empty message, is not.
#[test]
fn only_a_call_of_the_sessions_own_conversation_is_kept() {
    let payload = json!({
        "hookEventName": "PreToolUse",
        "lastUserInput": "make a file",
        "toolCall": { "id": "c1", "name": "run_command", "args": { "CommandLine": "ls" } }
    });
    let seen = AntigravityInteractions
        .call_started("PreToolUse", &payload)
        .expect("a call");
    assert_eq!(seen.id.as_deref(), Some("c1"));
    assert_eq!(seen.tool, "run_command");
    assert_eq!(seen.input, json!({ "CommandLine": "ls" }));
    assert!(AntigravityInteractions
        .call_started("PostToolUse", &payload)
        .is_none());

    let mut subagent = payload;
    subagent["lastUserInput"] = Value::from("");
    assert!(AntigravityInteractions
        .call_started("PreToolUse", &subagent)
        .is_none());
}
