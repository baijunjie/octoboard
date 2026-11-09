use std::fmt::Write;

use alacritty_terminal::grid::{Cursor, Dimensions};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::TermMode;

use crate::terminal::snapshot::tests::replay;
use crate::terminal::snapshot::{TerminalSnapshot, SCROLLBACK_LINES};

impl TerminalSnapshot {
    /// Everything a first paint restores, one line per item and per row of the scrollback and
    /// the screen, for comparing two terminals' states.
    fn state(&self) -> Vec<String> {
        let restored = TermMode::SHOW_CURSOR
            | TermMode::APP_CURSOR
            | TermMode::APP_KEYPAD
            | TermMode::MOUSE_MODE
            | TermMode::BRACKETED_PASTE
            | TermMode::SGR_MOUSE
            | TermMode::UTF8_MOUSE
            | TermMode::LINE_WRAP
            | TermMode::LINE_FEED_NEW_LINE
            | TermMode::ORIGIN
            | TermMode::INSERT
            | TermMode::FOCUS_IN_OUT
            | TermMode::ALT_SCREEN;
        let grid = self.terminal.grid();
        let cursor = |cursor: &Cursor<Cell>| {
            format!(
                "{:?} wrap pending {} pen {:?} {:?} {:?} {:?} link {:?}",
                cursor.point,
                cursor.input_needs_wrap,
                cursor.template.fg,
                cursor.template.bg,
                cursor.template.underline_color(),
                cursor.template.flags,
                cursor
                    .template
                    .hyperlink()
                    .map(|link| link.uri().to_string()),
            )
        };
        let mut state = vec![
            format!("mode {:?}", *self.terminal.mode() & restored),
            format!("size {}x{}", grid.screen_lines(), grid.columns()),
            format!("cursor {}", cursor(&grid.cursor)),
            format!("saved {}", cursor(&grid.saved_cursor)),
            format!("region {:?}", self.unexposed.region),
            format!("cursor style {:?}", self.unexposed.cursor_style),
            format!("title {:?}", self.unexposed.title),
            format!("history {}", grid.history_size()),
        ];
        for row in -(grid.history_size() as i32)..grid.screen_lines() as i32 {
            let mut line = format!("{row:>6} ");
            let mut previous = None;
            for col in 0..grid.columns() {
                let cell = &grid[Line(row)][Column(col)];
                let style = (
                    cell.fg,
                    cell.bg,
                    cell.underline_color(),
                    cell.flags,
                    cell.hyperlink().map(|link| link.uri().to_string()),
                );
                if previous.as_ref() != Some(&style) {
                    write!(line, "{style:?}").expect("writing a String");
                    previous = Some(style);
                }
                line.push(if cell.c == '\t' { ' ' } else { cell.c });
                if let Some(chars) = cell.zerowidth() {
                    line.extend(chars);
                }
            }
            state.push(line);
        }
        state
    }
}

/// Asserts that `painted`, a terminal that took a first paint of `original`, is in the state
/// `original` is in, naming the first line that differs.
pub(crate) fn assert_same_state(painted: &TerminalSnapshot, original: &TerminalSnapshot) {
    let (painted, original) = (painted.state(), original.state());
    for (at, (got, want)) in painted.iter().zip(&original).enumerate() {
        assert_eq!(got, want, "line {at} of the state differs");
    }
    assert_eq!(painted.len(), original.len(), "the states differ in length");
}

fn painted(original: &TerminalSnapshot, history: usize) -> TerminalSnapshot {
    let grid = original.terminal.grid();
    let mut terminal = TerminalSnapshot::new(grid.screen_lines() as u16, grid.columns() as u16);
    terminal.process(&original.paint(history));
    terminal
}

#[test]
fn a_first_paint_reproduces_recorded_agent_screens_as_their_replay_does() {
    for (capture, rows, cols) in [
        (
            &include_bytes!("../../../../testdata/claude_trust_screen.bin")[..],
            32,
            120,
        ),
        (
            &include_bytes!("../../../../testdata/claude_input_restored.bin")[..],
            32,
            120,
        ),
        (
            &include_bytes!("../../../../testdata/claude_native_stashed.bin")[..],
            32,
            120,
        ),
        (
            &include_bytes!("../../../../testdata/codex_0166_macos_120x40.bin")[..],
            40,
            120,
        ),
        (
            &include_bytes!("../../../../testdata/codex_0166_macos_44x40.bin")[..],
            40,
            44,
        ),
        (
            &include_bytes!("../../../../testdata/codex_trust_screen.bin")[..],
            32,
            120,
        ),
        (
            &include_bytes!("../../../../testdata/codex_hook_review.bin")[..],
            32,
            120,
        ),
        (
            &include_bytes!("../../../../testdata/grok_trust_screen.bin")[..],
            32,
            120,
        ),
        (
            &include_bytes!("../../../../testdata/grok_composer_draft.bin")[..],
            32,
            120,
        ),
        (
            &include_bytes!("../../../../testdata/conhost_codex_turn_running.bin")[..],
            32,
            120,
        ),
    ] {
        // The raw output replayed into a terminal of the same size: what a client attached
        // throughout shows.
        let replayed = replay(capture, rows, cols);
        assert_same_state(&painted(&replayed, SCROLLBACK_LINES), &replayed);
    }
}

#[test]
fn a_first_paint_restores_the_alternate_screen_modes_cursor_region_and_title() {
    let mut original = TerminalSnapshot::new(6, 20);
    original.process(
        b"\x1b]2;pushed\x07\x1b[22;0t\x1b]2;the title\x07normal\r\n\x1b[?1049h\x1b[?1000h\x1b[?1006h\
          \x1b[?2004h\x1b[?1h\x1b=\x1b[?1004h\x1b[5 q\x1b[4h\x1b[2;5r\x1b[?6h\
          \x1b[3;3H\x1b[1;38;5;3mSAVED\x1b7\x1b[0m\x1b[1;1H\
          \x1b]8;id=x;https://example.com\x1b\\link\x1b]8;;\x1b\\\
          \x1b[4;1H\x1b[4l12345678901234567890\x1b[3;48;2;1;2;3m\x1b[4h",
    );
    let painted = painted(&original, 100);
    assert_same_state(&painted, &original);
    assert!(painted.terminal.grid().cursor.input_needs_wrap);
    assert_eq!(painted.unexposed.title.as_deref(), Some("the title"));
    assert_eq!(painted.unexposed.region, Some((1, 5)));
}

#[test]
fn a_first_paint_carries_the_scrollback_it_is_asked_for_and_keeps_soft_wraps() {
    let mut original = TerminalSnapshot::new(3, 10);
    for line in 0..20 {
        original.process(format!("line {line}\r\n").as_bytes());
    }
    // A soft wrap followed by a row cleared after it, and a wide glyph that wrapped.
    original.process("0123456789abc\r\x1b[2K\r\n123456789界".as_bytes());
    assert_same_state(&painted(&original, SCROLLBACK_LINES), &original);

    let paint = painted(&original, 5);
    assert_eq!(paint.terminal.grid().history_size(), 5);
    assert_eq!(paint.visible_text(), original.visible_text());
    assert_eq!(painted(&original, 0).terminal.grid().history_size(), 0);
}

#[test]
fn a_first_paint_starts_from_a_reset_terminal_and_asks_nothing() {
    let mut original = TerminalSnapshot::new(4, 20);
    original.process(b"\x1b[6nprompt\x1b]11;?\x07");
    let paint = original.paint(10);
    assert!(paint.starts_with(b"\x1bc"));

    let mut client = TerminalSnapshot::new(4, 20);
    client.answer_queries(false);
    client.process(b"\x1b[?1049h\x1b[?1000hstale");
    let taken = client.take_in(&paint);
    assert!(taken.asked.is_empty(), "the paint asked {:?}", taken.asked);
    assert_eq!(taken.delivered, paint);
    assert_same_state(&client, &original);
}

#[test]
fn a_first_paint_keeps_named_and_underline_colours_and_a_link_and_pen_at_a_pending_wrap() {
    let mut original = TerminalSnapshot::new(4, 10);
    original.process(
        b"\x1b[31;102mnamed\x1b[0m \x1b[4;58;5;9mund\x1b[0m\r\n\
          \x1b[?12h\x1b[3;1H12345678\x1b]8;;https://example.com/a\x1b\\Zz\x1b7\
          \x1b]8;;\x1b\\\x1b[4;1H123456789\x1b]8;;https://example.com/b\x1b\\Q\x1b[1m",
    );
    let grid = original.terminal.grid();
    assert!(grid.cursor.input_needs_wrap && grid.saved_cursor.input_needs_wrap);
    let painted = painted(&original, 0);
    assert_same_state(&painted, &original);
    assert_eq!(
        painted.unexposed.cursor_style.map(|style| style.blinking),
        Some(true)
    );
}

#[test]
fn a_first_paint_follows_a_column_mode_switch_resetting_the_scroll_region() {
    let mut original = TerminalSnapshot::new(6, 20);
    original.process(b"\x1b[2;4rtext\x1b[?3h");
    let painted = painted(&original, 0);
    assert_eq!(painted.unexposed.region, None);
    assert_same_state(&painted, &original);
}
