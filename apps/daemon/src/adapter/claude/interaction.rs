//! Claude Code's prompts to the user, read off its hooks and its screen and answered with keys
//! (`crate::session::interaction`). Measured on 2.1.328 (`docs/agent-cli-reference.md`, section
//! "Claude Code", and `testdata/claude-interaction-captures.md`).
//!
//! - **Calls.** `PreToolUse` says what a call is (`tool_use_id`, `tool_name`, `tool_input`, the
//!   questions of an `AskUserQuestion` among them). Its `PermissionRequest` follows 50 to 200 ms
//!   later, with no `tool_use_id` and permission suggestions that do not predict the options on
//!   screen, so the screen is what says which answers a prompt offers.
//! - **Permission prompts** (Bash, Write, Edit, WebFetch; any other tool whose dialog has the same
//!   layout): a solid rule, a title, what the call does between dashed rules, `Do you want …?`, the
//!   numbered options, and a footer `Esc to cancel`, with `· Tab to amend` while the highlight is
//!   on the first or last option and no text field is open (WebFetch has no footer and no amend).
//!   Each prompt is matched to a call by the command, the file name or the address it shows. The
//!   options that change the permission mode (`Yes, and switch to …`) are not published.
//! - **Questions** (`AskUserQuestion`): a chip row (`☐ Colour`, or `←  ☐ A  ☐ B  ✔ Submit  →` with
//!   several questions or a multi-select), the question, numbered options, a `Type something` row,
//!   a rule and `Chat about this`, which is not published. A multi-select has `[ ]` boxes and an
//!   unnumbered `Next`/`Submit` row; a single-select answers by its digit, a multi-select by
//!   toggling digits and pressing Enter on that row; the answers are reviewed on a last page.
//!   What the prompt asks is the call's own input, so it reads the same on every page; only the
//!   first page of an unanswered prompt is published, and the later pages are `Up`.
//! - **Gone** is the `❯` input box back with no dialog: the dialog replaces the box and the mode
//!   line while it is up.
//!
//! The faint cells of a screen are blanked here (`visible_text_undimmed`), which is how Claude Code
//! writes the placeholder of a text field: an empty `No, and tell Claude what to do differently`
//! field reads `No,`, and the focused empty `Type something.` row reads `3.`.

use serde_json::Value;

use super::feed::describe;
use super::screen::{input_box_rows, is_rule};
use crate::platform::term::Input;
use crate::protocol::{
    ChoiceEffect, ChoiceText, FreeText, InteractionPrompt, PermissionChoice, PermissionPrompt,
    Question, QuestionAnswer, QuestionOption, QuestionPrompt,
};
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};

pub(super) struct ClaudeInteractions;

/// The tool whose questions the user answers.
const ASK: &str = "AskUserQuestion";

/// The mark of the highlighted option.
const MARK: char = '\u{276F}';

/// The fewest characters of a string of a call's input that may identify it on the screen.
const MIN_SHOWN: usize = 3;

/// How many characters (whitespace left out) of a long command are compared with what is drawn,
/// which may cut it.
const LONG_COMMAND: usize = 300;

impl InteractionReader for ClaudeInteractions {
    /// Faint cells blanked: Claude Code draws the placeholder of an empty text field faint, which
    /// would read as typed text (see the module comment).
    fn faint_blanked(&self) -> bool {
        true
    }

    /// Only the main agent's own calls, by the hook interpreter's own test for a subagent's
    /// events. Whether a subagent's permission prompt is drawn in the main terminal was not
    /// measured; if it is, it matches no call here and reads as `Up`.
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        use crate::hooks::HookInterpreter;
        if event != "PreToolUse" || super::hooks::ClaudeHooks.is_subagent(payload) {
            return None;
        }
        Some(CallSeen::new(
            payload
                .get("tool_use_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            payload.get("tool_name").and_then(Value::as_str)?,
            payload.get("tool_input").unwrap_or(&Value::Null),
        ))
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        match Dialog::on(screen) {
            Some(dialog) => match dialog.kind {
                Kind::Permission => read_permission(&dialog, calls),
                Kind::Question => read_question(&dialog, calls),
            },
            None if input_box_rows(screen).is_some() => Reading::Gone,
            None => Reading::Unknown,
        }
    }

    fn next_key(&self, screen: &str, call: &CallSeen, answer: &Answer) -> NextKey {
        let Some(dialog) = Dialog::on(screen) else {
            return NextKey::Stuck("no prompt is on the screen");
        };
        match (answer, dialog.kind) {
            (Answer::Choice { id, text }, Kind::Permission) => {
                permission_key(&dialog, call, id, text.as_deref())
            }
            (Answer::Questions(answers), Kind::Question) => question_key(&dialog, call, answers),
            (Answer::Dismiss, Kind::Question) => NextKey::Press(Input::Escape),
            _ => NextKey::Stuck("the prompt on the screen is not the kind answered"),
        }
    }
}

/// The text without any whitespace, which is how screen text and a call's strings are compared:
/// a wrapped line loses its break or its space.
fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Permission,
    Question,
}

/// The dialog at the bottom of the screen: the rows under its top rule.
struct Dialog {
    kind: Kind,
    rows: Vec<String>,
    /// The width of the top rule, which is the terminal's.
    width: usize,
}

impl Dialog {
    /// The last solid rule that opens a dialog, a question's chip row or a permission prompt's
    /// `Do you want …` line below it. A question's own inner rule, above `Chat about this`, and
    /// the rules of the input box open none.
    fn on(screen: &str) -> Option<Dialog> {
        // A dialog replaces the input box: with the box drawn, a rule and a row that looks like a
        // dialog's (a transcript quoting one, a draft) is text, and must not hide the box's return.
        if input_box_rows(screen).is_some() {
            return None;
        }
        let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
        for at in (0..rows.len()).rev().filter(|&at| is_rule(rows[at])) {
            let below = &rows[at + 1..];
            let first = below.iter().find(|row| !row.trim().is_empty());
            let kind = if first.is_some_and(|row| chips(row)) {
                Kind::Question
            } else if below
                .iter()
                .any(|row| row.trim_start().starts_with("Do you want"))
            {
                Kind::Permission
            } else {
                continue;
            };
            return Some(Dialog {
                kind,
                rows: below.iter().map(|row| row.to_string()).collect(),
                width: rows[at].trim().chars().count(),
            });
        }
        None
    }
}

/// Whether `row` is a question's chip row: `☐ Colour`, `←  ☒ A  ☐ B  ✔ Submit  →`.
fn chips(row: &str) -> bool {
    row.trim_start()
        .starts_with(['\u{2190}', '\u{2610}', '\u{2612}', '\u{2714}'])
}

/// One numbered option of a dialog.
#[derive(Debug, Clone)]
struct Row {
    n: u32,
    highlighted: bool,
    label: String,
}

/// The mark and the text of a row that begins at most `column_limit` columns in, with the mark and
/// the spaces around it taken off. Descriptions and wrapped rows begin further in.
fn after_mark(row: &str, column_limit: usize) -> Option<(bool, &str)> {
    let trimmed = row.trim_start_matches(' ');
    let (highlighted, rest) = match trimmed.strip_prefix(MARK) {
        Some(rest) => (true, rest.trim_start_matches(' ')),
        None => (false, trimmed),
    };
    let column = row.chars().count() - rest.chars().count();
    (column <= column_limit && !rest.is_empty()).then_some((highlighted, rest))
}

/// `3. Label`, `❯ 1. Yes`, or the focused empty `❯ 3.`.
fn numbered(row: &str) -> Option<Row> {
    let (highlighted, rest) = after_mark(row, 4)?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let label = rest.get(digits..)?.strip_prefix('.')?;
    if digits == 0 || !(label.is_empty() || label.starts_with(' ')) {
        return None;
    }
    Some(Row {
        n: rest[..digits].parse().ok()?,
        highlighted,
        label: label.trim().to_string(),
    })
}

/// The `n`th row of a list is the one the highlight is on.
fn highlight(rows: &[Row]) -> Option<usize> {
    rows.iter().position(|row| row.highlighted)
}

// ---------------------------------------------------------------------------------------------
// Permission prompts
// ---------------------------------------------------------------------------------------------

/// A permission prompt as the screen shows it.
struct Permission {
    /// The rows above the options, each with its whitespace squashed: the title, what the call
    /// does, and the question.
    head: Vec<String>,
    /// The rows between the first two dashed rules, compacted: the command, the diff, the url.
    block: String,
    rows: Vec<Row>,
    /// The footer, `Esc to cancel …`, where there is one.
    footer: Option<String>,
}

impl Permission {
    fn on(dialog: &Dialog) -> Option<Permission> {
        let ask = dialog
            .rows
            .iter()
            .rposition(|row| row.trim_start().starts_with("Do you want"))?;
        let mut rows: Vec<Row> = Vec::new();
        let mut footer = None;
        let mut previous = "";
        for row in &dialog.rows[ask + 1..] {
            if row.trim_start().starts_with("Esc to cancel") {
                footer = Some(row.trim().to_string());
                break;
            }
            if let Some(next) = numbered(row) {
                rows.push(next);
            } else if row.starts_with("     ") && !row.trim().is_empty() {
                // A label that wraps goes on in the column of its text, where a long word breaks
                // at the edge of the terminal.
                if let Some(last) = rows.last_mut() {
                    if previous.chars().count() + 1 < dialog.width {
                        last.label.push(' ');
                    }
                    last.label.push_str(row.trim());
                }
            }
            previous = row;
        }
        if rows.is_empty() {
            return None;
        }
        let head: Vec<String> = dialog.rows[..=ask]
            .iter()
            .map(|row| squash(row))
            .filter(|row| !row.is_empty())
            .collect();
        let dashed =
            |row: &String| row.chars().count() >= 20 && row.chars().all(|c| c == '\u{254C}');
        let block = match head.iter().position(dashed) {
            Some(open) => {
                let inner = &head[open + 1..];
                let close = inner.iter().position(dashed).unwrap_or(inner.len() - 1);
                compact(&inner[..close].join("\n"))
            }
            None => String::new(),
        };
        Some(Permission {
            head,
            block,
            rows,
            footer,
        })
    }

    /// Whether the footer offers Tab to write to Claude.
    fn amend(&self) -> bool {
        self.footer
            .as_deref()
            .is_some_and(|footer| footer.contains("Tab to amend"))
    }

    /// Whether a text field is open: the footer stops offering Tab while the highlight, which is
    /// where the field opens (the first option and the last), is in one.
    fn field_open(&self) -> bool {
        self.footer.is_some()
            && !self.amend()
            && highlight(&self.rows).is_some_and(|at| at == 0 || at + 1 == self.rows.len())
    }

    /// The prompt as it first appears: the highlight on the first option and no field open.
    fn pristine(&self) -> bool {
        highlight(&self.rows) == Some(0) && (self.footer.is_none() || self.amend())
    }

    /// The options the daemon can tell the effect of, with the text each takes.
    fn choices(&self) -> Vec<PermissionChoice> {
        let last = self.rows.len() - 1;
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(at, row)| {
                let effect = effect(&row.label)?;
                let takes_text = self.amend()
                    && ((effect == ChoiceEffect::Allow && at == 0)
                        || (effect == ChoiceEffect::Deny && at == last));
                Some(PermissionChoice {
                    id: row.n.to_string(),
                    label: row.label.clone(),
                    effect,
                    text: takes_text.then_some(ChoiceText::Optional),
                })
            })
            .collect()
    }
}

/// What choosing the option labelled `label` does, or `None` for one that is not published: those
/// that switch the permission mode (`Yes, and switch to auto mode`, `… accept edits … for this
/// session`), and any other this does not know.
fn effect(label: &str) -> Option<ChoiceEffect> {
    if label == "Yes" {
        Some(ChoiceEffect::Allow)
    } else if label.starts_with("Yes, and")
        && (label.contains("always allow") || label.contains("ask again for"))
    {
        Some(ChoiceEffect::AllowRemember)
    } else if label == "No" || label.starts_with("No, and tell Claude") {
        Some(ChoiceEffect::Deny)
    } else {
        None
    }
}

/// Whether the dialog shows `call`: Bash's whole command as the block between the dashed rules,
/// the call's file as a row of the head (as given, or relative to the working directory) or as
/// the file the question names, its address as the `url:` row, else a string of its input as a
/// whole row or the value of a `name: value` row. Compared by whole rows or whole blocks, never by
/// a piece of the screen, so `ls` is not found in `tools`.
fn shows(permission: &Permission, call: &CallSeen) -> bool {
    let input = &call.input;
    let field = |key: &str| input.get(key).and_then(Value::as_str);
    let row = |text: &str| {
        let text = squash(text);
        text.chars().count() >= MIN_SHOWN
            && permission
                .head
                .iter()
                .any(|row| *row == text || row.ends_with(&format!(": {text}")))
    };
    if let Some(command) = field("command") {
        let command = compact(command);
        if command.is_empty() {
            return false;
        }
        // A long command may be cut where it is drawn: its start is what is compared.
        let take = |text: &str| text.chars().take(LONG_COMMAND).collect::<String>();
        return if command.chars().count() > LONG_COMMAND {
            take(&permission.block) == take(&command)
        } else {
            permission.block == command
        };
    }
    if let Some(path) = field("file_path").or_else(|| field("notebook_path")) {
        let path = path.replace('\\', "/");
        let name = path.rsplit('/').next().unwrap_or(&path);
        let relative = |shown: &str| {
            let shown = shown.replace('\\', "/");
            shown.chars().count() >= MIN_SHOWN
                && (shown == path || path.ends_with(&format!("/{shown}")))
        };
        let question = permission
            .head
            .last()
            .and_then(|ask| ask.strip_suffix('?'))
            .and_then(|ask| ask.rsplit(' ').next());
        return permission.head.iter().any(|row| relative(row))
            || (name.chars().count() >= MIN_SHOWN && question == Some(name));
    }
    if let Some(url) = field("url") {
        return row(url);
    }
    strings(input).into_iter().any(row)
}

/// `text` with every run of whitespace one space and the ends trimmed.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every string in `value`.
fn strings(value: &Value) -> Vec<&str> {
    match value {
        Value::String(text) => vec![text.as_str()],
        Value::Array(items) => items.iter().flat_map(strings).collect(),
        Value::Object(map) => map.values().flat_map(strings).collect(),
        _ => Vec::new(),
    }
}

fn read_permission(dialog: &Dialog, calls: &[CallSeen]) -> Reading {
    let Some(permission) = Permission::on(dialog) else {
        return Reading::Up;
    };
    let Some(call) = calls
        .iter()
        .rposition(|call| call.tool != ASK && shows(&permission, call))
    else {
        return Reading::Up;
    };
    // Moved off the first option or in a text field, the prompt is not as it was published, and
    // what it offers there (the footer's Tab) differs.
    let choices = permission.choices();
    if !permission.pristine() || choices.is_empty() {
        return Reading::Up;
    }
    let seen = &calls[call];
    let (category, title, _) = describe(&seen.tool, &seen.input);
    let detail = seen
        .input
        .get("description")
        .and_then(Value::as_str)
        .filter(|detail| !detail.is_empty() && Some(*detail) != title.as_deref())
        .map(str::to_string);
    Reading::Shown {
        call,
        prompt: InteractionPrompt::Permission(PermissionPrompt {
            tool: seen.tool.clone(),
            category: Some(category),
            title,
            detail,
            input: seen.input.clone(),
            input_truncated: seen.input_truncated,
            choices,
        }),
    }
}

/// The text a field that is open holds while it is empty: written faint, so it reads as its
/// first word and a comma.
fn empty_field(label: &str) -> bool {
    matches!(
        compact(label).as_str(),
        "Yes," | "No," | "Yes,andtellClaudewhattodonext" | "No,andtellClaudewhattododifferently"
    )
}

/// Whether the option labelled `label` is the editable rule row, `Yes, and don't ask again for:
/// <rule>`.
fn rule_field(label: &str) -> bool {
    label.contains("ask again for:")
}

/// A digit that picks the option: those of two digits are not pressed.
fn digit(n: u32) -> NextKey {
    if n > 9 {
        return NextKey::Stuck("an option past the ninth cannot be picked by its digit");
    }
    NextKey::Press(Input::Text(n.to_string()))
}

/// The keys to the choice `id`, with `text` for Claude where the choice takes it. A plain choice
/// is its digit. With text, the highlight goes Down to the option (the first, where it starts,
/// needs none), Tab opens the field, the text is pasted, and Enter submits.
fn permission_key(dialog: &Dialog, call: &CallSeen, id: &str, text: Option<&str>) -> NextKey {
    let Some(permission) = Permission::on(dialog) else {
        return NextKey::Stuck("the choices are not on the screen");
    };
    // A prompt queued behind the one the key was chosen for must not take it.
    if !shows(&permission, call) {
        return NextKey::Stuck("the prompt on the screen is not the call's");
    }
    let Some(target) = permission
        .rows
        .iter()
        .position(|row| row.n.to_string() == id)
    else {
        return NextKey::Stuck("the prompt no longer offers that choice");
    };
    let highlighted = highlight(&permission.rows);
    let field_open = permission.field_open();
    let Some(text) = text else {
        // A digit typed in a text field is text, and the rule row of a `don't ask again for:`
        // option is one (not measured with a digit).
        let in_rule = highlighted.is_some_and(|at| rule_field(&permission.rows[at].label));
        if field_open || in_rule {
            return NextKey::Stuck("a text field has the focus");
        }
        return digit(permission.rows[target].n);
    };
    let Some(at) = highlighted else {
        return NextKey::Stuck("no option is highlighted");
    };
    let ends = target == 0 || target + 1 == permission.rows.len();
    if !ends {
        return NextKey::Stuck("this option takes no text");
    }
    if field_open {
        if at != target {
            return NextKey::Stuck("a text field is open on another option");
        }
        return if empty_field(&permission.rows[at].label) {
            NextKey::Press(Input::Paste(text.to_string()))
        } else {
            NextKey::Press(Input::Enter)
        };
    }
    // Down through the rule's field, or out of it, was not measured.
    if permission.rows[at.min(target)..=at.max(target)]
        .iter()
        .any(|row| rule_field(&row.label))
    {
        return NextKey::Stuck("the highlight would cross a rule's text field");
    }
    match at.cmp(&target) {
        std::cmp::Ordering::Less => NextKey::Press(Input::Down),
        std::cmp::Ordering::Equal if permission.amend() => NextKey::Press(Input::Ctrl('i')),
        std::cmp::Ordering::Equal => NextKey::Stuck("the prompt does not offer to write to Claude"),
        std::cmp::Ordering::Greater => NextKey::Stuck("the highlight is past the option"),
    }
}

// ---------------------------------------------------------------------------------------------
// Questions
// ---------------------------------------------------------------------------------------------

/// A question as the call's input gives it.
struct Asked {
    text: String,
    header: Option<String>,
    multi: bool,
    options: Vec<(String, Option<String>)>,
}

/// The questions of an `AskUserQuestion` call, or `None` for any other call or an input this does
/// not read.
fn asked(call: &CallSeen) -> Option<Vec<Asked>> {
    if call.tool != ASK {
        return None;
    }
    let field = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|text| !text.is_empty())
    };
    let questions: Vec<Asked> = call
        .input
        .get("questions")?
        .as_array()?
        .iter()
        .map(|question| {
            let options = question
                .get("options")?
                .as_array()?
                .iter()
                .map(|option| Some((field(option, "label")?, field(option, "description"))))
                .collect::<Option<Vec<_>>>()?;
            Some(Asked {
                text: field(question, "question")?,
                header: field(question, "header"),
                multi: question.get("multiSelect").and_then(Value::as_bool) == Some(true),
                options,
            })
        })
        .collect::<Option<_>>()?;
    (!questions.is_empty()).then_some(questions)
}

/// Where the highlight is on a question's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    /// An option or the `Type something` row, by its place in the list.
    Option(usize),
    /// The unnumbered `Next` or `Submit` row of a multi-select.
    Action,
    Chat,
    Nowhere,
}

/// One question's page.
struct Page {
    /// Which of the call's questions it is.
    index: usize,
    /// The numbered rows but `Chat about this`: the options, then `Type something`.
    rows: Vec<Row>,
    /// The unnumbered `Next` or `Submit` row, with whether the highlight is on it.
    action: Option<bool>,
    chat: Option<Row>,
}

enum Shown {
    Page(Page),
    /// `Review your answers`, with its `Submit answers` row.
    Review(Option<Row>),
}

/// A question dialog as the screen shows it, read against the call's questions.
struct Screen {
    /// Whether the chip row has an answered question (`☒`).
    answered: bool,
    shown: Shown,
}

impl Screen {
    fn on(dialog: &Dialog, questions: &[Asked]) -> Option<Screen> {
        let chip_row = dialog.rows.iter().position(|row| !row.trim().is_empty())?;
        let answered = dialog.rows[chip_row].contains('\u{2612}');
        let below = &dialog.rows[chip_row + 1..];
        if below.iter().any(|row| row.trim() == "Review your answers") {
            // The review lists the answered questions, and warns when some are not. It is the
            // call's by being the newest question call; it is submitted only when it lists them
            // all and warns of none.
            let listed = compact(&below.join("\n"));
            let complete = !below
                .iter()
                .any(|row| row.trim_start().starts_with('\u{26A0}'))
                && questions.iter().all(|q| listed.contains(&compact(&q.text)));
            let submit = below
                .iter()
                .filter_map(|row| numbered(row))
                .find(|row| complete && row.label == "Submit answers");
            return Some(Screen {
                answered,
                shown: Shown::Review(submit),
            });
        }
        let first_option = below.iter().position(|row| numbered(row).is_some())?;
        let head = compact(&below[..first_option].join("\n"));
        let index = questions.iter().position(|q| compact(&q.text) == head)?;
        let mut page = Page {
            index,
            rows: Vec::new(),
            action: None,
            chat: None,
        };
        for row in &below[first_option..] {
            if let Some(row) = numbered(row) {
                if row.label == "Chat about this" {
                    page.chat = Some(row);
                } else {
                    page.rows.push(row);
                }
            } else if let Some((highlighted, rest)) = after_mark(row, 6) {
                // Drawn under the options' text, five columns in (`❯` and four spaces when
                // highlighted); a single-select has none.
                if questions[index].multi && (rest == "Next" || rest == "Submit") {
                    page.action = Some(highlighted);
                }
            }
        }
        Some(Screen {
            answered,
            shown: Shown::Page(page),
        })
    }
}

/// A row's label with its box taken off a multi-select's: whether it is checked, and the rest.
fn boxed(label: &str) -> Option<(bool, &str)> {
    if let Some(rest) = label.strip_prefix("[\u{2714}]") {
        Some((true, rest.trim()))
    } else {
        label.strip_prefix("[ ]").map(|rest| (false, rest.trim()))
    }
}

/// The text a `Type something` row shows while nothing is written in it.
fn blank_row(text: &str) -> bool {
    matches!(
        compact(text).as_str(),
        "" | "Typesomething" | "Typesomething."
    )
}

impl Page {
    fn focus(&self) -> Focus {
        if let Some(at) = highlight(&self.rows) {
            Focus::Option(at)
        } else if self.action == Some(true) {
            Focus::Action
        } else if self.chat.as_ref().is_some_and(|row| row.highlighted) {
            Focus::Chat
        } else {
            Focus::Nowhere
        }
    }

    /// Whether the rows are those of `question`, the options and then the row for a written
    /// answer, numbered from 1, each multi-select row with a box.
    fn fits(&self, question: &Asked) -> bool {
        let count = question.options.len();
        if self.rows.len() != count + 1
            || self.chat.is_none()
            || self.action.is_some() != question.multi
        {
            return false;
        }
        self.rows.iter().enumerate().all(|(at, row)| {
            let text = match (question.multi, boxed(&row.label)) {
                (true, Some((_, text))) => text,
                (false, None) => row.label.as_str(),
                _ => return false,
            };
            if row.n as usize != at + 1 {
                return false;
            }
            match question.options.get(at) {
                Some((label, _)) => {
                    let (text, label) = (compact(text), compact(label));
                    !text.is_empty() && label.starts_with(&text)
                }
                None => true,
            }
        })
    }

    /// The text of the row `at` without its box.
    fn text(&self, at: usize, multi: bool) -> &str {
        let label = self.rows[at].label.as_str();
        if multi {
            boxed(label).map_or(label, |(_, text)| text)
        } else {
            label
        }
    }

    fn checked(&self, at: usize) -> bool {
        boxed(&self.rows[at].label).is_some_and(|(checked, _)| checked)
    }
}

/// The prompt the call's questions are, which every page of the dialog asks the same.
fn question_prompt(questions: &[Asked]) -> InteractionPrompt {
    InteractionPrompt::Question(QuestionPrompt {
        questions: questions
            .iter()
            .map(|question| Question {
                text: question.text.clone(),
                header: question.header.clone(),
                multi_select: question.multi,
                options: question
                    .options
                    .iter()
                    .map(|(label, description)| QuestionOption {
                        label: label.clone(),
                        description: description.clone(),
                    })
                    .collect(),
                // Typing in a multi-select's row checks it beside the options; in a single-select
                // the written answer takes the place of an option.
                free_text: Some(if question.multi {
                    FreeText::With
                } else {
                    FreeText::Instead
                }),
            })
            .collect(),
        dismissible: true,
    })
}

fn read_question(dialog: &Dialog, calls: &[CallSeen]) -> Reading {
    let found = calls.iter().enumerate().rev().find_map(|(at, call)| {
        let questions = asked(call)?;
        let screen = Screen::on(dialog, &questions)?;
        Some((at, questions, screen))
    });
    let Some((call, questions, screen)) = found else {
        return Reading::Up;
    };
    // Only the first page of a prompt nothing has been answered in: a later page, the review,
    // a field with text in it or a box checked is a prompt being answered, which the press, or
    // the person at the terminal, goes on with.
    let Shown::Page(page) = &screen.shown else {
        return Reading::Up;
    };
    let question = &questions[page.index];
    let k = question.options.len();
    let untouched = page.index == 0
        && !screen.answered
        && page.fits(question)
        && page.focus() == Focus::Option(0)
        && blank_row(page.text(k, question.multi))
        && (0..k).all(|at| !question.multi || !page.checked(at));
    if !untouched {
        return Reading::Up;
    }
    Reading::Shown {
        call,
        prompt: question_prompt(&questions),
    }
}

/// The next key of `answers` to the questions of `call`, from the page the screen is on.
///
/// A single-select answers by its digit, which moves on to the next question (and, for the only
/// question, submits); a written answer goes Down to the `Type something` row, pastes, and presses
/// Enter, which does the same. A multi-select toggles the digits that differ from the answer (a
/// digit toggles without moving the highlight), writes its text in the last row, which checks it,
/// and goes Down to the `Next` or `Submit` row for an Enter. The review page takes `1`.
fn question_key(dialog: &Dialog, call: &CallSeen, answers: &[QuestionAnswer]) -> NextKey {
    let Some(questions) = asked(call) else {
        return NextKey::Stuck("the call has no questions");
    };
    if answers.len() != questions.len() {
        return NextKey::Stuck("the answers are not one to a question");
    }
    let Some(screen) = Screen::on(dialog, &questions) else {
        return NextKey::Stuck("the questions are not on the screen");
    };
    let page = match screen.shown {
        Shown::Review(Some(submit)) if submit.n == 1 => return digit(1),
        Shown::Review(_) => return NextKey::Stuck("the review page has no Submit answers"),
        Shown::Page(page) => page,
    };
    let (question, wanted) = (&questions[page.index], &answers[page.index]);
    if !page.fits(question) {
        return NextKey::Stuck("the rows of the question are not as the call gave them");
    }
    let k = question.options.len();
    let focus = page.focus();
    let at_text = focus == Focus::Option(k);
    let written = !blank_row(page.text(k, question.multi));
    // Down to the row for a written answer, from an option above it.
    let towards_text = || match focus {
        Focus::Option(at) if at < k => NextKey::Press(Input::Down),
        _ => NextKey::Stuck("the highlight is not above the row for a written answer"),
    };
    let paste = |text: &str| NextKey::Press(Input::Paste(text.to_string()));
    if !question.multi {
        if let Some(text) = &wanted.text {
            return match (at_text, written) {
                (true, false) => paste(text),
                (true, true) => NextKey::Press(Input::Enter),
                (false, _) => towards_text(),
            };
        }
        let Some(&pick) = wanted.selected.first() else {
            return NextKey::Stuck("no option is answered");
        };
        if at_text {
            return NextKey::Stuck("the written answer's row has the focus");
        }
        return digit(pick + 1);
    }
    for at in 0..k {
        if page.checked(at) != wanted.selected.contains(&(at as u32)) {
            if at_text {
                return NextKey::Stuck("the written answer's row has the focus");
            }
            return digit(at as u32 + 1);
        }
    }
    match (&wanted.text, written) {
        (Some(text), false) if at_text => return paste(text),
        (Some(_), false) => return towards_text(),
        (None, true) => return NextKey::Stuck("a written answer is already in the row"),
        _ => {}
    }
    match focus {
        Focus::Action => NextKey::Press(Input::Enter),
        Focus::Option(_) => NextKey::Press(Input::Down),
        _ => NextKey::Stuck("the highlight is not on an option or the Next row"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const PROMPTS: &[u8] = include_bytes!("../../../testdata/claude_interaction_prompts.bin");

    /// The screen of the first `end` bytes of a capture, as the daemon reads it.
    fn screen_of(bytes: &[u8], end: usize) -> String {
        let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
        screen.process(&bytes[..end]);
        screen.visible_text_undimmed()
    }

    fn at(end: usize) -> String {
        screen_of(PROMPTS, end)
    }

    fn call(tool: &str, input: Value) -> CallSeen {
        CallSeen::new(Some(format!("toolu_{tool}")), tool, &input)
    }

    fn bash() -> CallSeen {
        call(
            "Bash",
            json!({"command": "touch probe-bash2-marker.txt", "description": "Create a marker file"}),
        )
    }

    fn ask1() -> CallSeen {
        call(
            ASK,
            json!({"questions": [{"question": "Which colour do you prefer?", "header": "Colour",
                "multiSelect": false,
                "options": [{"label": "Red", "description": "The red one"},
                            {"label": "Blue", "description": "The blue one"}]}]}),
        )
    }

    fn ask_multi() -> CallSeen {
        call(
            ASK,
            json!({"questions": [{"question": "Which fruits do you like?", "header": "Fruits",
                "multiSelect": true,
                "options": [{"label": "Apple", "description": "Crisp"},
                            {"label": "Banana", "description": "Soft"},
                            {"label": "Cherry", "description": "Small"}]}]}),
        )
    }

    fn ask3() -> CallSeen {
        call(
            ASK,
            json!({"questions": [
                {"question": "Which colour do you prefer?", "header": "Colour", "multiSelect": false,
                 "options": [{"label": "Red", "description": "The red one"},
                             {"label": "Blue", "description": "The blue one"}]},
                {"question": "Which fruits do you like?", "header": "Fruits", "multiSelect": true,
                 "options": [{"label": "Apple", "description": "Crisp"},
                             {"label": "Banana", "description": "Soft"},
                             {"label": "Cherry", "description": "Small"}]},
                {"question": "Which size?", "header": "Size", "multiSelect": false,
                 "options": [{"label": "Small", "description": "S"},
                             {"label": "Large", "description": "L"}]}]}),
        )
    }

    fn read(screen: &str, calls: &[CallSeen]) -> Reading {
        ClaudeInteractions.read(screen, calls)
    }

    fn key(screen: &str, call: &CallSeen, answer: &Answer) -> NextKey {
        ClaudeInteractions.next_key(screen, call, answer)
    }

    fn press(input: Input) -> NextKey {
        NextKey::Press(input)
    }

    fn choice(id: &str, text: Option<&str>) -> Answer {
        Answer::Choice {
            id: id.to_string(),
            text: text.map(str::to_string),
        }
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

    fn summary(prompt: &PermissionPrompt) -> Vec<(String, ChoiceEffect, bool)> {
        prompt
            .choices
            .iter()
            .map(|c| (c.id.clone(), c.effect, c.text.is_some()))
            .collect()
    }

    #[test]
    fn only_a_main_agents_pre_tool_use_starts_a_call() {
        let payload = json!({"tool_use_id": "t1", "tool_name": "Bash",
            "tool_input": {"command": "ls"}});
        let seen = ClaudeInteractions
            .call_started("PreToolUse", &payload)
            .unwrap();
        assert_eq!(seen.id.as_deref(), Some("t1"));
        assert_eq!(seen.tool, "Bash");
        assert_eq!(seen.input, json!({"command": "ls"}));
        assert!(ClaudeInteractions
            .call_started("PostToolUse", &payload)
            .is_none());
        let mut sub = payload.clone();
        sub["agent_type"] = json!("general-purpose");
        assert!(ClaudeInteractions
            .call_started("PreToolUse", &sub)
            .is_none());
        let mut helper = payload;
        helper["agent_id"] = json!("a1");
        assert!(ClaudeInteractions
            .call_started("PreToolUse", &helper)
            .is_none());
    }

    /// 2.1.328 Bash prompt: the auto-mode row is left out, the always-allow row is a remembered
    /// allow, and the first and last rows take text while the footer offers Tab.
    #[test]
    fn a_bash_prompt_is_published_without_its_mode_switch() {
        let calls = [bash()];
        let (call, prompt) = permission(read(&at(17860), &calls));
        assert_eq!(call, 0);
        assert_eq!(prompt.tool, "Bash");
        assert_eq!(
            prompt.title.as_deref(),
            Some("touch probe-bash2-marker.txt")
        );
        assert_eq!(prompt.detail.as_deref(), Some("Create a marker file"));
        assert_eq!(
            summary(&prompt),
            [
                ("1".to_string(), ChoiceEffect::Allow, true),
                ("2".to_string(), ChoiceEffect::AllowRemember, false),
                ("4".to_string(), ChoiceEffect::Deny, true),
            ]
        );
        assert!(prompt.choices[1]
            .label
            .starts_with("Yes, and always allow access to "));
        assert!(prompt.choices[1].label.ends_with(" from this project"));
    }

    #[test]
    fn a_prefix_rule_row_is_a_remembered_allow() {
        let calls = [call("Bash", json!({"command": "python3 -c \"print(42)\""}))];
        let (_, prompt) = permission(read(&at(166285), &calls));
        assert_eq!(prompt.choices[1].effect, ChoiceEffect::AllowRemember);
        assert_eq!(
            prompt.choices[1].label,
            "Yes, and don\u{2019}t ask again for: python3 *"
        );
        assert_eq!(prompt.choices.len(), 3);
    }

    #[test]
    fn write_and_edit_prompts_leave_out_the_accept_edits_row() {
        let write = call(
            "Write",
            json!({"file_path": "/x/y/probe-write.txt", "content": "hello from the stub\n"}),
        );
        let (_, prompt) = permission(read(&at(107289), &[write]));
        assert_eq!(
            prompt.category,
            Some(crate::protocol::ToolCategory::FileEdit)
        );
        assert_eq!(prompt.title.as_deref(), Some("/x/y/probe-write.txt"));
        assert_eq!(
            summary(&prompt),
            [
                ("1".to_string(), ChoiceEffect::Allow, true),
                ("3".to_string(), ChoiceEffect::Deny, true),
            ]
        );
        let edit = call(
            "Edit",
            json!({"file_path": "/x/existing.txt", "old_string": "alpha", "new_string": "beta"}),
        );
        let (_, prompt) = permission(read(&at(127132), &[edit]));
        assert_eq!(summary(&prompt).len(), 2);
    }

    /// WebFetch has no footer: no text goes with its choices.
    #[test]
    fn a_fetch_prompt_takes_no_text() {
        let fetch = call(
            "WebFetch",
            json!({"url": "http://127.0.0.1:47811/page", "prompt": "Summarize the page"}),
        );
        let (_, prompt) = permission(read(&at(142424), &[fetch]));
        assert_eq!(prompt.category, Some(crate::protocol::ToolCategory::Web));
        assert_eq!(
            summary(&prompt),
            [
                ("1".to_string(), ChoiceEffect::Allow, false),
                ("2".to_string(), ChoiceEffect::AllowRemember, false),
                ("3".to_string(), ChoiceEffect::Deny, false),
            ]
        );
    }

    /// The category and title come from the function the conversation feed classes the same call
    /// with (`feed::describe`, the one `read_tool_use` calls), so they cannot differ.
    #[test]
    fn categories_are_the_feeds() {
        use crate::protocol::ToolCategory::{Command, FileEdit, Web};
        let write = call("Write", json!({"file_path": "/x/y/probe-write.txt"}));
        let fetch = call("WebFetch", json!({"url": "http://127.0.0.1:47811/page"}));
        for (end, call, category) in [
            (17860, bash(), Command),
            (107289, write, FileEdit),
            (142424, fetch, Web),
        ] {
            let (_, prompt) = permission(read(&at(end), std::slice::from_ref(&call)));
            let (expected, title, _) = describe(&call.tool, &call.input);
            assert_eq!(prompt.category, Some(expected));
            assert_eq!(prompt.category, Some(category));
            assert_eq!(prompt.title, title);
        }
    }

    #[test]
    fn a_prompt_for_no_call_the_hooks_reported_is_up() {
        let other = call("Bash", json!({"command": "make all"}));
        assert_eq!(read(&at(17860), &[other]), Reading::Up);
        assert_eq!(read(&at(17860), &[]), Reading::Up);
        // The newest of two matching calls is the one.
        let calls = [bash(), call("Bash", json!({"command": "ls"})), bash()];
        let (index, _) = permission(read(&at(17860), &calls));
        assert_eq!(index, 2);
    }

    /// Moved off the first row, or with a text field open, a prompt is up but not as published.
    #[test]
    fn a_prompt_being_answered_is_up() {
        let calls = [bash()];
        for end in [18172, 18622, 18750, 19282, 60801, 60872, 184101] {
            let calls = if end == 184101 {
                vec![call(
                    "Bash",
                    json!({"command": "curl -s http://127.0.0.1:47811/v1/models"}),
                )]
            } else {
                calls.to_vec()
            };
            assert_eq!(read(&at(end), &calls), Reading::Up, "at {end}");
        }
    }

    #[test]
    fn the_input_box_back_is_gone_and_a_foreign_screen_unknown() {
        assert_eq!(read(&at(22796), &[bash()]), Reading::Gone);
        assert_eq!(read(&at(241708), &[ask1()]), Reading::Gone);
        assert_eq!(read(&at(250003), &[ask_multi()]), Reading::Gone);
        assert_eq!(read(&at(265336), &[ask3()]), Reading::Gone);
        assert_eq!(read("some other view\n", &[]), Reading::Unknown);
    }

    #[test]
    fn plain_choices_are_digits_and_text_goes_through_the_field() {
        let call = bash();
        let none = choice("2", None);
        assert_eq!(
            key(&at(17860), &call, &none),
            press(Input::Text("2".into()))
        );
        // Text on Yes: the highlight is on it, so Tab; then paste into the empty field; then Enter.
        let yes = choice("1", Some("go on"));
        assert_eq!(key(&at(17860), &call, &yes), press(Input::Ctrl('i')));
        assert_eq!(
            key(&at(60801), &call, &yes),
            press(Input::Paste("go on".into()))
        );
        assert_eq!(key(&at(60872), &call, &yes), press(Input::Enter));
        // Text on No: Down until the last row, Tab, paste, Enter.
        let no = choice("4", Some("not now"));
        assert_eq!(key(&at(17860), &call, &no), press(Input::Down));
        assert_eq!(key(&at(18172), &call, &no), press(Input::Down));
        assert_eq!(key(&at(18622), &call, &no), press(Input::Ctrl('i')));
        assert_eq!(
            key(&at(18750), &call, &no),
            press(Input::Paste("not now".into()))
        );
        assert_eq!(key(&at(19282), &call, &no), press(Input::Enter));
        // A digit is not pressed into an open field, nor text on a row that takes none.
        assert!(matches!(key(&at(60801), &call, &none), NextKey::Stuck(_)));
        assert!(matches!(
            key(&at(17860), &call, &choice("2", Some("x"))),
            NextKey::Stuck(_)
        ));
        assert!(matches!(
            key(&at(17860), &call, &choice("9", None)),
            NextKey::Stuck(_)
        ));
        // The rule row is a field too.
        let curl = call_curl();
        assert!(matches!(
            key(&at(184101), &curl, &choice("1", None)),
            NextKey::Stuck(_)
        ));
    }

    fn call_curl() -> CallSeen {
        call(
            "Bash",
            json!({"command": "curl -s http://127.0.0.1:47811/v1/models"}),
        )
    }

    fn questions(prompt: Reading) -> QuestionPrompt {
        match prompt {
            Reading::Shown {
                prompt: InteractionPrompt::Question(prompt),
                ..
            } => prompt,
            other => panic!("not a question prompt: {other:?}"),
        }
    }

    #[test]
    fn questions_are_the_calls_own() {
        let prompt = questions(read(&at(207990), &[ask1()]));
        assert!(prompt.dismissible);
        assert_eq!(prompt.questions.len(), 1);
        let question = &prompt.questions[0];
        assert_eq!(question.text, "Which colour do you prefer?");
        assert_eq!(question.header.as_deref(), Some("Colour"));
        assert_eq!(question.free_text, Some(FreeText::Instead));
        assert!(!question.multi_select);
        assert_eq!(question.options[1].label, "Blue");
        assert_eq!(
            question.options[1].description.as_deref(),
            Some("The blue one")
        );

        let prompt = questions(read(&at(244837), &[ask_multi()]));
        assert!(prompt.questions[0].multi_select);
        assert_eq!(prompt.questions[0].free_text, Some(FreeText::With));

        let prompt = questions(read(&at(252986), &[ask3()]));
        assert_eq!(prompt.questions.len(), 3);
    }

    /// A page past the first, a checked box, text in the row, the review: up, never gone.
    #[test]
    fn a_question_being_answered_is_up() {
        for (end, call) in [
            (226190, ask1()),
            (226406, ask1()),
            (244953, ask_multi()),
            (245551, ask_multi()),
            (245628, ask_multi()),
            (246464, ask_multi()),
            (255523, ask3()),
            (259382, ask3()),
            (260347, ask3()),
            (261654, ask3()),
        ] {
            assert_eq!(read(&at(end), &[call]), Reading::Up, "at {end}");
        }
    }

    #[test]
    fn a_question_matches_only_its_own_call() {
        assert_eq!(read(&at(207990), &[ask_multi()]), Reading::Up);
        assert_eq!(read(&at(207990), &[bash()]), Reading::Up);
    }

    #[test]
    fn questions_are_answered_by_digit_text_or_dismissal() {
        let c = ask1();
        let pick = Answer::Questions(vec![QuestionAnswer {
            selected: vec![1],
            text: None,
        }]);
        assert_eq!(key(&at(207990), &c, &pick), press(Input::Text("2".into())));
        let text = Answer::Questions(vec![QuestionAnswer {
            selected: vec![],
            text: Some("Green".into()),
        }]);
        assert_eq!(key(&at(207990), &c, &text), press(Input::Down));
        assert_eq!(
            key(&at(226190), &c, &text),
            press(Input::Paste("Green".into()))
        );
        assert_eq!(key(&at(226406), &c, &text), press(Input::Enter));
        // The digit would be typed into the row that has the focus.
        assert!(matches!(key(&at(226190), &c, &pick), NextKey::Stuck(_)));
        assert_eq!(key(&at(207990), &c, &Answer::Dismiss), press(Input::Escape));
        assert_eq!(
            key(&at(244837), &ask_multi(), &Answer::Dismiss),
            press(Input::Escape)
        );
        assert!(matches!(key(&at(17860), &bash(), &pick), NextKey::Stuck(_)));
        assert!(matches!(
            key(&at(207990), &c, &choice("1", None)),
            NextKey::Stuck(_)
        ));
    }

    #[test]
    fn a_multi_select_toggles_writes_and_goes_to_submit() {
        let c = ask_multi();
        let answer = Answer::Questions(vec![QuestionAnswer {
            selected: vec![0, 2],
            text: Some("Durian".into()),
        }]);
        // Nothing checked: toggle Apple. Then (Apple checked) Cherry.
        assert_eq!(
            key(&at(244837), &c, &answer),
            press(Input::Text("1".into()))
        );
        assert_eq!(
            key(&at(244953), &c, &answer),
            press(Input::Text("3".into()))
        );
        // Apple and Cherry checked (a synthetic screen: Cherry toggled), text next: Down to the row.
        let cherry = at(244953).replace("3. [ ] Cherry", "3. [\u{2714}] Cherry");
        assert_eq!(key(&cherry, &c, &answer), press(Input::Down));
        // The row has the focus and is empty: paste.
        let focused = cherry
            .replace("❯ 1. [✔] Apple", "  1. [✔] Apple")
            .replace("  4. [ ] Type something", "❯ 4. [ ]");
        assert_eq!(
            key(&focused, &c, &answer),
            press(Input::Paste("Durian".into()))
        );
        // Written (the capture of a typed row): Down to Submit, then Enter on it.
        assert_eq!(key(&at(245551), &c, &answer), press(Input::Down));
        assert_eq!(key(&at(245628), &c, &answer), press(Input::Enter));
        // The review page takes 1.
        assert_eq!(
            key(&at(246464), &c, &answer),
            press(Input::Text("1".into()))
        );
        // Without text and with the wanted boxes checked, straight to Submit.
        let plain = Answer::Questions(vec![QuestionAnswer {
            selected: vec![0],
            text: None,
        }]);
        assert_eq!(key(&at(244953), &c, &plain), press(Input::Down));
        // Text already in the row that the answer does not want is not pressed past.
        assert!(matches!(key(&at(245551), &c, &plain), NextKey::Stuck(_)));
    }

    #[test]
    fn several_questions_are_answered_tab_by_tab() {
        let c = ask3();
        let answer = Answer::Questions(vec![
            QuestionAnswer {
                selected: vec![1],
                text: None,
            },
            QuestionAnswer {
                selected: vec![0, 1],
                text: None,
            },
            QuestionAnswer {
                selected: vec![],
                text: Some("Medium".into()),
            },
        ]);
        // First tab: single-select, the digit advances.
        assert_eq!(
            key(&at(252986), &c, &answer),
            press(Input::Text("2".into()))
        );
        // Second tab: multi-select with its Next row.
        assert_eq!(
            key(&at(259382), &c, &answer),
            press(Input::Text("1".into()))
        );
        // Third tab: the written answer goes Down to the row.
        assert_eq!(key(&at(260347), &c, &answer), press(Input::Down));
        // The review page.
        assert_eq!(
            key(&at(261654), &c, &answer),
            press(Input::Text("1".into()))
        );
        // A review that warns of unanswered questions is never submitted.
        assert!(matches!(key(&at(255523), &c, &answer), NextKey::Stuck(_)));
        // Nor one that does not list every question.
        let short = at(261654).replace("Which size?", "Which other?");
        assert!(matches!(key(&short, &c, &answer), NextKey::Stuck(_)));
        // A question or answer that says "unanswered" does not block it; only the warning row.
        let word = at(261654).replace("→ Blue", "→ unanswered");
        assert_eq!(key(&word, &c, &answer), press(Input::Text("1".into())));
    }

    /// A key chosen for one call is never pressed on the prompt of another, whatever the screen.
    #[test]
    fn a_key_is_pressed_only_on_its_calls_prompt() {
        let other = call("Bash", json!({"command": "make all"}));
        assert!(matches!(
            key(&at(17860), &other, &choice("1", None)),
            NextKey::Stuck(_)
        ));
        assert_eq!(
            key(&at(17860), &bash(), &choice("1", None)),
            press(Input::Text("1".into()))
        );
    }

    /// A command is the whole block between the dashed rules, not a piece of the screen: `ls`
    /// does not match a prompt for `ls -la`, nor one whose title says `tools`.
    #[test]
    fn a_command_must_be_the_whole_block() {
        let ls = call("Bash", json!({"command": "touch probe-bash"}));
        assert_eq!(read(&at(17860), &[ls]), Reading::Up);
        let short = call("Bash", json!({"command": "ls"}));
        assert_eq!(read(&at(17860), &[short]), Reading::Up);
    }

    /// Down across the rule's text field, which was not measured, is not pressed.
    #[test]
    fn the_highlight_is_not_moved_across_a_rule_field() {
        let python = call("Bash", json!({"command": "python3 -c \"print(42)\""}));
        assert!(matches!(
            key(&at(166285), &python, &choice("4", Some("no"))),
            NextKey::Stuck(_)
        ));
        assert_eq!(
            key(&at(166285), &python, &choice("1", Some("ok"))),
            press(Input::Ctrl('i'))
        );
        // The highlight below the target (no panic): moved to row 2, the rule field itself.
        let on_rule = at(166285)
            .replace("❯ 1. Yes", "  1. Yes")
            .replace("  2. Yes, and don", "❯ 2. Yes, and don");
        assert!(matches!(
            key(&on_rule, &python, &choice("1", Some("ok"))),
            NextKey::Stuck(_)
        ));
    }

    /// Text that looks like a dialog above the input box does not hide that the box is back.
    #[test]
    fn a_quoted_dialog_above_the_input_box_is_not_a_dialog() {
        let mut screen = at(17860);
        screen.push_str(&at(22796));
        assert_eq!(read(&screen, &[bash()]), Reading::Gone);
    }

    fn composer(end: usize) -> String {
        screen_of(
            include_bytes!("../../../testdata/claude_interaction_composer.bin"),
            end,
        )
    }

    #[test]
    fn the_highlighted_command_of_the_completion_popup() {
        let screen = crate::adapter::provider(crate::protocol::AgentId::CLAUDE).screen();
        // Pasted `/he`, typed `/help`, pasted `/probec`, pasted `/probe`, which highlights
        // `/probe` while another offers `/probecmd`.
        assert_eq!(
            screen.command_completion(&composer(24127)).as_deref(),
            Some("help")
        );
        assert_eq!(
            screen.command_completion(&composer(31947)).as_deref(),
            Some("help")
        );
        assert_eq!(
            screen.command_completion(&composer(44794)).as_deref(),
            Some("probecmd")
        );
        assert_eq!(
            screen.command_completion(&composer(69410)).as_deref(),
            Some("probecmd")
        );
        assert_eq!(
            screen.command_completion(&composer(73351)).as_deref(),
            Some("probe")
        );
        // No popup: the command pasted whole with its arguments, an idle prompt, a prompt screen.
        assert_eq!(screen.command_completion(&composer(37584)), None);
        assert_eq!(screen.command_completion(&composer(116051)), None);
        assert_eq!(screen.command_completion(&at(22796)), None);
    }

    #[test]
    fn image_placeholders_are_counted_in_the_box_text() {
        let screen = crate::adapter::provider(crate::protocol::AgentId::CLAUDE).screen();
        let rows = screen
            .input_box_rows(&composer(116051))
            .expect("the box holds the pasted message");
        let text = rows.join("\n");
        assert_eq!(
            text.trim_start_matches('\u{276F}').trim(),
            "compare [Image #8] and [Image #9]"
        );
        assert_eq!(screen.image_placeholders(&text), 2);
        assert_eq!(screen.image_placeholders("[Image #1]describe"), 1);
        assert_eq!(
            screen.image_placeholders("[Image #] [Image 1] [Pasted text #1]"),
            0
        );
        assert_eq!(screen.image_placeholders(""), 0);
    }
}
