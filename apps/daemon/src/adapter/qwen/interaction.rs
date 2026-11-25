//! How Qwen Code's pending interactions are read and answered: its tool permission prompt, and the
//! form of its own `ask_user_question` tool. Measured on 0.28.3 against a local stand-in, the
//! screens in `testdata/qwen-captures.md`, summarised in `docs/agent-cli-reference.md`, "Qwen
//! Code", "Answering its prompts".
//!
//! What the hooks give is the call (`PreToolUse`: the tool, its id and its input); what the prompt
//! offers, which row the cursor is on and how far a form has been answered show only on the
//! screen. Either is drawn in place of the input box, under the call's row, which starts `? ` and
//! ends with `←`, down to the `Waiting for user confirmation...` row:
//!
//! - a permission prompt: the call, then what it does (the command, the diff, the address, the MCP
//!   server and tool), then `N. label` rows, the cursor's marked `›`. A digit chooses its row at
//!   once. "No, suggest changes" declines the call and ends the turn with no text taken; "Yes,
//!   allow always" of a file change lets it run and switches the launch to accepting file changes
//!   ("auto-accept edits") until it exits, writing nothing; "Always allow …" records a rule where
//!   it says.
//! - a question form: for more than one question a tab row (`▸ Colour   Size  Submit`, an answered
//!   tab marked `✓`), else the header alone; the question; the options `N. label`, each with its
//!   description on the row below, `[ ]` / `[✓]` before them where more than one may be chosen, the
//!   cursor's marked `❯`; and a last `Type something...` row, which opens as a field where the
//!   cursor rests (`> Type something...`, then `> <text>`). A digit chooses a single choice's
//!   option and goes on to the next question; on a multiple choice it only moves the cursor, Space
//!   ticks the row and Enter goes on. After the last question of several comes a `Submit` tab
//!   (`Ready to submit your answers?`, `1. Submit answers`); a single question has none. Enter in
//!   the field goes straight to `Submit`, past any question after it, so only the last question
//!   takes a written answer. Esc dismisses the form.
//!
//! While the history scrolls, Qwen Code draws a scrollbar in the last column of the rows above the
//! closing row (`█` on its thumb, `│` on its track), which fades a few seconds after; it is taken
//! off each row before the prompt is read.
//!
//! Only the arrow Down is a key a press may write besides text, Enter and Esc
//! (`crate::platform::term::Input`), and Down does not wrap; digits reach every row instead.

use serde_json::Value;

use super::screen::input_box_present;
use crate::platform::term::Input;
use crate::protocol::{
    AgentId, ChoiceEffect, FreeText, InteractionPrompt, PermissionChoice, PermissionPrompt,
    Question, QuestionAnswer, QuestionOption, QuestionPrompt, ToolCategory,
};
use crate::session::interaction::{Answer, CallSeen, InteractionReader, NextKey, Reading};

pub(super) struct QwenInteractions;

impl InteractionReader for QwenInteractions {
    fn call_started(&self, event: &str, payload: &Value) -> Option<CallSeen> {
        if event != "PreToolUse" {
            return None;
        }
        let text = |key: &str| payload.get(key).and_then(Value::as_str);
        Some(CallSeen::new(
            text("tool_use_id")
                .or(text("tool_call_id"))
                .map(str::to_string),
            text("tool_name")?,
            payload.get("tool_input").unwrap_or(&Value::Null),
        ))
    }

    fn read(&self, screen: &str, calls: &[CallSeen]) -> Reading {
        match View::of(screen) {
            View::Prompt(prompt) if prompt.display == QUESTION_DISPLAY => question(&prompt, calls),
            View::Prompt(prompt) => permission(&prompt, calls),
            View::InputBox => Reading::Gone,
            View::Other => Reading::Unknown,
        }
    }

    fn next_key(&self, screen: &str, call: &CallSeen, answer: &Answer) -> NextKey {
        let View::Prompt(prompt) = View::of(screen) else {
            return NextKey::Stuck("no prompt on the screen");
        };
        match (prompt.display == QUESTION_DISPLAY, answer) {
            (false, Answer::Choice { id, text }) => {
                permission_key(&prompt, call, id, text.as_deref())
            }
            (true, Answer::Questions(answers)) => question_key(&prompt, call, answers),
            (true, Answer::Dismiss) => NextKey::Press(Input::Escape),
            _ => NextKey::Stuck("the prompt on the screen is not the kind this answer is for"),
        }
    }
}

/// The name the call's row gives `ask_user_question`.
const QUESTION_DISPLAY: &str = "AskUserQuestion";

/// The row that closes a prompt.
const WAITING: &str = "Waiting for user confirmation...";

/// What the screen shows.
enum View {
    Prompt(Prompt),
    /// No prompt, and the input box is drawn.
    InputBox,
    Other,
}

/// A prompt as drawn.
struct Prompt {
    /// The tool's name on the call's row (`Shell`, `WriteFile`, `AskUserQuestion`).
    display: String,
    /// The rest of the call's row, without the closing `←`.
    title: String,
    /// The rows between the call's row and the closing row, trimmed.
    rows: Vec<String>,
}

impl View {
    fn of(screen: &str) -> View {
        let rows: Vec<&str> = screen
            .lines()
            .map(|row| without_scrollbar(row).trim_start())
            .collect();
        let call = rows
            .iter()
            .rposition(|row| row.starts_with("? ") && row.ends_with('\u{2190}'));
        let waiting = rows.iter().rposition(|row| row.ends_with(WAITING));
        match (call, waiting) {
            (Some(call), Some(waiting)) if call < waiting && !input_box_present(screen) => {
                let line = rows[call][2..].trim_end_matches('\u{2190}').trim();
                let (display, title) = line.split_once(' ').unwrap_or((line, ""));
                View::Prompt(Prompt {
                    display: display.to_string(),
                    title: title.trim().to_string(),
                    rows: rows[call + 1..waiting]
                        .iter()
                        .map(|row| row.to_string())
                        .collect(),
                })
            }
            _ if input_box_present(screen) => View::InputBox,
            _ => View::Other,
        }
    }
}

/// `row` without the scrollbar cell at its end: a `█` or `│` after a space, as Qwen Code draws it
/// in the last column while the history scrolls (0.28.3).
fn without_scrollbar(row: &str) -> &str {
    let row = row.trim_end();
    match row.strip_suffix(['\u{2588}', '\u{2502}']) {
        Some(rest) if rest.is_empty() || rest.ends_with(' ') => rest.trim_end(),
        _ => row,
    }
}

/// An option row: `› 2. label`, `❯ [✓] 1. label`, `3. > text`.
#[derive(Debug)]
struct Row {
    number: u32,
    cursor: bool,
    /// `Some` where the row has a box to tick, `true` ticked.
    ticked: Option<bool>,
    label: String,
}

impl Row {
    fn parse(row: &str) -> Option<Row> {
        let (cursor, rest) = match row
            .strip_prefix('\u{203A}')
            .or_else(|| row.strip_prefix('\u{276F}'))
        {
            Some(rest) => (true, rest.trim_start()),
            None => (false, row),
        };
        let (ticked, rest) = if let Some(rest) = rest.strip_prefix("[ ]") {
            (Some(false), rest.trim_start())
        } else if let Some(rest) = rest.strip_prefix("[\u{2713}]") {
            (Some(true), rest.trim_start())
        } else {
            (None, rest)
        };
        let (number, label) = rest.split_once(". ")?;
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Row {
            number: number.parse().ok()?,
            cursor,
            ticked,
            label: label.trim().to_string(),
        })
    }

    /// The written answer's row: `Type something...`, or a field open on it (`> …`).
    fn is_field(&self) -> bool {
        self.label == TYPE_SOMETHING || self.label.starts_with("> ")
    }

    /// The text written in the open field; `None` where it is not open or holds nothing yet.
    fn written(&self) -> Option<&str> {
        self.label
            .strip_prefix("> ")
            .filter(|text| *text != TYPE_SOMETHING)
    }
}

/// The last row of a question's options.
const TYPE_SOMETHING: &str = "Type something...";

fn option_rows(prompt: &Prompt) -> Vec<Row> {
    prompt
        .rows
        .iter()
        .filter_map(|row| Row::parse(row))
        .collect()
}

/// `text` with its whitespace made single spaces, for comparing what wraps.
fn normalised(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---- permission prompts ----

fn permission(prompt: &Prompt, calls: &[CallSeen]) -> Reading {
    let choices: Vec<PermissionChoice> = option_rows(prompt)
        .into_iter()
        .filter_map(|row| {
            Some(PermissionChoice {
                id: row.number.to_string(),
                effect: classify(&row.label)?,
                label: row.label,
                text: None,
            })
        })
        .collect();
    if choices.is_empty() {
        return Reading::Up;
    }
    let Some(at) = calls.iter().rposition(|call| asks_about(prompt, call)) else {
        return Reading::Up;
    };
    let call = &calls[at];
    let (category, title) = describe(call);
    Reading::Shown {
        call: at,
        prompt: InteractionPrompt::Permission(PermissionPrompt {
            tool: call.tool.clone(),
            category: Some(category),
            title,
            detail: call
                .input
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            input: call.input.clone(),
            input_truncated: call.input_truncated,
            choices,
        }),
    }
}

/// What choosing a row does, by its label (0.28.3). The two "Always allow …" rows of a command,
/// an address or an MCP tool record a rule in the project's or the user's settings, as they say;
/// "Yes, allow always" of a file change switches the launch to accepting file changes until it
/// exits, and writes nothing (measured: the footer read "auto-accept edits", the next write and
/// edit ran with no prompt, a command still asked, and a new launch asked again).
fn classify(label: &str) -> Option<ChoiceEffect> {
    if label == "Yes, allow once" {
        Some(ChoiceEffect::Allow)
    } else if label == "Yes, allow always" || label.starts_with("Always allow ") {
        Some(ChoiceEffect::AllowRemember)
    } else if label.starts_with("No, suggest changes") {
        Some(ChoiceEffect::Deny)
    } else {
        None
    }
}

/// The class and one-line title of a call the prompts are measured for.
fn describe(call: &CallSeen) -> (ToolCategory, Option<String>) {
    describe_tool(&call.tool, &call.input)
}

pub(super) fn describe_tool(name: &str, input: &Value) -> (ToolCategory, Option<String>) {
    let text = |key: &str| input.get(key).and_then(Value::as_str).map(str::to_owned);
    match name {
        "run_shell_command" => (ToolCategory::Command, text("command")),
        "write_file" | "edit" => (ToolCategory::FileEdit, text("file_path")),
        "notebook_edit" => (ToolCategory::FileEdit, text("notebook_path")),
        "read_file" => (ToolCategory::FileRead, text("file_path")),
        "glob" | "grep_search" => (ToolCategory::Search, text("pattern")),
        "web_fetch" => (ToolCategory::Web, text("url")),
        tool if mcp_tool(tool).is_some() => (ToolCategory::Mcp, Some(tool.to_string())),
        _ => (ToolCategory::Other, None),
    }
}

/// Whether `prompt` asks about `call`, by what it shows of it (0.28.3):
///
/// - a command: the rows of the command, the block above `Allow execution of: …`, are the call's
///   command line for line (a row too long for the terminal wraps into more rows);
/// - a file write or edit: the call's row, `WriteFile Writing to <file>` or `Edit <file>`;
/// - a notebook cell: `NotebookEdit <what it does> in <file>`;
/// - a fetch: the address's own row under `URLs to fetch:` (`- <url>`);
/// - an MCP tool (`mcp__<server>__<tool>`): its `MCP Server: <server>` and `Tool: <tool>` rows;
/// - any other tool, its prompts not measured: a text of the call's input that is a whole row, or
///   the end of one after `: `, as Claude Code's reader matches them, on a prompt not drawn for a
///   measured tool.
///
/// A file is named as written or relative to the working directory.
fn asks_about(prompt: &Prompt, call: &CallSeen) -> bool {
    let text = |key: &str| call.input.get(key).and_then(Value::as_str);
    match call.tool.as_str() {
        "run_shell_command" => {
            prompt.display == "Shell" && text("command").is_some_and(|c| shows_command(prompt, c))
        }
        "write_file" => {
            prompt.display == "WriteFile"
                && text("file_path").is_some_and(|path| {
                    prompt
                        .title
                        .strip_prefix("Writing to ")
                        .is_some_and(|shown| shows_path(shown.trim(), path))
                })
        }
        "edit" => {
            prompt.display == "Edit"
                && text("file_path").is_some_and(|path| shows_path(&prompt.title, path))
        }
        "notebook_edit" => {
            prompt.display == "NotebookEdit"
                && text("notebook_path").is_some_and(|path| {
                    prompt
                        .title
                        .rsplit_once(" in ")
                        .is_some_and(|(_, shown)| shows_path(shown.trim(), path))
                })
        }
        "web_fetch" => {
            prompt.display == "WebFetch"
                && text("url").is_some_and(|url| {
                    let row = format!("- {}", normalised(url));
                    prompt.rows.iter().any(|shown| normalised(shown) == row)
                })
        }
        tool => match mcp_tool(tool) {
            Some((server, name)) => {
                let has = |row: String| prompt.rows.iter().any(|shown| normalised(shown) == row);
                has(format!("MCP Server: {server}")) && has(format!("Tool: {name}"))
            }
            None => !drawn_for_a_measured_tool(prompt) && shows_some_text(prompt, &call.input),
        },
    }
}

/// The names the call's row gives the tools whose prompts are measured (0.28.3).
const MEASURED_DISPLAYS: [&str; 6] = [
    "Shell",
    "WriteFile",
    "Edit",
    "NotebookEdit",
    "WebFetch",
    QUESTION_DISPLAY,
];

/// Whether `prompt` is drawn for a tool whose prompts are measured: its call's row names one of
/// them, or it is an MCP tool's (`? <tool> (<server> MCP Server) …`, with `MCP Server: <server>`
/// among its rows). Such a prompt belongs to that tool's call alone, also when the call failed to
/// match it, so the text fallback of an unmeasured tool never claims it.
fn drawn_for_a_measured_tool(prompt: &Prompt) -> bool {
    MEASURED_DISPLAYS.contains(&prompt.display.as_str())
        || prompt.title.starts_with('(') && prompt.title.contains(" MCP Server)")
        || prompt
            .rows
            .iter()
            .any(|row| normalised(row).starts_with("MCP Server: "))
}

/// The server and the tool of an MCP tool's name, `mcp__<server>__<tool>`.
fn mcp_tool(tool: &str) -> Option<(&str, &str)> {
    let (server, name) = tool.strip_prefix("mcp__")?.split_once("__")?;
    (!server.is_empty() && !name.is_empty()).then_some((server, name))
}

/// The prompt's rows parted where a blank row comes, each part its rows.
fn blocks(prompt: &Prompt) -> Vec<Vec<&str>> {
    prompt
        .rows
        .split(|row| row.is_empty())
        .filter(|block| !block.is_empty())
        .map(|block| block.iter().map(String::as_str).collect())
        .collect()
}

/// Whether the rows of the command, the block above `Allow execution of: …`, are `command`: the
/// same lines, each with its whitespace made single spaces, or, where a line was too long for the
/// terminal and wrapped into more rows, the same text with no whitespace at all.
fn shows_command(prompt: &Prompt, command: &str) -> bool {
    let blocks = blocks(prompt);
    let Some(at) = blocks
        .iter()
        .position(|block| block[0].starts_with("Allow execution of"))
    else {
        return false;
    };
    let Some(shown) = at.checked_sub(1).map(|before| &blocks[before]) else {
        return false;
    };
    let lines: Vec<String> = command
        .lines()
        .map(normalised)
        .filter(|line| !line.is_empty())
        .collect();
    let shown: Vec<String> = shown.iter().map(|row| normalised(row)).collect();
    let squeezed = |rows: &[String]| -> String { rows.concat().split_whitespace().collect() };
    !lines.is_empty()
        && (shown == lines || (shown.len() > lines.len() && squeezed(&shown) == squeezed(&lines)))
}

/// The shortest text of a call's input matched as a row on its own, so that a one- or two-letter
/// value does not match a row by chance.
const MIN_SHOWN: usize = 3;

/// Whether a text of `input` (a string at its top level) is a whole row of `prompt` after its call's
/// row, or ends one after `: `, quoted or not.
fn shows_some_text(prompt: &Prompt, input: &Value) -> bool {
    let Some(input) = input.as_object() else {
        return false;
    };
    input
        .values()
        .filter_map(Value::as_str)
        .map(normalised)
        .filter(|text| text.chars().count() >= MIN_SHOWN)
        .any(|text| {
            prompt.rows.iter().any(|row| {
                let row = normalised(row);
                row == text
                    || row.ends_with(&format!(": {text}"))
                    || row.ends_with(&format!(": \"{text}\""))
            })
        })
}

/// Whether `shown` names `path`: as written, or relative to a directory above it.
fn shows_path(shown: &str, path: &str) -> bool {
    !shown.is_empty() && (path == shown || path.ends_with(&format!("/{shown}")))
}

/// The key for the choice `id` of the prompt on the screen, which must still be the one asking
/// about `call`: a prompt that came up after it must not take the key.
fn permission_key(prompt: &Prompt, call: &CallSeen, id: &str, text: Option<&str>) -> NextKey {
    if !asks_about(prompt, call) {
        return NextKey::Stuck("the prompt on the screen is not the call's");
    }
    if text.is_some() {
        return NextKey::Stuck("no choice of this prompt takes text");
    }
    if !option_rows(prompt)
        .iter()
        .any(|row| row.number.to_string() == id)
    {
        return NextKey::Stuck("the prompt no longer offers that choice");
    }
    NextKey::Press(Input::Text(id.to_string()))
}

// ---- question forms ----

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
                multi_select: question
                    .get("multiSelect")
                    .and_then(Value::as_bool)
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

/// Where a form stands.
enum Step {
    /// The question at this index of the call's is shown.
    Question(usize),
    /// The `Submit` tab.
    Submit,
}

/// The tab row of a form of several questions: the one holding `Submit` with a `▸` on it.
fn tab_row(prompt: &Prompt) -> Option<&str> {
    prompt
        .rows
        .iter()
        .find(|row| row.contains('\u{25B8}') && row.ends_with("Submit"))
        .map(String::as_str)
}

/// Which question, or the `Submit` tab, the form shows, for `questions` of the call.
fn step(prompt: &Prompt, questions: &[Asked]) -> Option<Step> {
    let Some(tabs) = tab_row(prompt) else {
        return (questions.len() == 1).then_some(Step::Question(0));
    };
    let (before, after) = tabs.split_once('\u{25B8}')?;
    let after = after.trim_start();
    if after == "Submit" {
        return Some(Step::Submit);
    }
    // Every question has a header: Qwen Code refuses a call with a header missing or blank
    // (`"header" must be a non-empty string`, 0.28), so no form is drawn for one.
    let headers: Vec<&str> = questions
        .iter()
        .map(|question| question.header.as_deref().filter(|h| !h.trim().is_empty()))
        .collect::<Option<_>>()?;
    // The active tab is the one right after the marker: the tabs before it are the questions'
    // headers in their order, each found after the one before, so a header drawn twice or one
    // that begins another is counted where it is.
    let mut rest = before;
    let mut index = 0;
    for header in &headers {
        let Some(at) = rest.find(header) else {
            break;
        };
        rest = &rest[at + header.len()..];
        index += 1;
    }
    after
        .starts_with(headers.get(index)?)
        .then_some(Step::Question(index))
}

/// The question text a form shows: the rows after the tab row, or the header row, up to the
/// first option.
fn shown_text(prompt: &Prompt, has_tabs: bool) -> String {
    let first_option = prompt
        .rows
        .iter()
        .position(|row| Row::parse(row).is_some())
        .unwrap_or(prompt.rows.len());
    let skip_label = if has_tabs {
        prompt
            .rows
            .iter()
            .position(|row| row.contains('\u{25B8}'))
            .map_or(0, |at| at + 1)
    } else {
        // The header, alone on the first row that holds text.
        prompt
            .rows
            .iter()
            .position(|row| !row.is_empty())
            .map_or(0, |at| at + 1)
    };
    normalised(&prompt.rows[skip_label.min(first_option)..first_option].join(" "))
}

fn question(prompt: &Prompt, calls: &[CallSeen]) -> Reading {
    let rows = option_rows(prompt);
    // Only the form as Qwen Code drew it is published: one half answered is left to the terminal.
    let untouched = rows.first().is_some_and(|row| row.cursor)
        && rows.iter().all(|row| row.ticked != Some(true))
        && rows.last().is_some_and(|row| row.label == TYPE_SOMETHING)
        && tab_row(prompt).is_none_or(|tabs| {
            tabs.trim_start().starts_with('\u{25B8}') && !tabs.contains('\u{2713}')
        });
    if !untouched {
        return Reading::Up;
    }
    let has_tabs = tab_row(prompt).is_some();
    let text = shown_text(prompt, has_tabs);
    let options = &rows[..rows.len() - 1];
    let ask_tool = AgentId::QWEN.descriptor();
    let Some((at, questions)) = calls.iter().enumerate().rev().find_map(|(at, call)| {
        let questions = ask_tool
            .is_ask_question_tool(Some(&call.tool))
            .then(|| asked(call))??;
        let first = questions.first()?;
        let fits = (questions.len() > 1) == has_tabs
            && text == normalised(&first.text)
            && first.options.len() == options.len()
            && options
                .iter()
                .all(|row| row.ticked.is_some() == first.multi_select)
            && first
                .options
                .iter()
                .zip(options)
                .all(|((label, _), row)| normalised(label) == normalised(&row.label));
        fits.then_some((at, questions))
    }) else {
        return Reading::Up;
    };
    let count = questions.len();
    Reading::Shown {
        call: at,
        prompt: InteractionPrompt::Question(QuestionPrompt {
            questions: questions
                .into_iter()
                .enumerate()
                .map(|(index, question)| Question {
                    text: question.text,
                    header: question.header,
                    multi_select: question.multi_select,
                    options: question
                        .options
                        .into_iter()
                        .map(|(label, description)| QuestionOption { label, description })
                        .collect(),
                    // Enter in the field goes straight to `Submit`, so only the last question
                    // takes a written answer; ticked rows stay ticked beside it.
                    free_text: (index + 1 == count).then_some(if question.multi_select {
                        FreeText::With
                    } else {
                        FreeText::Instead
                    }),
                })
                .collect(),
            dismissible: true,
        }),
    }
}

fn question_key(prompt: &Prompt, call: &CallSeen, answers: &[QuestionAnswer]) -> NextKey {
    let Some(questions) = asked(call) else {
        return NextKey::Stuck("the call's questions cannot be read");
    };
    if answers.len() != questions.len() {
        return NextKey::Stuck("the answers do not fit the questions");
    }
    // A question left out has no key measured that passes it.
    if answers
        .iter()
        .any(|answer| answer.selected.is_empty() && answer.text.is_none())
    {
        return NextKey::Stuck("every question needs an answer");
    }
    if answers[..answers.len() - 1]
        .iter()
        .any(|answer| answer.text.is_some())
    {
        return NextKey::Stuck("only the last question takes a written answer");
    }
    match step(prompt, &questions) {
        Some(Step::Submit) => {
            let rows = option_rows(prompt);
            match rows.iter().find(|row| row.cursor) {
                Some(row) if row.number == 1 && row.label == "Submit answers" => {
                    NextKey::Press(Input::Enter)
                }
                _ => NextKey::Stuck("the cursor is not on Submit answers"),
            }
        }
        Some(Step::Question(index)) => {
            answer_key(prompt, questions[index].multi_select, &answers[index])
        }
        None => NextKey::Stuck("the question on the screen is not one of the call's"),
    }
}

/// The next key towards `answer` on the question the form shows.
fn answer_key(prompt: &Prompt, multi_select: bool, answer: &QuestionAnswer) -> NextKey {
    let rows = option_rows(prompt);
    let Some(field) = rows.last().filter(|row| row.is_field()) else {
        return NextKey::Stuck("the question's rows are not readable");
    };
    let options = &rows[..rows.len() - 1];
    let digit = |number: u32| NextKey::Press(Input::Text(number.to_string()));
    if multi_select {
        // A digit moves the cursor to its row and Space ticks it.
        if let Some(row) = options
            .iter()
            .find(|row| row.ticked != Some(answer.selected.contains(&(row.number - 1))))
        {
            return if row.cursor {
                NextKey::Press(Input::Text(" ".to_string()))
            } else {
                digit(row.number)
            };
        }
    } else if answer.text.is_none() {
        // A digit chooses the option and goes on.
        return match answer.selected.as_slice() {
            [one] if (*one as usize) < options.len() => digit(one + 1),
            _ => NextKey::Stuck("a single choice takes one listed option"),
        };
    }
    let Some(text) = &answer.text else {
        return NextKey::Press(Input::Enter);
    };
    match field.written() {
        Some(written) if normalised(written) == normalised(text) => NextKey::Press(Input::Enter),
        Some(_) => NextKey::Stuck("the field holds other text"),
        None if field.cursor => NextKey::Press(Input::Paste(text.clone())),
        None => digit(field.number),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::terminal::snapshot::TerminalSnapshot;

    /// A screen of `testdata/qwen-captures.md`, replayed from Qwen Code's output into a terminal of
    /// the capture's size and read as drawn, as the session gives it to this reader
    /// ([`InteractionReader::prompt_screen`]).
    macro_rules! capture {
        ($name:literal) => {
            &replayed(include_bytes!(concat!(
                "../../../testdata/qwen_",
                $name,
                ".bin"
            )))
        };
    }

    fn replayed(bytes: &[u8]) -> String {
        let mut terminal = TerminalSnapshot::new(40, 120);
        terminal.process(bytes);
        terminal.visible_text()
    }

    /// The call of line `n` (from 0) of `testdata/qwen_hooks_prompts.jsonl`, as its `PreToolUse`
    /// reported it.
    fn hooked(n: usize) -> CallSeen {
        let line = include_str!("../../../testdata/qwen_hooks_prompts.jsonl")
            .lines()
            .nth(n)
            .expect("a payload");
        let payload: Value = serde_json::from_str(line).expect("JSON");
        QwenInteractions
            .call_started("PreToolUse", &payload)
            .expect("a call")
    }

    fn call(tool: &str, input: Value) -> CallSeen {
        CallSeen::new(Some("call-1".to_string()), tool, &input)
    }

    fn write_call() -> CallSeen {
        hooked(1)
    }

    fn shell_call(command: &str) -> CallSeen {
        call(
            "run_shell_command",
            json!({ "command": command, "description": "Make a directory" }),
        )
    }

    /// The two questions the probe asked: a single choice, then a multiple one.
    fn two_questions() -> CallSeen {
        call(
            "ask_user_question",
            json!({ "questions": [
                { "question": "Which colour do you prefer?", "header": "Colour", "multiSelect": false,
                  "options": [{ "label": "Red", "description": "The warm one" },
                              { "label": "Blue", "description": "The cool one" }] },
                { "question": "Which sizes fit?", "header": "Size", "multiSelect": true,
                  "options": [{ "label": "Small", "description": "S" },
                              { "label": "Large", "description": "L" },
                              { "label": "Huge", "description": "XL" }] },
            ]}),
        )
    }

    fn one_question(multi: bool) -> CallSeen {
        let (question, header) = if multi {
            ("Pick fruits?", "Fruits")
        } else {
            ("Pick one fruit?", "Fruit")
        };
        call(
            "ask_user_question",
            json!({ "questions": [{ "question": question, "header": header, "multiSelect": multi,
                "options": [{ "label": "Apple", "description": "red" },
                            { "label": "Pear", "description": "green" }] }]}),
        )
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

    fn choices(prompt: &PermissionPrompt) -> Vec<(&str, ChoiceEffect)> {
        prompt
            .choices
            .iter()
            .map(|choice| (choice.id.as_str(), choice.effect))
            .collect()
    }

    /// A file write's prompt is matched to its call by the file it names, and offers allowing
    /// once, allowing file changes for the rest of the launch, and declining; an edit's likewise.
    #[test]
    fn a_file_writes_prompt_offers_its_three_choices() {
        let calls = [shell_call("ls"), write_call()];
        let (at, prompt) =
            permission_of(QwenInteractions.read(capture!("prompt_write_file"), &calls));
        assert_eq!(at, 1);
        assert_eq!(prompt.tool, "write_file");
        assert_eq!(prompt.category, Some(ToolCategory::FileEdit));
        assert_eq!(
            prompt.title.as_deref(),
            Some("/scratch/q26-run/work/ok.txt")
        );
        assert_eq!(
            choices(&prompt),
            [
                ("1", ChoiceEffect::Allow),
                ("2", ChoiceEffect::AllowRemember),
                ("3", ChoiceEffect::Deny)
            ]
        );
        assert!(prompt.choices.iter().all(|choice| choice.text.is_none()));

        let (at, prompt) =
            permission_of(QwenInteractions.read(capture!("prompt_edit"), &[hooked(2)]));
        assert_eq!(at, 0);
        assert_eq!(prompt.tool, "edit");
        assert_eq!(prompt.choices.len(), 3);
    }

    /// A command's prompt is matched by the command, and offers allowing once, the two rules
    /// that remember, and declining; with the cursor moved it is the same prompt.
    #[test]
    fn a_commands_prompt_offers_its_rules_too() {
        let calls = [hooked(0)];
        let (_, prompt) = permission_of(QwenInteractions.read(capture!("prompt_shell"), &calls));
        assert_eq!(prompt.category, Some(ToolCategory::Command));
        assert_eq!(prompt.title.as_deref(), Some("mkdir probe-dir"));
        assert_eq!(prompt.detail.as_deref(), Some("Make a directory"));
        assert_eq!(
            choices(&prompt),
            [
                ("1", ChoiceEffect::Allow),
                ("2", ChoiceEffect::AllowRemember),
                ("3", ChoiceEffect::AllowRemember),
                ("4", ChoiceEffect::Deny),
            ]
        );
        permission_of(QwenInteractions.read(capture!("prompt_shell_cursor_moved"), &calls));
    }

    /// A command of two lines, its call's row wrapped, is matched by both lines; one of its lines
    /// alone is not the command.
    #[test]
    fn a_command_of_several_lines_is_matched_whole() {
        let screen = capture!("prompt_shell_multiline");
        let (_, prompt) = permission_of(QwenInteractions.read(screen, &[hooked(6)]));
        assert_eq!(prompt.title.as_deref(), Some("mkdir m1 &&\nmkdir m2"));
        for part in ["mkdir m1 &&", "mkdir m2", "mkdir m1"] {
            assert_eq!(
                QwenInteractions.read(screen, &[shell_call(part)]),
                Reading::Up,
                "{part}"
            );
        }
    }

    /// A command that only begins as the one on the screen is not its call: `git push` is not
    /// `git push --force origin main`, and the answer meant for it is not pressed there.
    #[test]
    fn a_command_that_begins_the_shown_one_is_not_its_call() {
        let screen = "\
  ? Shell git push --force origin main (Push) \u{2190}

     git push --force origin main

   Allow execution of: 'git'?

   \u{203A} 1. Yes, allow once
     2. Always allow run 'git *' commands in this project
     3. Always allow run 'git *' commands for this user
     4. No, suggest changes (esc)

  Waiting for user confirmation...
";
        let short = shell_call("git push");
        assert_eq!(
            QwenInteractions.read(screen, std::slice::from_ref(&short)),
            Reading::Up
        );
        let allow = Answer::Choice {
            id: "1".to_string(),
            text: None,
        };
        assert!(matches!(
            QwenInteractions.next_key(screen, &short, &allow),
            NextKey::Stuck(_)
        ));
        let whole = shell_call("git push --force origin main");
        permission_of(QwenInteractions.read(screen, std::slice::from_ref(&whole)));
        assert_eq!(
            QwenInteractions.next_key(screen, &whole, &allow),
            NextKey::Press(Input::Text("1".to_string()))
        );
    }

    /// A fetch is matched by its address's own row, an MCP tool by its server and tool rows, and a
    /// notebook cell by the notebook it names; each offers its rules or its allowing for the
    /// launch, and the scrollbar drawn beside a prompt is no part of it.
    #[test]
    fn a_fetch_an_mcp_tool_and_a_notebook_cell_are_matched() {
        let fetch = capture!("prompt_web_fetch");
        assert!(fetch
            .lines()
            .any(|row| row.trim_end().ends_with('\u{2588}')));
        let (_, prompt) = permission_of(QwenInteractions.read(fetch, &[hooked(3)]));
        assert_eq!(prompt.category, Some(ToolCategory::Web));
        assert_eq!(prompt.title.as_deref(), Some("http://127.0.0.1:9/page"));
        assert_eq!(
            choices(&prompt),
            [
                ("1", ChoiceEffect::Allow),
                ("2", ChoiceEffect::AllowRemember),
                ("3", ChoiceEffect::AllowRemember),
                ("4", ChoiceEffect::Deny),
            ]
        );
        let other_url = call("web_fetch", json!({ "url": "http://127.0.0.1:9/pag" }));
        assert_eq!(QwenInteractions.read(fetch, &[other_url]), Reading::Up);

        let mcp = capture!("prompt_mcp");
        let (_, prompt) = permission_of(QwenInteractions.read(mcp, &[hooked(4)]));
        assert_eq!(prompt.tool, "mcp__octoboard__nap");
        assert_eq!(prompt.category, Some(ToolCategory::Mcp));
        assert_eq!(prompt.choices.len(), 4);
        let other_tool = call("mcp__octoboard__ping", json!({ "seconds": 0 }));
        assert_eq!(QwenInteractions.read(mcp, &[other_tool]), Reading::Up);

        let notebook = capture!("prompt_notebook");
        let (_, prompt) = permission_of(QwenInteractions.read(notebook, &[hooked(5)]));
        assert_eq!(prompt.category, Some(ToolCategory::FileEdit));
        assert_eq!(
            choices(&prompt),
            [
                ("1", ChoiceEffect::Allow),
                ("2", ChoiceEffect::AllowRemember),
                ("3", ChoiceEffect::Deny)
            ]
        );
    }

    /// A tool whose prompt was not measured is matched by a text of its input drawn as a whole row
    /// or after `: `; a short text, or none, matches nothing.
    #[test]
    fn another_tools_prompt_is_matched_by_a_text_of_its_input() {
        let screen = "\
  ? WebSearch Searching the web for: \"probe query\" \u{2190}

     Search the web for: \"probe query\"

   Do you want to proceed?

   \u{203A} 1. Yes, allow once
     2. No, suggest changes (esc)

  Waiting for user confirmation...
";
        let search = call("web_search", json!({ "query": "probe query" }));
        let (_, prompt) = permission_of(QwenInteractions.read(screen, &[search]));
        assert_eq!(prompt.category, Some(ToolCategory::Other));
        assert_eq!(
            choices(&prompt),
            [("1", ChoiceEffect::Allow), ("2", ChoiceEffect::Deny)]
        );
        for input in [
            json!({ "query": "probe" }),
            json!({ "query": "y" }),
            json!({}),
        ] {
            assert_eq!(
                QwenInteractions.read(screen, &[call("web_search", input.clone())]),
                Reading::Up,
                "{input}"
            );
        }
    }

    /// The text fallback of an unmeasured tool never claims a prompt drawn for a measured tool,
    /// though a text of its input is drawn there: the prompt is that tool's call's, which failed
    /// to match it.
    #[test]
    fn a_measured_tools_prompt_is_not_claimed_by_the_fallback() {
        let shell = capture!("prompt_shell");
        let (_, prompt) = permission_of(QwenInteractions.read(shell, &[hooked(0)]));
        let command = prompt.title.expect("the command");
        let other = call("web_search", json!({ "query": command }));
        assert_eq!(
            QwenInteractions.read(shell, std::slice::from_ref(&other)),
            Reading::Up
        );
        assert_eq!(
            QwenInteractions.read(shell, &[shell_call("mkdir elsewhere"), other]),
            Reading::Up
        );

        let mcp = capture!("prompt_mcp");
        let other = call("web_search", json!({ "query": "octoboard" }));
        assert_eq!(QwenInteractions.read(mcp, &[other]), Reading::Up);
    }

    /// A prompt that matches none of the calls is up but not published; the input box back is
    /// the prompt gone.
    #[test]
    fn an_unmatched_prompt_is_up_and_the_input_box_is_gone() {
        let other = [shell_call("mkdir elsewhere"), write_call()];
        assert_eq!(
            QwenInteractions.read(capture!("prompt_shell"), &other),
            Reading::Up
        );
        assert_eq!(
            QwenInteractions.read(capture!("prompt_answered"), &other),
            Reading::Gone
        );
        assert_eq!(
            QwenInteractions.read("nothing here", &other),
            Reading::Unknown
        );
    }

    /// A choice is its digit, which takes it at once, pressed only on the call's own prompt; no
    /// choice takes text.
    #[test]
    fn a_choice_is_pressed_as_its_digit() {
        let screen = capture!("prompt_shell");
        let choose = |call: &CallSeen, id: &str, text: Option<&str>| {
            QwenInteractions.next_key(
                screen,
                call,
                &Answer::Choice {
                    id: id.to_string(),
                    text: text.map(str::to_string),
                },
            )
        };
        let call = hooked(0);
        assert_eq!(
            choose(&call, "4", None),
            NextKey::Press(Input::Text("4".to_string()))
        );
        assert!(matches!(choose(&call, "4", Some("why")), NextKey::Stuck(_)));
        assert!(matches!(choose(&call, "7", None), NextKey::Stuck(_)));
        assert!(matches!(
            choose(&write_call(), "1", None),
            NextKey::Stuck(_)
        ));
        assert!(matches!(
            QwenInteractions.next_key(capture!("prompt_answered"), &call, &Answer::Dismiss),
            NextKey::Stuck(_)
        ));
    }

    /// Only a scrollbar cell after a space is taken off a row.
    #[test]
    fn only_the_scrollbar_cell_is_taken_off_a_row() {
        assert_eq!(
            without_scrollbar("  1. Yes, allow once      \u{2588}"),
            "  1. Yes, allow once"
        );
        assert_eq!(without_scrollbar("  done.   \u{2502}  "), "  done.");
        assert_eq!(without_scrollbar("\u{2588}"), "");
        assert_eq!(without_scrollbar("a\u{2588}"), "a\u{2588}");
        assert_eq!(without_scrollbar("echo |"), "echo |");
    }

    fn questions_of(reading: Reading) -> QuestionPrompt {
        match reading {
            Reading::Shown {
                prompt: InteractionPrompt::Question(prompt),
                ..
            } => prompt,
            other => panic!("a question form: {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_reported_qwen_wait_publishes_its_hooked_question() {
        use crate::protocol::SessionStatus;
        use crate::state::test_support::insert_record;
        use crate::test_support::{app_state, fake_live_session, PATIENCE};

        let (state, dir) = app_state("qwen-reported-question");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let mut record = state.session_record("s").unwrap();
        record.agent = AgentId::QWEN;
        state.save_session(&record).unwrap();
        let live = fake_live_session("s", AgentId::QWEN, 120, 40, "sleep 30");
        state.register_live(live.clone());

        let payload = serde_json::from_str(include_str!(
            "../../../testdata/qwen_question_0.28.4_hook.json"
        ))
        .unwrap();
        live.interaction.call_started(
            QwenInteractions
                .call_started("PreToolUse", &payload)
                .unwrap(),
        );
        let mut events = state.subscribe();
        live.on_output(include_bytes!("../../../testdata/qwen_question_0.28.4.bin"));

        let question = tokio::time::timeout(PATIENCE, async {
            loop {
                if let Some(interaction) = live.interaction.interaction() {
                    break interaction.prompt;
                }
                events.recv().await.unwrap();
            }
        })
        .await
        .expect("the blocked report publishes the question");
        let InteractionPrompt::Question(form) = question else {
            panic!("expected a question");
        };
        assert_eq!(form.questions[0].text, "Pick a smoke test value");
        assert_eq!(form.questions[0].options[0].label, "Alpha");
    }

    /// A form of two questions, as drawn, is published with both; only the last takes a written
    /// answer, beside its options as it is a multiple choice.
    #[test]
    fn a_form_of_two_questions_is_published_whole() {
        let form = questions_of(
            QwenInteractions.read(capture!("question_first_of_two"), &[two_questions()]),
        );
        assert!(form.dismissible);
        let summary: Vec<(&str, bool, usize, Option<FreeText>)> = form
            .questions
            .iter()
            .map(|q| {
                (
                    q.text.as_str(),
                    q.multi_select,
                    q.options.len(),
                    q.free_text,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("Which colour do you prefer?", false, 2, None),
                ("Which sizes fit?", true, 3, Some(FreeText::With)),
            ]
        );
        assert_eq!(form.questions[0].header.as_deref(), Some("Colour"));
        assert_eq!(
            form.questions[0].options[1].description.as_deref(),
            Some("The cool one")
        );
    }

    /// A single question has no tabs; it is published too, a single choice's written answer in
    /// place of the options and a multiple one's beside them.
    #[test]
    fn a_single_question_is_published() {
        let single = questions_of(
            QwenInteractions.read(capture!("question_single"), &[one_question(false)]),
        );
        assert_eq!(single.questions[0].free_text, Some(FreeText::Instead));
        let multi = questions_of(
            QwenInteractions.read(capture!("question_single_multi"), &[one_question(true)]),
        );
        assert!(multi.questions[0].multi_select);
        assert_eq!(multi.questions[0].free_text, Some(FreeText::With));
        // The questions of another call are not this form's.
        assert_eq!(
            QwenInteractions.read(capture!("question_single"), &[one_question(true)]),
            Reading::Up
        );
    }

    /// A form half answered (the cursor moved, a question answered, the field written in) is up
    /// but left to the terminal.
    #[test]
    fn a_form_half_answered_is_not_published() {
        let calls = [two_questions()];
        for screen in [
            capture!("question_cursor_moved"),
            capture!("question_second_multi"),
            capture!("question_submit"),
            capture!("question_typed"),
        ] {
            assert_eq!(QwenInteractions.read(screen, &calls), Reading::Up);
        }
    }

    fn answer(selected: &[u32], text: Option<&str>) -> QuestionAnswer {
        QuestionAnswer {
            selected: selected.to_vec(),
            text: text.map(str::to_string),
        }
    }

    /// Answering the two questions: a digit chooses the single choice, Space ticks the row under
    /// the cursor and a digit moves it to the next one to tick, the field is reached and written
    /// and Enter takes it, and Enter on `Submit answers` sends the form.
    #[test]
    fn the_answers_are_pressed_key_by_key() {
        let call = two_questions();
        let press = |screen: &str, answers: Vec<QuestionAnswer>| {
            QwenInteractions.next_key(screen, &call, &Answer::Questions(answers))
        };
        let text = |s: &str| NextKey::Press(Input::Text(s.to_string()));
        assert_eq!(
            press(
                capture!("question_first_of_two"),
                vec![answer(&[1], None), answer(&[0, 1], None)]
            ),
            text("2")
        );
        // The second question, nothing ticked, the cursor on its first row.
        let second = capture!("question_second_multi");
        assert_eq!(
            press(second, vec![answer(&[1], None), answer(&[0, 1], None)]),
            text(" ")
        );
        assert_eq!(
            press(second, vec![answer(&[1], None), answer(&[1], None)]),
            text("2")
        );
        assert_eq!(
            press(second, vec![answer(&[1], None), answer(&[], Some("mine"))]),
            text("4")
        );
        // The field open and written with what the answer says, the first row ticked.
        let typed = capture!("question_multi_typed");
        assert_eq!(
            press(
                typed,
                vec![answer(&[1], None), answer(&[0], Some("my own size"))]
            ),
            NextKey::Press(Input::Enter)
        );
        assert!(matches!(
            press(typed, vec![answer(&[1], None), answer(&[0], Some("other"))]),
            NextKey::Stuck(_)
        ));
        assert_eq!(
            press(
                capture!("question_submit"),
                vec![answer(&[1], None), answer(&[0, 1], None)]
            ),
            NextKey::Press(Input::Enter)
        );
        // A written answer before the last question, or none at all, is not pressed.
        for answers in [
            vec![answer(&[], Some("purple")), answer(&[0], None)],
            vec![answer(&[1], None), answer(&[], None)],
            vec![answer(&[1], None)],
        ] {
            assert!(matches!(
                press(capture!("question_first_of_two"), answers),
                NextKey::Stuck(_)
            ));
        }
    }

    /// The question a tab row shows is found by the headers in their order, each after the one
    /// before: a header that begins another, or one drawn twice, is counted where it stands.
    #[test]
    fn the_shown_question_is_found_by_the_headers_in_order() {
        let asked = |headers: &[&str]| -> Vec<Asked> {
            headers
                .iter()
                .map(|header| Asked {
                    text: format!("{header}?"),
                    header: Some(header.to_string()),
                    multi_select: false,
                    options: Vec::new(),
                })
                .collect()
        };
        let index = |tabs: &str, headers: &[&str]| {
            let prompt = Prompt {
                display: QUESTION_DISPLAY.to_string(),
                title: String::new(),
                rows: vec![tabs.to_string()],
            };
            match step(&prompt, &asked(headers)) {
                Some(Step::Question(index)) => Some(index),
                Some(Step::Submit) => Some(usize::MAX),
                None => None,
            }
        };
        // `Size` is part of `Sizes`: the tab before the marker is `Sizes` alone.
        assert_eq!(
            index("Sizes \u{2713} \u{25B8} Size  Submit", &["Sizes", "Size"]),
            Some(1)
        );
        // `Colour` drawn twice: the marker is on the second.
        assert_eq!(
            index(
                "Colour \u{2713} \u{25B8} Colour  Submit",
                &["Colour", "Colour"]
            ),
            Some(1)
        );
        assert_eq!(
            index("\u{25B8} Colour   Colour  Submit", &["Colour", "Colour"]),
            Some(0)
        );
        assert_eq!(
            index("A \u{2713}  B \u{2713} \u{25B8} Submit", &["A", "B"]),
            Some(usize::MAX)
        );
        // A call with a blank header draws no form, so no tab row is its.
        assert_eq!(
            index("A \u{2713} \u{25B8} B   Submit", &["A", "B", " "]),
            None
        );
    }

    /// A single question's written answer: its row's digit opens the field, the text is pasted,
    /// and Esc dismisses the form.
    #[test]
    fn a_single_choices_written_answer_opens_the_field_first() {
        let call = one_question(false);
        let screen = capture!("question_single");
        assert_eq!(
            QwenInteractions.next_key(
                screen,
                &call,
                &Answer::Questions(vec![answer(&[], Some("plum"))])
            ),
            NextKey::Press(Input::Text("3".to_string()))
        );
        assert_eq!(
            QwenInteractions.next_key(screen, &call, &Answer::Questions(vec![answer(&[0], None)])),
            NextKey::Press(Input::Text("1".to_string()))
        );
        assert_eq!(
            QwenInteractions.next_key(screen, &call, &Answer::Dismiss),
            NextKey::Press(Input::Escape)
        );
    }

    /// A call is kept from its `PreToolUse`, with its id; another event keeps none.
    #[test]
    fn a_call_is_kept_from_its_pre_tool_use() {
        let payload = include_str!("../../../testdata/qwen_hooks_guard.jsonl")
            .lines()
            .next()
            .expect("a payload");
        let payload: Value = serde_json::from_str(payload).expect("JSON");
        let seen = QwenInteractions
            .call_started("PreToolUse", &payload)
            .expect("a call");
        assert_eq!(seen.tool, "write_file");
        assert_eq!(seen.input["file_path"], "/private/tmp/DENYME-other/x.txt");
        assert!(seen.id.is_some());
        assert!(QwenInteractions
            .call_started("PostToolUse", &payload)
            .is_none());
    }
}
