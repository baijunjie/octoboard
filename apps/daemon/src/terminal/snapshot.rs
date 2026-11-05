//! A bounded terminal state, flattened into ordinary scrolling output for archived previews, the
//! source of the mode preamble a live replay opens with once its ring has lost the start, and,
//! once asked, of the title changes the program's output makes.

use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Timeout};
use alacritty_terminal::Term;

use crate::terminal::title::TitleChange;

/// The UI retains this many lines too. Complete cells and their attributes survive even when
/// the escape sequence that established them is no longer in the raw replay ring.
const SCROLLBACK_LINES: usize = 10_000;
// Bound allocation before accepting a client-controlled PTY size.
pub const MAX_COLS: u16 = 1_000;
pub const MAX_ROWS: u16 = 500;

struct Size {
    rows: usize,
    cols: usize,
}
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Synchronized updates delay drawing in a window, but this headless screen has no drawing
/// loop. Parse every byte immediately, including a final frame captured before its end marker.
#[derive(Default)]
struct ImmediateUpdates;
impl Timeout for ImmediateUpdates {
    fn set_timeout(&mut self, _duration: Duration) {}
    fn clear_timeout(&mut self) {}
    fn pending_timeout(&self) -> bool {
        false
    }
}

/// What the parsed terminal reports as it takes the output in, of which only title changes and
/// the answers to two queries are kept, each only once asked for
/// ([`TerminalSnapshot::report_titles`], [`TerminalSnapshot::answer_queries`]). The rest (bells,
/// other answers to the program's queries, clipboard requests) is dropped, as before there was
/// anything to keep: the attached clients answer and act on the output themselves.
#[derive(Clone, Default)]
struct Events {
    titles: Arc<Mutex<Option<Vec<TitleChange>>>>,
    answers: Arc<Mutex<Option<Vec<u8>>>>,
}

impl Events {
    fn titles(&self) -> std::sync::MutexGuard<'_, Option<Vec<TitleChange>>> {
        self.titles.lock().expect("terminal events mutex poisoned")
    }

    fn answers(&self) -> std::sync::MutexGuard<'_, Option<Vec<u8>>> {
        self.answers.lock().expect("terminal events mutex poisoned")
    }
}

impl EventListener for Events {
    fn send_event(&self, event: Event) {
        let change = match event {
            Event::Title(title) => Some(title),
            Event::ResetTitle => None,
            Event::PtyWrite(answer) => {
                if let Some(answers) = self.answers().as_mut().filter(|_| is_kept(&answer)) {
                    answers.extend_from_slice(answer.as_bytes());
                }
                return;
            }
            _ => return,
        };
        if let Some(titles) = self.titles().as_mut() {
            titles.push(change);
        }
    }
}

/// Whether `answer`, which the parsed terminal writes to the program's input, is the answer to
/// the primary device attributes query (`CSI c`: `CSI ? 6 c`) or to the cursor position report
/// (`CSI 6 n`: `CSI row ; column R`). Every other answer it has (the secondary attributes, the
/// status report, mode reports, the keyboard mode) is left to the clients.
fn is_kept(answer: &str) -> bool {
    if answer == "\x1b[?6c" {
        return true;
    }
    answer
        .strip_prefix("\x1b[")
        .and_then(|rest| rest.strip_suffix('R'))
        .and_then(|position| position.split_once(';'))
        .is_some_and(|(row, column)| {
            [row, column]
                .iter()
                .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
}

pub struct TerminalSnapshot {
    terminal: Term<Events>,
    parser: Processor<ImmediateUpdates>,
    frozen: Option<Vec<u8>>,
    events: Events,
    /// The screen has shown something other than blanks since titles have been reported, so a
    /// title is no longer one that precedes any drawing ([`Self::report_titles`]).
    drawn: bool,
}

impl TerminalSnapshot {
    pub fn new(rows: u16, cols: u16) -> Self {
        let size = Size {
            rows: usize::from(rows.clamp(1, MAX_ROWS)),
            cols: usize::from(cols.clamp(2, MAX_COLS)),
        };
        let events = Events::default();
        Self {
            terminal: Term::new(
                // `kitty_keyboard` stays off: in alacritty_terminal, `push_keyboard_mode` panics
                // once the stack reaches 4096 entries, which a run of `CSI > 1 u` in agent output
                // reaches, and the panic would land under the session's output lock. The client,
                // xterm.js, has no kitty keyboard support to restore the flags into.
                Config {
                    scrolling_history: SCROLLBACK_LINES,
                    ..Config::default()
                },
                &size,
                events.clone(),
            ),
            parser: Processor::new(),
            frozen: None,
            events,
            drawn: false,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        if self.drawn || self.events.titles().is_none() {
            self.parser.advance(&mut self.terminal, bytes);
            return;
        }
        // Until the screen shows anything, a byte at a time, so that a title is known to precede
        // every drawing or not whatever reads the output was cut into. A byte draws something or
        // completes a title, never both, and what it draws is on the cursor's row once it has.
        for (at, byte) in bytes.iter().enumerate() {
            self.parser
                .advance(&mut self.terminal, std::slice::from_ref(byte));
            if self.cursor_row_shows_anything() {
                self.drawn = true;
                self.parser.advance(&mut self.terminal, &bytes[at + 1..]);
                return;
            }
            self.events.titles().iter_mut().for_each(Vec::clear);
        }
    }

    /// Keeps the title changes the output makes from now on, for [`Self::take_titles`], leaving out
    /// any made before the screen shows anything but blanks: a host that renders what the program
    /// draws can set a title of its own as it starts (conhost names the console after the program it
    /// runs), and the program has drawn nothing by then.
    pub fn report_titles(&mut self) {
        self.events.titles().get_or_insert_with(Vec::new);
    }

    /// Keeps the answers the parsed terminal gives to the output's primary device attributes and
    /// cursor position queries from now on, for [`Self::take_answers`].
    pub fn answer_queries(&mut self) {
        self.events.answers().get_or_insert_with(Vec::new);
    }

    /// The answers to those queries the output has made since the last call, written one after
    /// the other; none until [`Self::answer_queries`] has asked for them.
    pub fn take_answers(&mut self) -> Vec<u8> {
        self.events
            .answers()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// The title changes the output has made since the last call, in order; none until
    /// [`Self::report_titles`] has asked for them.
    pub fn take_titles(&mut self) -> Vec<TitleChange> {
        self.events
            .titles()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Whether any cell of the row the cursor is on shows something other than a blank.
    fn cursor_row_shows_anything(&self) -> bool {
        let grid = self.terminal.grid();
        let cells = &grid[grid.cursor.point.line];
        (0..grid.columns()).any(|col| !matches!(cells[Column(col)].c, ' ' | '\t'))
    }

    /// Whether bracketed paste is on now, read off the parsed state so that a sequence split
    /// across reads counts.
    pub fn bracketed_paste_on(&self) -> bool {
        self.terminal.mode().contains(TermMode::BRACKETED_PASTE)
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.terminal.resize(Size {
            rows: usize::from(rows),
            cols: usize::from(cols.max(2)),
        });
    }

    pub fn frozen(&self) -> Option<&[u8]> {
        self.frozen.as_deref()
    }

    /// Returns whether this call created the frozen screen.
    pub fn freeze(&mut self) -> bool {
        if self.frozen.is_some() {
            return false;
        }
        self.frozen = Some(self.render());
        true
    }

    pub fn discard_frozen(&mut self) {
        self.frozen = None;
    }

    pub fn output(&self) -> Vec<u8> {
        self.frozen.clone().unwrap_or_else(|| self.render())
    }

    /// Escape sequences that put a freshly reset terminal into the input and screen modes this
    /// one is in now. Programs such as Codex announce them once at startup, so a replay window
    /// that no longer reaches back that far has to be told them again or the client never learns
    /// the alternate screen is active or that mouse reporting is on.
    pub fn mode_preamble(&self) -> Vec<u8> {
        let mode = *self.terminal.mode();
        let mut output = String::new();
        for (flag, set) in [
            (TermMode::ALT_SCREEN, "\x1b[?1049h"),
            (TermMode::MOUSE_REPORT_CLICK, "\x1b[?1000h"),
            (TermMode::MOUSE_DRAG, "\x1b[?1002h"),
            (TermMode::MOUSE_MOTION, "\x1b[?1003h"),
            (TermMode::UTF8_MOUSE, "\x1b[?1005h"),
            (TermMode::SGR_MOUSE, "\x1b[?1006h"),
            (TermMode::FOCUS_IN_OUT, "\x1b[?1004h"),
            (TermMode::BRACKETED_PASTE, "\x1b[?2004h"),
            (TermMode::APP_CURSOR, "\x1b[?1h"),
            (TermMode::APP_KEYPAD, "\x1b="),
        ] {
            if mode.contains(flag) {
                output.push_str(set);
            }
        }
        // A reset client has both on, so only their absence needs saying.
        for (flag, reset) in [
            (TermMode::SHOW_CURSOR, "\x1b[?25l"),
            (TermMode::LINE_WRAP, "\x1b[?7l"),
        ] {
            if !mode.contains(flag) {
                output.push_str(reset);
            }
        }
        output.into_bytes()
    }

    /// The rows the screen shows right now as plain text, one line per row, without the scrollback.
    /// Recognising what an agent is showing goes by this and never by the byte stream: Claude Code
    /// places words with cursor moves and Codex redraws only the cells that changed, so the bytes
    /// hold no whole phrase.
    pub fn visible_text(&self) -> String {
        self.visible_text_where(|_, _| true)
    }

    /// [`Self::visible_text`] with every faint (SGR 2) cell shown as a space: what an agent
    /// writes as a hint rather than as text, such as Claude Code's placeholder in an empty input
    /// box. An inverse-video cell right before a faint one goes too: Claude Code draws its own
    /// cursor as an inverse cell, and over a hint it covers the hint's first character, which it
    /// leaves out of the faint run (`❯ Press up to edit queued messages`, 2.1.316).
    pub fn visible_text_undimmed(&self) -> String {
        let faint = |cell: &Cell| cell.flags.contains(Flags::DIM);
        self.visible_text_where(|cell, next| {
            !faint(cell) && !(cell.flags.contains(Flags::INVERSE) && next.is_some_and(faint))
        })
    }

    /// [`Self::visible_text`] with every cell that is not drawn in inverse video shown as a space:
    /// what a menu marks as its selected row by swapping foreground and background, as Codex
    /// marks the option its cursor is on.
    pub fn visible_text_inverse(&self) -> String {
        self.visible_text_where(|cell, _| cell.flags.contains(Flags::INVERSE))
    }

    /// The visible rows, a cell `shown` rejects written as a space. `shown` is given the cell
    /// and the next one drawn on its row.
    fn visible_text_where(&self, shown: impl Fn(&Cell, Option<&Cell>) -> bool) -> String {
        let spacer = |cell: &Cell| {
            cell.flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        };
        let grid = self.terminal.grid();
        let mut text = String::new();
        for row in 0..grid.screen_lines() as i32 {
            let cells = &grid[Line(row)];
            for col in 0..grid.columns() {
                let cell = &cells[Column(col)];
                if spacer(cell) {
                    continue;
                }
                let next = (col + 1..grid.columns())
                    .map(|col| &cells[Column(col)])
                    .find(|cell| !spacer(cell));
                if !shown(cell, next) {
                    text.push(' ');
                    continue;
                }
                text.push(cell.c);
                if let Some(chars) = cell.zerowidth() {
                    text.extend(chars);
                }
            }
            text.push('\n');
        }
        text
    }

    fn render(&self) -> Vec<u8> {
        let grid = self.terminal.grid();
        // Normal buffers include their history. Alternate screens have no scrollback, exactly
        // as in the attached terminal; the inactive normal buffer is not part of this view.
        let mut lines = Vec::with_capacity(grid.total_lines());
        for row in -(grid.history_size() as i32)..grid.screen_lines() as i32 {
            lines.push(render_row(grid, Line(row)));
        }
        while lines.last().is_some_and(|(line, _)| line.is_empty()) {
            lines.pop();
        }
        let mut output = String::from("\x1b[0m\x1b[?25l");
        let mut wrapped = false;
        for (index, (line, next_wrapped)) in lines.into_iter().enumerate() {
            if index > 0 && !wrapped {
                output.push_str("\r\n");
            }
            output.push_str(&line);
            wrapped = next_wrapped;
        }
        output.push_str("\x1b[0m");
        output.into_bytes()
    }
}

#[derive(PartialEq, Eq)]
struct Attributes {
    fg: Color,
    bg: Color,
    flags: Flags,
}
impl Default for Attributes {
    fn default() -> Self {
        Self::from(&Cell::default())
    }
}
impl From<&Cell> for Attributes {
    fn from(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags
                & !(Flags::WRAPLINE
                    | Flags::WIDE_CHAR
                    | Flags::WIDE_CHAR_SPACER
                    | Flags::LEADING_WIDE_CHAR_SPACER),
        }
    }
}

fn render_row(grid: &Grid<Cell>, row: Line) -> (String, bool) {
    let cells = &grid[row];
    let cols = grid.columns();
    let wrapped = cells[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
    let end = if wrapped {
        cols
    } else {
        (0..cols)
            .rev()
            .find(|&col| {
                let cell = &cells[Column(col)];
                cell.c != ' '
                    || cell.zerowidth().is_some_and(|chars| !chars.is_empty())
                    || Attributes::from(cell) != Attributes::default()
            })
            .map_or(0, |col| col + 1)
    };
    let mut output = String::new();
    let mut previous = Attributes::default();
    for col in 0..end {
        let cell = &cells[Column(col)];
        // A leading spacer is the unused last column before a wide glyph wraps. Emitting a
        // space there would insert a character when the archived line reflows on a wider client.
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let attrs = Attributes::from(cell);
        if attrs != previous {
            output.push_str("\x1b[0");
            for (flag, code) in [
                (Flags::BOLD, 1),
                (Flags::DIM, 2),
                (Flags::ITALIC, 3),
                (Flags::UNDERLINE, 4),
                (Flags::INVERSE, 7),
                (Flags::HIDDEN, 8),
                (Flags::STRIKEOUT, 9),
                (Flags::DOUBLE_UNDERLINE, 21),
            ] {
                if attrs.flags.contains(flag) {
                    write!(output, ";{code}").expect("writing a String");
                }
            }
            for (flag, style) in [
                (Flags::UNDERCURL, 3),
                (Flags::DOTTED_UNDERLINE, 4),
                (Flags::DASHED_UNDERLINE, 5),
            ] {
                if attrs.flags.contains(flag) {
                    write!(output, ";4:{style}").expect("writing a String");
                }
            }
            write_color(&mut output, attrs.fg, 38);
            write_color(&mut output, attrs.bg, 48);
            output.push('m');
            previous = attrs;
        }
        // Alacritty marks the first cell crossed by a tab with a literal tab. Its remaining
        // columns already exist in the grid, so replay it as one space rather than moving twice.
        output.push(if cell.c == '\t' { ' ' } else { cell.c });
        if let Some(chars) = cell.zerowidth() {
            output.extend(chars);
        }
    }
    if previous != Attributes::default() {
        output.push_str("\x1b[0m");
    }
    (output, wrapped)
}

fn write_color(output: &mut String, color: Color, code: u8) {
    match color {
        Color::Named(NamedColor::Foreground | NamedColor::Background) => {}
        Color::Named(named) => {
            // SGR named colors occupy the first sixteen palette entries. Internal derived
            // colors do not originate in terminal cells and need no separate serialized mode.
            let index = named as usize;
            if index < 16 {
                write!(output, ";{code};5;{index}").expect("writing a String");
            }
        }
        Color::Indexed(index) => write!(output, ";{code};5;{index}").expect("writing a String"),
        Color::Spec(rgb) => {
            write!(output, ";{code};2;{};{};{}", rgb.r, rgb.g, rgb.b).expect("writing a String")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::TermMode;

    fn replay(output: &[u8], rows: u16, cols: u16) -> TerminalSnapshot {
        let mut terminal = TerminalSnapshot::new(rows, cols);
        terminal.process(output);
        terminal
    }

    fn contents(terminal: &TerminalSnapshot) -> String {
        let grid = terminal.terminal.grid();
        let mut lines = Vec::new();
        for row in -(grid.history_size() as i32)..grid.screen_lines() as i32 {
            let mut text = String::new();
            for col in 0..grid.columns() {
                let cell = &grid[Line(row)][Column(col)];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                text.push(cell.c);
                if let Some(chars) = cell.zerowidth() {
                    text.extend(chars);
                }
            }
            lines.push(text.trim_end().to_string());
        }
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }

    #[test]
    fn only_the_device_attributes_and_cursor_position_answers_are_kept_and_only_when_asked_for() {
        let queries = b"\x1b[c\x1b[0c\x1b[>c\x1b[5n\x1b[?2004$p\x1b[?u\x1b]11;?\x07";
        let mut terminal = TerminalSnapshot::new(24, 80);
        terminal.process(queries);
        assert!(
            terminal.take_answers().is_empty(),
            "nothing is kept unasked"
        );

        terminal.answer_queries();
        terminal.process(queries);
        assert_eq!(terminal.take_answers(), b"\x1b[?6c\x1b[?6c");
        assert!(terminal.take_answers().is_empty());

        terminal.process(b"abc\x1b[6n");
        assert_eq!(terminal.take_answers(), b"\x1b[1;4R");
    }

    #[test]
    fn the_mode_preamble_restores_the_modes_a_reset_terminal_lacks() {
        let preamble = |output: &[u8]| {
            let mut terminal = TerminalSnapshot::new(4, 20);
            terminal.process(output);
            String::from_utf8(terminal.mode_preamble()).unwrap()
        };
        assert_eq!(preamble(b"plain"), "");
        let modes = preamble(b"\x1b[?1049h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?25l\x1b[?7l");
        for expected in ["?1049h", "?1002h", "?1006h", "?2004h", "?25l", "?7l"] {
            assert!(
                modes.contains(expected),
                "{expected} missing from {modes:?}"
            );
        }
    }

    #[test]
    fn captured_agent_screens_stay_readable_after_exit_cleanup() {
        for capture in [
            &include_bytes!("../../testdata/claude_trust_screen.bin")[..],
            &include_bytes!("../../testdata/grok_trust_screen.bin")[..],
        ] {
            let mut terminal = TerminalSnapshot::new(32, 120);
            for byte in capture {
                terminal.process(&[*byte]);
            }
            terminal.freeze();
            let frozen = terminal.output();
            let screen = replay(&frozen, 64, 160);
            assert!(
                contents(&screen).to_lowercase().contains("trust"),
                "{}",
                contents(&screen)
            );
            assert!(!screen.terminal.mode().intersects(
                TermMode::ALT_SCREEN | TermMode::MOUSE_MODE | TermMode::BRACKETED_PASTE
            ));
            terminal.process(b"\x1b[?1049l\x1b[2J\x1b[HResume this session with: exit");
            terminal.freeze();
            assert_eq!(terminal.output(), frozen);
        }
    }

    #[test]
    fn normal_history_survives_raw_ring_eviction_and_alternate_view_is_independent() {
        let mut terminal = TerminalSnapshot::new(3, 40);
        terminal.process(b"retained history\r\nsecond\r\nthird\r\nfourth");
        let mut ring =
            crate::terminal::ringbuf::RingBuffer::new(crate::terminal::fan::RING_CAPACITY);
        let redraw = b"\r\x1b[2Kfourth";
        for _ in 0..(crate::terminal::fan::RING_CAPACITY / redraw.len() + 1) {
            ring.push(redraw);
            terminal.process(redraw);
        }
        assert!(contents(&replay(&terminal.output(), 10, 40))
            .starts_with("retained history\nsecond\nthird\nfourth"));
        assert!(!String::from_utf8_lossy(&ring.snapshot()).contains("retained history"));
        terminal.process(b"\x1b[?1049h\x1b[Hactive screen");
        assert_eq!(
            contents(&replay(&terminal.output(), 10, 40)),
            "active screen"
        );
        terminal.process(b"\x1b[?1049l");
        assert!(contents(&replay(&terminal.output(), 10, 40)).contains("retained history"));
    }

    #[test]
    fn wraps_reflow_and_cell_styles_and_wide_characters_survive() {
        let mut terminal = TerminalSnapshot::new(4, 6);
        terminal
            .process("\x1b[1;3;4;38;2;1;2;3;48;5;42m界e\u{301}ABCDEF\x1b[0m\r\nlast".as_bytes());
        let screen = replay(&terminal.output(), 10, 20);
        assert_eq!(contents(&screen), "界e\u{301}ABCDEF\nlast");
        let first = &screen.terminal.grid()[Line(0)][Column(0)];
        assert!(first
            .flags
            .contains(Flags::WIDE_CHAR | Flags::BOLD | Flags::ITALIC | Flags::UNDERLINE));
        assert_eq!(first.bg, Color::Indexed(42));
        assert_eq!(
            first.fg,
            Color::Spec(alacritty_terminal::vte::ansi::Rgb { r: 1, g: 2, b: 3 })
        );
    }

    #[test]
    fn resizing_keeps_normal_viewport_and_history_and_accepts_continued_output() {
        let mut terminal = TerminalSnapshot::new(4, 12);
        terminal.process(b"abcdefghijkl\r\none\r\ntwo\r\nthree");
        terminal.resize(2, 4);
        assert!(contents(&replay(&terminal.output(), 20, 40)).contains("abcdefghijkl"));
        terminal.process(b"\r\nfour");
        terminal.resize(6, 20);
        assert_eq!(
            contents(&replay(&terminal.output(), 20, 40)),
            "abcdefghijkl\none\ntwo\nthree\nfour"
        );
    }

    #[test]
    fn parser_keeps_incomplete_escape_and_utf8_sequences_across_resizes() {
        let mut terminal = TerminalSnapshot::new(4, 12);
        terminal.process(b"\x1b[38;2;1;");
        terminal.resize(5, 20);
        terminal.process(b"2;3m\xe7");
        terminal.resize(6, 30);
        terminal.process(b"\x95\x8c");
        let screen = replay(&terminal.output(), 6, 30);
        assert_eq!(contents(&screen), "界");
        assert_eq!(
            screen.terminal.grid()[Line(0)][Column(0)].fg,
            Color::Spec(alacritty_terminal::vte::ansi::Rgb { r: 1, g: 2, b: 3 })
        );
    }

    #[test]
    fn a_wide_character_at_the_margin_keeps_its_soft_wrap() {
        let mut terminal = TerminalSnapshot::new(5, 4);
        terminal.process("abc界DEF".as_bytes());
        assert_eq!(contents(&replay(&terminal.output(), 5, 40)), "abc界DEF");
    }

    #[test]
    fn tabs_are_flattened_without_reapplying_cursor_movement() {
        let mut terminal = TerminalSnapshot::new(2, 20);
        terminal.process(b"a\tb");
        assert_eq!(contents(&replay(&terminal.output(), 2, 20)), "a       b");
        assert!(!terminal.output().contains(&b'\t'));
    }

    #[test]
    fn styled_trailing_blanks_and_hidden_struck_text_are_preserved() {
        let mut terminal = TerminalSnapshot::new(2, 20);
        terminal.process(b"x\x1b[48;5;21m  \x1b[0m\x1b[8;9msecret");
        let screen = replay(&terminal.output(), 2, 20);
        let grid = screen.terminal.grid();
        assert_eq!(grid[Line(0)][Column(2)].bg, Color::Indexed(21));
        assert_eq!(
            grid[Line(0)][Column(3)].bg,
            Color::Named(NamedColor::Background)
        );
        assert!(grid[Line(0)][Column(3)]
            .flags
            .contains(Flags::HIDDEN | Flags::STRIKEOUT));
    }
}
