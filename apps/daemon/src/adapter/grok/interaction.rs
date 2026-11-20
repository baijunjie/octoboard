//! How Grok Build's pending interactions are read and answered: a tool permission card, and the
//! card of its own `ask_user_question` tool. Measured on 1.0.89 (`--no-leader`) in
//! `testdata/grok-interaction-captures.md`, summarised in `docs/agent-cli-reference.md`, "Grok
//! Build".
//!
//! What the hooks give is the call (`PreToolUse`: the tool, its id and input, in full) and that a
//! prompt is up (`Notification`, which names no call and no options); what the card offers, its
//! numbering, which row is highlighted and whether the keyboard is on the card all show only on
//! the screen. The card is a block of rows drawn with a `┃` margin in place of the composer, with a
//! footer under it:
//!
//! - a permission card: `›  <what it asks>`, the command or arguments behind `│`, then the rows
//!   `N (●) label`, numbered from 2 (an unnumbered last row, the always-approve mode, is never
//!   published: it also rewrites the user's own config) and the footer `2/4:select │ …`;
//! - a question card: the question, then `N (○) label  description` rows (`[ ]` / `[x]` when more
//!   than one may be chosen) and a `z` row for a written answer, and the footer
//!   `Tab:next answer │ …`. Several questions are shown one at a time, `[k/n]` on the hint row;
//! - Esc parks the keyboard in the scrollback, which the footer says (`Tab:permission` /
//!   `Tab:question`) with the card still drawn; Tab brings it back. The card is the same prompt,
//!   so it is published, and a press starts with Tab;
//! - a written answer opens a field in its row (`❯ text`, the footer `Enter:send │ Esc:back`),
//!   which is a prompt half answered and is left to the terminal.

use serde_json::Value;

use super::feed::describe_hook_call;
use super::hooks::{field, GrokHooks};
use super::screen::{composer, PROMPT_MARK};
use crate::hooks::HookInterpreter;
use crate::platform::term::Input;
use crate::protocol::{
    AgentId, ChoiceEffect, ChoiceText, FreeText, InteractionPrompt, PermissionChoice,
    PermissionPrompt, Question, QuestionAnswer, QuestionOption, QuestionPrompt, ToolCategory,
};
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};

pub(super) struct GrokInteractions;

impl InteractionReader for GrokInteractions {
    /// Faint cells blanked, as its composer is read (`GrokScreen`); its cards draw nothing faint
    /// that a reading needs (the replayed captures, `testdata/grok-interaction-captures.md`).
    fn faint_blanked(&self) -> bool {
        true
    }

    /// A call of the session itself: a subagent's calls are asked about on its own card, but are
    /// not the session's to publish.
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        if event != "PreToolUse" || GrokHooks.is_subagent(payload) {
            return None;
        }
        let input = payload
            .get("toolInput")
            .or_else(|| payload.get("tool_input"))
            .unwrap_or(&Value::Null);
        Some(CallSeen::new(
            field(payload, "toolUseId", "tool_use_id").map(str::to_string),
            field(payload, "toolName", "tool_name")?,
            input,
        ))
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        match View::of(screen) {
            View::Card(card) => match card.kind {
                Kind::Permission => permission(&card, calls),
                Kind::Question => question(&card, calls),
            },
            View::Unreadable => Reading::Up,
            // The composer is drawn in place of the card, idle or while a turn runs.
            View::Composer => Reading::Gone,
            View::Other => Reading::Unknown,
        }
    }

    fn next_key(&self, screen: &str, _call: &CallSeen, answer: &Answer) -> NextKey {
        let View::Card(card) = View::of(screen) else {
            return NextKey::Stuck("no prompt on the screen");
        };
        if card.state == State::Parked {
            return NextKey::Press(Input::Ctrl('i'));
        }
        match (card.kind, answer) {
            (Kind::Permission, Answer::Choice { id, text }) => {
                permission_key(&card, id, text.as_deref())
            }
            (Kind::Question, Answer::Questions(answers)) => question_key(&card, answers),
            (Kind::Question, Answer::Dismiss) if card.state == State::Editing => {
                NextKey::Stuck("a written answer is open")
            }
            // Shift+X, which dismisses the questions and lets the turn carry on.
            (Kind::Question, Answer::Dismiss) => NextKey::Press(Input::Text("X".to_string())),
            _ => NextKey::Stuck("the prompt on the screen is not the kind this answer is for"),
        }
    }
}

/// What the screen shows.
enum View {
    /// A card with its footer recognised.
    Card(Card),
    /// A card block whose footer is not one measured.
    Unreadable,
    /// No card, and the composer is drawn.
    Composer,
    Other,
}

struct Card {
    kind: Kind,
    state: State,
    /// Whether the footer offers `Esc:unselect`: something on the card has been picked.
    picked: bool,
    /// The rows between the `┃` margin and the end of the row, trimmed.
    rows: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Permission,
    Question,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// The card has the keyboard.
    Ready,
    /// The keyboard is in the scrollback.
    Parked,
    /// A row's text field is open.
    Editing,
}

impl View {
    fn of(screen: &str) -> View {
        let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
        let Some(last) = rows.iter().rposition(|row| !row.trim().is_empty()) else {
            return View::Other;
        };
        let mut end = last;
        while end > 0 && rows[end - 1].trim().is_empty() {
            end -= 1;
        }
        let mut top = end;
        while top > 0 && rows[top - 1].trim_start().starts_with(MARGIN) {
            top -= 1;
        }
        if top == end {
            return if composer(screen).is_some() {
                View::Composer
            } else {
                View::Other
            };
        }
        let footer = rows[last].trim_start();
        let card_rows: Vec<String> = rows[top..end]
            .iter()
            .map(|row| {
                row.trim_start()
                    .trim_start_matches(MARGIN)
                    .trim()
                    .to_string()
            })
            .collect();
        let state = if footer.starts_with("Tab:permission") || footer.starts_with("Tab:question") {
            State::Parked
        } else if footer.starts_with("Enter:send")
            || (footer.starts_with("Enter:submit") && footer.contains("Esc:back"))
        {
            State::Editing
        } else if footer.starts_with("Tab:next answer")
            || (footer.starts_with(|c: char| c.is_ascii_digit()) && footer.contains(":select"))
        {
            State::Ready
        } else {
            return View::Unreadable;
        };
        let kind = if card_rows.iter().any(|row| row.contains(NAVIGATE_HINT)) {
            Kind::Question
        } else {
            Kind::Permission
        };
        View::Card(Card {
            kind,
            state,
            picked: footer.contains("Esc:unselect"),
            rows: card_rows,
        })
    }
}

/// The margin of every row of a card.
const MARGIN: char = '\u{2503}';

/// What a question card's hint row says, which a permission card's does not.
const NAVIGATE_HINT: &str = "\u{2191}/\u{2193} navigate";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Marker {
    /// `(●)`. On a permission card it is the highlighted row; on a question card it means picked,
    /// and the cursor there is a background colour only, which the screen text does not show.
    Focused,
    /// `(○)`.
    Radio,
    /// `[x]`.
    Checked,
    /// `[ ]`.
    Unchecked,
}

/// An option row: `2 (●) Yes, proceed`, `z [x] ❯ text`, or the unnumbered `(○) …`.
#[derive(Debug)]
struct Row {
    key: Option<char>,
    marker: Marker,
    /// The label and, for a question, its description as drawn; or a field's `❯ text`.
    text: String,
}

impl Row {
    fn parse(row: &str) -> Option<Row> {
        let mut chars = row.chars();
        let first = chars.next()?;
        let (key, rest) = if first.is_ascii_alphanumeric() && chars.next() == Some(' ') {
            (Some(first), row[first.len_utf8() + 1..].trim_start())
        } else {
            (None, row)
        };
        let (marker, text) = [
            ("(\u{25CF})", Marker::Focused),
            ("(\u{25CB})", Marker::Radio),
            ("[x]", Marker::Checked),
            ("[ ]", Marker::Unchecked),
        ]
        .into_iter()
        .find_map(|(mark, marker)| Some((marker, rest.strip_prefix(mark)?)))?;
        Some(Row {
            key,
            marker,
            text: text.trim().to_string(),
        })
    }

    /// Whether a text field is open in the row, and holds something.
    fn written(&self) -> bool {
        self.text
            .strip_prefix(PROMPT_MARK)
            .is_some_and(|text| !text.trim().is_empty())
    }

    fn open(&self) -> bool {
        self.text.starts_with(PROMPT_MARK)
    }
}

fn option_rows(card: &Card) -> Vec<Row> {
    card.rows.iter().filter_map(|row| Row::parse(row)).collect()
}

/// `text` with its whitespace made single spaces, for comparing what wraps.
fn normalised(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---- permission cards ----

fn permission(card: &Card, calls: &[CallSeen]) -> Reading {
    if card.state == State::Editing {
        return Reading::Up;
    }
    let choices: Vec<PermissionChoice> = option_rows(card)
        .into_iter()
        .filter_map(|row| {
            let (effect, text) = classify(&row.text)?;
            Some(PermissionChoice {
                id: row.key?.to_string(),
                label: row.text,
                effect,
                text,
            })
        })
        .collect();
    if choices.is_empty() {
        return Reading::Up;
    }
    let title = card
        .rows
        .iter()
        .find_map(|row| row.strip_prefix('\u{203A}'))
        .map(str::trim);
    let shown: Vec<&str> = card
        .rows
        .iter()
        .filter_map(|row| row.strip_prefix('\u{2502}'))
        .map(str::trim)
        .collect();
    let Some(call) = calls
        .iter()
        .rposition(|call| asks_about(call, title, &shown))
    else {
        return Reading::Up;
    };
    let (category, title, _) = describe_hook_call(&calls[call].tool, &calls[call].input);
    Reading::Shown {
        call,
        prompt: InteractionPrompt::Permission(PermissionPrompt {
            tool: calls[call].tool.clone(),
            category: Some(category),
            title,
            detail: calls[call]
                .input
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            input: calls[call].input.clone(),
            input_truncated: calls[call].input_truncated,
            choices,
        }),
    }
}

/// What choosing a row of a permission card does, by its label. Not the always-approve row, which
/// switches the session's permission mode, nor a label not measured.
fn classify(label: &str) -> Option<(ChoiceEffect, Option<ChoiceText>)> {
    if label == "Yes" || label.starts_with("Yes, proceed") {
        Some((ChoiceEffect::Allow, None))
    } else if label.starts_with("Yes, allow edits in") || label.starts_with("Always allow") {
        Some((ChoiceEffect::AllowRemember, None))
    } else if label.starts_with("No, reject") {
        Some((ChoiceEffect::Deny, Some(ChoiceText::Optional)))
    } else if label.starts_with("Never allow") {
        Some((ChoiceEffect::DenyRemember, None))
    } else {
        None
    }
}

/// Whether the card, with its `›` line `title` and the rows behind `│` `shown`, asks about
/// `call`, by what each kind of card shows of it: a command's own text, the file an edit is to,
/// the server and tool of an MCP call (with its arguments, where they are drawn whole).
fn asks_about(call: &CallSeen, title: Option<&str>, shown: &[&str]) -> bool {
    let (category, _, paths) = describe_hook_call(&call.tool, &call.input);
    match category {
        ToolCategory::Command => call
            .input
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| shows_text(&normalised(&shown.join(" ")), command)),
        ToolCategory::FileEdit => title
            .and_then(|title| title.strip_prefix("Allow "))
            .and_then(|title| title.strip_suffix('?'))
            .and_then(|title| title.split_once(" to "))
            .is_some_and(|(_, shown)| paths.iter().any(|path| shows_path(shown.trim(), path))),
        ToolCategory::Mcp => {
            let name = |text: &str| {
                text.chars()
                    .filter(char::is_ascii_alphanumeric)
                    .collect::<String>()
                    .to_ascii_lowercase()
            };
            let named = title
                .and_then(|title| title.strip_prefix("Allow "))
                .and_then(|title| title.strip_suffix('?'))
                .is_some_and(|shown| name(shown) == name(&call.tool));
            let arguments = shown
                .iter()
                .position(|row| row.starts_with('{'))
                .and_then(|from| serde_json::from_str::<Value>(&shown[from..].join("\n")).ok());
            named
                && arguments.is_none_or(|arguments| {
                    call.input
                        .get("tool_input")
                        .is_none_or(|input| *input == arguments)
                })
        }
        _ => false,
    }
}

/// Whether `shown` (whitespace made single spaces) is `text`, or its start where it ends in `…`.
fn shows_text(shown: &str, text: &str) -> bool {
    let text = normalised(text);
    match shown.strip_suffix('\u{2026}') {
        Some(start) => !start.is_empty() && text.starts_with(start.trim_end()),
        None => shown == text,
    }
}

/// Whether `shown` names `path`: as written, relative to it, or with its middle cut to `…`.
fn shows_path(shown: &str, path: &str) -> bool {
    match shown.split_once('\u{2026}') {
        Some((start, end)) => {
            path.len() >= start.len() + end.len() && path.starts_with(start) && path.ends_with(end)
        }
        None => path == shown || path.ends_with(&format!("/{shown}")),
    }
}

fn permission_key(card: &Card, id: &str, text: Option<&str>) -> NextKey {
    let rows = option_rows(card);
    let Some(target) = rows
        .iter()
        .position(|row| row.key.is_some_and(|key| key.to_string() == id))
    else {
        return NextKey::Stuck("the card no longer offers that choice");
    };
    let Some(text) = text else {
        // A digit typed into the open field would be part of the message.
        return if card.state == State::Editing {
            NextKey::Stuck("a message is being written")
        } else {
            NextKey::Press(Input::Text(id.to_string()))
        };
    };
    // A digit rejects at once, without the message: the row is reached with the arrows, and the
    // text pasted on it starts the message. Down stops at the last row and there is no key that
    // moves up (no Shift+Tab, by decision: `Input` crosses the holder protocol), so from below the
    // row Tab wraps round to the top, passing over the always-approve row on the way; a press
    // stopped midway (the gate re-checks every key) may leave the highlight on that row, which
    // only an Enter, never pressed here, would take.
    let Some(focus) = rows.iter().position(|row| row.marker == Marker::Focused) else {
        return NextKey::Stuck("no row of the card is highlighted");
    };
    if focus != target {
        return if card.state == State::Editing {
            NextKey::Stuck("a message is being written on another row")
        } else if focus < target {
            NextKey::Press(Input::Down)
        } else {
            NextKey::Press(Input::Ctrl('i'))
        };
    }
    if rows[target].written() {
        NextKey::Press(Input::Enter)
    } else {
        NextKey::Press(Input::Paste(text.to_string()))
    }
}

// ---- question cards ----

/// The question a card shows now.
struct Shown {
    /// Which question of the call, from 0.
    index: usize,
    total: usize,
    text: String,
    options: Vec<Row>,
    /// The `z` row.
    free: Option<Row>,
}

impl Shown {
    fn of(card: &Card) -> Option<Shown> {
        let (mut index, mut total) = (0, 1);
        let mut text = Vec::new();
        let mut options = Vec::new();
        let mut free = None;
        for row in &card.rows {
            if let Some(parsed) = Row::parse(row) {
                match parsed.key {
                    Some('z') => free = Some(parsed),
                    Some(key) if key.is_ascii_digit() => options.push(parsed),
                    _ => return None,
                }
            } else if let Some((at, of)) = position(row) {
                (index, total) = (at, of);
            } else if options.is_empty() && !row.is_empty() {
                text.push(row.as_str());
            }
        }
        Some(Shown {
            index,
            total,
            text: normalised(&text.join(" ")),
            options,
            free,
        })
    }

    fn multi_select(&self) -> bool {
        self.options
            .first()
            .is_some_and(|row| matches!(row.marker, Marker::Checked | Marker::Unchecked))
    }
}

/// The `[k/n]` a hint row opens with, as the question's index from 0 and the number of questions.
fn position(row: &str) -> Option<(usize, usize)> {
    let (at, rest) = row.strip_prefix('[')?.split_once('/')?;
    let (of, _) = rest.split_once(']')?;
    let (at, of): (usize, usize) = (at.parse().ok()?, of.parse().ok()?);
    (at >= 1 && at <= of).then_some((at - 1, of))
}

/// A question of the call's input.
struct Asked {
    text: String,
    header: Option<String>,
    multi_select: bool,
    options: Vec<(String, Option<String>)>,
}

fn asked(call: &CallSeen) -> Option<Vec<Asked>> {
    call.input
        .get("questions")?
        .as_array()?
        .iter()
        .map(|question| {
            let text = |key: &str| question.get(key)?.as_str().map(str::to_string);
            Some(Asked {
                text: text("question")?,
                header: text("header"),
                multi_select: ["multi_select", "multiSelect"]
                    .into_iter()
                    .find_map(|key| question.get(key)?.as_bool())
                    .unwrap_or(false),
                options: question
                    .get("options")?
                    .as_array()?
                    .iter()
                    .map(|option| {
                        let text = |key: &str| option.get(key)?.as_str().map(str::to_string);
                        Some((text("label")?, text("description")))
                    })
                    .collect::<Option<_>>()?,
            })
        })
        .collect()
}

fn question(card: &Card, calls: &[CallSeen]) -> Reading {
    // Only the card as the agent drew it is published: one half answered (a field open, an
    // earlier question answered, an option picked or ticked, which the footer's `Esc:unselect`
    // also says) is left to the terminal, since an answer pressed over it would meet the leftovers.
    let Some(shown) = Shown::of(card).filter(|shown| {
        card.state != State::Editing
            && !card.picked
            && shown.index == 0
            && shown.free.is_some()
            && shown
                .options
                .iter()
                .chain(&shown.free)
                .all(|row| !matches!(row.marker, Marker::Focused | Marker::Checked))
    }) else {
        return Reading::Up;
    };
    let ask_tool = AgentId::GROK.descriptor();
    let Some((call, questions)) = calls.iter().enumerate().rev().find_map(|(at, call)| {
        let questions = ask_tool
            .is_ask_question_tool(Some(&call.tool))
            .then(|| asked(call))??;
        asks(&shown, &questions).then_some((at, questions))
    }) else {
        return Reading::Up;
    };
    let count = questions.len();
    Reading::Shown {
        call,
        prompt: InteractionPrompt::Question(QuestionPrompt {
            questions: questions
                .into_iter()
                .enumerate()
                .map(|(at, question)| Question {
                    text: question.text,
                    header: question.header,
                    multi_select: question.multi_select,
                    options: question
                        .options
                        .into_iter()
                        .map(|(label, description)| QuestionOption { label, description })
                        .collect(),
                    // Whether Enter in the written field before the last question advances or
                    // submits all is not measured, so only the last question takes a written
                    // answer (`question_key` refuses it elsewhere).
                    free_text: (at + 1 == count).then_some(FreeText::Instead),
                })
                .collect(),
            dismissible: true,
        }),
    }
}

/// Whether the question the card shows is the first of `questions`, as the card draws it.
fn asks(shown: &Shown, questions: &[Asked]) -> bool {
    let Some(first) = questions.first() else {
        return false;
    };
    questions.len() == shown.total
        && shown.text.contains(&normalised(&first.text))
        && first.multi_select == shown.multi_select()
        && first.options.len() == shown.options.len()
        && first
            .options
            .iter()
            .zip(&shown.options)
            .enumerate()
            .all(|(at, ((label, _), row))| {
                row.key == char::from_digit(at as u32 + 1, 10)
                    && !row.text.is_empty()
                    && (row.text.starts_with(label.as_str()) || label.starts_with(&row.text))
            })
}

fn question_key(card: &Card, answers: &[QuestionAnswer]) -> NextKey {
    let Some(shown) = Shown::of(card) else {
        return NextKey::Stuck("the question card is not readable");
    };
    if answers.len() != shown.total {
        return NextKey::Stuck("the answers do not fit the questions on the screen");
    }
    let answer = &answers[shown.index];
    // Whether Enter in the written field moves on to the next question or submits them all is not
    // measured (`testdata/grok-interaction-captures.md`), so only the last question takes one.
    if answer.text.is_some() && shown.index + 1 < shown.total {
        return NextKey::Stuck("a written answer before the last question is not measured");
    }
    let field_open = shown.free.as_ref().is_some_and(Row::open);
    if let Some(text) = &answer.text {
        return if field_open {
            if shown.free.as_ref().is_some_and(Row::written) {
                NextKey::Press(Input::Enter)
            } else {
                NextKey::Press(Input::Paste(text.clone()))
            }
        } else if card.state == State::Editing {
            NextKey::Stuck("a text field is open on another row")
        } else {
            NextKey::Press(Input::Text("z".to_string()))
        };
    }
    if field_open || card.state == State::Editing {
        return NextKey::Stuck("a written answer is open");
    }
    let digit = |at: usize| NextKey::Press(Input::Text((at + 1).to_string()));
    if shown.multi_select() {
        // Each digit toggles its row; Enter then takes the ticked ones and goes on.
        let wrong = shown.options.iter().enumerate().find(|(at, row)| {
            answer.selected.contains(&(*at as u32)) != (row.marker == Marker::Checked)
        });
        return match wrong {
            Some((at, _)) => digit(at),
            None => NextKey::Press(Input::Enter),
        };
    }
    match answer.selected.first() {
        // A digit picks the option and goes on to the next question, or submits the last.
        // Where the wanted row already shows picked on the last question, the digit has been
        // pressed: Enter submits them all (measured), where the digit again might only move.
        Some(at) if (*at as usize) < shown.options.len() => {
            let row = &shown.options[*at as usize];
            if row.marker == Marker::Focused && shown.index + 1 == shown.total {
                NextKey::Press(Input::Enter)
            } else {
                digit(*at as usize)
            }
        }
        _ => NextKey::Stuck("the answer picks no option of the question"),
    }
}
