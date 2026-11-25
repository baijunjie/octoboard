//! Tests of Grok Build's pending interactions against the screens of
//! `testdata/grok-interaction-captures.md` and the calls its hooks report.

use serde_json::{json, Value};

use super::feed::describe_hook_call;
use super::interaction::GrokInteractions;
use crate::platform::term::Input;
use crate::protocol::{
    ChoiceEffect, ChoiceText, FreeText, InteractionPrompt, PermissionPrompt, QuestionAnswer,
    QuestionPrompt, ToolCategory,
};
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};
use crate::terminal::snapshot::TerminalSnapshot;

macro_rules! screen {
    ($name:literal) => {
        replayed(include_bytes!(concat!(
            "../../../testdata/grok_interaction_",
            $name,
            ".bin"
        )))
    };
}

fn replayed(bytes: &[u8]) -> String {
    let mut screen = TerminalSnapshot::new(40, 120);
    screen.process(bytes);
    screen.visible_text_undimmed()
}

/// The call the probe's hooks reported under `name` (`testdata/grok_interaction_hooks.jsonl`).
fn hooked(name: &str) -> CallSeen {
    include_str!("../../../testdata/grok_interaction_hooks.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a hook line"))
        .find(|line| line["name"] == name)
        .and_then(|line| GrokInteractions.call_started("PreToolUse", &line["payload"]))
        .expect("a call")
}

fn shell() -> CallSeen {
    hooked("shell")
}

fn mcp() -> CallSeen {
    hooked("mcp")
}

fn edit() -> CallSeen {
    hooked("edit")
}

fn write() -> CallSeen {
    hooked("write")
}

fn single() -> CallSeen {
    hooked("single")
}

fn multi() -> CallSeen {
    hooked("multi")
}

fn three() -> CallSeen {
    hooked("three")
}

/// A screen edited from a capture by changing markers only: not measured, a stand-in for a state
/// the probe did not capture (`name` is the capture it derives from).
fn edited(name: &str, screen: String, edits: &[(&str, &str)]) -> String {
    edits.iter().fold(screen, |screen, (from, to)| {
        assert!(screen.contains(from), "{name} has no {from}");
        screen.replacen(from, to, 1)
    })
}

fn read(screen: &str, calls: &[CallSeen]) -> Reading {
    GrokInteractions.read(screen, calls)
}

fn key(screen: &str, answer: &Answer) -> NextKey {
    GrokInteractions.next_key(screen, &shell(), answer)
}

fn press(input: Input) -> NextKey {
    NextKey::Press(input)
}

fn text(text: &str) -> Input {
    Input::Text(text.to_string())
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

fn questions(reading: Reading) -> (usize, QuestionPrompt) {
    match reading {
        Reading::Shown {
            call,
            prompt: InteractionPrompt::Question(prompt),
        } => (call, prompt),
        other => panic!("not a question prompt: {other:?}"),
    }
}

fn choices(prompt: &PermissionPrompt) -> Vec<(&str, ChoiceEffect, bool)> {
    prompt
        .choices
        .iter()
        .map(|choice| (choice.id.as_str(), choice.effect, choice.text.is_some()))
        .collect()
}

fn choice(id: &str, text: Option<&str>) -> Answer {
    Answer::Choice {
        id: id.to_string(),
        text: text.map(str::to_string),
    }
}

fn picks(selected: &[u32]) -> QuestionAnswer {
    QuestionAnswer {
        selected: selected.to_vec(),
        text: None,
    }
}

fn written(answer: &str) -> QuestionAnswer {
    QuestionAnswer {
        selected: Vec::new(),
        text: Some(answer.to_string()),
    }
}

#[test]
fn a_shell_card_publishes_its_rows_but_not_the_always_approve_row() {
    let calls = [edit(), shell()];
    let (at, prompt) = permission(read(&screen!("shell_card"), &calls));
    assert_eq!(at, 1);
    assert_eq!(prompt.tool, "run_terminal_command");
    assert_eq!(prompt.category, Some(ToolCategory::Command));
    assert_eq!(
        prompt.title.as_deref(),
        Some("printf probe-shell-ran > shell_out.txt")
    );
    assert_eq!(
        prompt.detail.as_deref(),
        Some("Write a marker file for the probe")
    );
    assert_eq!(
        choices(&prompt),
        [
            ("2", ChoiceEffect::Allow, false),
            ("3", ChoiceEffect::Deny, true),
            ("4", ChoiceEffect::DenyRemember, false),
        ]
    );
    assert_eq!(prompt.choices[0].label, "Yes, proceed");
    assert_eq!(prompt.choices[1].text, Some(ChoiceText::Optional));
}

#[test]
fn a_card_that_matches_no_call_is_left_to_the_terminal() {
    let screen = screen!("shell_card");
    assert_eq!(read(&screen, &[]), Reading::Up);
    assert_eq!(read(&screen, &[edit(), mcp()]), Reading::Up);
    assert_eq!(read(&screen!("mcp_card"), &[shell()]), Reading::Up);
}

#[test]
fn edit_and_write_cards_offer_the_session_wide_rule_as_a_remembered_allow() {
    let expected = [
        ("2", ChoiceEffect::AllowRemember, false),
        ("3", ChoiceEffect::Allow, false),
        ("4", ChoiceEffect::Deny, true),
    ];
    let (at, prompt) = permission(read(&screen!("edit_card"), &[shell(), edit()]));
    assert_eq!(at, 1);
    assert_eq!(prompt.category, Some(ToolCategory::FileEdit));
    assert_eq!(choices(&prompt), expected);
    assert_eq!(
        prompt.choices[0].label,
        "Yes, allow edits in work/ for this session"
    );
    // The title cuts the long path in the middle.
    let (at, prompt) = permission(read(&screen!("write_card"), &[write(), edit()]));
    assert_eq!(at, 0);
    assert_eq!(choices(&prompt), expected);
    assert_eq!(read(&screen!("write_card"), &[edit()]), Reading::Up);
}

#[test]
fn an_mcp_card_is_matched_by_the_server_and_tool_and_its_arguments() {
    let screen = screen!("mcp_card");
    let (at, prompt) = permission(read(&screen, &[shell(), mcp()]));
    assert_eq!(at, 1);
    assert_eq!(prompt.category, Some(ToolCategory::Mcp));
    assert_eq!(
        choices(&prompt),
        [
            ("2", ChoiceEffect::AllowRemember, false),
            ("3", ChoiceEffect::Allow, false),
            ("4", ChoiceEffect::Deny, true),
            ("5", ChoiceEffect::DenyRemember, false),
        ]
    );
    let other_arguments = CallSeen::new(
        None,
        "probemcp__echo",
        &json!({"tool_name": "probemcp__echo", "tool_input": {"text": "else"}}),
    );
    assert_eq!(read(&screen, &[other_arguments]), Reading::Up);
}

/// A hook names an MCP tool by itself where the record reaches it through `use_tool`; both are
/// classed the same, and the other calls as the feed classes their tools.
#[test]
fn a_hook_call_is_classed_as_its_feed_form_is() {
    let record = json!({"tool_name": "probemcp__echo", "tool_input": {}});
    let hooked_as_mcp = describe_hook_call(&mcp().tool, &mcp().input).0;
    assert_eq!(hooked_as_mcp, ToolCategory::Mcp);
    assert_eq!(describe_hook_call("use_tool", &record).0, hooked_as_mcp);
    for (call, category) in [
        (shell(), ToolCategory::Command),
        (edit(), ToolCategory::FileEdit),
        (write(), ToolCategory::FileEdit),
    ] {
        assert_eq!(describe_hook_call(&call.tool, &call.input).0, category);
    }
}

#[test]
fn a_card_in_the_scrollback_is_published_and_a_press_starts_with_tab() {
    let parked = screen!("shell_parked");
    let (_, prompt) = permission(read(&parked, &[shell()]));
    assert_eq!(prompt.choices.len(), 3);
    assert_eq!(key(&parked, &choice("2", None)), press(Input::Ctrl('i')));
    let parked = screen!("single_parked");
    questions(read(&parked, &[single()]));
    assert_eq!(
        GrokInteractions.next_key(&parked, &shell(), &Answer::Dismiss),
        press(Input::Ctrl('i'))
    );
}

#[test]
fn a_message_being_written_on_the_no_row_is_a_card_half_answered() {
    for name in ["typing", "typed"] {
        let screen = match name {
            "typing" => screen!("shell_typing"),
            _ => screen!("shell_typed"),
        };
        assert_eq!(read(&screen, &[shell()]), Reading::Up, "{name}");
    }
}

#[test]
fn the_composer_back_with_no_card_is_gone_and_other_views_are_unknown() {
    for gone in [
        screen!("idle"),
        screen!("dismissed"),
        screen!("slash_partial"),
    ] {
        assert_eq!(read(&gone, &[shell()]), Reading::Gone, "{gone}");
    }
    assert_eq!(read("", &[]), Reading::Unknown);
    assert_eq!(read("some other view\nwith rows", &[]), Reading::Unknown);
}

#[test]
fn a_digit_takes_a_choice_with_no_text() {
    let screen = screen!("shell_card");
    assert_eq!(key(&screen, &choice("2", None)), press(text("2")));
    assert_eq!(key(&screen, &choice("4", None)), press(text("4")));
    assert!(matches!(
        key(&screen, &choice("9", None)),
        NextKey::Stuck(_)
    ));
    // A digit typed into an open message would be part of it.
    assert!(matches!(
        key(&screen!("shell_typing"), &choice("2", None)),
        NextKey::Stuck(_)
    ));
}

/// The sequence for "No" with a message, over the captures and the screens each key leaves:
/// Down to the row, the paste, Enter.
#[test]
fn a_message_goes_to_the_no_row_by_the_arrows_then_pasted_then_sent() {
    let reject = choice("3", Some("use a temp dir"));
    let initial = screen!("shell_card");
    assert_eq!(key(&initial, &reject), press(Input::Down));
    // Not measured: `shell_card` with the highlight moved to the No row.
    let on_no = edited(
        "shell_card",
        initial,
        &[
            ("2 (●) Yes, proceed", "2 (○) Yes, proceed"),
            ("3 (○) No, reject", "3 (●) No, reject"),
        ],
    );
    assert_eq!(
        key(&on_no, &reject),
        press(Input::Paste("use a temp dir".to_string()))
    );
    assert_eq!(key(&screen!("shell_typed"), &reject), press(Input::Enter));
    assert_eq!(key(&screen!("shell_typing"), &reject), press(Input::Enter));
    // From a row below the No row, Tab wraps round to the top, and Down goes on from there.
    let below = screen!("shell_on_always");
    assert_eq!(key(&below, &reject), press(Input::Ctrl('i')));
}

#[test]
fn a_single_question_publishes_its_options_and_a_written_answer_instead() {
    let calls = [shell(), single()];
    let (at, prompt) = questions(read(&screen!("single"), &calls));
    assert_eq!(at, 1);
    assert!(prompt.dismissible);
    let [question] = &prompt.questions[..] else {
        panic!("one question");
    };
    assert_eq!(question.text, "Which database should the service use?");
    assert!(!question.multi_select);
    assert_eq!(question.free_text, Some(FreeText::Instead));
    let labels: Vec<_> = question.options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, ["Postgres (Recommended)", "SQLite", "Redis"]);
    assert_eq!(
        question.options[1].description.as_deref(),
        Some("Single file, no server")
    );
}

#[test]
fn a_multi_select_question_and_several_questions_are_published_whole() {
    let (_, prompt) = questions(read(&screen!("multi"), &[multi()]));
    assert!(prompt.questions[0].multi_select);
    let (_, prompt) = questions(read(&screen!("three_q1"), &[three()]));
    assert_eq!(prompt.questions.len(), 3);
    assert!(prompt.questions[1].multi_select);
    assert_eq!(prompt.questions[2].options[1].label, "debug");
    // Only the last of several takes a written answer.
    let free: Vec<_> = prompt.questions.iter().map(|q| q.free_text).collect();
    assert_eq!(free, [None, None, Some(FreeText::Instead)]);
    // The screen shows three questions, the call one: not this call.
    assert_eq!(read(&screen!("three_q1"), &[single()]), Reading::Up);
}

/// Only the card as drawn is published: one with a tick or a pick on it, or at a later question,
/// is half answered, since a press over it would meet what is already there.
#[test]
fn a_question_card_half_answered_is_up() {
    assert_eq!(read(&screen!("single_field"), &[single()]), Reading::Up);
    assert_eq!(read(&screen!("single_typed"), &[single()]), Reading::Up);
    assert_eq!(read(&screen!("multi_typed"), &[multi()]), Reading::Up);
    assert_eq!(read(&screen!("multi_toggled"), &[multi()]), Reading::Up);
    assert_eq!(read(&screen!("three_q2"), &[three()]), Reading::Up);
    assert_eq!(read(&screen!("three_q3"), &[three()]), Reading::Up);
    assert_eq!(read(&screen!("three_q3_typed"), &[three()]), Reading::Up);
    assert_eq!(read(&screen!("three_back_q1"), &[three()]), Reading::Up);
    // And not matched to a call that asks something else.
    assert_eq!(read(&screen!("single"), &[multi()]), Reading::Up);
}

fn answers(answers: Vec<QuestionAnswer>) -> Answer {
    Answer::Questions(answers)
}

#[test]
fn a_single_select_answer_is_its_digit_and_a_written_one_goes_through_the_z_row() {
    let pick = answers(vec![picks(&[1])]);
    assert_eq!(key(&screen!("single"), &pick), press(text("2")));
    let typed = answers(vec![written("Mongo")]);
    assert_eq!(key(&screen!("single"), &typed), press(text("z")));
    assert_eq!(
        key(&screen!("single_field"), &typed),
        press(Input::Paste("Mongo".to_string()))
    );
    assert_eq!(key(&screen!("single_typed"), &typed), press(Input::Enter));
    assert_eq!(
        key(&screen!("single_parked"), &pick),
        press(Input::Ctrl('i'))
    );
    // Answers for other questions than the card shows.
    assert!(matches!(
        key(&screen!("single"), &answers(vec![picks(&[0]), picks(&[0])])),
        NextKey::Stuck(_)
    ));
}

#[test]
fn a_multi_select_answer_toggles_the_rows_that_differ_then_enters() {
    let wanted = answers(vec![picks(&[0, 2])]);
    let initial = screen!("multi");
    assert_eq!(key(&initial, &wanted), press(text("1")));
    // Not measured: `multi` with the first row ticked.
    let first = edited(
        "multi",
        initial,
        &[("1 [ ] Unit tests", "1 [x] Unit tests")],
    );
    assert_eq!(key(&first, &wanted), press(text("3")));
    assert_eq!(key(&screen!("multi_toggled"), &wanted), press(Input::Enter));
    // A row ticked that is not wanted is toggled off.
    let other = answers(vec![picks(&[0])]);
    assert_eq!(key(&screen!("multi_toggled"), &other), press(text("3")));
}

#[test]
fn several_questions_are_answered_in_turn() {
    let all = answers(vec![picks(&[1]), picks(&[0, 2]), picks(&[0])]);
    assert_eq!(key(&screen!("three_q1"), &all), press(text("2")));
    // The second is multi-select: toggles, then Enter moves on.
    assert_eq!(key(&screen!("three_q2"), &all), press(text("1")));
    // Not measured: `three_q2` with rows 1 and 3 ticked.
    let ticked = edited(
        "three_q2",
        screen!("three_q2"),
        &[
            ("1 [ ] Unit tests", "1 [x] Unit tests"),
            ("3 [ ] E2E", "3 [x] E2E"),
        ],
    );
    assert_eq!(key(&ticked, &all), press(Input::Enter));
    // The last takes its digit.
    assert_eq!(key(&screen!("three_q3"), &all), press(text("1")));
    // A written answer is pressed on the last question only: what Enter does in the field
    // elsewhere is not measured.
    let typed = answers(vec![written("a"), picks(&[0]), picks(&[0])]);
    assert!(matches!(
        key(&screen!("three_q1"), &typed),
        NextKey::Stuck(_)
    ));
    let last = answers(vec![picks(&[0]), picks(&[0]), written("a")]);
    assert_eq!(key(&screen!("three_q3"), &last), press(text("z")));
    // Not measured: `three_q3` with the wanted row picked (`(●)`) already, which Enter submits.
    let picked = edited(
        "three_q3",
        screen!("three_q3"),
        &[("1 (○) info", "1 (●) info")],
    );
    assert_eq!(key(&picked, &all), press(Input::Enter));
    // Not on a question before the last: the digit goes on to the next one.
    let picked = edited(
        "three_q1",
        screen!("three_q1"),
        &[("2 (○) SQLite", "2 (●) SQLite")],
    );
    assert_eq!(key(&picked, &all), press(text("2")));
    // A field open on the card where a pick is wanted is somebody else's answer.
    assert!(matches!(
        key(&screen!("three_q3_typed"), &all),
        NextKey::Stuck(_)
    ));
}

#[test]
fn dismissing_is_shift_x_unless_a_field_is_open() {
    assert_eq!(
        GrokInteractions.next_key(&screen!("single"), &shell(), &Answer::Dismiss),
        press(text("X"))
    );
    assert!(matches!(
        GrokInteractions.next_key(&screen!("single_field"), &shell(), &Answer::Dismiss),
        NextKey::Stuck(_)
    ));
    assert!(matches!(
        GrokInteractions.next_key(&screen!("idle"), &shell(), &Answer::Dismiss),
        NextKey::Stuck(_)
    ));
}

#[test]
fn a_call_is_kept_from_the_probes_pre_tool_use_lines() {
    let seen = shell();
    assert_eq!(seen.id.as_deref(), Some("call_2_0"));
    assert_eq!(seen.tool, "run_terminal_command");
    assert_eq!(
        seen.input["command"],
        "printf probe-shell-ran > shell_out.txt"
    );
    assert_eq!(mcp().tool, "probemcp__echo");
    assert_eq!(three().input["questions"].as_array().map(Vec::len), Some(3));
    // Only a start of a call is one.
    let line = include_str!("../../../testdata/grok_interaction_hooks.jsonl")
        .lines()
        .next()
        .map(|line| serde_json::from_str::<Value>(line).expect("a hook line"))
        .expect("a line");
    assert!(GrokInteractions
        .call_started("PostToolUse", &line["payload"])
        .is_none());
}

/// Grok Build's cards are read off the screen with faint cells blanked, as the captures here are
/// replayed and as its composer is read: read as drawn, what it draws faint would be a card's
/// text.
#[test]
fn a_card_is_not_read_with_its_faint_text() {
    assert_eq!(
        GrokInteractions.prompt_screen(),
        crate::session::screen::ScreenText::FaintBlanked
    );
}
