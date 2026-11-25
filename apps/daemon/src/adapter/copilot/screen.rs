//! How GitHub Copilot CLI's screens read: its composer, its folder-trust confirmation, the form
//! it puts to the user, its draft stash, and its interrupt key. The measurements are in
//! `docs/agent-cli-reference.md`, "GitHub Copilot CLI".

use crate::adapter::{AgentVersion, LaunchPlan, LaunchSpec};
use crate::platform::term::Input;
use crate::session::dialogs::{DialogPattern, WrittenMessage};
use crate::session::input_box::box_rows;
use crate::session::screen::{Interrupt, InterruptKey, ScreenReader};
use crate::session::trust::{Press, Screen, StartSignal, HOOK_FALLBACK};

pub(super) struct CopilotScreen;

impl ScreenReader for CopilotScreen {
    fn dialogs(&self) -> &'static [DialogPattern] {
        DIALOGS
    }

    fn input_box_rows(&self, screen: &str) -> Option<Vec<String>> {
        composer(screen)
    }

    fn trust_screen(&self) -> &'static Screen {
        &TRUST
    }

    // A resumed conversation can redraw its last prompt with the same glyph while the trust
    // confirmation is still up. Only the cursor between the confirmation's heading and footer
    // identifies a choice in the confirmation.
    fn trust_cursor_bounds(&self) -> Option<(&'static str, &'static str)> {
        Some(("Confirm folder trust", "esc to cancel"))
    }

    fn interrupt(&self) -> Option<&'static Interrupt> {
        Some(&INTERRUPT)
    }

    /// Ctrl-S stashes the draft and empties the composer, and Ctrl-S at an empty composer puts
    /// the stash back, as Claude Code's does (`testdata/copilot-captures.md`, "The draft
    /// stash"). Only on a version measured to. `copilot help config` names no setting that
    /// rebinds the key; its composer's `editorMode` of `vim` is not measured.
    fn native_stash(&self, spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> bool {
        spec.agent_version
            .is_some_and(|version| version >= STASH_MINIMUM)
    }

    fn stash_shown(&self, screen: &str) -> bool {
        // The right-hand items are set apart from the left-hand ones by a run of spaces, not by
        // the separator: `… · tab next tab           stashed · Copilot Fusion`.
        footer(screen).is_some_and(|row| {
            row.split(FOOTER_SEPARATOR)
                .flat_map(|item| item.split(FOOTER_GAP))
                .any(|item| item.trim() == STASH_WORD)
        })
    }

    fn stash_indicator_row(&self, screen: &str) -> Option<String> {
        footer(screen).map(|row| row.trim().to_string())
    }

    /// An image file's path pasted alone shows as `[image: <file name>]`, the placeholders of
    /// several images side by side with nothing between, and the message is sent with each image
    /// attached (`testdata/copilot-captures.md`, "Images").
    fn image_placeholders(&self, text: &str) -> usize {
        image_placeholder_spans(text).len()
    }

    fn without_image_placeholders(&self, text: &str) -> String {
        let mut rest = String::new();
        let mut from = 0;
        for (start, end) in image_placeholder_spans(text) {
            rest.push_str(&text[from..start]);
            from = end;
        }
        rest.push_str(&text[from..]);
        rest
    }

    fn command_completion(&self, screen: &str) -> Option<String> {
        command_completion(screen)
    }

    /// Ctrl-C at the idle prompt empties a draft of several lines at once, where Ctrl-U only
    /// empties a line (1.0.114, "Slash commands" in the reference); mid-turn it interrupts.
    fn input_box_clearing_key(&self) -> Option<Input> {
        Some(Input::Ctrl('c'))
    }
}

/// The first version measured to stash a draft with Ctrl-S.
const STASH_MINIMUM: AgentVersion = (1, 0, 114);

/// The item of the footer that says a draft is stashed, at its right end before the model's
/// name: `… · tab next tab   stashed · Copilot Fusion`, kept at 80 columns where the footer's
/// other items are cut short.
const STASH_WORD: &str = "stashed";

/// What separates the footer's items.
const FOOTER_SEPARATOR: char = '\u{b7}';
/// What sets the footer's right-hand items apart from its left-hand ones: a run of spaces, of
/// which two are enough to tell it from the one inside an item.
const FOOTER_GAP: &str = "  ";

/// Copilot's footer: the last row that shows anything, since Copilot fills the window. It holds
/// only Copilot's own items, never the user's text, and is drawn plain, so it reads the same with
/// the faint cells blanked. `None` unless the composer is drawn above it, so a dialog's last row
/// is not taken for it.
fn footer(screen: &str) -> Option<&str> {
    composer(screen)?;
    screen
        .lines()
        .map(str::trim_end)
        .rfind(|row| !row.is_empty())
}

/// The dialogs Copilot (1.0.114) draws that no hook announces. Its form, put to the user by an MCP
/// server's elicitation, raises no `notification` and leaves no record line (the call that asked
/// runs on, `preToolUse` to `postToolUse`), so the screen is the only sign that the session waits
/// on the user. The same form draws `ask_user`'s question, which its hooks do announce: both read
/// as the one dialog. It is told by its title, `<who> needs information.` (the server's name, or
/// `Copilot`), and its footer, which holds `ctrl+d decline` whichever field has the focus and
/// whatever is typed in it (it ends `esc cancel`, or `esc clear` once a text field holds text;
/// `testdata/copilot-captures.md`, "The form", "Interactions"). While it is up, a pasted message
/// is dropped and the Enter answers the field that has the focus.
const DIALOGS: &[DialogPattern] = &[DialogPattern::all_of(
    "copilot_form",
    WrittenMessage::Discarded,
    &[FORM_TITLE_END, FORM_FOOTER_DECLINE],
)];

/// How the form's title ends, whitespace removed: `<who> needs information.`.
pub(super) const FORM_TITLE_END: &str = "needsinformation.";
/// What the form's footer holds, whitespace removed, before `esc cancel` (`esc clear` once text is
/// typed in a field): `… · ctrl+d decline · esc cancel`.
pub(super) const FORM_FOOTER_DECLINE: &str = "ctrl+ddecline";

/// The prompt mark of Copilot's composer, which is also the cursor of its menus.
pub(super) const PROMPT_MARK: char = '\u{276F}';

/// The rows of Copilot's composer, read from what is left of it on a screen with the faint cells
/// blanked as well as on the screen as drawn (`testdata/copilot-captures.md`).
///
/// Copilot fills the window: its footer is the last row, the composer's bottom rule the row
/// above it, and the composer's rows end right above that rule. Both rules are drawn faint, so
/// blanked they are empty rows and cannot bound the box; the footer and the mark can. The box
/// starts at the last row that starts with `❯`, which is the composer's alone: every other `❯`
/// Copilot draws is indented (a prompt already sent, an `ask_user` form's cursor, the model
/// picker's search box) or inside a box's border with a space before it (the folder-trust and
/// permission menus). It ends two rows above the last row that shows anything, the footer, and
/// the row between has to be a rule or empty. An empty line inside a draft is kept, since the
/// end is counted from the footer and not from the first empty row.
///
/// With Copilot's sidebar open the whole view is drawn to the right of the sidebar's border,
/// `│❯ <draft>`, the border in a colour that is not faint. The window is read as drawn first,
/// and only when that gives no composer as what is right of each row's first border
/// ([`beside_sidebar`]).
fn composer(screen: &str) -> Option<Vec<String>> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    composer_in(&rows).or_else(|| composer_in(&beside_sidebar(&rows)?))
}

/// The border Copilot draws between its sidebar and the rest of the window.
const SIDEBAR_BORDER: char = '\u{2502}';

/// The rows of the view beside the sidebar: what each row holds after its first border, which
/// with the sidebar open is the sidebar's own on every row (the sidebar's text holds none). A
/// row with no border shows nothing of the view. `None` unless some row has the composer's mark
/// right after that border, so a window with no sidebar is not read this way.
fn beside_sidebar<'a>(rows: &[&'a str]) -> Option<Vec<&'a str>> {
    let view: Vec<&str> = rows.iter().map(|row| right_of_border(row)).collect();
    view.iter()
        .any(|row| row.starts_with(PROMPT_MARK))
        .then_some(view)
}

fn right_of_border(row: &str) -> &str {
    row.split_once(SIDEBAR_BORDER).map_or("", |(_, view)| view)
}

/// The composer in `rows`, each a row of the view from its first column.
fn composer_in(rows: &[&str]) -> Option<Vec<String>> {
    let footer = rows.iter().rposition(|row| !row.is_empty())?;
    if let Some(composer) = block_border_composer(&rows[..footer]) {
        return Some(composer);
    }
    let start = (1..footer)
        .rev()
        .find(|&row| rows[row].starts_with(PROMPT_MARK))?;
    let rule_or_empty = |row: &str| row.is_empty() || is_rule(row);
    // The top rule above the mark, the bottom rule above the footer.
    if !rule_or_empty(rows[start - 1]) || footer < start + 2 || !rule_or_empty(rows[footer - 1]) {
        return None;
    }
    // A draft's further rows, wrapped ones and further lines alike, are indented two columns or
    // empty. Rows that are not are something else drawn between the mark and the footer: a
    // timeline above a form that sits where the composer would.
    if !rows[start + 1..footer - 1]
        .iter()
        .all(|row| row.is_empty() || row.starts_with("  "))
    {
        return None;
    }
    Some(box_rows(
        rows[start..footer - 1].iter().copied(),
        PROMPT_MARK,
    ))
}

/// Copilot 1.0.114 on Windows draws this composer with block borders and no prompt glyph.
/// Its upper and lower rules meet the left rail, and each draft row starts inside that rail.
fn block_border_composer(rows: &[&str]) -> Option<Vec<String>> {
    let rule_width = |row: &str, corner: char, fill: char| {
        let rule = row.strip_prefix(corner)?;
        let width = rule.chars().count();
        (width >= 20 && rule.chars().all(|cell| cell == fill)).then_some(width)
    };
    let (bottom, content) = rows.split_last()?;
    let width = rule_width(bottom, '╹', '▀')?;
    let top = content
        .iter()
        .rposition(|row| rule_width(row, '╻', '▄') == Some(width))?;
    let rows = content.get(top + 1..)?;
    if rows.is_empty() {
        return None;
    }
    rows.iter()
        .map(|row| {
            let text = row.strip_prefix('┃')?;
            if text.is_empty() {
                Some(String::new())
            } else {
                Some(text.strip_prefix(' ')?.to_owned())
            }
        })
        .collect()
}

/// Whether a row is one of the horizontal rules around the composer.
fn is_rule(row: &str) -> bool {
    let row = row.trim();
    row.chars().count() >= 20 && row.chars().all(|c| c == '\u{2500}')
}

/// Copilot 1.0.114: "Confirm folder trust", the question, and three options with the cursor on
/// the first, "1. Yes", which trusts the folder for this session only. The option pressed is the
/// second, which remembers the folder so that later launches do not ask: a Down from the first,
/// then Enter. "1. Yes" is one of the others because it is not the option pressed.
pub(super) const TRUST: Screen = Screen {
    first_draw: &[&[
        "Confirmfoldertrust",
        "Doyoutrustthefilesinthisfolder?",
        "\u{276F}1.Yes",
        "2.Yes,andrememberthisfolderforfuturesessions",
        "3.No(Esc)",
    ]],
    accept: Some("2.Yes,andrememberthisfolderforfuturesessions"),
    others: &["1.Yes", "3.No(Esc)"],
    cursor: Some('\u{276F}'),
    press: Press::DownThenEnter,
    next_screen: None,
    // Copilot runs no hook until its first prompt, so an answer typed in the terminal shows in
    // nothing else.
    input_ends_wait: true,
    // Its first hook comes only with its first prompt, and it writes its title before the
    // confirmation. Its composer is drawn only once the confirmation is answered, or at once in
    // a folder already trusted.
    start_signal: StartSignal::InputBox,
    // For a composer the reader fails to recognise.
    hook_fallback: Some(HOOK_FALLBACK),
    waits_for_paste_mode: false,
};

/// Ctrl-C stops a running turn at once and leaves a draft in the composer as it is. Esc would
/// need a second press within half a second. The interrupted prompt is not put back.
const INTERRUPT: Interrupt = Interrupt {
    key: InterruptKey::CtrlC,
    restores_prompt: false,
    written_during_turn,
    queued_waiting: None,
};

/// A message written during a turn is shown as pending and is not sent while the model writes.
/// One written while a shell command runs moves that command to the background, which ends its
/// call, and joins the turn before the next model request. Ctrl-C drops the ones still pending,
/// with no hook and no line in the record (`docs/agent-cli-reference.md`, "GitHub Copilot CLI").
fn written_during_turn(written: usize) -> String {
    format!(
        " Octoboard wrote about {written} message(s) into the session during that turn; GitHub \
         Copilot CLI holds such a message as pending until the model's next request, and drops \
         the ones still pending when the turn is stopped. Check with `get_session` what reached \
         it before sending the next step."
    )
}

/// What an image's placeholder starts with; it ends at the next `]`.
const IMAGE_PLACEHOLDER: &str = "[image: ";

/// Where each `[image: <name>]` of `text` starts and ends: a name that is not empty and holds no
/// bracket and no line break.
fn image_placeholder_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(IMAGE_PLACEHOLDER) {
        let start = from + found;
        let name_start = start + IMAGE_PLACEHOLDER.len();
        let Some(length) = text[name_start..].find(['[', ']', '\n']) else {
            break;
        };
        let name_end = name_start + length;
        if text[name_end..].starts_with(']') && !text[name_start..name_end].trim().is_empty() {
            spans.push((start, name_end + 1));
            from = name_end + 1;
        } else {
            from = name_start;
        }
    }
    spans
}

/// The margin Copilot draws before the rows of its slash-command completion, which may also be
/// blanked or absent.
const COMPLETION_MARGIN: char = '\u{2503}';

/// The command the slash-command completion highlights, by its name without the slash: Copilot
/// shows the highlighted entry's name in the composer in place of what was typed (`/he` shows
/// `/help`), and Enter runs it (`testdata/copilot-captures.md`, "Slash commands"). The list is the
/// rows right above the composer's top rule, each `/<name>` with its arguments or its
/// description after two spaces, the highlighted one led by `❯`, all of them behind a margin of
/// `┃` (a space where it is blanked) or of two spaces. A prompt already sent in the timeline is
/// indented one column, and so is the working directory's row that sits above the rule when no
/// list is up, so neither is an entry.
fn command_completion(screen: &str) -> Option<String> {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    composer_in(&rows)?;
    let footer = rows.iter().rposition(|row| !row.is_empty())?;
    let start = (1..footer)
        .rev()
        .find(|&row| rows[row].starts_with(PROMPT_MARK))?;
    rows[..start.checked_sub(1)?]
        .iter()
        .rev()
        .take(COMPLETION_ROWS)
        .take_while(|row| row.is_empty() || completion_entry(row).is_some())
        .filter_map(|row| completion_entry(row))
        .find_map(|(highlighted, name)| highlighted.then(|| name.to_string()))
}

/// The most rows the completion list was seen to take.
const COMPLETION_ROWS: usize = 12;

/// A row of the completion list: whether it is highlighted, and the entry's name.
fn completion_entry(row: &str) -> Option<(bool, &str)> {
    let entry = row
        .strip_prefix(COMPLETION_MARGIN)
        .and_then(|rest| rest.strip_prefix(' '))
        .or_else(|| row.strip_prefix("  "))?;
    let (highlighted, entry) = match entry.trim_start().strip_prefix(PROMPT_MARK) {
        Some(rest) => (true, rest.trim_start()),
        None => (false, entry.trim_start()),
    };
    let name = entry.strip_prefix('/')?.split_whitespace().next()?;
    let is_name = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    is_name.then_some((highlighted, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentId;
    use crate::session::trust::{reading_of, Reading};
    use crate::terminal::snapshot::TerminalSnapshot;

    fn composer_of(screen: &str) -> Option<Vec<String>> {
        CopilotScreen.input_box_rows(screen)
    }

    #[test]
    fn the_windows_block_border_composer_reads_empty_and_typed_rows() {
        let top = format!("╻{}", "▄".repeat(119));
        let bottom = format!("╹{}", "▀".repeat(119));
        let footer = " ← open sidebar · Interactive · Manual Approval · / commands · ? help";
        for (row, draft) in [("┃", ""), ("┃ m36-draft", "m36-draft")] {
            let screen = format!(
                " ● MCP Servers reloaded: 1 server connected\n{top}\n{row}\n{bottom}\n{footer}"
            );
            assert_eq!(composer_of(&screen), Some(vec![draft.to_owned()]));
        }
        assert_eq!(composer_of(&format!("{top}\n┃ m36-draft\n{footer}")), None);
    }

    /// The composer captures of `testdata/copilot-captures.md` at one width, in the order they
    /// were taken: each is the bytes Copilot wrote since the one before it.
    macro_rules! composer_captures {
        ($width:literal) => {
            [
                (
                    "01_empty",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_01_empty.bin"
                    )) as &[u8],
                ),
                (
                    "02_one_line",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_02_one_line.bin"
                    )),
                ),
                (
                    "03_wrapped",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_03_wrapped.bin"
                    )),
                ),
                (
                    "04_empty_line",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_04_empty_line.bin"
                    )),
                ),
                (
                    "05_cleared",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_05_cleared.bin"
                    )),
                ),
                (
                    "06_during_turn",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_06_during_turn.bin"
                    )),
                ),
                (
                    "07_after_cancel",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_07_after_cancel.bin"
                    )),
                ),
                (
                    "08_permission_prompt",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_08_permission_prompt.bin"
                    )),
                ),
                (
                    "09_ask_user_form",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_09_ask_user_form.bin"
                    )),
                ),
                (
                    "10_back_at_prompt",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_composer_",
                        $width,
                        "_10_back_at_prompt.bin"
                    )),
                ),
            ]
        };
    }

    /// The screens at the capture `state`, at 120 and at 80 columns, each both as production
    /// reads a draft off it (faint cells blanked) and as drawn (the start signal's reading).
    fn screens_at(state: &str) -> Vec<String> {
        let mut screens = Vec::new();
        for (cols, captures) in [
            (120, composer_captures!("120")),
            (80, composer_captures!("80")),
        ] {
            let mut screen = TerminalSnapshot::new(40, cols);
            for (name, bytes) in captures {
                screen.process(bytes);
                if name == state {
                    break;
                }
            }
            screens.push(screen.visible_text_undimmed());
            screens.push(screen.visible_text());
        }
        screens
    }

    /// The composer reads as its rows in every state it is drawn in, at both widths, with the
    /// faint cells blanked and without: empty, with a line of draft, with a draft wrapped onto
    /// more rows and a second line, with an empty line inside the draft, and while a turn runs and
    /// after it is cancelled with a draft typed.
    #[test]
    fn the_composer_is_read_in_every_state_it_is_drawn_in() {
        for state in ["01_empty", "05_cleared", "10_back_at_prompt"] {
            for screen in screens_at(state) {
                assert_eq!(composer_of(&screen), Some(vec![String::new()]), "{state}");
            }
        }
        for screen in screens_at("02_one_line") {
            assert_eq!(
                composer_of(&screen),
                Some(vec!["one line draft".to_string()])
            );
        }
        for state in ["06_during_turn", "07_after_cancel"] {
            for screen in screens_at(state) {
                assert_eq!(
                    composer_of(&screen),
                    Some(vec!["typed during the turn".to_string()]),
                    "{state}"
                );
            }
        }
        for screen in screens_at("04_empty_line") {
            assert_eq!(
                composer_of(&screen),
                Some(vec![
                    "above the gap".to_string(),
                    String::new(),
                    "below the gap".to_string()
                ])
            );
        }
        // The wrapped rows differ with the width, the text they hold does not.
        for screen in screens_at("03_wrapped") {
            let rows = composer_of(&screen).expect("the composer");
            let (last, wrapped) = rows.split_last().expect("rows");
            assert_eq!(last, "second line");
            assert!(wrapped.len() >= 2, "{rows:?}");
            assert_eq!(
                wrapped.join(" "),
                "first line of a longer draft that keeps going well past the right edge of the \
                 window so that the composer has to wrap it onto another row"
            );
        }
    }

    /// A permission prompt and an `ask_user` form take the composer's place, and no composer is
    /// read: the prompt's cursor row is inside a box, and the form's is indented.
    #[test]
    fn a_permission_prompt_and_a_question_form_are_not_the_composer() {
        for state in ["08_permission_prompt", "09_ask_user_form"] {
            for screen in screens_at(state) {
                assert_eq!(composer_of(&screen), None, "{state}");
            }
        }
    }

    /// Hand-built, since no capture holds the shapes the reader must refuse: each row list is a
    /// screen with the footer as its last non-empty row, and is read with the rules as drawn and
    /// with them blanked.
    #[test]
    fn a_screen_that_is_not_a_composer_between_rules_and_a_footer_is_not_read() {
        let rule = "\u{2500}".repeat(60);
        let short = "\u{2500}".repeat(5);
        let read = |rows: &[&str]| -> (Option<Vec<String>>, Option<Vec<String>>) {
            let drawn = rows.join("\n").replace("RULE", &rule);
            let blanked = rows.join("\n").replace("RULE", "");
            (composer_of(&drawn), composer_of(&blanked))
        };
        let both = |rows: &[&str], expected: Option<&[&str]>| {
            let expected = expected.map(|rows| rows.iter().map(|row| row.to_string()).collect());
            assert_eq!(read(rows), (expected.clone(), expected), "{rows:?}");
        };
        // The shape that is read, with the rules drawn and blanked.
        both(
            &["timeline", "", "RULE", "\u{276F} hi", "RULE", "footer"],
            Some(&["hi"][..]),
        );
        // A mark that is the last row that shows anything is the footer's place, not a composer.
        both(&["timeline", "RULE", "\u{276F}"], None);
        // Nothing two rows below the mark: the footer is right under it.
        both(&["timeline", "RULE", "\u{276F} hi", "footer"], None);
        // A mark that is not in the first column: the prompt already sent, the picker's search box.
        both(
            &["timeline", "RULE", " \u{276F} hi", "RULE", "footer"],
            None,
        );
        both(
            &["RULE", " \u{276F}  Search models\u{2026}", "RULE", "footer"],
            None,
        );
        // Text or a short rule where the top or the bottom rule belongs.
        both(&["timeline", "text", "\u{276F} hi", "RULE", "footer"], None);
        both(
            &["timeline", "RULE", "\u{276F} hi", "not a rule", "footer"],
            None,
        );
        for row in [&short[..], "x"] {
            both(&["timeline", row, "\u{276F} hi", "RULE", "footer"], None);
            both(&["timeline", "RULE", "\u{276F} hi", row, "footer"], None);
        }
        both(&[], None);
        both(&["footer"], None);
    }

    /// A form that sits where the composer would, with a column-0 `❯` higher up in the timeline: the
    /// rows between that mark and the footer are timeline rows (`●`, a box's border), not a draft's,
    /// which are empty or indented two columns, so no composer is read. The form's own rules are
    /// plain, so both renderings show them. Hand-built from the captures' rows: no capture holds a
    /// column-0 `❯` in a timeline.
    #[test]
    fn a_form_below_a_stray_mark_with_timeline_rows_between_is_not_a_composer() {
        let rule = "\u{2500}".repeat(60);
        let screen = [
            "".to_string(),
            "\u{276F} a mark at column zero".to_string(),
            " \u{25CF} a timeline row".to_string(),
            "\u{2502} a box's border".to_string(),
            rule.clone(),
            " Copilot needs information.".to_string(),
            " Pick one?".to_string(),
            " \u{276F} alpha".to_string(),
            "   beta".to_string(),
            rule,
            " footer".to_string(),
        ]
        .join("\n");
        assert_eq!(composer_of(&screen), None);
        // The same rows with every timeline row gone are a draft of indented rows, and are read:
        // the check is on what lies between the mark and the bottom rule.
        let draft = [
            "",
            "\u{276F} first line",
            "  second line",
            "",
            "  after a gap",
            "",
            " footer",
        ]
        .join("\n");
        assert_eq!(
            composer_of(&draft),
            Some(vec![
                "first line".to_string(),
                "second line".to_string(),
                String::new(),
                "after a gap".to_string()
            ])
        );
    }

    /// The sidebar captures of `testdata/copilot-captures.md` at one width, in the order they were
    /// taken: each is the bytes Copilot wrote since the one before it.
    macro_rules! sidebar_captures {
        ($width:literal) => {
            [
                (
                    "01_open",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_sidebar_",
                        $width,
                        "_01_open.bin"
                    )) as &[u8],
                ),
                (
                    "02_one_line",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_sidebar_",
                        $width,
                        "_02_one_line.bin"
                    )),
                ),
                (
                    "03_wrapped",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_sidebar_",
                        $width,
                        "_03_wrapped.bin"
                    )),
                ),
                (
                    "04_cleared",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_sidebar_",
                        $width,
                        "_04_cleared.bin"
                    )),
                ),
            ]
        };
    }

    /// The screens at the sidebar capture `state`, at 120 and at 80 columns, each with the faint
    /// cells blanked and as drawn.
    fn sidebar_screens_at(state: &str) -> Vec<String> {
        let mut screens = Vec::new();
        for (cols, captures) in [
            (120, sidebar_captures!("120")),
            (80, sidebar_captures!("80")),
        ] {
            let mut screen = TerminalSnapshot::new(40, cols);
            for (name, bytes) in captures {
                screen.process(bytes);
                if name == state {
                    break;
                }
            }
            screens.push(screen.visible_text_undimmed());
            screens.push(screen.visible_text());
        }
        screens
    }

    /// With Copilot's sidebar open the composer is drawn to the right of the sidebar's border, and
    /// is read there in every state captured, at both widths and in both renderings: empty, with a
    /// line of draft, with a draft wrapped onto more rows and a second line, and cleared.
    #[test]
    fn the_composer_beside_the_open_sidebar_is_read() {
        for state in ["01_open", "04_cleared"] {
            for screen in sidebar_screens_at(state) {
                assert!(screen.contains("\u{2502}\u{276F}"), "{state}:\n{screen}");
                assert_eq!(composer_of(&screen), Some(vec![String::new()]), "{state}");
            }
        }
        for screen in sidebar_screens_at("02_one_line") {
            assert_eq!(
                composer_of(&screen),
                Some(vec!["one line draft".to_string()])
            );
        }
        for screen in sidebar_screens_at("03_wrapped") {
            let rows = composer_of(&screen).expect("the composer");
            let (last, wrapped) = rows.split_last().expect("rows");
            assert_eq!(last, "second line");
            assert!(wrapped.len() >= 2, "{rows:?}");
            assert_eq!(
                wrapped.join(" "),
                "first line of a longer draft that keeps going well past the right edge of the \
                 window so that the composer has to wrap it onto another row"
            );
        }
    }

    /// Beside the sidebar the same shapes are refused as without it: a menu's cursor row inside a
    /// box, a mark that is indented, and a view with no rule above the mark. Hand-built from the
    /// captures' rows, since no capture holds a prompt or a form beside the sidebar; the sidebar's
    /// own column holds text on its first row, as captured.
    #[test]
    fn beside_the_sidebar_what_is_not_a_composer_is_not_read() {
        let rule = "\u{2500}".repeat(40);
        let beside = |view: &[&str]| -> String {
            view.iter()
                .enumerate()
                .map(|(row, view)| {
                    let sidebar = if row == 0 {
                        "  Session 1  "
                    } else {
                        "             "
                    };
                    format!("{sidebar}\u{2502}{}", view.replace("RULE", &rule))
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        // The shape that is read, with the rules drawn and blanked.
        for rule_row in ["RULE", ""] {
            let screen = beside(&[
                " timeline",
                rule_row,
                "\u{276F} a draft",
                rule_row,
                " footer",
            ]);
            assert_eq!(composer_of(&screen), Some(vec!["a draft".to_string()]));
        }
        // A permission menu: its cursor row is inside the menu's own box.
        let menu = beside(&[
            " timeline",
            "\u{2502} Do you want to run this command? \u{2502}",
            "\u{2502} \u{276F} 1. Yes \u{2502}",
            "\u{2502}   2. No \u{2502}",
            " footer",
        ]);
        assert_eq!(composer_of(&menu), None);
        // A form's cursor row is indented.
        let form = beside(&["RULE", " Pick one?", " \u{276F} alpha", "RULE", " footer"]);
        assert_eq!(composer_of(&form), None);
        // Text where the top rule belongs.
        let no_rule = beside(&[" timeline", " text", "\u{276F} hi", "RULE", " footer"]);
        assert_eq!(composer_of(&no_rule), None);
    }

    macro_rules! bytes {
        ($name:literal, $width:literal) => {{
            let capture: &[u8] = include_bytes!(concat!(
                "../../../testdata/copilot_",
                $name,
                "_",
                $width,
                ".bin"
            ));
            capture
        }};
    }

    /// The screens, both renderings at both widths, after the folder-trust captures `upto` of
    /// `[trust_screen, trust_after_down, trust_dismissed]` fed in order into a terminal of 40 rows.
    fn trust_screens(upto: usize) -> Vec<String> {
        let mut screens = Vec::new();
        for (cols, captures) in [
            (
                120,
                [
                    bytes!("trust_screen", "120"),
                    bytes!("trust_after_down", "120"),
                    bytes!("trust_dismissed", "120"),
                ],
            ),
            (
                80,
                [
                    bytes!("trust_screen", "80"),
                    bytes!("trust_after_down", "80"),
                    bytes!("trust_dismissed", "80"),
                ],
            ),
        ] {
            let mut screen = TerminalSnapshot::new(40, cols);
            for capture in &captures[..upto] {
                screen.process(capture);
            }
            screens.push(screen.visible_text_undimmed());
            screens.push(screen.visible_text());
        }
        screens
    }

    /// The composer reader is not fooled by the folder-trust prompt as terminal bytes draw it, first
    /// drawn and with the cursor moved by a Down, and finds the empty composer the answer reveals:
    /// the captures fed in order, read with the faint cells blanked and as drawn.
    #[test]
    fn the_composer_reader_does_not_take_the_trust_prompt_for_the_composer() {
        for screen in trust_screens(1) {
            assert!(
                screen.contains("Do you trust the files in this folder?"),
                "{screen}"
            );
            assert_eq!(composer_of(&screen), None, "the trust prompt");
        }
        for screen in trust_screens(2) {
            assert_eq!(composer_of(&screen), None, "after Down");
        }
        for screen in trust_screens(3) {
            assert_eq!(
                composer_of(&screen),
                Some(vec![String::new()]),
                "the prompt answered:\n{screen}"
            );
        }
    }

    fn reading(capture: &[u8]) -> Reading {
        reading_of(&TRUST, capture)
    }

    /// Copilot's own bytes of the folder-trust prompt, at both widths: sighted in its first drawing
    /// with the cursor on "1. Yes", which Octoboard does not press; neither gone nor replaced.
    #[test]
    fn the_trust_prompt_is_sighted_with_the_cursor_on_the_first_option() {
        for capture in [bytes!("trust_screen", "120"), bytes!("trust_screen", "80")] {
            let first = reading(capture);
            assert!(first.shown, "{first:?}");
            assert!(first.up, "{first:?}");
            assert_eq!(first.cursor_on_accept, Some(false));
            assert!(!first.gone && !first.replaced, "{first:?}");
        }
    }

    #[test]
    fn the_trust_cursor_is_read_inside_the_modal_on_resume() {
        let capture = bytes!("trust_on_resume_1.0.114", "120");
        assert!(reading(capture).shown);
        assert_eq!(reading(capture).cursor_on_accept, None);

        let mut terminal = TerminalSnapshot::new(30, 120);
        terminal.process(capture);
        assert_eq!(
            crate::session::trust::rendered_cursor_on_accept(
                crate::providers::AgentId::COPILOT,
                &terminal.visible_text(),
            ),
            Some(false),
        );
    }

    /// After one Down the cursor is redrawn on "2. Yes, and remember this folder", the option
    /// Octoboard presses; the prompt is not gone.
    #[test]
    fn after_one_down_the_cursor_is_on_the_option_that_remembers_the_folder() {
        for capture in [
            bytes!("trust_after_down", "120"),
            bytes!("trust_after_down", "80"),
        ] {
            let down = reading(capture);
            assert_eq!(down.cursor_on_accept, Some(true), "{down:?}");
            assert!(!down.gone && !down.replaced, "{down:?}");
        }
    }

    /// What Copilot draws once Enter has trusted the folder holds none of the prompt: it is gone and
    /// replaced.
    #[test]
    fn once_enter_has_trusted_the_folder_the_prompt_is_gone() {
        for capture in [
            bytes!("trust_dismissed", "120"),
            bytes!("trust_dismissed", "80"),
        ] {
            let done = reading(capture);
            assert!(done.gone && done.replaced, "{done:?}");
            assert!(!done.shown && !done.up, "{done:?}");
            assert_eq!(done.cursor_on_accept, None);
        }
    }

    /// A permission prompt is not the trust prompt, although its options and cursor glyph look
    /// alike: it is not sighted, and not read as the prompt being up.
    #[test]
    fn a_permission_prompt_is_not_taken_for_the_trust_prompt() {
        for capture in [
            bytes!("permission_prompt", "120"),
            bytes!("permission_prompt", "80"),
        ] {
            let permission = reading(capture);
            assert!(!permission.shown && !permission.up, "{permission:?}");
        }
    }

    #[test]
    fn the_provider_serves_a_reader_that_finds_its_composer() {
        let reader = crate::adapter::provider(AgentId::COPILOT).screen();
        for screen in screens_at("02_one_line") {
            assert_eq!(
                reader.input_box_rows(&screen),
                Some(vec!["one line draft".to_string()])
            );
        }
    }

    /// Ctrl-C stops a running turn at once and leaves a draft as it was; the interrupted prompt is
    /// not put back, so the box is not read to clear it.
    #[test]
    fn a_turn_is_interrupted_by_ctrl_c_and_no_prompt_is_restored() {
        let interrupt = CopilotScreen.interrupt().expect("an interrupt key");
        assert_eq!(interrupt.key, InterruptKey::CtrlC);
        assert!(!interrupt.restores_prompt);
    }

    /// What the caller is told of the messages written into the session during the turn that was
    /// stopped: how many, that Copilot holds such a message as pending and drops the pending ones
    /// when the turn is stopped, and how to see what reached it. It does not say a message is taken
    /// into the running turn, which the measurement shows only for a shell command running.
    #[test]
    fn the_caller_is_told_how_many_messages_were_written_during_the_turn() {
        let interrupt = CopilotScreen.interrupt().expect("an interrupt key");
        let told = (interrupt.written_during_turn)(3);
        assert!(told.contains("3 message(s)"), "{told}");
        assert!(told.contains("pending"), "{told}");
        assert!(told.contains("drops"), "{told}");
        assert!(told.contains("get_session"), "{told}");
        assert!(
            !told.contains("takes such a message into the running turn"),
            "{told}"
        );
        assert_ne!(told, (interrupt.written_during_turn)(4));
    }

    /// The captures of a session whose MCP server puts a form to the user, at one width, in the
    /// order they were taken (`testdata/copilot-captures.md`, "The form").
    macro_rules! form_captures {
        ($prefix:literal, $width:literal, [$($name:literal),*]) => {
            [$((
                $name,
                include_bytes!(concat!(
                    "../../../testdata/copilot_",
                    $prefix,
                    "_",
                    $width,
                    "_",
                    $name,
                    ".bin"
                )) as &[u8],
            )),*]
        };
    }

    /// Both renderings of the screen after `captures` up to and including `state`, fed in order
    /// into a terminal of 40 rows and `cols` columns.
    fn screens_after(cols: u16, captures: &[(&str, &[u8])], state: &str) -> [String; 2] {
        let mut screen = TerminalSnapshot::new(40, cols);
        for (name, bytes) in captures {
            screen.process(bytes);
            if *name == state {
                return [screen.visible_text_undimmed(), screen.visible_text()];
            }
        }
        panic!("no capture {state}");
    }

    fn form_of(screen: &str) -> Option<crate::session::dialogs::Dialog> {
        crate::session::dialogs::recognise(AgentId::COPILOT, screen)
    }

    /// An MCP server's form, which no hook announces, is a dialog that puts something to the
    /// user, at both widths and in both renderings, whichever of its fields has the focus; the
    /// folder-trust prompt before it, the composer, and the screen after the form is dismissed
    /// are not.
    #[test]
    fn an_mcp_servers_form_is_a_question_and_the_screens_around_it_are_not() {
        let names = ["01_start", "02_trusted", "03_form", "04_escaped"];
        for (cols, captures) in [
            (
                120,
                form_captures!(
                    "elicitation",
                    "120",
                    ["01_start", "02_trusted", "03_form", "04_escaped"]
                ),
            ),
            (
                80,
                form_captures!(
                    "elicitation",
                    "80",
                    ["01_start", "02_trusted", "03_form", "04_escaped"]
                ),
            ),
        ] {
            for state in names {
                for screen in screens_after(cols, &captures, state) {
                    let dialog = form_of(&screen);
                    if state == "03_form" {
                        assert!(screen.contains("stub needs information."), "{screen}");
                        let dialog =
                            dialog.unwrap_or_else(|| panic!("no form at {cols}:\n{screen}"));
                        assert_eq!(dialog.code, "copilot_form");
                        assert_eq!(dialog.kind, crate::session::dialogs::DialogKind::Question);
                        assert_eq!(
                            dialog.written,
                            crate::session::dialogs::WrittenMessage::Discarded
                        );
                    } else {
                        assert_eq!(dialog, None, "{state} at {cols}:\n{screen}");
                    }
                }
            }
        }
        let fields = form_captures!(
            "elicitation_fields",
            "120",
            [
                "01_start",
                "02_trusted",
                "03_form",
                "04_tab_refused",
                "05_paste_and_enter"
            ]
        );
        for state in ["03_form", "04_tab_refused", "05_paste_and_enter"] {
            for screen in screens_after(120, &fields, state) {
                assert!(form_of(&screen).is_some(), "{state}:\n{screen}");
            }
        }
    }

    /// The near misses among the earlier captures: the composer in each of its states, a
    /// permission prompt in its box and the folder-trust prompt are no form; `ask_user`'s
    /// question, drawn in the same form, is.
    #[test]
    fn a_permission_prompt_and_the_composer_are_no_form_and_ask_users_question_is() {
        for (cols, captures) in [
            (120, composer_captures!("120")),
            (80, composer_captures!("80")),
        ] {
            let mut screen = TerminalSnapshot::new(40, cols);
            for (name, bytes) in captures {
                screen.process(bytes);
                for text in [screen.visible_text_undimmed(), screen.visible_text()] {
                    let dialog = form_of(&text);
                    if name == "09_ask_user_form" {
                        assert_eq!(
                            dialog.map(|dialog| dialog.code),
                            Some("copilot_form"),
                            "{text}"
                        );
                    } else {
                        assert_eq!(dialog, None, "{name} at {cols}:\n{text}");
                    }
                }
            }
        }
        for upto in 1..=3 {
            for screen in trust_screens(upto) {
                assert_eq!(form_of(&screen), None, "{screen}");
            }
        }
        for (cols, captures) in [
            (
                120,
                [
                    bytes!("trust_screen", "120"),
                    bytes!("trust_after_down", "120"),
                    bytes!("trust_dismissed", "120"),
                    bytes!("permission_prompt", "120"),
                ],
            ),
            (
                80,
                [
                    bytes!("trust_screen", "80"),
                    bytes!("trust_after_down", "80"),
                    bytes!("trust_dismissed", "80"),
                    bytes!("permission_prompt", "80"),
                ],
            ),
        ] {
            let mut screen = TerminalSnapshot::new(40, cols);
            for capture in captures {
                screen.process(capture);
            }
            assert!(screen
                .visible_text()
                .contains("Do you want to run this command?"));
            for text in [screen.visible_text_undimmed(), screen.visible_text()] {
                assert_eq!(form_of(&text), None, "{text}");
            }
        }
    }

    /// The form stays a dialog once text is typed in a field, when its footer ends `esc clear`
    /// in place of `esc cancel` (`copilot_question_wrap_120_02_typed`).
    #[test]
    fn the_form_with_text_typed_is_still_a_dialog() {
        let mut screen = TerminalSnapshot::new(40, 120);
        for capture in [
            &include_bytes!("../../../testdata/copilot_question_wrap_120_01_form.bin")[..],
            &include_bytes!("../../../testdata/copilot_question_wrap_120_02_typed.bin")[..],
        ] {
            screen.process(capture);
        }
        let text = screen.visible_text();
        assert!(text.contains("esc clear"), "{text}");
        assert!(form_of(&text).is_some(), "{text}");
    }

    /// A transcript that quotes the form's title or its footer alone, as the model's text or a
    /// command's output can, is no form: both have to be on the screen.
    #[test]
    fn the_forms_title_or_footer_alone_is_no_form() {
        for screen in [
            " ● The server said: stub needs information.\n\n❯ \n footer",
            " ● Press ctrl+d decline · esc cancel to leave it.\n\n❯ \n footer",
        ] {
            assert_eq!(form_of(screen), None, "{screen}");
        }
        assert!(form_of(" stub needs information.\n ctrl+d decline · esc cancel").is_some());
    }

    /// The draft stash captures (`testdata/copilot-captures.md`, "The draft stash") at one
    /// width, in order: a draft typed, Ctrl-S, Ctrl-S again.
    macro_rules! stash_captures {
        ($width:literal) => {
            [
                (
                    "01_draft",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_stash_",
                        $width,
                        "_01_draft.bin"
                    )) as &[u8],
                ),
                (
                    "02_stashed",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_stash_",
                        $width,
                        "_02_stashed.bin"
                    )),
                ),
                (
                    "03_restored",
                    include_bytes!(concat!(
                        "../../../testdata/copilot_stash_",
                        $width,
                        "_03_restored.bin"
                    )),
                ),
            ]
        };
    }

    /// The screens at the stash capture `state`, at 120 and at 80 columns, each with the faint
    /// cells blanked and as drawn.
    fn stash_screens_at(state: &str) -> Vec<String> {
        let mut screens = Vec::new();
        for (cols, captures) in [(120, stash_captures!("120")), (80, stash_captures!("80"))] {
            let mut screen = TerminalSnapshot::new(40, cols);
            for (name, bytes) in captures {
                screen.process(bytes);
                if name == state {
                    break;
                }
            }
            screens.push(screen.visible_text_undimmed());
            screens.push(screen.visible_text());
        }
        screens
    }

    /// Ctrl-S empties the composer and the footer says `stashed`; a second Ctrl-S puts the draft
    /// back and the footer no longer says it. At both widths, blanked and as drawn.
    #[test]
    fn a_stashed_draft_is_shown_in_the_footer_and_gone_once_restored() {
        for screen in stash_screens_at("01_draft") {
            assert!(!CopilotScreen.stash_shown(&screen), "{screen}");
            assert_eq!(
                composer_of(&screen),
                Some(vec!["keep this draft".to_string()]),
                "{screen}"
            );
        }
        for screen in stash_screens_at("02_stashed") {
            assert!(CopilotScreen.stash_shown(&screen), "{screen}");
            assert_eq!(composer_of(&screen), Some(vec![String::new()]), "{screen}");
            let row = CopilotScreen
                .stash_indicator_row(&screen)
                .expect("the footer");
            assert!(row.ends_with("stashed \u{b7} Copilot Fusion"), "{row}");
        }
        for screen in stash_screens_at("03_restored") {
            assert!(!CopilotScreen.stash_shown(&screen), "{screen}");
            assert_eq!(
                composer_of(&screen),
                Some(vec!["keep this draft".to_string()]),
                "{screen}"
            );
        }
    }

    /// Only the footer's own item counts: the word in the draft, a footer item that merely holds
    /// it, or a screen with no composer above its last row shows no stash.
    #[test]
    fn only_a_footer_item_under_the_composer_shows_a_stash() {
        let rule = "\u{2500}".repeat(40);
        let screen =
            |draft: &str, footer: &str| format!("{rule}\n\u{276F} {draft}\n{rule}\n {footer}");
        assert!(CopilotScreen.stash_shown(&screen(
            "",
            "Interactive \u{b7} stashed \u{b7} Copilot Fusion"
        )));
        assert!(!CopilotScreen.stash_shown(&screen("stashed", "Interactive \u{b7} Copilot Fusion")));
        assert!(!CopilotScreen.stash_shown(&screen(
            "",
            "Interactive \u{b7} not stashed \u{b7} Copilot Fusion"
        )));
        assert!(!CopilotScreen.stash_shown(" Do you want to run this command?\n stashed \u{b7} x"));
        assert_eq!(CopilotScreen.stash_indicator_row("no composer here"), None);
    }

    /// The stash is used from the first version measured to stash, and not when the version
    /// cannot be told.
    #[test]
    fn the_stash_is_used_only_on_a_measured_version() {
        use crate::adapter::tests::spec_fixture;
        use crate::adapter::LaunchPlan;
        let mut fixture = spec_fixture();
        for (version, used) in [
            (None, false),
            (Some((1, 0, 113)), false),
            (Some((1, 0, 114)), true),
            (Some((1, 1, 0)), true),
        ] {
            fixture.agent_version = version;
            assert_eq!(
                CopilotScreen.native_stash(&fixture.spec(), &LaunchPlan::default()),
                used,
                "{version:?}"
            );
        }
    }

    /// The screens after the capture `state` of `prefix` (`copilot_<prefix>_120_<nn>_<state>.bin`,
    /// in the order `states` names them), as drawn and with faint cells blanked.
    fn screens_of(prefix: &str, states: &[&str], state: &str) -> [String; 2] {
        let mut screen = TerminalSnapshot::new(40, 120);
        for (index, name) in states.iter().enumerate() {
            let path = format!(
                "{}/testdata/copilot_{prefix}_120_{:02}_{name}.bin",
                env!("CARGO_MANIFEST_DIR"),
                index + 1
            );
            screen.process(&std::fs::read(path).expect("capture"));
            if *name == state {
                return [screen.visible_text(), screen.visible_text_undimmed()];
            }
        }
        panic!("no capture {state}");
    }

    const IMAGE_STATES: &[&str] = &["one_image", "two_images", "with_text"];

    /// Each pasted image shows as a placeholder in the composer, the second right after the
    /// first, and what is typed after them is the rest of the box.
    #[test]
    fn pasted_images_are_counted_and_taken_out_of_the_box() {
        let screen = CopilotScreen;
        for (state, count, rest) in [
            ("one_image", 1, ""),
            ("two_images", 2, ""),
            ("with_text", 2, "what colours"),
        ] {
            for drawn in screens_of("images", IMAGE_STATES, state) {
                let text = composer_of(&drawn).expect("the composer").join("\n");
                assert_eq!(screen.image_placeholders(&text), count, "{state}");
                assert_eq!(screen.without_image_placeholders(&text), rest, "{state}");
            }
        }
    }

    /// Only a whole placeholder with a name counts.
    #[test]
    fn only_a_whole_named_placeholder_counts() {
        let screen = CopilotScreen;
        let text = "[image: a.png] and [image: ] [image: b [c] [image: d.png";
        assert_eq!(screen.image_placeholders(text), 1);
        assert_eq!(
            screen.without_image_placeholders(text),
            " and [image: ] [image: b [c] [image: d.png"
        );
        assert_eq!(screen.image_placeholders("[image: x.png][image: y.png]"), 2);
        assert_eq!(screen.image_placeholders("[Image #1]"), 0);
    }

    const COMMAND_STATES: &[&str] = &["idle", "slash", "partial", "ran", "with_argument"];

    /// The completion's highlighted entry is the command: the first for `/` alone, the one
    /// completing `/he`, and the command itself once an argument is being written; with no list
    /// up there is none, and the command's own view is no list.
    #[test]
    fn the_completions_highlighted_entry_is_the_command() {
        let screen = CopilotScreen;
        for (state, offered) in [
            ("idle", None),
            ("slash", Some("add-dir")),
            ("partial", Some("help")),
            ("ran", None),
            ("with_argument", Some("model")),
        ] {
            for drawn in screens_of("commands", COMMAND_STATES, state) {
                assert_eq!(
                    screen.command_completion(&drawn).as_deref(),
                    offered,
                    "{state}"
                );
            }
        }
    }

    /// A prompt already sent and the working directory's row, each indented one column, are no
    /// entry of a list; an entry's name is one word.
    #[test]
    fn a_sent_prompt_or_a_path_above_the_composer_is_no_completion() {
        let rule = "\u{2500}".repeat(40);
        let screen =
            |above: &str| format!("{above}\n{rule}\n\u{276F} /he\n{rule}\n footer \u{b7} x\n");
        assert_eq!(
            command_completion(&screen(" \u{276F} /help        23:02")),
            None
        );
        assert_eq!(command_completion(&screen(" /Volumes/work/dir")), None);
        assert_eq!(
            command_completion(&screen("  \u{276F} /help    Show help\n    /theme   Theme")),
            Some("help".to_string())
        );
        assert_eq!(
            command_completion(&screen("\u{2503} \u{276F} /add-dir   Allow")),
            Some("add-dir".to_string())
        );
        assert_eq!(command_completion(&screen("  \u{276F} /a/b   path")), None);
    }
}
