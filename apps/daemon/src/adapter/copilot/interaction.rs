//! How GitHub Copilot CLI's pending interactions are read and answered: its permission prompt and
//! the form of its own `ask_user` tool. Measured on 1.0.114 against a stand-in model
//! (`testdata/copilot-captures.md`, "Interactions"; `docs/agent-cli-reference.md`, "GitHub
//! Copilot CLI", "Interactions").
//!
//! The hooks give the call (`preToolUse`: the tool, its id and its arguments); what the prompt
//! offers and where its cursor is show only on the screen, in place of the composer:
//!
//! - **The permission prompt** is a box: the call's description, the command in a box of its own,
//!   `Do you want to run this command?`, the rows `1. Yes`, `2. Yes, and don't ask again for …`
//!   and `3. No, and tell Copilot what to do differently (Esc to stop)`, the cursor `❯` before one
//!   of them, and the hint `↑/↓ to navigate · enter to select · esc to cancel`. Down moves the
//!   cursor and stops at the last row (there is no key to move up among the inputs a press can
//!   write, so a cursor below the choice is stuck). Text typed or pasted on the third row takes
//!   the label's place (`3. use echo (Esc to stop)`) and Enter sends it as the user's feedback;
//!   Enter there with no text does nothing, and Esc refuses the call and stops the turn.
//! - **The `ask_user` form** sits between two rules: `Copilot needs information.`, the message,
//!   a row naming the fields (the current one in brackets) when there are several, then the
//!   current field: a choice (`<name>:`, its values with the cursor ` ❯ ` on one, and
//!   `Other (type your answer)`, on which a text field opens below the form) or a text field
//!   (` ❯ <text>` between rules of its own). Enter accepts the field and moves to the next, the
//!   last one sending the form; Ctrl-D declines it and the turn goes on, but only while no text
//!   field holds text (the footer then ends `esc clear`, and Esc empties the field). The fields
//!   are read from the call's `requestedSchema`; a field of any other kind than a string is not
//!   published.
//!
//! The prompts of a file created or edited, of a read outside the allowed paths and of an MCP tool
//! are boxes of the same kind, each showing its call in its own way ([`asks_about`]); a read's
//! offers four rows, the last `No (Esc)`, and no feedback row. A key is chosen only while the
//! prompt on the screen is still the answered call's: once a prompt is answered, the prompt of a
//! call queued behind it takes its place.

use serde_json::Value;

use super::hooks;
use super::screen::{FORM_FOOTER_DECLINE, FORM_TITLE_END, PROMPT_MARK};
use crate::platform::term::Input;
use crate::protocol::{
    AgentId, ChoiceEffect, ChoiceText, FreeText, InteractionPrompt, PermissionChoice,
    PermissionPrompt, Question, QuestionAnswer, QuestionOption, QuestionPrompt, ToolCategory,
};
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};
use crate::session::screen::ScreenReader;

pub(super) struct CopilotInteractions;

impl InteractionReader for CopilotInteractions {
    /// A call of the session itself: a subagent's events carry `agentId`, and a subagent's calls
    /// are not the session's to publish. `toolArgs` comes as an object, or as JSON text.
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        if event != hooks::PRE_TOOL_USE || payload.get("agentId").is_some() {
            return None;
        }
        let tool = payload.get("toolName")?.as_str()?;
        let input = match payload.get("toolArgs") {
            Some(Value::String(text)) => serde_json::from_str(text).unwrap_or(Value::Null),
            Some(args) => args.clone(),
            None => Value::Null,
        };
        Some(CallSeen::new(
            payload
                .get("toolCallId")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool,
            &input,
        ))
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        match View::of(screen) {
            View::Permission(menu) => permission(&menu, calls),
            View::Form(form) => question(&form, calls),
            View::Composer => Reading::Gone,
            View::Other => Reading::Unknown,
        }
    }

    fn next_key(&self, screen: &str, call: &CallSeen, answer: &Answer) -> NextKey {
        match (View::of(screen), answer) {
            (View::Permission(menu), Answer::Choice { id, text }) => {
                permission_key(&menu, call, id, text.as_deref())
            }
            (View::Form(form), Answer::Questions(answers)) => question_key(&form, call, answers),
            (View::Form(form), Answer::Dismiss) => dismiss_key(&form, call),
            (View::Permission(_) | View::Form(_), _) => {
                NextKey::Stuck("the prompt on the screen is not the kind this answer is for")
            }
            _ => NextKey::Stuck("no prompt on the screen"),
        }
    }
}

/// The mark of a menu's cursor, and of the composer.
const MARK: char = PROMPT_MARK;

/// A box's side.
const SIDE: char = '\u{2502}';

/// What the permission prompt's hint row ends with inside its box, whitespace removed:
/// `↑/↓ to navigate · enter to select · esc to cancel`, with `pgup/pgdn to scroll` between them
/// where the box is too tall for the screen.
const MENU_HINT_END: &str = "entertoselect\u{b7}esctocancel";

/// The most an option row is indented inside the box (` ❯ 1.` or `   1.`); the rows a long label
/// wraps onto are indented further, under the label.
const OPTION_INDENT: usize = 3;

/// How many characters (whitespace left out) of a long command or of an MCP call's arguments are
/// compared with what the box shows, which may show only the start of them.
const LONG_TEXT: usize = 300;

/// The last choice of a choice field, which opens a text field.
const OTHER_PREFIX: &str = "Other (type your answer";

/// What the text field of `Other` shows while it is empty.
const OTHER_PLACEHOLDER: &str = "Type your answer...";

/// The label the third row of the permission prompt has until text is typed on it.
const FEEDBACK_LABEL: &str = "No, and tell Copilot what to do differently";

/// What the third row ends with, before and after text is typed on it.
const FEEDBACK_END: &str = "(Esc to stop)";

/// What the screen shows.
enum View {
    Permission(Menu),
    Form(Form),
    Composer,
    Other,
}

impl View {
    fn of(screen: &str) -> View {
        let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
        if let Some(menu) = Menu::of(&rows) {
            return View::Permission(menu);
        }
        if let Some(form) = Form::of(&rows) {
            return View::Form(form);
        }
        if super::screen::CopilotScreen
            .input_box_rows(screen)
            .is_some()
        {
            View::Composer
        } else {
            View::Other
        }
    }
}

/// The text without any whitespace, which is how screen text and a call's strings are compared:
/// a wrapped line loses its break or its space.
fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Whether `shown` is `text` as drawn: the same with whitespace left out, or for a text longer
/// than [`LONG_TEXT`], the same start (the box may show only the start, and the call's own string
/// may be cut where it was kept).
fn same_text(shown: &str, text: &str) -> bool {
    let (shown, text) = (compact(shown), compact(text));
    if shown.is_empty() {
        return false;
    }
    if text.chars().count() > LONG_TEXT {
        let start = |text: &str| text.chars().take(LONG_TEXT).collect::<String>();
        start(&shown) == start(&text)
    } else {
        shown == text
    }
}

// ---- the permission prompt ----

/// The permission prompt: what its box holds, row by row, and its numbered rows.
struct Menu {
    /// The inside of each row of the box, trimmed, from its first row to its hint.
    rows: Vec<String>,
    options: Vec<MenuRow>,
}

struct MenuRow {
    /// The digit before the label.
    key: String,
    /// The label, with the rows it wraps onto joined by spaces.
    label: String,
    highlighted: bool,
}

impl Menu {
    fn of(rows: &[&str]) -> Option<Menu> {
        let hint = rows
            .iter()
            .rposition(|row| compact(row).contains(MENU_HINT_END))?;
        let top = rows[..hint]
            .iter()
            .rposition(|row| row.trim_start().starts_with('\u{256D}'))?;
        let inside: Vec<&str> = rows[top + 1..hint]
            .iter()
            .map(|row| inside_box(row))
            .collect();
        let mut options: Vec<MenuRow> = Vec::new();
        let mut wrapping = false;
        for row in &inside {
            let text = row.trim();
            let indent = row.chars().take_while(|c| c.is_whitespace()).count();
            if indent <= OPTION_INDENT {
                if let Some(option) = MenuRow::parse(text) {
                    // The options are one block numbered from 1, the last in the box: a row
                    // numbered `1.` starts it afresh (a numbered line of the description above the
                    // question was no option), and a number that does not follow the block's last
                    // is text.
                    if option.key == "1" {
                        options.clear();
                    }
                    if option.follows(options.last()) {
                        options.push(option);
                        wrapping = true;
                        continue;
                    }
                }
            }
            match options.last_mut() {
                Some(option) if wrapping && !text.is_empty() && indent > OPTION_INDENT => {
                    option.label.push(' ');
                    option.label.push_str(text);
                }
                _ => wrapping = false,
            }
        }
        if options.is_empty() {
            return None;
        }
        Some(Menu {
            rows: inside.iter().map(|row| row.trim().to_string()).collect(),
            options,
        })
    }

    fn highlighted(&self) -> Option<usize> {
        self.options.iter().position(|row| row.highlighted)
    }

    /// The text of each box drawn inside the prompt's (a command, a path, an MCP call's
    /// arguments), whitespace kept: its rows' insides joined by spaces. A box too tall for the
    /// screen is drawn without its bottom side, and ends where its rows do.
    fn inner_boxes(&self) -> Vec<String> {
        let mut boxes = Vec::new();
        let mut current: Option<Vec<&str>> = None;
        for row in &self.rows {
            if row.starts_with(SIDE) {
                current
                    .get_or_insert_with(Vec::new)
                    .push(inside_box(row).trim());
            } else if let Some(rows) = current.take() {
                boxes.push(rows.join(" "));
            }
        }
        boxes.extend(current.map(|rows| rows.join(" ")));
        boxes
    }
}

/// What a row of a box holds between its sides, its indentation kept.
fn inside_box(row: &str) -> &str {
    let row = row.trim();
    let row = row.strip_prefix(SIDE).unwrap_or(row);
    row.strip_suffix(SIDE).unwrap_or(row)
}

impl MenuRow {
    /// Whether this row's number comes next after `last`'s, or is 1 where there is none.
    fn follows(&self, last: Option<&MenuRow>) -> bool {
        let number = |row: &MenuRow| row.key.parse::<u32>().ok();
        match last {
            None => self.key == "1",
            Some(last) => number(last)
                .zip(number(self))
                .is_some_and(|(a, b)| b == a + 1),
        }
    }

    /// `❯ 1. Yes` or `1. Yes`.
    fn parse(row: &str) -> Option<MenuRow> {
        let (highlighted, rest) = match row.strip_prefix(MARK) {
            Some(rest) => (true, rest.trim_start()),
            None => (false, row),
        };
        let (key, label) = rest.split_once(". ")?;
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(MenuRow {
            key: key.to_string(),
            label: label.trim().to_string(),
            highlighted,
        })
    }

    /// Whether the feedback row shows its own label, with no text typed on it.
    fn feedback_bare(&self) -> bool {
        self.label.starts_with(FEEDBACK_LABEL)
    }

    /// Whether text has been typed on the feedback row in place of its label: the row ends as the
    /// feedback row does and is no other row.
    fn feedback_written(&self) -> bool {
        self.label.ends_with(FEEDBACK_END) && !self.feedback_bare() && classify(self).is_none()
    }

    /// The text typed on the feedback row, as drawn (whitespace may differ).
    fn feedback_text(&self) -> &str {
        self.label
            .strip_suffix(FEEDBACK_END)
            .unwrap_or(&self.label)
            .trim()
    }
}

/// What choosing a row does, by its label: `None` for a label not measured. The rows that make
/// Copilot keep a rule, for the directory or for the session, are `AllowRemember`.
fn classify(row: &MenuRow) -> Option<(ChoiceEffect, Option<ChoiceText>)> {
    const REMEMBER: &[&str] = &[
        // A command, a file operation, an MCP tool: `… in this directory (<dir>)`.
        "Yes, and don't ask again",
        // A path outside the allowed ones: `… for this session`.
        "Yes, and remember this path",
        "Yes, allow Copilot to read files in the listed directory",
    ];
    if row.label == "Yes" {
        Some((ChoiceEffect::Allow, None))
    } else if REMEMBER.iter().any(|label| row.label.starts_with(label)) {
        Some((ChoiceEffect::AllowRemember, None))
    } else if row.feedback_bare() {
        Some((ChoiceEffect::Deny, Some(ChoiceText::Optional)))
    } else if row.label == "No (Esc)" {
        Some((ChoiceEffect::Deny, None))
    } else {
        None
    }
}

/// What kind of tool a call is, and the line that says what it would do.
fn describe(call: &CallSeen) -> (ToolCategory, Option<String>) {
    let text = |key: &str| {
        call.input
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    match call.tool.as_str() {
        "bash" | "powershell" => (ToolCategory::Command, text("command")),
        "create" | "edit" => (ToolCategory::FileEdit, text("path")),
        "apply_patch" => (ToolCategory::FileEdit, None),
        "view" => (ToolCategory::FileRead, text("path")),
        // An MCP server's tool is named `<server>-<tool>`.
        tool if tool.contains('-') => (ToolCategory::Mcp, Some(tool.to_string())),
        _ => (ToolCategory::Other, None),
    }
}

/// Whether the menu asks about `call`, by what each kind of prompt shows of its call (each
/// measured, `testdata/copilot-captures.md`, "Interactions"):
///
/// - a command: the command in a box of its own;
/// - a file created or edited: the question `Do you want to create <path>?` (or `edit`);
/// - a file read outside the allowed paths: the path in a box of its own;
/// - an MCP server's tool `<server>-<tool>`: the tool's name on the box's first row and the
///   arguments, as indented JSON, in a box of their own.
///
/// Any other call (`apply_patch`, a tool not measured) is never matched, so its prompt is not
/// published.
fn asks_about(menu: &Menu, call: &CallSeen) -> bool {
    let in_a_box = |text: &str| {
        menu.inner_boxes()
            .iter()
            .any(|shown| same_text(shown, text))
    };
    match describe(call) {
        (ToolCategory::Command, Some(command)) => in_a_box(&command),
        (ToolCategory::FileEdit, Some(path)) => {
            let question = compact(&menu.rows.join(" "));
            ["create", "edit"]
                .iter()
                .any(|verb| question.contains(&compact(&format!("Do you want to {verb} {path}?"))))
        }
        (ToolCategory::FileRead, Some(path)) => in_a_box(&path),
        (ToolCategory::Mcp, Some(tool)) => {
            let named = menu
                .rows
                .first()
                .is_some_and(|name| !name.is_empty() && tool.ends_with(&format!("-{name}")));
            let arguments = serde_json::to_string(&call.input).unwrap_or_default();
            named && in_a_box(&arguments)
        }
        _ => false,
    }
}

fn permission(menu: &Menu, calls: &[CallSeen]) -> Reading {
    // Text being written on the feedback row is a prompt half answered.
    if menu.options.iter().any(MenuRow::feedback_written) {
        return Reading::Up;
    }
    let choices: Vec<PermissionChoice> = menu
        .options
        .iter()
        .filter_map(|row| {
            let (effect, text) = classify(row)?;
            Some(PermissionChoice {
                id: row.key.clone(),
                label: row.label.clone(),
                effect,
                text,
            })
        })
        .collect();
    if choices.is_empty() {
        return Reading::Up;
    }
    let Some(call) = calls.iter().rposition(|call| asks_about(menu, call)) else {
        return Reading::Up;
    };
    let seen = &calls[call];
    let (category, title) = describe(seen);
    Reading::Shown {
        call,
        prompt: InteractionPrompt::Permission(PermissionPrompt {
            tool: seen.tool.clone(),
            category: Some(category),
            title,
            detail: seen
                .input
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            input: seen.input.clone(),
            input_truncated: seen.input_truncated,
            choices,
        }),
    }
}

/// The next key towards choice `id` (with `text` for the feedback row) of the prompt for `call`:
/// stuck unless the prompt on the screen is that call's, since the prompt of a call queued behind
/// it takes its place once it is answered.
fn permission_key(menu: &Menu, call: &CallSeen, id: &str, text: Option<&str>) -> NextKey {
    if !asks_about(menu, call) {
        return NextKey::Stuck("the prompt on the screen is for another call");
    }
    let Some(target) = menu.options.iter().position(|row| row.key == id) else {
        return NextKey::Stuck("the prompt no longer offers that choice");
    };
    let row = &menu.options[target];
    let feedback = row.feedback_bare() || row.feedback_written();
    let text = text.map(one_line).filter(|text| !text.is_empty());
    // The feedback row takes no Enter without text: Esc refuses the call, and stops the turn.
    if feedback && text.is_none() {
        return NextKey::Press(Input::Escape);
    }
    let Some(at) = menu.highlighted() else {
        return NextKey::Stuck("no row of the prompt is highlighted");
    };
    if at < target {
        return NextKey::Press(Input::Down);
    }
    if at > target {
        return NextKey::Stuck("the highlight is below the choice");
    }
    match text {
        // Pasted only where the row shows its own label: once text is on it, however it wraps,
        // it is never pasted again.
        Some(text) if row.feedback_bare() => NextKey::Press(Input::Paste(text)),
        Some(text) if row.feedback_written() => {
            let shown = compact(row.feedback_text());
            if !shown.is_empty() && compact(&text).starts_with(&shown) {
                NextKey::Press(Input::Enter)
            } else {
                NextKey::Stuck("the feedback row holds other text")
            }
        }
        _ => NextKey::Press(Input::Enter),
    }
}

/// `text` on one line: a line break would be read as Enter.
fn one_line(text: &str) -> String {
    text.split(|c: char| c.is_control())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

// ---- the `ask_user` form ----

/// The form: its rows from the title to its hint, and the text field `Other` opens below it.
struct Form {
    rows: Vec<String>,
    /// What the text field below the form holds, where it is open, the rows it wraps onto joined:
    /// empty while it shows its placeholder.
    other_text: Option<String>,
    /// Whether the footer ends `esc clear`, as it does while a text field holds text: Ctrl-D then
    /// does nothing, and Esc empties the field.
    clearing: bool,
}

/// How the footer ends while a text field holds text, whitespace removed.
const FOOTER_CLEARING_END: &str = "escclear";

/// Whether a row is one of the rules the form draws around itself and its text field.
fn is_rule(row: &str) -> bool {
    row.starts_with('\u{2500}')
}

/// The text of a text field whose first row is `rows[0]` (` ❯ <text>`), with the rows it wraps
/// onto, up to the rule under it.
fn field_text(rows: &[impl AsRef<str>]) -> Option<String> {
    let first = rows.first()?.as_ref().trim().strip_prefix(MARK)?;
    let mut text = vec![first.trim()];
    text.extend(
        rows[1..]
            .iter()
            .map(|row| row.as_ref().trim())
            .take_while(|row| !row.is_empty() && !is_rule(row)),
    );
    Some(text.join(" "))
}

impl Form {
    fn of(rows: &[&str]) -> Option<Form> {
        let title = rows
            .iter()
            .rposition(|row| compact(row).ends_with(FORM_TITLE_END))?;
        let hint = title
            + rows[title..]
                .iter()
                .position(|row| compact(row).contains(FORM_FOOTER_DECLINE))?;
        let below = &rows[hint + 1..];
        let other_text = below
            .iter()
            .position(|row| row.trim().starts_with(MARK))
            .and_then(|at| field_text(&below[at..]))
            .map(|text| {
                if text == OTHER_PLACEHOLDER {
                    String::new()
                } else {
                    text
                }
            });
        Some(Form {
            rows: rows[title..hint]
                .iter()
                .map(|row| row.trim().to_string())
                .collect(),
            other_text,
            clearing: compact(rows[hint]).ends_with(FOOTER_CLEARING_END),
        })
    }

    /// The message: the rows after the title up to the first blank one, the fields' row or the
    /// first field's own rows, joined.
    fn message(&self, fields: &[Field]) -> String {
        let end = self
            .fields_row(fields)
            .or_else(|| {
                self.rows.iter().position(|row| {
                    fields
                        .iter()
                        .any(|field| *row == format!("{}:", field.name))
                })
            })
            .unwrap_or(self.rows.len());
        self.rows[1.min(end)..end]
            .iter()
            .take_while(|row| !row.is_empty() && !is_rule(row))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The row naming the fields, `[first]  second` with the current one in brackets: the row that
    /// is exactly the fields' names in order once the brackets are left out, with one of them
    /// bracketed. A form of one field draws none.
    fn fields_row(&self, fields: &[Field]) -> Option<usize> {
        if fields.len() < 2 {
            return None;
        }
        let names = compact(
            &fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<String>(),
        );
        self.rows.iter().position(|row| {
            row.matches('[').count() == 1
                && row.matches(']').count() == 1
                && compact(&row.replace(['[', ']'], "")) == names
        })
    }

    /// The field that has the focus: the bracketed name of the fields' row, or the only one where
    /// the form has one field.
    fn current(&self, fields: &[Field]) -> Option<usize> {
        if let [_] = fields {
            return Some(0);
        }
        let row = &self.rows[self.fields_row(fields)?];
        let start = row.find('[')?;
        let end = row[start..].find(']')? + start;
        let name = &row[start + 1..end];
        fields.iter().position(|field| field.name == name)
    }

    /// The rows of the current choice field's values, from the row after `<name>:`.
    fn choice_rows(&self, name: &str) -> Vec<(bool, &str)> {
        let label = format!("{name}:");
        let Some(from) = self.rows.iter().position(|row| *row == label) else {
            return Vec::new();
        };
        self.rows[from + 1..]
            .iter()
            .map(|row| match row.strip_prefix(MARK) {
                Some(rest) => (true, rest.trim()),
                None => (false, row.as_str()),
            })
            .filter(|(_, text)| !text.is_empty())
            .collect()
    }

    /// What the current text field holds: the row ` ❯ <text>` inside the form and the rows it
    /// wraps onto.
    fn text_field(&self) -> Option<String> {
        let at = self
            .rows
            .iter()
            .skip(2)
            .position(|row| row.starts_with(MARK))?;
        field_text(&self.rows[2 + at..])
    }
}

/// Whether the text field showing `shown` holds `text` (whitespace left out, `shown` its start).
fn holds(shown: &str, text: &str) -> bool {
    let shown = compact(shown);
    !shown.is_empty() && compact(text).starts_with(&shown)
}

/// One field of the form, from the call's `requestedSchema`.
struct Field {
    name: String,
    /// The values offered, for a choice; empty for a text field.
    values: Vec<String>,
    /// What the schema says the field is for.
    description: Option<String>,
}

/// The fields of an `ask_user` call, in the schema's order: `None` where any is not a string (a
/// choice or text), which is not measured.
fn fields_of(call: &CallSeen) -> Option<Vec<Field>> {
    let properties = call
        .input
        .pointer("/requestedSchema/properties")?
        .as_object()?;
    if properties.is_empty() {
        return None;
    }
    properties
        .iter()
        .map(|(name, schema)| {
            if schema.get("type").and_then(Value::as_str) != Some("string") {
                return None;
            }
            let values = match schema.get("enum") {
                None => Vec::new(),
                Some(values) => values
                    .as_array()?
                    .iter()
                    .map(|value| value.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?,
            };
            let description = schema
                .get("description")
                .or_else(|| schema.get("title"))
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(Field {
                name: name.clone(),
                values,
                description,
            })
        })
        .collect()
}

/// The newest `ask_user` call whose message is the form's (whitespace left out, the form's the
/// start of the call's) and whose fields the form names.
fn form_call(form: &Form, calls: &[CallSeen]) -> Option<usize> {
    calls.iter().rposition(|call| {
        if !AgentId::COPILOT
            .descriptor()
            .is_ask_question_tool(Some(&call.tool))
        {
            return false;
        }
        let Some(fields) = fields_of(call) else {
            return false;
        };
        if fields.len() > 1 && form.fields_row(&fields).is_none() {
            return false;
        }
        let shown = compact(&form.message(&fields));
        call.input
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|message| !shown.is_empty() && compact(message).starts_with(&shown))
    })
}

fn question(form: &Form, calls: &[CallSeen]) -> Reading {
    let Some(call) = form_call(form, calls) else {
        return Reading::Up;
    };
    let Some(fields) = fields_of(&calls[call]) else {
        return Reading::Up;
    };
    let message = calls[call]
        .input
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let questions = fields
        .iter()
        .map(|field| Question {
            text: if fields.len() == 1 {
                message.to_string()
            } else {
                field
                    .description
                    .clone()
                    .unwrap_or_else(|| field.name.clone())
            },
            header: Some(field.name.clone()),
            multi_select: false,
            options: field
                .values
                .iter()
                .map(|value| QuestionOption {
                    label: value.clone(),
                    description: None,
                })
                .collect(),
            // A choice ends with `Other`, which takes text in place of a value; a text field
            // takes text alone.
            free_text: Some(FreeText::Instead),
        })
        .collect();
    Reading::Shown {
        call,
        prompt: InteractionPrompt::Question(QuestionPrompt {
            questions,
            dismissible: true,
        }),
    }
}

/// The next key that declines the form of `call`: Ctrl-D, the turn going on; while a text field
/// holds text Ctrl-D does nothing, so Esc empties the field first.
fn dismiss_key(form: &Form, call: &CallSeen) -> NextKey {
    if form_call(form, std::slice::from_ref(call)).is_none() {
        return NextKey::Stuck("the form on the screen is for another call");
    }
    if form.clearing {
        NextKey::Press(Input::Escape)
    } else {
        NextKey::Press(Input::Ctrl('d'))
    }
}

/// The next key towards `answers` for the form of `call`: stuck unless the form on the screen is
/// that call's.
fn question_key(form: &Form, call: &CallSeen, answers: &[QuestionAnswer]) -> NextKey {
    if form_call(form, std::slice::from_ref(call)).is_none() {
        return NextKey::Stuck("the form on the screen is for another call");
    }
    let Some(fields) = fields_of(call) else {
        return NextKey::Stuck("the form has a field of a kind not measured");
    };
    if answers.len() != fields.len() {
        return NextKey::Stuck("the answer does not have one entry per question");
    }
    let Some(current) = form.current(&fields) else {
        return NextKey::Stuck("no field of the form has the focus");
    };
    let (field, answer) = (&fields[current], &answers[current]);
    let text = answer
        .text
        .as_deref()
        .map(one_line)
        .filter(|text| !text.is_empty());
    if field.values.is_empty() {
        let shown = form.text_field().unwrap_or_default();
        return match text {
            Some(text) if shown.is_empty() => NextKey::Press(Input::Text(text)),
            Some(text) if !holds(&shown, &text) => {
                NextKey::Stuck("the text field holds other text")
            }
            _ => NextKey::Press(Input::Enter),
        };
    }
    let rows = form.choice_rows(&field.name);
    let Some(at) = rows.iter().position(|(highlighted, _)| *highlighted) else {
        return NextKey::Stuck("no value of the field is highlighted");
    };
    let target = match (answer.selected.as_slice(), &text) {
        ([index], _) => {
            let Some(value) = field.values.get(*index as usize) else {
                return NextKey::Stuck("the answer names a value the field does not have");
            };
            match rows.iter().position(|(_, label)| label == value) {
                Some(target) => target,
                None => return NextKey::Stuck("the field does not show that value"),
            }
        }
        ([], Some(_)) => match rows
            .iter()
            .position(|(_, label)| label.starts_with(OTHER_PREFIX))
        {
            Some(target) => target,
            None => return NextKey::Stuck("the field offers no answer of one's own"),
        },
        _ => return NextKey::Stuck("a choice takes one value or text"),
    };
    if at < target {
        return NextKey::Press(Input::Down);
    }
    if at > target {
        return NextKey::Stuck("the highlight is below the answer");
    }
    match (answer.selected.is_empty(), text, form.other_text.as_deref()) {
        (true, Some(text), Some("")) => NextKey::Press(Input::Text(text)),
        (true, Some(text), Some(shown)) if !holds(shown, &text) => {
            NextKey::Stuck("the answer of one's own holds other text")
        }
        (true, Some(_), None) => NextKey::Stuck("the field for an answer of one's own is not open"),
        _ => NextKey::Press(Input::Enter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::snapshot::TerminalSnapshot;

    /// The permission captures (`testdata/copilot-captures.md`, "Interactions"), each the bytes
    /// written since the one before.
    const PERMISSION: &[(&str, &[u8])] = &[
        (
            "permission",
            include_bytes!("../../../testdata/copilot_permission_120_01_permission.bin"),
        ),
        (
            "on_third",
            include_bytes!("../../../testdata/copilot_permission_120_02_on_third.bin"),
        ),
        (
            "feedback_typed",
            include_bytes!("../../../testdata/copilot_permission_120_03_feedback_typed.bin"),
        ),
        (
            "denied",
            include_bytes!("../../../testdata/copilot_permission_120_04_denied.bin"),
        ),
    ];

    const QUESTION: &[(&str, &[u8])] = &[
        (
            "form",
            include_bytes!("../../../testdata/copilot_question_120_01_form.bin"),
        ),
        (
            "other",
            include_bytes!("../../../testdata/copilot_question_120_02_other.bin"),
        ),
        (
            "other_typed",
            include_bytes!("../../../testdata/copilot_question_120_03_other_typed.bin"),
        ),
        (
            "text_field",
            include_bytes!("../../../testdata/copilot_question_120_04_text_field.bin"),
        ),
        (
            "text_typed",
            include_bytes!("../../../testdata/copilot_question_120_05_text_typed.bin"),
        ),
        (
            "answered",
            include_bytes!("../../../testdata/copilot_question_120_06_answered.bin"),
        ),
    ];

    /// The earlier composer captures' `ask_user` form, a single choice field.
    fn single_field_form() -> String {
        let mut screen = TerminalSnapshot::new(40, 120);
        for name in [
            "01_empty",
            "02_one_line",
            "03_wrapped",
            "04_empty_line",
            "05_cleared",
            "06_during_turn",
            "07_after_cancel",
            "08_permission_prompt",
            "09_ask_user_form",
        ] {
            let path = format!(
                "{}/testdata/copilot_composer_120_{name}.bin",
                env!("CARGO_MANIFEST_DIR")
            );
            screen.process(&std::fs::read(path).expect("capture"));
        }
        screen.visible_text()
    }

    fn screen_at(captures: &[(&str, &[u8])], state: &str) -> String {
        let mut screen = TerminalSnapshot::new(40, 120);
        for (name, bytes) in captures {
            screen.process(bytes);
            if *name == state {
                return screen.visible_text();
            }
        }
        panic!("no capture {state}");
    }

    /// A numbered line above the question, at an option's indent, is no option: the options are
    /// the block numbered from 1 above the hint.
    #[test]
    fn only_the_option_block_is_read_as_options() {
        let rows = [
            "\u{256D}\u{2500}\u{2500}\u{256E}",
            "\u{2502} Run shell command                                  \u{2502}",
            "\u{2502} 1. first make the file                             \u{2502}",
            "\u{2502} 3. then list it                                    \u{2502}",
            "\u{2502} Do you want to run this command?                   \u{2502}",
            "\u{2502} \u{276F} 1. Yes                                          \u{2502}",
            "\u{2502}   2. Yes, and approve for the session              \u{2502}",
            "\u{2502}   3. No, and tell Copilot what to do differently   \u{2502}",
            "\u{2502}   7. not an option either                          \u{2502}",
            "\u{2502} \u{2191}/\u{2193} to navigate \u{b7} enter to select \u{b7} esc to cancel \u{2502}",
            "\u{2570}\u{2500}\u{2500}\u{256F}",
        ];
        let menu = Menu::of(&rows).expect("a menu");
        let options: Vec<(&str, &str)> = menu
            .options
            .iter()
            .map(|row| (row.key.as_str(), row.label.as_str()))
            .collect();
        assert_eq!(
            options,
            [
                ("1", "Yes"),
                ("2", "Yes, and approve for the session"),
                ("3", "No, and tell Copilot what to do differently"),
            ]
        );
        assert_eq!(menu.highlighted(), Some(0));
    }

    fn touch() -> CallSeen {
        CallSeen::new(
            Some("call_1".to_string()),
            "bash",
            &serde_json::json!({"command": "touch made-by-probe", "description": "make a file"}),
        )
    }

    fn other_command() -> CallSeen {
        CallSeen::new(
            None,
            "bash",
            &serde_json::json!({"command": "ls", "description": "list"}),
        )
    }

    fn two_fields() -> CallSeen {
        CallSeen::new(
            Some("call_2".to_string()),
            "ask_user",
            &serde_json::json!({"message": "Pick one?", "requestedSchema": {"properties": {
                "choice": {"type": "string", "enum": ["alpha", "beta"]},
                "note": {"type": "string", "description": "a note"}},
                "required": ["choice"]}}),
        )
    }

    fn one_field() -> CallSeen {
        CallSeen::new(
            Some("call_3".to_string()),
            "ask_user",
            &serde_json::json!({"message": "Pick one?", "requestedSchema": {"properties": {
                "choice": {"type": "string", "enum": ["alpha", "beta"]}},
                "required": ["choice"]}}),
        )
    }

    /// The session's own `preToolUse` is a call, its arguments as an object or as JSON text;
    /// a subagent's, and any other event, are not.
    #[test]
    fn a_call_is_the_sessions_own_pre_tool_use() {
        let payload = serde_json::json!({"toolCallId": "call_c", "toolName": "bash",
            "toolArgs": {"command": "touch x"}});
        let call = CopilotInteractions
            .call_started("preToolUse", &payload)
            .expect("a call");
        assert_eq!(call.id.as_deref(), Some("call_c"));
        assert_eq!(call.tool, "bash");
        assert_eq!(call.input["command"], "touch x");
        let text = serde_json::json!({"toolName": "edit", "toolArgs": "{\"path\":\"/a\"}"});
        let call = CopilotInteractions
            .call_started("preToolUse", &text)
            .expect("a call");
        assert_eq!(call.input["path"], "/a");
        let mut subagent = payload.clone();
        subagent["agentId"] = serde_json::json!("agent-1");
        assert_eq!(
            CopilotInteractions.call_started("preToolUse", &subagent),
            None
        );
        assert_eq!(
            CopilotInteractions.call_started("postToolUse", &payload),
            None
        );
    }

    /// The permission prompt is published for the command it shows, with its three rows: allow,
    /// allow and remember, and refuse with text.
    #[test]
    fn the_permission_prompt_is_published_for_its_command() {
        let screen = screen_at(PERMISSION, "permission");
        let reading = CopilotInteractions.read(&screen, &[touch(), other_command()]);
        let Reading::Shown { call, prompt } = reading else {
            panic!("shown: {reading:?}");
        };
        assert_eq!(call, 0);
        let InteractionPrompt::Permission(prompt) = prompt else {
            panic!("a permission prompt");
        };
        assert_eq!(prompt.tool, "bash");
        assert_eq!(prompt.category, Some(ToolCategory::Command));
        assert_eq!(prompt.title.as_deref(), Some("touch made-by-probe"));
        assert_eq!(prompt.detail.as_deref(), Some("make a file"));
        let choices: Vec<_> = prompt
            .choices
            .iter()
            .map(|choice| (choice.id.as_str(), choice.effect, choice.text))
            .collect();
        assert_eq!(
            choices,
            [
                ("1", ChoiceEffect::Allow, None),
                ("2", ChoiceEffect::AllowRemember, None),
                ("3", ChoiceEffect::Deny, Some(ChoiceText::Optional)),
            ]
        );
        // No call it shows: up, but not published.
        assert_eq!(
            CopilotInteractions.read(&screen, &[other_command()]),
            Reading::Up
        );
    }

    /// Text typed on the third row is a prompt half answered; once the call is refused the
    /// composer is back.
    #[test]
    fn feedback_being_written_is_up_and_the_composer_is_gone() {
        let typed = screen_at(PERMISSION, "feedback_typed");
        assert_eq!(CopilotInteractions.read(&typed, &[touch()]), Reading::Up);
        let denied = screen_at(PERMISSION, "denied");
        assert_eq!(CopilotInteractions.read(&denied, &[touch()]), Reading::Gone);
    }

    fn choice(id: &str, text: Option<&str>) -> Answer {
        Answer::Choice {
            id: id.to_string(),
            text: text.map(str::to_string),
        }
    }

    /// Allowing is Enter on the first row; the others are reached with Down; refusing with text
    /// pastes it on the third row and sends it with Enter; refusing with none is Esc.
    #[test]
    fn a_choice_is_pressed_with_down_paste_and_enter() {
        let fresh = screen_at(PERMISSION, "permission");
        let on_third = screen_at(PERMISSION, "on_third");
        let typed = screen_at(PERMISSION, "feedback_typed");
        let key =
            |screen: &str, answer: &Answer| CopilotInteractions.next_key(screen, &touch(), answer);
        assert_eq!(
            key(&fresh, &choice("1", None)),
            NextKey::Press(Input::Enter)
        );
        assert_eq!(key(&fresh, &choice("2", None)), NextKey::Press(Input::Down));
        assert_eq!(
            key(&fresh, &choice("3", Some("use echo"))),
            NextKey::Press(Input::Down)
        );
        assert_eq!(
            key(&on_third, &choice("3", Some("use\necho"))),
            NextKey::Press(Input::Paste("use echo".to_string()))
        );
        assert_eq!(
            key(&typed, &choice("3", Some("use echo"))),
            NextKey::Press(Input::Enter)
        );
        assert_eq!(
            key(&fresh, &choice("3", None)),
            NextKey::Press(Input::Escape)
        );
        assert_eq!(
            key(&on_third, &choice("3", Some("  "))),
            NextKey::Press(Input::Escape)
        );
        assert!(matches!(
            key(&on_third, &choice("1", None)),
            NextKey::Stuck(_)
        ));
        assert!(matches!(key(&fresh, &choice("4", None)), NextKey::Stuck(_)));
        assert!(matches!(key(&fresh, &Answer::Dismiss), NextKey::Stuck(_)));
    }

    /// A form of two string fields is published as two questions, a choice with its values and a
    /// text field, each taking text; a single field's question is the message. A form whose call
    /// has a field of another kind is up but not published.
    #[test]
    fn the_form_is_published_from_its_calls_fields() {
        let screen = screen_at(QUESTION, "form");
        let reading = CopilotInteractions.read(&screen, &[two_fields()]);
        let Reading::Shown {
            call: 0,
            prompt: InteractionPrompt::Question(prompt),
        } = reading
        else {
            panic!("shown: {reading:?}");
        };
        assert!(prompt.dismissible);
        let [choice, note] = prompt.questions.as_slice() else {
            panic!("two questions");
        };
        assert_eq!(choice.header.as_deref(), Some("choice"));
        assert_eq!(choice.text, "choice");
        let labels: Vec<_> = choice.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, ["alpha", "beta"]);
        assert_eq!(choice.free_text, Some(FreeText::Instead));
        assert_eq!(note.text, "a note");
        assert!(note.options.is_empty());

        let single = single_field_form();
        let reading = CopilotInteractions.read(&single, &[one_field()]);
        let Reading::Shown {
            prompt: InteractionPrompt::Question(prompt),
            ..
        } = reading
        else {
            panic!("shown: {reading:?}");
        };
        assert_eq!(prompt.questions[0].text, "Pick one?");

        let mut boolean = two_fields();
        boolean.input["requestedSchema"]["properties"]["note"]["type"] =
            serde_json::json!("boolean");
        assert_eq!(CopilotInteractions.read(&screen, &[boolean]), Reading::Up);
        assert_eq!(CopilotInteractions.read(&screen, &[touch()]), Reading::Up);
        let answered = screen_at(QUESTION, "answered");
        assert_eq!(
            CopilotInteractions.read(&answered, &[two_fields()]),
            Reading::Gone
        );
    }

    fn answers(first: (Vec<u32>, Option<&str>), second: Option<&str>) -> Answer {
        Answer::Questions(vec![
            QuestionAnswer {
                selected: first.0,
                text: first.1.map(str::to_string),
            },
            QuestionAnswer {
                selected: Vec::new(),
                text: second.map(str::to_string),
            },
        ])
    }

    /// A value is reached with Down and taken with Enter; an answer of one's own goes to `Other`,
    /// is typed in the field that opens and taken with Enter; a text field is typed and taken;
    /// declining is Ctrl-D.
    #[test]
    fn a_form_is_answered_field_by_field() {
        let call = two_fields();
        let key = |state: &str, answer: &Answer| {
            CopilotInteractions.next_key(&screen_at(QUESTION, state), &call, answer)
        };
        let beta = answers((vec![1], None), Some("hello"));
        assert_eq!(
            key("form", &answers((vec![0], None), None)),
            NextKey::Press(Input::Enter)
        );
        assert_eq!(key("form", &beta), NextKey::Press(Input::Down));
        let own = answers((Vec::new(), Some("zeta")), Some("hello"));
        assert_eq!(key("form", &own), NextKey::Press(Input::Down));
        assert_eq!(
            key("other", &own),
            NextKey::Press(Input::Text("zeta".to_string()))
        );
        assert_eq!(key("other_typed", &own), NextKey::Press(Input::Enter));
        assert!(matches!(key("other", &beta), NextKey::Stuck(_)));
        assert_eq!(
            key("text_field", &own),
            NextKey::Press(Input::Text("hello".to_string()))
        );
        assert_eq!(key("text_typed", &own), NextKey::Press(Input::Enter));
        assert_eq!(
            key("text_field", &answers((vec![0], None), None)),
            NextKey::Press(Input::Enter)
        );
        assert_eq!(
            key("form", &Answer::Dismiss),
            NextKey::Press(Input::Ctrl('d'))
        );
        assert!(matches!(
            key("form", &Answer::Questions(Vec::new())),
            NextKey::Stuck(_)
        ));
        assert!(matches!(key("answered", &beta), NextKey::Stuck(_)));
    }

    /// Bytes of a capture of the 2026-11-24 series (`testdata/copilot-captures.md`,
    /// "Interactions").
    macro_rules! capture {
        ($name:literal) => {
            (
                $name,
                include_bytes!(concat!("../../../testdata/", $name, ".bin")),
            )
        };
    }

    /// The probe's working directory, as the 2026-11-24 captures show it.
    const WORK: &str = "/private/tmp/nap-r5-cop/work";

    /// The screen once every capture of `captures` up to `state` is fed in.
    fn series_at(captures: &[(&str, &[u8])], state: &str) -> String {
        let mut screen = TerminalSnapshot::new(40, 120);
        for (name, bytes) in captures {
            screen.process(bytes);
            if name.ends_with(state) {
                return screen.visible_text();
            }
        }
        panic!("no capture {state}");
    }

    fn call(tool: &str, input: serde_json::Value) -> CallSeen {
        CallSeen::new(Some("call_x".to_string()), tool, &input)
    }

    fn effects(reading: &Reading) -> Vec<(String, ChoiceEffect, Option<ChoiceText>)> {
        let Reading::Shown {
            prompt: InteractionPrompt::Permission(prompt),
            ..
        } = reading
        else {
            panic!("a permission prompt shown: {reading:?}");
        };
        prompt
            .choices
            .iter()
            .map(|choice| (choice.id.clone(), choice.effect, choice.text))
            .collect()
    }

    fn effect_list(
        list: &[(&str, ChoiceEffect, Option<ChoiceText>)],
    ) -> Vec<(String, ChoiceEffect, Option<ChoiceText>)> {
        list.iter()
            .map(|(id, effect, text)| (id.to_string(), *effect, *text))
            .collect()
    }

    /// A press is for the prompt of the call it answers: once that prompt is answered, the prompt
    /// of a call queued behind it takes its place, and a key chosen for the first must not answer
    /// the second.
    #[test]
    fn a_prompt_for_another_call_is_never_pressed() {
        let fresh = screen_at(PERMISSION, "permission");
        assert_eq!(
            CopilotInteractions.next_key(&fresh, &touch(), &choice("1", None)),
            NextKey::Press(Input::Enter)
        );
        assert!(matches!(
            CopilotInteractions.next_key(&fresh, &other_command(), &choice("1", None)),
            NextKey::Stuck(_)
        ));
        let form = screen_at(QUESTION, "form");
        let mut other = two_fields();
        other.input["message"] = serde_json::json!("Something else?");
        let answer = answers((vec![0], None), None);
        assert_eq!(
            CopilotInteractions.next_key(&form, &two_fields(), &answer),
            NextKey::Press(Input::Enter)
        );
        assert!(matches!(
            CopilotInteractions.next_key(&form, &other, &answer),
            NextKey::Stuck(_)
        ));
    }

    const FEEDBACK_WRAP: &[(&str, &[u8])] = &[
        capture!("copilot_feedback_wrap_120_01_prompt"),
        capture!("copilot_feedback_wrap_120_02_on_third"),
        capture!("copilot_feedback_wrap_120_03_wrapped"),
        capture!("copilot_feedback_wrap_120_04_sent"),
    ];

    /// The feedback the 2026-11-24 capture pasted, long enough to wrap.
    const FEEDBACK: &str =
        "please do not touch that file at all and instead write the result into \
        a scratch folder that you create under the temporary directory, then list what you made \
        there and stop";

    /// Feedback that wraps onto further rows is written: it is never pasted again, it is sent
    /// with Enter while the row shows the answer's text, and the prompt is a prompt half answered.
    #[test]
    fn wrapped_feedback_is_pasted_once_and_sent() {
        let key = |state: &str, text: &str| {
            CopilotInteractions.next_key(
                &series_at(FEEDBACK_WRAP, state),
                &touch(),
                &choice("3", Some(text)),
            )
        };
        assert_eq!(
            key("02_on_third", FEEDBACK),
            NextKey::Press(Input::Paste(FEEDBACK.to_string()))
        );
        assert_eq!(key("03_wrapped", FEEDBACK), NextKey::Press(Input::Enter));
        assert!(matches!(
            key("03_wrapped", "something else entirely"),
            NextKey::Stuck(_)
        ));
        let wrapped = series_at(FEEDBACK_WRAP, "03_wrapped");
        assert_eq!(CopilotInteractions.read(&wrapped, &[touch()]), Reading::Up);
        let sent = series_at(FEEDBACK_WRAP, "04_sent");
        assert_eq!(CopilotInteractions.read(&sent, &[touch()]), Reading::Gone);
    }

    const CREATE: &[(&str, &[u8])] = &[capture!("copilot_permission_create_120_01_prompt")];
    const EDIT: &[(&str, &[u8])] = &[capture!("copilot_permission_edit_120_01_prompt")];
    const VIEW: &[(&str, &[u8])] = &[
        capture!("copilot_permission_view_120_01_prompt"),
        capture!("copilot_permission_view_120_02_on_no"),
        capture!("copilot_permission_view_120_03_chosen"),
    ];
    const MCP: &[(&str, &[u8])] = &[
        capture!("copilot_permission_mcp_120_01_prompt"),
        capture!("copilot_permission_mcp_120_02_refused"),
    ];

    /// A file created or edited is matched by the prompt's question, which names its path; its
    /// rows are allow, allow and remember for the directory, and refuse with text.
    #[test]
    fn a_file_operation_is_matched_by_the_path_its_question_names() {
        let three = effect_list(&[
            ("1", ChoiceEffect::Allow, None),
            ("2", ChoiceEffect::AllowRemember, None),
            ("3", ChoiceEffect::Deny, Some(ChoiceText::Optional)),
        ]);
        let create = call(
            "create",
            serde_json::json!({"path": format!("{WORK}/made.txt"), "file_text": "hi"}),
        );
        let screen = series_at(CREATE, "01_prompt");
        assert_eq!(
            effects(&CopilotInteractions.read(&screen, &[create])),
            three
        );
        let elsewhere = call(
            "create",
            serde_json::json!({"path": format!("{WORK}/other.txt"), "file_text": "hi"}),
        );
        assert_eq!(CopilotInteractions.read(&screen, &[elsewhere]), Reading::Up);
        let edit = call(
            "edit",
            serde_json::json!({"path": format!("{WORK}/existing.txt"), "old_str": "hi", "new_str": "ho"}),
        );
        let screen = series_at(EDIT, "01_prompt");
        assert_eq!(effects(&CopilotInteractions.read(&screen, &[edit])), three);
    }

    /// A file read outside the allowed paths is matched by the path in its box; its two rows that
    /// remember for the session are allow and remember, and `No (Esc)` refuses with no text,
    /// reached with Down and taken with Enter.
    #[test]
    fn a_read_outside_the_allowed_paths_is_matched_by_its_path() {
        let view = call("view", serde_json::json!({"path": "/etc/hosts"}));
        let screen = series_at(VIEW, "01_prompt");
        let reading = CopilotInteractions.read(&screen, std::slice::from_ref(&view));
        assert_eq!(
            effects(&reading),
            effect_list(&[
                ("1", ChoiceEffect::Allow, None),
                ("2", ChoiceEffect::AllowRemember, None),
                ("3", ChoiceEffect::AllowRemember, None),
                ("4", ChoiceEffect::Deny, None),
            ])
        );
        let other = call("view", serde_json::json!({"path": "/etc/passwd"}));
        assert_eq!(CopilotInteractions.read(&screen, &[other]), Reading::Up);
        assert_eq!(
            CopilotInteractions.next_key(&screen, &view, &choice("4", None)),
            NextKey::Press(Input::Down)
        );
        let on_no = series_at(VIEW, "02_on_no");
        assert_eq!(
            CopilotInteractions.next_key(&on_no, &view, &choice("4", None)),
            NextKey::Press(Input::Enter)
        );
        let chosen = series_at(VIEW, "03_chosen");
        assert_eq!(CopilotInteractions.read(&chosen, &[view]), Reading::Gone);
    }

    /// An MCP server's tool is matched by its name on the box's first row and its arguments in
    /// their box; the same tool with other arguments is not that prompt's call.
    #[test]
    fn an_mcp_tool_is_matched_by_its_name_and_arguments() {
        let slow = call("octoboard-slow", serde_json::json!({"seconds": 0}));
        let screen = series_at(MCP, "01_prompt");
        let reading = CopilotInteractions.read(&screen, std::slice::from_ref(&slow));
        assert_eq!(
            effects(&reading),
            effect_list(&[
                ("1", ChoiceEffect::Allow, None),
                ("2", ChoiceEffect::AllowRemember, None),
                ("3", ChoiceEffect::Deny, Some(ChoiceText::Optional)),
            ])
        );
        let other_args = call("octoboard-slow", serde_json::json!({"seconds": 5}));
        assert_eq!(
            CopilotInteractions.read(&screen, &[other_args]),
            Reading::Up
        );
        let other_tool = call("octoboard-ask", serde_json::json!({"seconds": 0}));
        assert_eq!(
            CopilotInteractions.read(&screen, &[other_tool]),
            Reading::Up
        );
        let refused = series_at(MCP, "02_refused");
        assert_eq!(CopilotInteractions.read(&refused, &[slow]), Reading::Gone);
    }

    /// `touch` with `count` segments, as the long-command captures ran it.
    fn long_touch(count: usize) -> CallSeen {
        let segments: Vec<String> = (0..count)
            .map(|i| format!("segment-{i:03}-abcdefgh"))
            .collect();
        call(
            "bash",
            serde_json::json!({"command": format!("touch {}", segments.join(" ")),
                "description": "a long echo"}),
        )
    }

    /// A command that wraps in its box is matched; so is one too long for the screen, whose box
    /// shows only its start under a hint that adds `pgup/pgdn to scroll`, and whose kept text is
    /// cut. A command that differs within the start compared is not.
    #[test]
    fn long_and_wrapped_commands_are_matched() {
        let long = series_at(
            &[capture!("copilot_permission_long_120_01_prompt")],
            "01_prompt",
        );
        let reading = CopilotInteractions.read(&long, &[long_touch(40)]);
        assert!(
            matches!(reading, Reading::Shown { call: 0, .. }),
            "{reading:?}"
        );
        let mut renamed = long_touch(40);
        renamed.input["command"] = serde_json::json!(renamed.input["command"]
            .as_str()
            .unwrap()
            .replacen("segment-000", "segment-999", 1));
        assert_eq!(CopilotInteractions.read(&long, &[renamed]), Reading::Up);
        let huge = series_at(
            &[capture!("copilot_permission_huge_120_01_prompt")],
            "01_prompt",
        );
        let cut = long_touch(200);
        assert!(
            cut.input_truncated,
            "the capture's command is longer than what is kept"
        );
        let reading = CopilotInteractions.read(&huge, std::slice::from_ref(&cut));
        assert!(
            matches!(reading, Reading::Shown { call: 0, .. }),
            "{reading:?}"
        );
        assert_eq!(
            CopilotInteractions.next_key(&huge, &cut, &choice("1", None)),
            NextKey::Press(Input::Enter)
        );
        let mut differs = cut.clone();
        differs.input["command"] = serde_json::json!("touch segment-000-abcdefgh other");
        assert_eq!(CopilotInteractions.read(&huge, &[differs]), Reading::Up);
    }

    const QUESTION_WRAP: &[(&str, &[u8])] = &[
        capture!("copilot_question_wrap_120_01_form"),
        capture!("copilot_question_wrap_120_02_typed"),
        capture!("copilot_question_wrap_120_03_ctrl_d"),
        capture!("copilot_question_wrap_120_04_esc"),
        capture!("copilot_question_wrap_120_05_declined"),
    ];

    /// The `ask_user` call of the wrapped-form capture: a message that wraps, two text fields.
    fn wrapped_form() -> CallSeen {
        let message = ["Before I go on I need two things from you."; 5].join(" ");
        call(
            "ask_user",
            serde_json::json!({"message": message, "requestedSchema": {"type": "object",
                "properties": {"first": {"type": "string", "description": "the first"},
                "second": {"type": "string", "description": "the second"}},
                "required": ["first"]}}),
        )
    }

    /// A form whose message wraps is matched, its fields' row found below the message; text that
    /// wraps in its field is the answer's, taken with Enter, and other text is not.
    #[test]
    fn a_form_whose_message_and_text_wrap_is_answered() {
        let form = wrapped_form();
        let answer_text = "a long answer typed into the first text field that goes on ".repeat(3);
        let answer = |first: &str| {
            Answer::Questions(vec![
                QuestionAnswer {
                    selected: Vec::new(),
                    text: Some(first.to_string()),
                },
                QuestionAnswer {
                    selected: Vec::new(),
                    text: None,
                },
            ])
        };
        let fresh = series_at(QUESTION_WRAP, "01_form");
        let reading = CopilotInteractions.read(&fresh, std::slice::from_ref(&form));
        let Reading::Shown {
            prompt: InteractionPrompt::Question(prompt),
            ..
        } = reading
        else {
            panic!("shown: {reading:?}");
        };
        assert_eq!(prompt.questions.len(), 2);
        assert_eq!(
            CopilotInteractions.next_key(&fresh, &form, &answer(&answer_text)),
            NextKey::Press(Input::Text(answer_text.trim().to_string()))
        );
        let typed = series_at(QUESTION_WRAP, "02_typed");
        assert_eq!(
            CopilotInteractions.next_key(&typed, &form, &answer(&answer_text)),
            NextKey::Press(Input::Enter)
        );
        assert!(matches!(
            CopilotInteractions.next_key(&typed, &form, &answer("another answer")),
            NextKey::Stuck(_)
        ));
        let declined = series_at(QUESTION_WRAP, "05_declined");
        assert_eq!(CopilotInteractions.read(&declined, &[form]), Reading::Gone);
    }

    /// Declining is Ctrl-D, but a text field that holds text takes no Ctrl-D (measured: the form
    /// stays), so Esc empties it first; a form of another call is not declined.
    #[test]
    fn declining_empties_a_field_that_holds_text_first() {
        let form = wrapped_form();
        let key = |state: &str, call: &CallSeen| {
            CopilotInteractions.next_key(&series_at(QUESTION_WRAP, state), call, &Answer::Dismiss)
        };
        assert_eq!(key("01_form", &form), NextKey::Press(Input::Ctrl('d')));
        assert_eq!(key("02_typed", &form), NextKey::Press(Input::Escape));
        assert_eq!(key("03_ctrl_d", &form), NextKey::Press(Input::Escape));
        assert_eq!(key("04_esc", &form), NextKey::Press(Input::Ctrl('d')));
        assert!(matches!(key("04_esc", &two_fields()), NextKey::Stuck(_)));
    }
}
