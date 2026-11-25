//! How Antigravity CLI's pending interactions are read and answered: its numbered permission
//! prompts and the form of its own `ask_question` tool. Measured on 1.5.14 against the stand-in,
//! in `testdata/antigravity-captures.md`, summarised in `docs/agent-cli-reference.md`, "Antigravity
//! CLI", "Capabilities measured on 1.5.14".
//!
//! The hooks give the call (`PreToolUse`: the tool, its id and its arguments, in full, before the
//! prompt is drawn) and the OSC 7501 report that the session waits (`blocked` with the kind
//! `permission` or `question`); what the prompt offers shows only on the screen. Both take the
//! input box's place at the bottom of the screen, under a title and a full-width rule:
//!
//! - a permission prompt: what the call would do (`Requesting permission for:` and the command;
//!   the file and its diff; the MCP server and tool), the question (`Run this command?`, `Allow
//!   creation of this file?`, `Accept this file edit?`, `Allow calling this tool?`), the rows
//!   `> 1. Yes, …` to `N. No, …` with `>` on the highlighted one, and a hint row
//!   (`↑/↓ Navigate · …`). A digit answers its row at once. The command and MCP prompts offer to
//!   allow or deny calls like it for the conversation, and to do so "(Persist to settings.json)",
//!   which writes the rule into the user's own settings file: each row that keeps a rule is
//!   published as remembering the answer, under the CLI's own label, which says where it is kept
//!   (the user picks it, as in the terminal; Octoboard writes nothing);
//! - the form: `Question k/n: <text>`, the options numbered (`[ ]` / `[x]` before each where more
//!   than one may be chosen) and `Write-in...` last, then a hint row. On a question of one choice a
//!   digit picks its option and goes on to the next question, or sends the answers after the last;
//!   on one of several a digit ticks or unticks its option and Enter goes on. The digit of
//!   `Write-in...` opens a field (`Your answer:`) under the rows, whose text Enter takes as the
//!   answer and goes on. Esc skips the question on the screen (the model is told `User Skipped`),
//!   or closes an open field.

use serde_json::Value;

use super::hooks::{EDIT_FILE, PRE_TOOL_USE, RUN_COMMAND, WRITE_FILE};
use super::screen::{rule_width, AntigravityScreen};
use crate::platform::term::Input;
use crate::protocol::{
    AgentId, ChoiceEffect, FreeText, InteractionPrompt, PermissionChoice, PermissionPrompt,
    Question, QuestionAnswer, QuestionOption, QuestionPrompt, ToolCategory,
};
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};
use crate::session::screen::ScreenReader;

pub(super) struct AntigravityInteractions;

/// The tool of an MCP server's tool call, beside the command and file tools a permission prompt
/// is drawn for.
const CALL_MCP_TOOL: &str = "call_mcp_tool";

/// What each permission prompt asks, on the row above its options.
const ASKS_COMMAND: &str = "Run this command?";
const ASKS_CREATION: &str = "Allow creation of this file?";
const ASKS_EDIT: &str = "Accept this file edit?";
const ASKS_MCP: &str = "Allow calling this tool?";

/// The row a command prompt's command follows.
const COMMAND_SHOWN: &str = "Requesting permission for:";

/// The last option of every question, which opens a field for an answer of one's own.
const WRITE_IN: &str = "Write-in...";

/// The row an open field's text follows.
const FIELD_OPEN: &str = "Your answer:";

impl InteractionReader for AntigravityInteractions {
    /// A call of the session's own conversation: a subagent's calls are not the session's to
    /// publish.
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        if event != PRE_TOOL_USE || super::background::is_subagent(payload) {
            return None;
        }
        let call = payload.get("toolCall")?;
        Some(CallSeen::new(
            call.get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string),
            call.get("name")?.as_str()?,
            call.get("args").unwrap_or(&Value::Null),
        ))
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        match View::of(screen) {
            View::Prompt(prompt) => match prompt.question() {
                Some(form) => question(&form, calls),
                None => permission(&prompt, calls),
            },
            View::InputBox => Reading::Gone,
            View::Other => Reading::Unknown,
        }
    }

    fn next_key(&self, screen: &str, _call: &CallSeen, answer: &Answer) -> NextKey {
        let View::Prompt(prompt) = View::of(screen) else {
            return NextKey::Stuck("no prompt on the screen");
        };
        match (prompt.question(), answer) {
            (None, Answer::Choice { id, text }) => {
                if text.is_some() {
                    NextKey::Stuck("this prompt takes no text")
                } else if prompt.options.iter().any(|option| option.key == *id) {
                    NextKey::Press(Input::Text(id.clone()))
                } else {
                    NextKey::Stuck("the prompt no longer offers that choice")
                }
            }
            (Some(form), Answer::Questions(answers)) => question_key(&form, answers),
            // Each Esc skips the question on the screen, or closes its open field first.
            (Some(_), Answer::Dismiss) => NextKey::Press(Input::Escape),
            _ => NextKey::Stuck("the prompt on the screen is not the kind this answer is for"),
        }
    }
}

/// What the screen shows.
enum View {
    /// A prompt in the input box's place.
    Prompt(Prompt),
    /// The input box, idle or under a running turn.
    InputBox,
    Other,
}

/// The rows of a prompt under its rule.
struct Prompt {
    /// The rows above the options, trimmed, empty ones left out.
    above: Vec<String>,
    options: Vec<OptionRow>,
    /// The text of the open field, where one is open (empty while nothing is written).
    field: Option<String>,
}

/// A numbered row, with the rows its label wraps onto.
struct OptionRow {
    /// Its digits, what answers it.
    key: String,
    /// Whether `>` marks it.
    focused: bool,
    label: String,
}

impl OptionRow {
    /// `> 1. label`, `  2. label`: the mark or two spaces, the number, a dot and a space.
    fn parse(row: &str) -> Option<OptionRow> {
        let (focused, rest) = match row.strip_prefix("> ") {
            Some(rest) => (true, rest),
            None => (false, row.strip_prefix("  ")?),
        };
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let label = rest[digits..].strip_prefix(". ")?;
        Some(OptionRow {
            key: rest[..digits].to_string(),
            focused,
            label: label.trim().to_string(),
        })
    }

    /// The tick a row of a question of several choices starts with: `Some(true)` for `[x]`.
    fn ticked(&self) -> Option<bool> {
        if self.label.starts_with("[x] ") {
            Some(true)
        } else if self.label.starts_with("[ ] ") {
            Some(false)
        } else {
            None
        }
    }

    /// The label without its tick.
    fn text(&self) -> &str {
        match self.ticked() {
            Some(_) => self.label[4..].trim_start(),
            None => &self.label,
        }
    }
}

impl View {
    fn of(screen: &str) -> View {
        if AntigravityScreen.input_box_rows(screen).is_some() {
            return View::InputBox;
        }
        let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
        // The last full-width rule, the one a prompt is drawn under. The box's own rules are
        // read by `AntigravityScreen`, which is asked first.
        let Some(rule) = rows.iter().rposition(|row| rule_width(row).is_some()) else {
            return View::Other;
        };
        let mut above = Vec::new();
        let mut options: Vec<OptionRow> = Vec::new();
        let mut field = None;
        let mut hint = false;
        let mut wrapping = false;
        for row in &rows[rule + 1..] {
            if let Some(option) = OptionRow::parse(row) {
                if field.is_some() || hint {
                    return View::Other;
                }
                options.push(option);
                wrapping = true;
                continue;
            }
            let text = row.trim();
            if text.is_empty() {
                wrapping = false;
            } else if is_hint(text) {
                hint = true;
                break;
            } else if text == FIELD_OPEN {
                field = Some(String::new());
                wrapping = false;
            } else if let Some(written) = field.as_mut() {
                if !written.is_empty() {
                    written.push('\n');
                }
                written.push_str(text);
            } else if wrapping {
                // A label too long for its row goes on under it.
                if let Some(last) = options.last_mut() {
                    last.label.push(' ');
                    last.label.push_str(text);
                }
            } else if options.is_empty() {
                above.push(text.to_string());
            } else {
                return View::Other;
            }
        }
        if options.is_empty() || !hint {
            return View::Other;
        }
        View::Prompt(Prompt {
            above,
            options,
            field,
        })
    }
}

/// The row under a prompt's options: `↑/↓ Navigate · …`, or `enter Submit All · esc Back` under
/// an open field.
fn is_hint(text: &str) -> bool {
    text.starts_with("\u{2191}/\u{2193} Navigate") || text.starts_with("enter Submit")
}

/// `text` with its whitespace made single spaces, for comparing what wraps.
fn normalised(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text argument `name` of `call`.
fn argument<'a>(call: &'a CallSeen, name: &str) -> Option<&'a str> {
    call.input.get(name)?.as_str()
}

// ---- permission prompts ----

fn permission(prompt: &Prompt, calls: &[CallSeen]) -> Reading {
    let Some(asks) = prompt.above.last().map(String::as_str) else {
        return Reading::Up;
    };
    let choices: Vec<PermissionChoice> = prompt
        .options
        .iter()
        .filter_map(|option| {
            Some(PermissionChoice {
                id: option.key.clone(),
                label: option.label.clone(),
                effect: classify(&option.label)?,
                text: None,
            })
        })
        .collect();
    if choices.is_empty() {
        return Reading::Up;
    }
    let Some(call) = calls
        .iter()
        .rposition(|call| asks_about(call, asks, &prompt.above))
    else {
        return Reading::Up;
    };
    let call_seen = &calls[call];
    let (category, title) = match call_seen.tool.as_str() {
        RUN_COMMAND => (ToolCategory::Command, argument(call_seen, "CommandLine")),
        WRITE_FILE | EDIT_FILE => (ToolCategory::FileEdit, argument(call_seen, "Path")),
        _ => (ToolCategory::Mcp, None),
    };
    let title = title.map(str::to_string).or_else(|| mcp_tool(call_seen));
    Reading::Shown {
        call,
        prompt: InteractionPrompt::Permission(PermissionPrompt {
            tool: call_seen.tool.clone(),
            category: Some(category),
            title,
            detail: argument(call_seen, "toolSummary").map(str::to_string),
            input: call_seen.input.clone(),
            input_truncated: call_seen.input_truncated,
            choices,
        }),
    }
}

/// What choosing a row does, by its label; `None` for a label not measured. A row that keeps a
/// rule, for the conversation or "(Persist to settings.json)", remembers the answer; its label
/// says which.
fn classify(label: &str) -> Option<ChoiceEffect> {
    if label.starts_with("Yes, and always allow") {
        return Some(ChoiceEffect::AllowRemember);
    }
    if label.starts_with("No, and always deny") {
        return Some(ChoiceEffect::DenyRemember);
    }
    match label {
        "Yes, run command"
        | "Yes, allow creation"
        | "Yes, accept this change"
        | "Yes, allow tool call" => Some(ChoiceEffect::Allow),
        "No, cancel" | "No, deny creation" | "No, reject this change" | "No, deny tool call" => {
            Some(ChoiceEffect::Deny)
        }
        _ => None,
    }
}

/// `<server>/<tool>` of an MCP call, as its prompt names it.
fn mcp_tool(call: &CallSeen) -> Option<String> {
    Some(format!(
        "{}/{}",
        argument(call, "ServerName")?,
        argument(call, "ToolName")?
    ))
}

/// Whether the prompt that asks `asks`, with the rows `above` its options, asks about `call`, by
/// what each prompt shows of its call: a command's own text, the file's path at the head of its
/// diff, the server and tool of an MCP call.
fn asks_about(call: &CallSeen, asks: &str, above: &[String]) -> bool {
    match (call.tool.as_str(), asks) {
        (RUN_COMMAND, ASKS_COMMAND) => {
            let Some(from) = above.iter().position(|row| row == COMMAND_SHOWN) else {
                return false;
            };
            let shown = normalised(&above[from + 1..above.len() - 1].join(" "));
            argument(call, "CommandLine").is_some_and(|command| shows_text(&shown, command))
        }
        (WRITE_FILE, ASKS_CREATION) | (EDIT_FILE, ASKS_EDIT) => {
            argument(call, "Path").is_some_and(|path| {
                above.iter().any(|row| {
                    row.strip_prefix(path)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with("  "))
                })
            })
        }
        (CALL_MCP_TOOL, ASKS_MCP) => mcp_tool(call).is_some_and(|named| above.contains(&named)),
        _ => false,
    }
}

/// Whether `shown` (whitespace made single spaces) is `text`, or its start where it ends in `…`.
fn shows_text(shown: &str, text: &str) -> bool {
    let text = normalised(text);
    match shown.strip_suffix('\u{2026}') {
        Some(start) => !start.trim().is_empty() && text.starts_with(start.trim_end()),
        None => shown == text,
    }
}

// ---- the question form ----

/// The question the form shows now.
struct Form<'a> {
    /// Which question of the call, from 0.
    index: usize,
    total: usize,
    text: String,
    /// The options, `Write-in...` left out.
    options: &'a [OptionRow],
    /// `Write-in...`.
    write_in: &'a OptionRow,
    /// The open field's text.
    field: Option<&'a str>,
}

impl Prompt {
    /// The question the prompt is, where it is the form: `Question k/n: <text>` above the
    /// options (the text wrapping onto the rows under it), `Write-in...` the last of them.
    fn question(&self) -> Option<Form<'_>> {
        let at = self
            .above
            .iter()
            .rposition(|row| row.starts_with("Question "))?;
        let (position, text) = self.above[at]["Question ".len()..].split_once(": ")?;
        let (index, total) = position.split_once('/')?;
        let (index, total): (usize, usize) = (index.parse().ok()?, total.parse().ok()?);
        if index == 0 || index > total {
            return None;
        }
        let (write_in, options) = self.options.split_last()?;
        if write_in.label != WRITE_IN {
            return None;
        }
        let mut text = vec![text];
        text.extend(self.above[at + 1..].iter().map(String::as_str));
        Some(Form {
            index: index - 1,
            total,
            text: normalised(&text.join(" ")),
            options,
            write_in,
            field: self.field.as_deref(),
        })
    }
}

impl Form<'_> {
    fn multi_select(&self) -> bool {
        self.options
            .first()
            .is_some_and(|option| option.ticked().is_some())
    }
}

/// A question of the call's arguments: `questions[]` of `question`, `options` (plain labels) and
/// `is_multi_select`.
struct Asked {
    text: String,
    multi_select: bool,
    options: Vec<String>,
}

fn asked(call: &CallSeen) -> Option<Vec<Asked>> {
    call.input
        .get("questions")?
        .as_array()?
        .iter()
        .map(|question| {
            Some(Asked {
                text: question.get("question")?.as_str()?.to_string(),
                multi_select: question
                    .get("is_multi_select")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                options: question
                    .get("options")?
                    .as_array()?
                    .iter()
                    .map(|option| option.as_str().map(str::to_string))
                    .collect::<Option<_>>()?,
            })
        })
        .collect()
}

fn question(form: &Form<'_>, calls: &[CallSeen]) -> Reading {
    // Only the form as the CLI drew it is published: one half answered (a later question on the
    // screen, an option ticked, the highlight moved, a field open) is left to the terminal, since
    // an answer pressed over it would meet the leftovers.
    let pristine = form.index == 0
        && form.field.is_none()
        && form.options.first().is_some_and(|option| option.focused)
        && form
            .options
            .iter()
            .all(|option| option.ticked() != Some(true));
    if !pristine {
        return Reading::Up;
    }
    let ask_tool = AgentId::ANTIGRAVITY.descriptor();
    let Some((call, questions)) = calls.iter().enumerate().rev().find_map(|(at, call)| {
        let questions = ask_tool
            .is_ask_question_tool(Some(&call.tool))
            .then(|| asked(call))??;
        asks(form, &questions).then_some((at, questions))
    }) else {
        return Reading::Up;
    };
    Reading::Shown {
        call,
        prompt: InteractionPrompt::Question(QuestionPrompt {
            questions: questions
                .into_iter()
                .map(|question| Question {
                    text: question.text,
                    header: None,
                    // A written answer was measured on questions of one choice only.
                    free_text: (!question.multi_select).then_some(FreeText::Instead),
                    multi_select: question.multi_select,
                    options: question
                        .options
                        .into_iter()
                        .map(|label| QuestionOption {
                            label,
                            description: None,
                        })
                        .collect(),
                })
                .collect(),
            dismissible: true,
        }),
    }
}

/// Whether the question the form shows is the first of `questions`, as the form draws it.
fn asks(form: &Form<'_>, questions: &[Asked]) -> bool {
    let Some(first) = questions.first() else {
        return false;
    };
    questions.len() == form.total
        && form.text == normalised(&first.text)
        && first.multi_select == form.multi_select()
        && first.options.len() == form.options.len()
        && first
            .options
            .iter()
            .zip(form.options)
            .enumerate()
            .all(|(at, (label, row))| {
                row.key == (at + 1).to_string() && normalised(row.text()) == normalised(label)
            })
}

fn question_key(form: &Form<'_>, answers: &[QuestionAnswer]) -> NextKey {
    if answers.len() != form.total {
        return NextKey::Stuck("the answers do not fit the questions on the screen");
    }
    let answer = &answers[form.index];
    if let Some(text) = &answer.text {
        if form.multi_select() {
            return NextKey::Stuck("a written answer to a question of several choices");
        }
        return match form.field {
            Some("") => NextKey::Press(Input::Paste(text.clone())),
            Some(_) => NextKey::Press(Input::Enter),
            None => NextKey::Press(Input::Text(form.write_in.key.clone())),
        };
    }
    if form.field.is_some() {
        return NextKey::Stuck("a written answer is open");
    }
    let digit = |at: usize| NextKey::Press(Input::Text(form.options[at].key.clone()));
    if answer
        .selected
        .iter()
        .any(|at| *at as usize >= form.options.len())
    {
        return NextKey::Stuck("the answer picks an option the question does not have");
    }
    if form.multi_select() {
        if answer.selected.is_empty() {
            return NextKey::Stuck("the answer picks no option of the question");
        }
        // Each digit ticks or unticks its option; Enter then takes the ticked ones and goes on.
        let wrong = form.options.iter().enumerate().find(|(at, option)| {
            answer.selected.contains(&(*at as u32)) != (option.ticked() == Some(true))
        });
        return match wrong {
            Some((at, _)) => digit(at),
            None => NextKey::Press(Input::Enter),
        };
    }
    match answer.selected.first() {
        // The digit picks the option and goes on, or sends the answers after the last question.
        Some(at) => digit(*at as usize),
        None => NextKey::Stuck("the answer picks no option of the question"),
    }
}

#[cfg(test)]
mod tests;
