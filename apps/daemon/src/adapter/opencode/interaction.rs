//! OpenCode's prompts to the user, read off its plugin's events and its screen and answered with
//! keys (`crate::session::interaction`). Measured on 1.19.6 (`docs/agent-cli-reference.md`,
//! "OpenCode", "Capabilities the released agents have", and `testdata/opencode-captures.md`).
//!
//! - **Calls.** OpenCode has no hooks; the plugin forwards `permission.asked` and `question.asked`
//!   once a prompt has stayed open, with the call's id (`call_id`), the permission's name as the
//!   tool (`bash`, `edit`, `external_directory`, …) or `question`, and what it asks (`input`): a
//!   permission's `patterns`, its `always` and its metadata (`command`, `filepath`), with an
//!   `edit`'s file as the prompt draws it (`drawn_path`), a question's `questions`. Each is a call
//!   of its own here, so a prompt is matched to the event that announced it: by the whole of what
//!   the prompt draws for it, over the rows it wraps to and with whitespace taken out (`$
//!   <command>` for `bash`, `→ Edit <file>` for `edit`, the file exactly as `drawn_path` says, the
//!   listed patterns for `external_directory`, every pattern for any other permission). A call with
//!   no patterns is no prompt's, and a prompt that calls asking different things all fit is up but
//!   not published.
//! - **Permission prompts** are drawn in place of the input box, down its bar: `△ Permission
//!   required`, what the call does, and the row `Allow once   Allow always   Reject`. The selection
//!   is drawn in colour alone (a background), so it cannot be read off the screen, and a key that
//!   moves it changes no text. So every answer is pressed from the prompt as it opens, with `Allow
//!   once` selected, and the reader declares its prompts unreadable after input
//!   (`InteractionReader::prompt_unreadable_after_input`): the watch publishes a prompt only while
//!   nothing has been written into the session since it first saw it up, and never again once
//!   anything has, so a key typed at the terminal that moved the selection unseen keeps the prompt
//!   to the terminal. That holds for every prompt of OpenCode's, questions included, though a
//!   question's choices can be read after a key: the declaration is the reader's, not a prompt's,
//!   so a question typed into at the terminal is withdrawn for good too. `Allow always` is taken by
//!   `l` and Enter written together (`NextKey::PressAll`); it then shows a second step, `△ Always
//!   allow`, listing the call's `always`, whose `Confirm` an Enter takes. `Allow once` is an Enter,
//!   and `Reject` is Esc, which rejects at once (`reply: "reject"`) where Enter on `Reject` would
//!   open a text field for a subagent's prompt.
//! - **Questions**: the question, its options numbered (`[ ]`/`[✓]` boxes for one that takes
//!   several), `Type your own answer` as the last number, and a key hint ending `esc dismiss`.
//!   Several questions, or one that takes several answers, get a row of tabs (the headers and
//!   `Confirm`) and a last `Review` page that Enter submits. A digit picks an option and goes on
//!   (sends a lone question, moves to the next tab), or toggles one that takes several; the digit
//!   after the last option opens a field for an answer of one's own, its text pasted and its Enter
//!   pressed; `l` moves on from a question that takes several; Esc dismisses. The question and each
//!   option's label and description wrap by word, and are read over their rows with whitespace
//!   taken out. What is asked is the call's own input; only the first question of an unanswered
//!   prompt is published, and the pages after it are `Up`. OpenCode offers an answer of one's own
//!   to every question, whatever the tool's `custom` says (measured with `custom: false`).
//! - **Gone** is the input box back (`super::screen::input_box_rows`) with no prompt.

use serde_json::Value;

use super::screen::{input_box_rows, BAR, BOX_INDENT};
use crate::platform::term::Input;
use crate::protocol::{
    ChoiceEffect, FreeText, InteractionPrompt, PermissionChoice, PermissionPrompt, Question,
    QuestionAnswer, QuestionOption, QuestionPrompt, ToolCategory,
};
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};

pub(super) struct OpenCodeInteractions;

const PERMISSION_ASKED: &str = "permission.asked";
const QUESTION_ASKED: &str = "question.asked";
/// The tool a question's event names.
const QUESTION_TOOL: &str = "question";

/// The first row of a permission prompt, and of its second step for `Allow always`.
const PERMISSION_TITLE: &str = "\u{25B3} Permission required";
const ALWAYS_TITLE: &str = "\u{25B3} Always allow";
/// The labels of the permission prompt's row of answers, in their order.
const ALLOW_ONCE: &str = "Allow once";
const ALLOW_ALWAYS: &str = "Allow always";
const V2_ALLOW_ALWAYS: &str = "Always allow";
const REJECT: &str = "Reject";
/// The labels of the second step's answers.
const CONFIRM: &str = "Confirm";
const CANCEL: &str = "Cancel";
/// The title of the field a subagent's prompt opens for a rejection's reason.
const REJECT_FIELD: &str = "Reject permission";
/// What the prompt draws under its title for a `bash` call, an `edit` call and a directory
/// outside the working directory, and above the patterns it lists for the latter.
const SHELL_HEADER: &str = "# Shell command";
const EDIT_HEADER: &str = "\u{2192} Edit ";
const EXTERNAL_HEADER: &str = "\u{2190} Access external directory";
const PATTERNS_LABEL: &str = "Patterns";
/// What the second step of `Allow always` draws above the patterns it will allow.
const ALWAYS_PATTERNS_LABEL: &str =
    "This will allow the following patterns until OpenCode is restarted";

/// The end of a question's key hint, and the page that reviews the answers of a tabbed one.
const QUESTION_HINT_END: &str = "esc dismiss";
const REVIEW: &str = "Review";
/// The last numbered row of a question, and the placeholder of its field while empty.
const OWN_ANSWER: &str = "Type your own answer";
/// What OpenCode adds to a question that takes several answers.
const SEVERAL_SUFFIX: &str = " (select all that apply)";

/// The choices' ids.
const ONCE: &str = "once";
const ALWAYS: &str = "always";
const REJECTED: &str = "reject";

/// The key that moves the permission prompt's selection to the next answer, and a question that
/// takes several answers on to the next tab.
const NEXT: &str = "l";

impl InteractionReader for OpenCodeInteractions {
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        if event != PERMISSION_ASKED && event != QUESTION_ASKED {
            return None;
        }
        let tool = payload.get("tool").and_then(Value::as_str)?;
        let id = payload
            .get("call_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let input = payload.get("input").cloned().unwrap_or(Value::Null);
        Some(CallSeen::new(id, tool, &input))
    }

    /// The permission prompt's selection is drawn in colour alone, so a key typed at the terminal
    /// may have moved it unseen; every answer is pressed from the prompt as it opens.
    fn prompt_unreadable_after_input(&self) -> bool {
        true
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        let rows = prompt_rows(screen);
        if let Some(permission) = Permission::on(&rows) {
            return read_permission(&permission, &rows, calls);
        }
        if let Some(page) = Page::on(&rows) {
            return read_question(&page, calls);
        }
        if input_box_rows(screen).is_some() {
            Reading::Gone
        } else {
            Reading::Unknown
        }
    }

    fn next_key(&self, screen: &str, call: &CallSeen, answer: &Answer) -> NextKey {
        let rows = prompt_rows(screen);
        if let Some(permission) = Permission::on(&rows) {
            // A key goes only into the call's own prompt; the rejection's field takes none.
            let is_calls = match permission {
                Permission::Asking | Permission::AskingV2 => permission_detail(permission, &rows)
                    .is_some_and(|detail| shows_call(&detail, call, permission)),
                Permission::Always => {
                    Detail::always(&rows).is_some_and(|detail| always_shows_call(&detail, call))
                }
                Permission::RejectField => true,
            };
            if !is_calls {
                return NextKey::Stuck("the permission prompt shown is not the call's");
            }
            return match answer {
                Answer::Choice { id, text: None } => {
                    if permission == Permission::AskingV2
                        && id == ALWAYS
                        && answers_row(&rows, &[ALLOW_ONCE, V2_ALLOW_ALWAYS, REJECT]).is_none()
                    {
                        return NextKey::Stuck("the permission prompt offers no remembered allow");
                    }
                    permission_key(permission, id)
                }
                _ => NextKey::Stuck("a permission prompt takes a choice with no text"),
            };
        }
        if let Some(page) = Page::on(&rows) {
            return match answer {
                Answer::Dismiss => NextKey::Press(Input::Escape),
                Answer::Questions(answers) => match asked(call) {
                    Some(questions) => question_key(&page, &questions, answers),
                    None => NextKey::Stuck("the call names no questions"),
                },
                Answer::Choice { .. } => NextKey::Stuck("a question takes answers, not a choice"),
            };
        }
        NextKey::Stuck("no prompt on the screen")
    }
}

/// The rows of what is drawn down the bar at the foot of the screen, where a prompt stands in
/// place of the input box: the text right of the bar, its indent taken off, oldest row first.
/// Empty where the screen does not end in rows of the bar.
fn prompt_rows(screen: &str) -> Vec<String> {
    let mut rows: Vec<String> = screen
        .lines()
        .rev()
        .skip_while(|row| row.trim().is_empty())
        .map_while(|row| {
            let rest = row.trim_start_matches(' ').strip_prefix(BAR)?;
            let rest = rest.strip_prefix(BOX_INDENT).unwrap_or(rest);
            Some(rest.trim_end().to_string())
        })
        .collect();
    rows.reverse();
    rows
}

/// `text` without whitespace, for telling text that may wrap.
fn squash(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The row of a prompt's answers: the row holding every label of `labels`, in their order.
fn answers_row(rows: &[String], labels: &[&str]) -> Option<usize> {
    rows.iter().rposition(|row| {
        let mut rest = row.as_str();
        labels.iter().all(|label| match rest.find(label) {
            Some(at) => {
                rest = &rest[at + label.len()..];
                true
            }
            None => false,
        })
    })
}

/// Which step of a permission prompt is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Permission {
    /// The prompt as it opens: `Allow once`, `Allow always`, `Reject`.
    Asking,
    /// The v2 prompt applies the choice directly, including a remembered allow.
    AskingV2,
    /// The second step of `Allow always`: `Confirm`, `Cancel`.
    Always,
    /// The field a subagent's rejection opens for its reason.
    RejectField,
}

impl Permission {
    fn on(rows: &[String]) -> Option<Self> {
        let has = |title: &str| rows.iter().any(|row| row.trim() == title);
        if has(PERMISSION_TITLE) && answers_row(rows, &[ALLOW_ONCE, ALLOW_ALWAYS, REJECT]).is_some()
        {
            Some(Self::Asking)
        } else if has(PERMISSION_TITLE)
            && answers_row(rows, &[ALLOW_ONCE, REJECT])
                .is_some_and(|at| rows[at].ends_with("enter confirm"))
        {
            Some(Self::AskingV2)
        } else if has(ALWAYS_TITLE) && answers_row(rows, &[CONFIRM, CANCEL]).is_some() {
            Some(Self::Always)
        } else if rows.iter().any(|row| row.contains(REJECT_FIELD)) {
            Some(Self::RejectField)
        } else {
            None
        }
    }
}

/// What a step of a permission prompt shows between its title and its row of answers, each part
/// joined over the rows it wraps to, whitespace taken out: its header, the rows right under the
/// title up to the first empty one (the icon and what the call does: `# Shell command`,
/// `→ Edit made.txt`), and its body, every row after that.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Detail {
    header: String,
    body: String,
}

impl Detail {
    /// The detail of the step titled `title` whose answers are `labels`, on `rows`.
    fn on(rows: &[String], title: &str, labels: &[&str]) -> Option<Self> {
        let top = rows.iter().position(|row| row.trim() == title)?;
        let bottom = answers_row(rows, labels)?;
        let between = rows.get(top + 1..bottom)?;
        let header_rows = between
            .iter()
            .position(|row| row.trim().is_empty())
            .unwrap_or(between.len());
        Some(Self {
            header: squash(&between[..header_rows].concat()),
            body: squash(&between[header_rows..].concat()),
        })
    }

    fn asking(rows: &[String]) -> Option<Self> {
        Self::on(rows, PERMISSION_TITLE, &[ALLOW_ONCE, ALLOW_ALWAYS, REJECT])
    }

    fn always(rows: &[String]) -> Option<Self> {
        Self::on(rows, ALWAYS_TITLE, &[CONFIRM, CANCEL])
    }
}

/// Whether the prompt as it opens, showing `detail`, is `call`'s, by what OpenCode 1.19.6 draws
/// for its permission: `$ <command>` under `# Shell command` for `bash`; `→ Edit <file>` for
/// `edit`, the file exactly as the plugin says the prompt draws it (`drawn_path`: relative to the
/// session's directory, else under `~`, else whole); the patterns listed under `Patterns` for
/// `external_directory`; and for any other permission, a header or body that holds every pattern.
/// A call with no patterns is no permission's.
fn shows_call(detail: &Detail, call: &CallSeen, permission: Permission) -> bool {
    let v2 = permission == Permission::AskingV2;
    if v2 != (call.input.get("v2").and_then(Value::as_str) == Some("true")) {
        return false;
    }
    let patterns = strings(call.input.get("patterns"));
    if call.tool == QUESTION_TOOL || patterns.is_empty() {
        return false;
    }
    let field = |name: &str| call.input.get(name).and_then(Value::as_str);
    match call.tool.as_str() {
        "bash" => field("command").is_some_and(|command| {
            (if v2 {
                detail.header.is_empty()
            } else {
                detail.header == squash(SHELL_HEADER)
            }) && detail.body == squash(&format!("$ {command}"))
        }),
        "edit" => field("drawn_path").is_some_and(|drawn| {
            detail
                .header
                .strip_prefix(&squash(EDIT_HEADER))
                .is_some_and(|shown| !shown.is_empty() && shown == squash(drawn))
        }),
        "external_directory" => {
            let listed: String = patterns
                .iter()
                .map(|pattern| squash(&format!("- {pattern}")))
                .collect();
            detail.header.starts_with(&squash(EXTERNAL_HEADER))
                && detail.body == format!("{}{listed}", squash(PATTERNS_LABEL))
        }
        _ => {
            let shown = format!("{}{}", detail.header, detail.body);
            patterns
                .iter()
                .all(|pattern| shown.contains(&squash(pattern)))
        }
    }
}

/// Whether the second step of `Allow always`, showing `detail`, is `call`'s: what it says it
/// will allow is the call's `always`, its patterns listed, or the permission named where that
/// is `*` alone.
fn always_shows_call(detail: &Detail, call: &CallSeen) -> bool {
    let always = strings(call.input.get("always"));
    if always.is_empty() {
        return false;
    }
    let expected = if always == ["*"] {
        squash(&format!(
            "This will allow {} until OpenCode is restarted.",
            call.tool
        ))
    } else {
        let listed: String = always
            .iter()
            .map(|pattern| squash(&format!("- {pattern}")))
            .collect();
        format!("{}{listed}", squash(ALWAYS_PATTERNS_LABEL))
    };
    format!("{}{}", detail.header, detail.body) == expected
}

fn permission_detail(permission: Permission, rows: &[String]) -> Option<Detail> {
    match permission {
        Permission::Asking => Detail::asking(rows),
        Permission::AskingV2 => Detail::on(rows, PERMISSION_TITLE, &[ALLOW_ONCE, REJECT]),
        _ => None,
    }
}

/// Finds the newest call matching the permission's detail, unless a different call also fits.
fn read_permission(permission: &Permission, rows: &[String], calls: &[CallSeen]) -> Reading {
    let Some(detail) = permission_detail(*permission, rows) else {
        return Reading::Up;
    };
    let matching: Vec<usize> = (0..calls.len())
        .filter(|&at| shows_call(&detail, &calls[at], *permission))
        .collect();
    let Some(&at) = matching.last() else {
        return Reading::Up;
    };
    let call = &calls[at];
    if matching
        .iter()
        .any(|&other| calls[other].tool != call.tool || calls[other].input != call.input)
    {
        return Reading::Up;
    }
    let choice = |id: &str, label: &str, effect| PermissionChoice {
        id: id.to_string(),
        label: label.to_string(),
        effect,
        text: None,
    };
    let mut choices = vec![choice(ONCE, ALLOW_ONCE, ChoiceEffect::Allow)];
    let remember = if *permission == Permission::AskingV2 {
        V2_ALLOW_ALWAYS
    } else {
        ALLOW_ALWAYS
    };
    if answers_row(rows, &[ALLOW_ONCE, remember, REJECT]).is_some() {
        choices.push(choice(ALWAYS, remember, ChoiceEffect::AllowRemember));
    }
    choices.push(choice(REJECTED, REJECT, ChoiceEffect::Deny));
    Reading::Shown {
        call: at,
        prompt: InteractionPrompt::Permission(PermissionPrompt {
            tool: call.tool.clone(),
            category: Some(category(&call.tool)),
            title: title(call, rows),
            detail: None,
            input: call.input.clone(),
            input_truncated: call.input_truncated,
            choices,
        }),
    }
}

/// The strings of a JSON array, none for anything else.
fn strings(value: Option<&Value>) -> Vec<&str> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// How the conversation feed classes a permission (1.19.6's names).
fn category(permission: &str) -> ToolCategory {
    match permission {
        "bash" => ToolCategory::Command,
        "edit" => ToolCategory::FileEdit,
        "read" | "external_directory" => ToolCategory::FileRead,
        "glob" | "grep" | "list" => ToolCategory::Search,
        "webfetch" | "websearch" => ToolCategory::Web,
        "task" => ToolCategory::Subagent,
        _ => ToolCategory::Other,
    }
}

/// One line saying what the call would do: its command or file, else the row under the prompt's
/// title (`← Access external directory /etc`) without its arrow.
fn title(call: &CallSeen, rows: &[String]) -> Option<String> {
    for field in ["command", "filepath"] {
        if let Some(text) = call.input.get(field).and_then(Value::as_str) {
            return Some(text.to_string());
        }
    }
    let at = rows.iter().position(|row| row.trim() == PERMISSION_TITLE)?;
    let row = rows.get(at + 1)?.trim();
    let row = row
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim();
    (!row.is_empty()).then(|| row.to_string())
}

/// The keys to the choice `id` on the step of the prompt shown. Every choice starts from the
/// prompt as it opens, `Allow once` selected, since the selection cannot be read.
fn permission_key(permission: Permission, id: &str) -> NextKey {
    match (permission, id) {
        (Permission::Asking | Permission::AskingV2, ONCE) => NextKey::Press(Input::Enter),
        (Permission::Asking | Permission::AskingV2, ALWAYS) => {
            NextKey::PressAll(vec![Input::Text(NEXT.to_string()), Input::Enter])
        }
        (Permission::Asking | Permission::AskingV2, REJECTED) => NextKey::Press(Input::Escape),
        (Permission::Always, ALWAYS) => NextKey::Press(Input::Enter),
        (Permission::Always, _) => NextKey::Stuck("the prompt is at the step of allowing always"),
        (Permission::RejectField, _) => NextKey::Stuck("the rejection's field is open"),
        (Permission::Asking | Permission::AskingV2, _) => {
            NextKey::Stuck("the prompt offers no such choice")
        }
    }
}

/// One question as the call asks it.
#[derive(Debug, Clone, PartialEq)]
struct Asked {
    text: String,
    header: Option<String>,
    several: bool,
    options: Vec<QuestionOption>,
}

/// The questions `call` asks, from its input; `None` for a call that asks none.
fn asked(call: &CallSeen) -> Option<Vec<Asked>> {
    if call.tool != QUESTION_TOOL {
        return None;
    }
    let questions: Vec<Asked> = call
        .input
        .get("questions")?
        .as_array()?
        .iter()
        .filter_map(|question| {
            let text = question.get("question")?.as_str()?.to_string();
            let options = question
                .get("options")
                .and_then(Value::as_array)
                .map(|options| {
                    options
                        .iter()
                        .filter_map(|option| {
                            Some(QuestionOption {
                                label: option.get("label")?.as_str()?.to_string(),
                                description: option
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(Asked {
                text,
                header: question
                    .get("header")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                several: question.get("multiple").and_then(Value::as_bool) == Some(true),
                options,
            })
        })
        .collect();
    (!questions.is_empty()).then_some(questions)
}

/// A numbered row of a question: its number, its box (`Some(checked)` for one that takes several),
/// its label as the row shows it, and the rows under it up to the next numbered row (what its
/// label and description wrap to, or an own answer's field), joined with whitespace taken out.
#[derive(Debug, Clone, PartialEq)]
struct Numbered {
    number: u32,
    checked: Option<bool>,
    label: String,
    under: String,
}

fn numbered(row: &str) -> Option<Numbered> {
    let (number, rest) = row.split_once(". ")?;
    let number = number.parse().ok()?;
    let (checked, label) = if let Some(label) = rest.strip_prefix("[ ] ") {
        (Some(false), label)
    } else if let Some(label) = rest.strip_prefix("[\u{2713}] ") {
        (Some(true), label)
    } else {
        (None, rest)
    };
    Some(Numbered {
        number,
        checked,
        label: label.trim().to_string(),
        under: String::new(),
    })
}

/// A page of a question prompt.
#[derive(Debug, Clone, PartialEq)]
enum Page {
    /// A question: its text as drawn (with the suffix of one that takes several) over the rows it
    /// wraps to, its numbered rows, and what its own answer's field holds, `None` while it is
    /// closed (the placeholder while it is open and empty); text and field with whitespace taken
    /// out.
    Question {
        text: String,
        rows: Vec<Numbered>,
        field: Option<String>,
    },
    /// The review of a tabbed prompt's answers, which Enter submits.
    Review,
}

impl Page {
    fn on(rows: &[String]) -> Option<Self> {
        let hint = rows
            .iter()
            .rposition(|row| row.trim_end().ends_with(QUESTION_HINT_END))?;
        let body: Vec<&str> = rows[..hint].iter().map(|row| row.as_str()).collect();
        if body.iter().any(|row| row.trim() == REVIEW) {
            return Some(Self::Review);
        }
        let first = body.iter().position(|row| numbered(row).is_some())?;
        // The question is the block of rows right above the options, from the empty row under the
        // tabs (or the top) on: it wraps by word.
        let above = &body[..first];
        let end = above.iter().rposition(|row| !row.trim().is_empty())?;
        let start = above[..end]
            .iter()
            .rposition(|row| row.trim().is_empty())
            .map_or(0, |at| at + 1);
        let text = squash(&above[start..=end].concat());
        let mut numbered_rows: Vec<Numbered> = Vec::new();
        for row in &body[first..] {
            match (numbered(row), numbered_rows.last_mut()) {
                (Some(found), _) => numbered_rows.push(found),
                (None, Some(last)) => last.under.push_str(&squash(row)),
                (None, None) => {}
            }
        }
        let field = numbered_rows
            .last()
            .filter(|row| row.label == OWN_ANSWER && !row.under.is_empty())
            .map(|row| row.under.clone());
        Some(Self::Question {
            text,
            rows: numbered_rows,
            field,
        })
    }
}

/// The text OpenCode draws for `question`.
fn drawn_text(question: &Asked) -> String {
    if question.several {
        format!("{}{SEVERAL_SUFFIX}", question.text)
    } else {
        question.text.clone()
    }
}

/// Whether the page shows `question`: its text, and its numbered rows are its options in order,
/// each label with its description under it over the rows they wrap to, then the own answer.
fn shows_question(text: &str, rows: &[Numbered], question: &Asked) -> bool {
    squash(&drawn_text(question)) == text && rows_fit(rows, question)
}

fn rows_fit(rows: &[Numbered], question: &Asked) -> bool {
    rows.len() == question.options.len() + 1
        && rows
            .iter()
            .zip(1..)
            .all(|(row, number)| row.number == number)
        && rows.iter().zip(&question.options).all(|(row, option)| {
            let drawn = format!("{}{}", squash(&row.label), row.under);
            let asked = squash(&format!(
                "{}{}",
                option.label,
                option.description.as_deref().unwrap_or("")
            ));
            drawn == asked
        })
        && rows.last().is_some_and(|row| row.label == OWN_ANSWER)
}

fn question_prompt(questions: &[Asked]) -> InteractionPrompt {
    InteractionPrompt::Question(QuestionPrompt {
        questions: questions
            .iter()
            .map(|question| Question {
                text: question.text.clone(),
                header: question.header.clone(),
                multi_select: question.several,
                options: question.options.clone(),
                free_text: Some(if question.several {
                    FreeText::With
                } else {
                    FreeText::Instead
                }),
            })
            .collect(),
        dismissible: true,
    })
}

/// What the question page shows of a call of `calls`: the newest question call whose first
/// question it is, published while nothing of it is answered; any other page is `Up`.
fn read_question(page: &Page, calls: &[CallSeen]) -> Reading {
    let Page::Question { text, rows, field } = page else {
        return Reading::Up;
    };
    let found = calls.iter().enumerate().rev().find_map(|(at, call)| {
        let questions = asked(call)?;
        let first = &questions[0];
        shows_question(text, rows, first).then_some((at, questions))
    });
    let Some((at, questions)) = found else {
        return Reading::Up;
    };
    let untouched = field.is_none() && rows.iter().all(|row| row.checked != Some(true));
    if !untouched {
        return Reading::Up;
    }
    Reading::Shown {
        call: at,
        prompt: question_prompt(&questions),
    }
}

fn digit(n: u32) -> NextKey {
    if n > 9 {
        return NextKey::Stuck("an option past the ninth cannot be picked by its digit");
    }
    NextKey::Press(Input::Text(n.to_string()))
}

/// The next key towards `answers` on the page shown.
fn question_key(page: &Page, questions: &[Asked], answers: &[QuestionAnswer]) -> NextKey {
    let Page::Question { text, rows, field } = page else {
        return NextKey::Press(Input::Enter);
    };
    let Some(at) = questions
        .iter()
        .position(|question| shows_question(text, rows, question))
    else {
        return NextKey::Stuck("the question shown is not one the call asks");
    };
    let question = &questions[at];
    let Some(answer) = answers.get(at) else {
        return NextKey::Stuck("no answer for the question shown");
    };
    let own = question.options.len() as u32 + 1;
    let own_text = answer.text.as_deref();
    if own_text.is_some_and(|text| text.contains('\n')) {
        return NextKey::Stuck("an answer of one's own is one line");
    }
    if !question.several {
        return match own_text {
            Some(text) => own_answer_key(field.as_deref(), text, own),
            None => match answer.selected.as_slice() {
                [only] => digit(only + 1),
                _ => NextKey::Stuck("a question that takes one answer takes one option"),
            },
        };
    }
    for (row, index) in rows.iter().zip(0u32..).take(question.options.len()) {
        if row.checked != Some(answer.selected.contains(&index)) {
            return digit(index + 1);
        }
    }
    let own_checked = rows.last().and_then(|row| row.checked) == Some(true);
    match own_text {
        Some(text) if !own_checked => own_answer_key(field.as_deref(), text, own),
        Some(text) if field.as_deref() != Some(squash(text).as_str()) => {
            NextKey::Stuck("the answer of one's own holds other text")
        }
        None if own_checked => digit(own),
        _ => NextKey::Press(Input::Text(NEXT.to_string())),
    }
}

/// The key towards the answer of one's own `text`, its field holding `field` (whitespace taken
/// out): the digit `own` opens it, the text is pasted into it empty, and Enter takes it.
fn own_answer_key(field: Option<&str>, text: &str, own: u32) -> NextKey {
    match field {
        None => digit(own),
        Some(shown) if shown == squash(OWN_ANSWER) => {
            NextKey::Press(Input::Paste(text.to_string()))
        }
        Some(shown) if shown == squash(text) => NextKey::Press(Input::Enter),
        Some(_) => NextKey::Stuck("the field of one's own answer holds other text"),
    }
}

#[cfg(test)]
mod tests;
