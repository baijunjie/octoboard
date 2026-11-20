//! How Codex's prompts are read and answered (`crate::session::interaction`): the tool approvals
//! of its terminal and the questions of `request_user_input`. Measured on Codex 0.167.1 with the
//! private server and a `--remote` terminal, as Octoboard launches it (`testdata/
//! codex-interaction-captures.md`; `docs/agent-cli-reference.md`, section "Codex").
//!
//! The hooks say which call is up, the screen says what is offered:
//!
//! - **Command and patch approvals** replace the composer with `Would you like to run the
//!   following command?` or `Would you like to make the following edits?`, the three options
//!   numbered and each ending in the key that also takes it, `Press enter to confirm or esc to
//!   cancel` below. A digit takes its option at once. There is no field for text, so no choice
//!   takes any.
//! - **MCP tool approvals** are a one-field form, `Allow the <server> MCP server to run tool
//!   "<tool>"?`, whose four options carry no key letters; a digit selects and submits.
//! - **MCP elicitations** (a form an MCP server raises, `Field 1/1 (1 required unanswered)`) fire
//!   no hook, so nothing is ever matched to one: it is left to the terminal.
//! - **`request_user_input`** questions (Plan mode) are one page per question, the options
//!   numbered with the terminal's own `None of the above` after them. Its `PreToolUse` hook
//!   carries the questions and no `PermissionRequest` precedes it.
//!
//! A prompt is published only when a call of the turn is found that it is about, by the command,
//! the patch's destination, the MCP server and tool, or the questions themselves.

use serde_json::Value;

use super::feed::tool_category;
use crate::platform::names::PATH_STYLE;
use crate::platform::term::Input;
use crate::protocol::{
    ChoiceEffect, FreeText, InteractionPrompt, PermissionChoice, PermissionPrompt, Question,
    QuestionAnswer, QuestionOption, QuestionPrompt, ToolCategory,
};
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};

/// The tool whose input carries the questions of a Plan-mode question.
pub(super) const ASK_TOOL: &str = "request_user_input";

/// The option the terminal adds after every question's own, which takes a note in its place.
const NONE_OF_THE_ABOVE: &str = "None of the above";

const COMMAND_TITLE: &str = "Would you like to run the following command?";
const PATCH_TITLE: &str = "Would you like to make the following edits?";
const APPROVAL_FOOTER: &str = "Press enter to confirm or esc to cancel";
const FORM_FOOTER: &str = "enter to submit | esc to cancel";
const UNANSWERED_TITLE: &str = "Submit with unanswered questions?";
const UNANSWERED_FOOTER: &str = "Press enter to confirm or esc to go back";
/// What a question page's footer starts with, with its notes closed and open.
const QUESTION_FOOTERS: [&str; 2] = [
    "tab to add notes | enter to submit",
    "tab or esc to clear notes | enter to submit",
];
/// What an empty notes field shows.
const NOTES_PLACEHOLDER: &str = "Add notes";

pub(super) struct CodexInteractions;

impl InteractionReader for CodexInteractions {
    /// Only `PreToolUse` starts a call: it names the call (`tool_use_id`). `PermissionRequest`
    /// names none, and its input differs from the call's (a command's carries Codex's
    /// `description`), and it fires in the same millisecond as the call's `PreToolUse`, in either
    /// order (`docs/agent-cli-reference.md`), long before the watch reads a screen, so taking it as
    /// a call too would only keep the same call twice.
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        (event == "PreToolUse").then(|| {
            CallSeen::new(
                payload
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                payload
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                payload.get("tool_input").unwrap_or(&Value::Null),
            )
        })
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        match View::of(screen) {
            View::Gone => Reading::Gone,
            View::Unknown => Reading::Unknown,
            View::Unrecognised | View::Unanswered => Reading::Up,
            View::Approval(approval) => {
                let Some(call) = calls.iter().rposition(|call| approval.is_about(call)) else {
                    return Reading::Up;
                };
                match approval.prompt(&calls[call]) {
                    Some(prompt) => Reading::Shown { call, prompt },
                    None => Reading::Up,
                }
            }
            View::Question(page) => {
                let Some(call) = calls.iter().rposition(|call| page.is_about(call)) else {
                    return Reading::Up;
                };
                // Only the first page of a set none of whose questions is answered, with the
                // highlight on its first option and no note begun, has the whole set to answer.
                // Anything else is someone answering in the terminal, or a press halfway: a later
                // page, a page gone back to (`testdata/codex_interaction_question1_partly_answered.txt`),
                // a highlight moved, an open notes field.
                if page.number != 1
                    || page.unanswered != Some(page.total)
                    || page.highlight != Some(0)
                    || page.notes.is_some()
                {
                    return Reading::Up;
                }
                match question_prompt(&calls[call]) {
                    Some(prompt) => Reading::Shown { call, prompt },
                    None => Reading::Up,
                }
            }
        }
    }

    fn next_key(&self, screen: &str, call: &CallSeen, answer: &Answer) -> NextKey {
        match (View::of(screen), answer) {
            (View::Approval(approval), _) if !approval.is_about(call) => {
                NextKey::Stuck("the prompt on the screen is not the one answered")
            }
            (View::Question(page), _) if !page.is_about(call) => {
                NextKey::Stuck("the questions on the screen are not the ones answered")
            }
            (View::Approval(approval), Answer::Choice { id, text: None }) => {
                match approval.choices.iter().position(|choice| choice.id == *id) {
                    Some(at) => NextKey::Press(approval.keys[at].clone()),
                    None => NextKey::Stuck("the prompt does not offer that choice"),
                }
            }
            (View::Approval(_), Answer::Choice { .. }) => {
                NextKey::Stuck("a Codex approval takes no text")
            }
            (View::Question(_) | View::Unanswered, Answer::Dismiss) => {
                // Esc interrupts the turn, which is what declining the questions comes to
                // (`docs/agent-cli-reference.md`); on the unanswered page it goes back a page.
                NextKey::Press(Input::Escape)
            }
            (View::Question(page), Answer::Questions(answers)) => {
                question_key(&page, call, answers)
            }
            (View::Unanswered, Answer::Questions(_)) => {
                NextKey::Stuck("Codex asks whether to submit with unanswered questions")
            }
            (View::Gone | View::Unknown, _) => NextKey::Stuck("no prompt on the screen"),
            _ => NextKey::Stuck("the screen shows a prompt this answer does not fit"),
        }
    }
}

/// What the screen shows, by its last row.
enum View {
    /// The composer is back with no prompt.
    Gone,
    /// Neither a prompt nor the composer.
    Unknown,
    /// A prompt of a kind this reader does not publish: an elicitation, another confirmation, an
    /// approval laid out in a way not measured.
    Unrecognised,
    Approval(Approval),
    Question(Page),
    /// The page a question set submitted with questions left asks to confirm.
    Unanswered,
}

impl View {
    fn of(screen: &str) -> View {
        let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
        let Some(last) = rows.iter().rposition(|row| !row.trim().is_empty()) else {
            return View::Unknown;
        };
        let footer = rows[last].trim();
        let above = &rows[..last];
        if footer == APPROVAL_FOOTER {
            return Approval::command_or_patch(above).map_or(View::Unrecognised, View::Approval);
        }
        if footer == FORM_FOOTER {
            return Approval::mcp(above).map_or(View::Unrecognised, View::Approval);
        }
        if footer == UNANSWERED_FOOTER {
            let near = &above[above.len().saturating_sub(8)..];
            return if near.iter().any(|row| row.trim() == UNANSWERED_TITLE) {
                View::Unanswered
            } else {
                View::Unknown
            };
        }
        if QUESTION_FOOTERS
            .iter()
            .any(|prefix| footer.starts_with(prefix))
        {
            return Page::read(above).map_or(View::Unrecognised, View::Question);
        }
        if super::screen::composer(screen, PATH_STYLE).is_some() {
            View::Gone
        } else {
            View::Unknown
        }
    }
}

/// One numbered option row.
struct Row {
    number: u8,
    highlighted: bool,
    /// The text after the number, its continuation rows joined.
    text: String,
}

/// `row` as an option row: an optional `›` marking the highlight, a number, a full stop.
fn option_row(row: &str) -> Option<Row> {
    let row = row.trim();
    let (highlighted, rest) = match row.strip_prefix('›') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, row),
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let number = rest[..digits].parse().ok().filter(|_| digits <= 2)?;
    let text = rest[digits..].strip_prefix(". ")?;
    Some(Row {
        number,
        highlighted,
        text: text.trim().to_string(),
    })
}

/// The option rows that start at the first one of `rows`, up to the first blank row, a `›`-led row
/// that is no option or does not continue the numbering (the notes field) or the end, with the rows between them that are no
/// option's own taken as the previous one's continuation; and how many rows of `rows` that took.
fn option_rows(rows: &[&str]) -> (Vec<Row>, usize) {
    let mut found: Vec<Row> = Vec::new();
    let mut consumed = 0;
    for row in rows {
        let text = row.trim();
        if text.is_empty() {
            break;
        }
        match option_row(row) {
            // An option continues the numbering; a `›` row that does not (a note that begins
            // `1. Use red`) is the notes field.
            // Only one option carries the highlight, so a `›` row after the highlighted one is
            // the notes field however it reads.
            Some(option)
                if usize::from(option.number) == found.len() + 1
                    && !(option.highlighted && found.iter().any(|row| row.highlighted)) =>
            {
                found.push(option)
            }
            _ if text.starts_with('›') => break,
            _ => {
                if let Some(previous) = found.last_mut() {
                    previous.text.push(' ');
                    previous.text.push_str(text);
                }
            }
        }
        consumed += 1;
    }
    (found, consumed)
}

/// `text` without whitespace, for comparing what the screen wraps with what a call holds.
fn squashed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Whether the screen's `shown` is `whole`, the call's text, which may have been cut.
fn same_text(shown: &str, whole: &str, cut: bool) -> bool {
    let (shown, whole) = (squashed(shown), squashed(whole));
    if cut {
        shown.starts_with(&whole)
    } else {
        shown == whole
    }
}

/// A tool approval on the screen.
struct Approval {
    what: What,
    choices: Vec<PermissionChoice>,
    /// The key that takes each choice, in the order of `choices`.
    keys: Vec<Input>,
}

enum What {
    Command {
        command: String,
        reason: Option<String>,
    },
    Patch {
        destination: String,
        description: Option<String>,
    },
    Mcp {
        server: String,
        tool: String,
    },
}

impl Approval {
    /// The approval `above`, the rows over the footer, shows of a command or an edit.
    fn command_or_patch(above: &[&str]) -> Option<Approval> {
        let title = above
            .iter()
            .rposition(|row| matches!(row.trim(), COMMAND_TITLE | PATCH_TITLE))?;
        let patch = above[title].trim() == PATCH_TITLE;
        let body = &above[title + 1..];
        let first = body.iter().position(|row| option_row(row).is_some())?;
        let (meta, options) = (&body[..first], &body[first..]);
        let (choices, keys) = options_with_keys(&option_rows(options).0);
        let what = if patch {
            let destination = block(meta, "Destination:")?;
            What::Patch {
                destination,
                description: block(meta, "Description:"),
            }
        } else {
            let start = meta.iter().position(|row| row.trim().starts_with("$ "))?;
            let mut command: Vec<&str> = meta[start..].iter().map(|row| row.trim()).collect();
            command[0] = command[0].trim_start_matches('$').trim_start();
            while command.last().is_some_and(|row| row.is_empty()) {
                command.pop();
            }
            What::Command {
                command: command.join("\n"),
                reason: block(meta, "Reason:"),
            }
        };
        Some(Approval {
            what,
            choices,
            keys,
        })
    }

    /// The approval `above` shows of an MCP tool.
    fn mcp(above: &[&str]) -> Option<Approval> {
        let title = above.iter().rposition(|row| {
            let row = row.trim();
            row.starts_with("Allow the ") && row.ends_with("\"?")
        })?;
        let line = above[title].trim();
        let line = line.strip_prefix("Allow the ")?.strip_suffix("\"?")?;
        let (server, tool) = line.split_once(" MCP server to run tool \"")?;
        let options = &above[title + 1..];
        let first = options.iter().position(|row| option_row(row).is_some())?;
        // An option's description follows its label after two blanks or more; rows between the
        // options (a wrapped description) are not options.
        let rows = options[first..]
            .iter()
            .filter_map(|row| option_row(row))
            .collect::<Vec<_>>();
        let (choices, keys) = rows
            .iter()
            .filter_map(|row| {
                let label = row.text.split("  ").next()?.trim();
                let effect = match label {
                    "Allow" => ChoiceEffect::Allow,
                    "Allow for this session" | "Always allow" => ChoiceEffect::AllowRemember,
                    "Cancel" => ChoiceEffect::Deny,
                    _ => return None,
                };
                // The digit selects and submits (`docs/agent-cli-reference.md`).
                Some((
                    PermissionChoice {
                        id: row.number.to_string(),
                        label: label.to_string(),
                        effect,
                        text: None,
                    },
                    Input::Text(row.number.to_string()),
                ))
            })
            .unzip();
        Some(Approval {
            what: What::Mcp {
                server: server.to_string(),
                tool: tool.to_string(),
            },
            choices,
            keys,
        })
    }

    /// Whether `call` is the call this approval is about.
    fn is_about(&self, call: &CallSeen) -> bool {
        let input_text = |field: &str| call.input.get(field).and_then(Value::as_str);
        match &self.what {
            What::Command { command, .. } => {
                call.tool == "Bash"
                    && input_text("command")
                        .is_some_and(|whole| same_text(command, whole, call.input_truncated))
            }
            What::Patch { destination, .. } => {
                call.tool == "apply_patch"
                    && input_text("command").is_some_and(|patch| {
                        let destination = squashed(destination);
                        super::conversation::patch_paths(patch).iter().any(|path| {
                            let path = squashed(path.trim_start_matches("./"));
                            let path = path.trim_start_matches("../");
                            // The path is the destination's tail from a separator on, or all of
                            // it.
                            !path.is_empty()
                                && destination.strip_suffix(path).is_some_and(|head| {
                                    head.is_empty() || head.ends_with(['/', '\\'])
                                })
                        })
                    })
            }
            What::Mcp { server, tool } => {
                let prefix = crate::providers::AgentId::CODEX
                    .descriptor()
                    .tool_name_prefix;
                // Codex names a tool for its model with the characters a name may hold; a server
                // or tool name with others is read as the same name with `_` for each of them
                // (unmeasured).
                let named = |name: &str| -> String {
                    name.chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                        .collect()
                };
                named(&call.tool) == named(&format!("{prefix}{server}__{tool}"))
            }
        }
    }

    /// The prompt this approval is for `call`, or `None` when it does not offer both a way to
    /// allow and a way to refuse.
    fn prompt(&self, call: &CallSeen) -> Option<InteractionPrompt> {
        // A prompt that offers no way to allow, or none to refuse, is left to the terminal.
        let has = |effects: &[ChoiceEffect]| {
            self.choices
                .iter()
                .any(|choice| effects.contains(&choice.effect))
        };
        if !has(&[ChoiceEffect::Allow, ChoiceEffect::AllowRemember]) || !has(&[ChoiceEffect::Deny])
        {
            return None;
        }
        let (title, detail) = match &self.what {
            What::Command { command, reason } => (command.clone(), reason.clone()),
            What::Patch {
                destination,
                description,
            } => (destination.clone(), description.clone()),
            // As the feed titles an MCP call.
            What::Mcp { server, tool } => (format!("{server}/{tool}"), None),
        };
        Some(InteractionPrompt::Permission(PermissionPrompt {
            tool: call.tool.clone(),
            category: Some(hook_category(&call.tool)),
            title: Some(title),
            detail,
            input: call.input.clone(),
            input_truncated: call.input_truncated,
            choices: self.choices.clone(),
        }))
    }
}

/// What the rows of a command or edit approval above its options start with.
const LABELS: [&str; 5] = [
    "Environment:",
    "Reason:",
    "Description:",
    "Destination:",
    "$ ",
];

/// What kind of tool a hook names: `Bash` is every command, and an MCP tool is
/// `mcp__<server>__<tool>`, which only the hooks say (a rollout's MCP calls are items of their
/// own, categorised by the feed as MCP); any other name is read as the feed reads it.
fn hook_category(tool: &str) -> ToolCategory {
    if tool == "Bash" {
        ToolCategory::Command
    } else if tool.starts_with(
        crate::providers::AgentId::CODEX
            .descriptor()
            .tool_name_prefix,
    ) {
        ToolCategory::Mcp
    } else {
        tool_category(tool)
    }
}

/// The text of the block that starts at the row beginning with `label`, from the rest of that row
/// to the next blank row or label; a row is joined to the one before without a space where the label
/// belongs to a path that the terminal cut mid-word (`Destination:`), with one otherwise.
fn block(meta: &[&str], label: &str) -> Option<String> {
    let start = meta.iter().position(|row| row.trim().starts_with(label))?;
    let mut parts = vec![meta[start].trim().trim_start_matches(label).trim()];
    parts.extend(
        meta[start + 1..]
            .iter()
            .take_while(|row| {
                let row = row.trim();
                !row.is_empty() && !LABELS.iter().any(|label| row.starts_with(label))
            })
            .map(|row| row.trim()),
    );
    let parts: Vec<&str> = parts.into_iter().filter(|part| !part.is_empty()).collect();
    let joined = if label == "Destination:" {
        parts.concat()
    } else {
        parts.join(" ")
    };
    (!joined.is_empty()).then_some(joined)
}

/// The options of a command or edit approval, each with the key its row ends in, as choices: `y`
/// allows, `p` and `a` allow and remember a rule, `esc` refuses; and the key that takes each. The
/// letters are pressed as they are (each measured on 0.167.1), and a refusal as its digit, since
/// an Esc is also what interrupts a turn. A row ending in any other key is left out.
fn options_with_keys(rows: &[Row]) -> (Vec<PermissionChoice>, Vec<Input>) {
    rows.iter()
        .filter_map(|row| {
            let (label, key) = row.text.strip_suffix(')')?.rsplit_once(" (")?;
            let (effect, press) = match key {
                "y" => (ChoiceEffect::Allow, Input::Text("y".to_string())),
                "p" | "a" => (ChoiceEffect::AllowRemember, Input::Text(key.to_string())),
                "esc" => (ChoiceEffect::Deny, Input::Text(row.number.to_string())),
                _ => return None,
            };
            Some((
                PermissionChoice {
                    id: row.number.to_string(),
                    label: label.trim().to_string(),
                    effect,
                    text: None,
                },
                press,
            ))
        })
        .unzip()
}

/// One page of a `request_user_input` question set on the screen.
struct Page {
    /// Which question it is, from 1, and of how many.
    number: usize,
    total: usize,
    /// How many questions the header counts as unanswered, where it says.
    unanswered: Option<usize>,
    text: String,
    /// The options, the terminal's own `None of the above` last, as (label, description).
    options: Vec<(String, String)>,
    /// The index of the highlighted option.
    highlight: Option<usize>,
    /// What the notes field holds, `Some("")` when it is open and empty, `None` when it is not
    /// open.
    notes: Option<String>,
}

impl Page {
    fn read(above: &[&str]) -> Option<Page> {
        let header = above.iter().rposition(|row| {
            row.trim().starts_with("Question ") && question_position(row.trim()).is_some()
        })?;
        let (number, total) = question_position(above[header].trim())?;
        let body = &above[header + 1..];
        let first = body.iter().position(|row| option_row(row).is_some())?;
        let text = body[..first]
            .iter()
            .map(|row| row.trim())
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let (rows, consumed) = option_rows(&body[first..]);
        let highlight = rows.iter().position(|row| row.highlighted);
        let options = rows
            .iter()
            .map(|row| match row.text.split_once("  ") {
                Some((label, description)) => {
                    (label.trim().to_string(), description.trim().to_string())
                }
                None => (row.text.clone(), String::new()),
            })
            .collect();
        // The notes field is a row of its own after the options, marked like the highlight.
        let after = &body[first + consumed..];
        let notes = after
            .iter()
            .position(|row| row.trim().starts_with('›'))
            .map(|start| {
                let lines: Vec<&str> = after[start..]
                    .iter()
                    .take_while(|row| !row.trim().is_empty())
                    .map(|row| row.trim().trim_start_matches('›').trim())
                    .collect();
                let text = lines.join("\n");
                if text == NOTES_PLACEHOLDER {
                    String::new()
                } else {
                    text
                }
            });
        Some(Page {
            number,
            total,
            unanswered: unanswered_count(above[header].trim()),
            text,
            options,
            highlight,
            notes,
        })
    }

    /// Whether `call` is the question set this page belongs to.
    fn is_about(&self, call: &CallSeen) -> bool {
        let Some(questions) = call
            .input
            .get("questions")
            .and_then(Value::as_array)
            .filter(|_| call.tool == ASK_TOOL)
        else {
            return false;
        };
        let Some(question) = questions.get(self.number.wrapping_sub(1)) else {
            return false;
        };
        let labels: Vec<&str> = question["options"]
            .as_array()
            .map(|options| options.iter().filter_map(|o| o["label"].as_str()).collect())
            .unwrap_or_default();
        questions.len() == self.total
            && question["question"]
                .as_str()
                .is_some_and(|text| squashed(text) == squashed(&self.text))
            && self.options.len() == labels.len() + 1
            && self
                .options
                .last()
                .is_some_and(|(label, _)| label == NONE_OF_THE_ABOVE)
            && labels
                .iter()
                .zip(&self.options)
                .all(|(label, (shown, _))| squashed(label) == squashed(shown))
    }
}

/// `n` of the `(n unanswered)` a `Question k/n` row ends in.
fn unanswered_count(row: &str) -> Option<usize> {
    row.rsplit_once('(')?
        .1
        .strip_suffix(" unanswered)")?
        .parse()
        .ok()
}

/// `k` and `n` of a `Question k/n (…)` row.
fn question_position(row: &str) -> Option<(usize, usize)> {
    let counts = row.strip_prefix("Question ")?.split_whitespace().next()?;
    let (number, total) = counts.split_once('/')?;
    let (number, total) = (number.parse().ok()?, total.parse().ok()?);
    (number >= 1 && number <= total).then_some((number, total))
}

/// The questions `call` holds, as a prompt. The terminal's `None of the above` is no option of
/// theirs: a written answer stands for it, and goes in the notes field. A written answer with an
/// option goes in the notes under that option. Esc interrupts the turn, which is how they are
/// declined.
fn question_prompt(call: &CallSeen) -> Option<InteractionPrompt> {
    let questions = call.input.get("questions")?.as_array()?;
    let questions = questions
        .iter()
        .map(|question| {
            let options = question["options"].as_array()?;
            Some(Question {
                text: question["question"].as_str()?.to_string(),
                header: question["header"].as_str().map(str::to_string),
                multi_select: false,
                options: options
                    .iter()
                    .map(|option| {
                        Some(QuestionOption {
                            label: option["label"].as_str()?.to_string(),
                            description: option["description"].as_str().map(str::to_string),
                        })
                    })
                    .collect::<Option<_>>()?,
                free_text: Some(FreeText::With),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (!questions.is_empty()).then_some(InteractionPrompt::Question(QuestionPrompt {
        questions,
        dismissible: true,
    }))
}

/// The next key towards `answers` on the question page `page`. An option alone is its digit,
/// which answers and goes on to the next question, or submits the last. With a note, the key
/// sequence is: Down to the option the note belongs to (`None of the above` for a note alone),
/// Tab to open the note, a paste of its text, and Enter, which answers and goes on as the digit
/// does. A highlight past the option wanted cannot be moved back (there is no Up key to send), and
/// the page that asks to submit with questions unanswered is never to be reached, since every
/// question is answered in turn: both are `Stuck`.
fn question_key(page: &Page, call: &CallSeen, answers: &[QuestionAnswer]) -> NextKey {
    let Some(questions) = call.input.get("questions").and_then(Value::as_array) else {
        return NextKey::Stuck("the call holds no questions");
    };
    if questions.len() != page.total || answers.len() != page.total {
        return NextKey::Stuck("the answers do not match the questions on the screen");
    }
    let answer = &answers[page.number - 1];
    let chosen = answer.selected.first().map(|index| *index as usize);
    let Some(text) = &answer.text else {
        let Some(chosen) = chosen else {
            return NextKey::Stuck("the question has no option chosen");
        };
        if page.notes.is_some() {
            // Tab closes the notes field again.
            return NextKey::Press(Input::Ctrl('i'));
        }
        return match u8::try_from(chosen + 1).ok().filter(|number| *number <= 9) {
            Some(number) => NextKey::Press(Input::Text(number.to_string())),
            None => NextKey::Stuck("the option has no digit"),
        };
    };
    let wanted = chosen.unwrap_or(page.options.len() - 1);
    let Some(highlight) = page.highlight else {
        return NextKey::Stuck("no option is highlighted");
    };
    if highlight != wanted {
        return if page.notes.is_none() && highlight < wanted {
            NextKey::Press(Input::Down)
        } else {
            NextKey::Stuck("the highlight is not where the note belongs")
        };
    }
    match &page.notes {
        None => NextKey::Press(Input::Ctrl('i')),
        Some(notes) if notes.is_empty() => NextKey::Press(Input::Paste(text.clone())),
        Some(_) => NextKey::Press(Input::Enter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(capture: &[u8]) -> String {
        let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
        screen.process(capture);
        screen.visible_text()
    }

    /// The `PreToolUse` calls of the real hook payloads (`testdata/codex-interaction-captures.md`).
    fn calls() -> Vec<CallSeen> {
        include_str!("../../../testdata/codex_interaction_hooks.jsonl")
            .lines()
            .filter_map(|line| {
                let line: Value = serde_json::from_str(line).unwrap();
                CodexInteractions.call_started(line["event"].as_str()?, &line["payload"])
            })
            .collect()
    }

    fn shown(screen: &str) -> (CallSeen, InteractionPrompt) {
        let calls = calls();
        match CodexInteractions.read(screen, &calls) {
            Reading::Shown { call, prompt } => (calls[call].clone(), prompt),
            other => panic!("not shown: {other:?}\n{screen}"),
        }
    }

    fn permission(prompt: InteractionPrompt) -> PermissionPrompt {
        match prompt {
            InteractionPrompt::Permission(permission) => permission,
            other => panic!("{other:?}"),
        }
    }

    fn effects(prompt: &PermissionPrompt) -> Vec<(&str, ChoiceEffect)> {
        prompt
            .choices
            .iter()
            .map(|choice| (choice.id.as_str(), choice.effect))
            .collect()
    }

    #[test]
    fn a_command_approval_is_matched_to_its_call_and_offers_three_choices() {
        let screen = replay(include_bytes!(
            "../../../testdata/codex_interaction_command.bin"
        ));
        let (call, prompt) = shown(&screen);
        assert_eq!(call.tool, "Bash");
        let prompt = permission(prompt);
        assert_eq!(prompt.category, Some(ToolCategory::Command));
        assert_eq!(
            prompt.title.as_deref(),
            Some("touch probe_cmd_out.txt && echo created")
        );
        assert_eq!(
            prompt.detail.as_deref(),
            Some("Do you want to let the probe create probe_cmd_out.txt?")
        );
        assert_eq!(
            effects(&prompt),
            [
                ("1", ChoiceEffect::Allow),
                ("2", ChoiceEffect::AllowRemember),
                ("3", ChoiceEffect::Deny)
            ]
        );
        assert_eq!(prompt.choices[0].label, "Yes, proceed");
        assert!(prompt.choices.iter().all(|choice| choice.text.is_none()));
    }

    #[test]
    fn a_command_approval_without_a_matching_call_is_left_to_the_terminal() {
        let screen = replay(include_bytes!(
            "../../../testdata/codex_interaction_command.bin"
        ));
        assert_eq!(CodexInteractions.read(&screen, &[]), Reading::Up);
        let other = CallSeen::new(None, "Bash", &serde_json::json!({"command": "ls"}));
        assert_eq!(CodexInteractions.read(&screen, &[other]), Reading::Up);
    }

    #[test]
    fn the_composer_back_after_an_answer_is_gone() {
        for capture in [
            &include_bytes!("../../../testdata/codex_interaction_command_granted.bin")[..],
            include_bytes!("../../../testdata/codex_interaction_mcp_granted.bin"),
        ] {
            assert_eq!(
                CodexInteractions.read(&replay(capture), &calls()),
                Reading::Gone
            );
        }
        assert_eq!(CodexInteractions.read("", &calls()), Reading::Unknown);
    }

    #[test]
    fn a_patch_approval_is_matched_by_its_destination() {
        let screen = replay(include_bytes!(
            "../../../testdata/codex_interaction_patch.bin"
        ));
        let (call, prompt) = shown(&screen);
        assert_eq!(call.tool, "apply_patch");
        let prompt = permission(prompt);
        assert_eq!(prompt.category, Some(ToolCategory::FileEdit));
        assert!(prompt
            .title
            .unwrap()
            .ends_with("probe-codex/outside_probe.txt"));
        assert_eq!(prompt.detail.as_deref(), Some("Apply proposed file edits"));
        assert_eq!(
            effects(&permission(shown(&screen).1)),
            [
                ("1", ChoiceEffect::Allow),
                ("2", ChoiceEffect::AllowRemember),
                ("3", ChoiceEffect::Deny)
            ]
        );
    }

    #[test]
    fn a_patch_path_must_start_at_a_separator_of_the_destination() {
        let screen = replay(include_bytes!(
            "../../../testdata/codex_interaction_patch.bin"
        ));
        let patch = |path: &str| {
            let text = format!("*** Begin Patch\n*** Add File: {path}\n+x\n*** End Patch\n");
            CallSeen::new(None, "apply_patch", &serde_json::json!({ "command": text }))
        };
        assert_eq!(
            CodexInteractions.read(&screen, &[patch("../utside_probe.txt")]),
            Reading::Up
        );
        assert!(matches!(
            CodexInteractions.read(&screen, &[patch("../outside_probe.txt")]),
            Reading::Shown { .. }
        ));
    }

    #[test]
    fn an_mcp_approval_is_matched_by_server_and_tool() {
        let screen = replay(include_bytes!(
            "../../../testdata/codex_interaction_mcp.bin"
        ));
        let (call, prompt) = shown(&screen);
        assert_eq!(call.tool, "mcp__stubmcp__probe_tool");
        let prompt = permission(prompt);
        assert_eq!(prompt.category, Some(ToolCategory::Mcp));
        assert_eq!(prompt.title.as_deref(), Some("stubmcp/probe_tool"));
        assert_eq!(
            prompt
                .choices
                .iter()
                .map(|choice| (choice.id.as_str(), choice.label.as_str(), choice.effect))
                .collect::<Vec<_>>(),
            [
                ("1", "Allow", ChoiceEffect::Allow),
                ("2", "Allow for this session", ChoiceEffect::AllowRemember),
                ("3", "Always allow", ChoiceEffect::AllowRemember),
                ("4", "Cancel", ChoiceEffect::Deny)
            ]
        );
    }

    #[test]
    fn an_mcp_elicitation_fires_no_hook_and_is_left_to_the_terminal() {
        let text = include_str!("../../../testdata/codex_interaction_elicitation.txt");
        assert_eq!(CodexInteractions.read(text, &calls()), Reading::Up);
    }

    #[test]
    fn categories_agree_with_the_feeds() {
        for (tool, category) in [
            ("Bash", ToolCategory::Command),
            ("exec", ToolCategory::Command),
            ("apply_patch", ToolCategory::FileEdit),
            ("mcp__a__b", ToolCategory::Mcp),
            (ASK_TOOL, ToolCategory::Other),
        ] {
            assert_eq!(hook_category(tool), category, "{tool}");
        }
    }

    #[test]
    fn approvals_are_answered_with_their_digit_and_never_with_text() {
        let screen = replay(include_bytes!(
            "../../../testdata/codex_interaction_command.bin"
        ));
        let call = calls()[0].clone();
        let choice = |id: &str, text: Option<&str>| Answer::Choice {
            id: id.to_string(),
            text: text.map(str::to_string),
        };
        // The measured letters take the allowing rows; a refusal is its digit.
        for (id, key) in [("1", "y"), ("2", "p"), ("3", "3")] {
            assert_eq!(
                CodexInteractions.next_key(&screen, &call, &choice(id, None)),
                NextKey::Press(Input::Text(key.to_string())),
                "{id}"
            );
        }
        let patch = replay(include_bytes!(
            "../../../testdata/codex_interaction_patch.bin"
        ));
        let patch_call = calls()
            .into_iter()
            .find(|call| call.tool == "apply_patch")
            .unwrap();
        assert_eq!(
            CodexInteractions.next_key(&patch, &patch_call, &choice("2", None)),
            NextKey::Press(Input::Text("a".to_string()))
        );
        let mcp = replay(include_bytes!(
            "../../../testdata/codex_interaction_mcp.bin"
        ));
        let mcp_call = calls()
            .into_iter()
            .find(|call| call.tool == "mcp__stubmcp__probe_tool")
            .unwrap();
        assert_eq!(
            CodexInteractions.next_key(&mcp, &mcp_call, &choice("3", None)),
            NextKey::Press(Input::Text("3".to_string()))
        );
        // A prompt that is not the call's.
        assert!(matches!(
            CodexInteractions.next_key(&screen, &patch_call, &choice("1", None)),
            NextKey::Stuck(_)
        ));
        assert!(matches!(
            CodexInteractions.next_key(&screen, &call, &choice("9", None)),
            NextKey::Stuck(_)
        ));
        assert!(matches!(
            CodexInteractions.next_key(&screen, &call, &choice("1", Some("why"))),
            NextKey::Stuck(_)
        ));
        let gone = replay(include_bytes!(
            "../../../testdata/codex_interaction_command_granted.bin"
        ));
        assert!(matches!(
            CodexInteractions.next_key(&gone, &call, &choice("1", None)),
            NextKey::Stuck(_)
        ));
    }

    fn ask_calls() -> Vec<CallSeen> {
        calls()
            .into_iter()
            .filter(|call| call.tool == ASK_TOOL)
            .collect()
    }

    /// The one-question call that was asked in Plan mode and answered (its `PostToolUse` is in the
    /// fixture); the fixture's first `request_user_input` was rejected in Default mode.
    fn plan_call() -> CallSeen {
        ask_calls()
            .into_iter()
            .find(|call| call.id.as_deref() == Some("call_resp_1795104813291"))
            .unwrap()
    }

    #[test]
    fn a_question_is_published_from_the_call_with_a_note_beside_the_options() {
        let screen = replay(include_bytes!(
            "../../../testdata/codex_interaction_question.bin"
        ));
        let (_, prompt) = shown(&screen);
        let InteractionPrompt::Question(prompt) = prompt else {
            panic!("{prompt:?}")
        };
        assert!(prompt.dismissible);
        assert_eq!(prompt.questions.len(), 1);
        let question = &prompt.questions[0];
        assert_eq!(question.text, "Which color should the probe use?");
        assert_eq!(question.header.as_deref(), Some("Color"));
        assert!(!question.multi_select);
        assert_eq!(question.free_text, Some(FreeText::With));
        let labels: Vec<_> = question.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, ["Red (Recommended)", "Blue"]);
        assert_eq!(
            question.options[0].description.as_deref(),
            Some("The warm one.")
        );
    }

    #[test]
    fn later_pages_notes_and_the_unanswered_page_are_up() {
        for text in [
            include_str!("../../../testdata/codex_interaction_notes_open.txt"),
            include_str!("../../../testdata/codex_interaction_notes_pasted.txt"),
            include_str!("../../../testdata/codex_interaction_unanswered.txt"),
        ] {
            assert_eq!(
                CodexInteractions.read(text, &calls()),
                Reading::Up,
                "{text}"
            );
        }
        // Pages 2 and 3 of the three-question set match their call but are not published.
        for text in [
            include_str!("../../../testdata/codex_interaction_question2.txt"),
            include_str!("../../../testdata/codex_interaction_question3.txt"),
        ] {
            assert_eq!(CodexInteractions.read(text, &calls()), Reading::Up);
        }
    }

    /// Page 1 gone back to after an answer on it (`(2 unanswered)`, the highlight on option 2):
    /// it matches its call, and is still not the whole set to answer.
    #[test]
    fn a_partly_answered_first_page_is_up() {
        let text =
            include_str!("../../../testdata/codex_interaction_question1_partly_answered.txt");
        let call = ask3();
        let page = View::of(text);
        assert!(matches!(&page, View::Question(page) if page.is_about(&call)));
        assert_eq!(CodexInteractions.read(text, &calls()), Reading::Up);
    }

    #[test]
    fn wrapped_option_rows_are_counted_and_the_notes_field_is_not_an_option() {
        let rows = [
            "  › 1. Red  a description that",
            "      wraps",
            "    2. Blue  short",
            "",
            "  › Add notes",
        ];
        let (found, consumed) = option_rows(&rows);
        assert_eq!(consumed, 3);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].text, "Red  a description that wraps");
        let (found, consumed) = option_rows(&["    1. Red", "  › Add notes"]);
        assert_eq!((found.len(), consumed), (1, 1));
        // A note that looks like an option row.
        let (found, consumed) = option_rows(&["    1. Red", "    2. Blue", "  › 1. Use red"]);
        assert_eq!((found.len(), consumed), (2, 2));
        // One past the last option, after the highlighted one.
        let (found, consumed) = option_rows(&[
            "  › 1. Red",
            "    2. Blue",
            "    3. None of the above",
            "  › 4. Use green",
        ]);
        assert_eq!((found.len(), consumed), (3, 3));
    }

    fn ask3() -> CallSeen {
        ask_calls()
            .into_iter()
            .find(|call| call.input["questions"].as_array().unwrap().len() == 3)
            .unwrap()
    }

    fn answer(selected: &[u32], text: Option<&str>) -> QuestionAnswer {
        QuestionAnswer {
            selected: selected.to_vec(),
            text: text.map(str::to_string),
        }
    }

    fn key(screen: &str, call: &CallSeen, answers: Vec<QuestionAnswer>) -> NextKey {
        CodexInteractions.next_key(screen, call, &Answer::Questions(answers))
    }

    #[test]
    fn an_option_answer_is_its_digit_on_each_page() {
        let answers = vec![answer(&[1], None), answer(&[0], None), answer(&[0], None)];
        let call = ask3();
        let page2 = include_str!("../../../testdata/codex_interaction_question2.txt");
        let page3 = include_str!("../../../testdata/codex_interaction_question3.txt");
        assert_eq!(
            key(page2, &call, answers.clone()),
            NextKey::Press(Input::Text("1".to_string()))
        );
        let again = vec![answer(&[1], None), answer(&[2], None), answer(&[1], None)];
        assert_eq!(
            key(page2, &call, again.clone()),
            NextKey::Press(Input::Text("3".to_string()))
        );
        assert_eq!(
            key(page3, &call, again),
            NextKey::Press(Input::Text("2".to_string()))
        );
    }

    #[test]
    fn a_note_is_typed_through_down_tab_paste_and_enter() {
        let call = plan_call();
        let first = replay(include_bytes!(
            "../../../testdata/codex_interaction_question.bin"
        ));
        let alone = vec![answer(&[], Some("Green please"))];
        // `None of the above` is the third row: two Downs from the first.
        assert_eq!(
            key(&first, &call, alone.clone()),
            NextKey::Press(Input::Down)
        );
        let on_none = include_str!("../../../testdata/codex_interaction_on_none.txt");
        assert_eq!(
            key(on_none, &call, alone.clone()),
            NextKey::Press(Input::Ctrl('i'))
        );
        let open = include_str!("../../../testdata/codex_interaction_notes_open.txt");
        assert_eq!(
            key(open, &call, alone.clone()),
            NextKey::Press(Input::Paste("Green please".to_string()))
        );
        let pasted = include_str!("../../../testdata/codex_interaction_notes_pasted.txt");
        assert_eq!(key(pasted, &call, alone), NextKey::Press(Input::Enter));
        // A note under an option: the highlight is already on it, so Tab opens the field.
        assert_eq!(
            key(&first, &call, vec![answer(&[0], Some("why"))]),
            NextKey::Press(Input::Ctrl('i'))
        );
        // A highlight past the wanted option cannot be moved back.
        assert!(matches!(
            key(on_none, &call, vec![answer(&[0], Some("why"))]),
            NextKey::Stuck(_)
        ));
    }

    #[test]
    fn the_unanswered_page_is_never_gone_through_and_a_dismissal_is_escape() {
        let unanswered = include_str!("../../../testdata/codex_interaction_unanswered.txt");
        let call = ask3();
        assert!(matches!(
            key(unanswered, &call, vec![answer(&[0], None); 3]),
            NextKey::Stuck(_)
        ));
        let first = replay(include_bytes!(
            "../../../testdata/codex_interaction_question.bin"
        ));
        assert_eq!(
            CodexInteractions.next_key(unanswered, &call, &Answer::Dismiss),
            NextKey::Press(Input::Escape)
        );
        assert_eq!(
            CodexInteractions.next_key(&first, &plan_call(), &Answer::Dismiss),
            NextKey::Press(Input::Escape)
        );
        // Not the questions of the call answered.
        assert!(matches!(
            CodexInteractions.next_key(&first, &call, &Answer::Dismiss),
            NextKey::Stuck(_)
        ));
        assert!(matches!(
            key(&first, &plan_call(), vec![answer(&[0], None); 3]),
            NextKey::Stuck(_)
        ));
    }
}
