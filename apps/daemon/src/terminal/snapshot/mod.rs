//! A bounded terminal state: flattened into ordinary scrolling output for archived previews,
//! serialised as the paint a client attaching to a terminal starts from and as the screen a
//! planned restart saves, and, once asked, the source of the title changes the program's output
//! makes and of the answers to the program's queries.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Cursor, Dimensions};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Processor, Rgb, Timeout};
use alacritty_terminal::Term;

use crate::terminal::title::TitleChange;
use cells::render_row;
use unended::Unended;
use unexposed::Unexposed;

mod answers;
mod cells;
mod paint;
mod save;
mod unended;
mod unexposed;

pub(crate) use answers::TakenIn;
#[cfg(test)]
pub(crate) use paint::tests::assert_same_state;

/// The UI retains this many lines too. Complete cells and their attributes survive even when
/// the escape sequence that established them is no longer in the raw replay ring.
const SCROLLBACK_LINES: usize = 10_000;
// Bound allocation before accepting a client-controlled PTY size.
pub const MAX_COLS: u16 = 1_000;
pub const MAX_ROWS: u16 = 500;

/// Whether a terminal of `rows` by `cols` is within the range the daemon supports.
pub fn is_supported_size(rows: u16, cols: u16) -> bool {
    (1..=MAX_ROWS).contains(&rows) && (2..=MAX_COLS).contains(&cols)
}

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

/// What the parsed terminal asks of the window it would draw in while it parses a query: the text
/// of an answer it has itself, or one that needs a colour or the window's size to be written.
enum Request {
    Text(String),
    Color(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send>),
    Area(Arc<dyn Fn(WindowSize) -> String + Sync + Send>),
}

/// What the parsed terminal reports as it takes the output in, of which only title changes and the
/// requests a query makes are kept: titles only once asked for
/// ([`TerminalSnapshot::report_titles`]), requests only while [`TerminalSnapshot::take_in`] parses
/// a query it recognised. The rest (bells, clipboard requests, the answers to queries it did not
/// recognise) is dropped: the attached clients act on that output themselves.
#[derive(Clone, Default)]
struct Events {
    titles: Arc<Mutex<Option<Vec<TitleChange>>>>,
    requests: Arc<Mutex<Option<Vec<Request>>>>,
}

impl Events {
    fn titles(&self) -> std::sync::MutexGuard<'_, Option<Vec<TitleChange>>> {
        self.titles.lock().expect("terminal events mutex poisoned")
    }

    fn requests(&self) -> std::sync::MutexGuard<'_, Option<Vec<Request>>> {
        self.requests
            .lock()
            .expect("terminal events mutex poisoned")
    }
}

impl EventListener for Events {
    fn send_event(&self, event: Event) {
        let change = match event {
            Event::Title(title) => Some(title),
            Event::ResetTitle => None,
            Event::PtyWrite(answer) => return self.request(Request::Text(answer)),
            Event::ColorRequest(index, answer) => {
                return self.request(Request::Color(index, answer))
            }
            Event::TextAreaSizeRequest(answer) => return self.request(Request::Area(answer)),
            _ => return,
        };
        if let Some(titles) = self.titles().as_mut() {
            titles.push(change);
        }
    }
}

impl Events {
    fn request(&self, request: Request) {
        if let Some(requests) = self.requests().as_mut() {
            requests.push(request);
        }
    }
}

pub struct TerminalSnapshot {
    terminal: Term<Events>,
    parser: Processor<ImmediateUpdates>,
    unexposed: Unexposed,
    unexposed_parser: Processor<ImmediateUpdates>,
    /// The sequence both parsers have begun and not ended, for [`Self::save`].
    unended: Unended,
    frozen: Option<Vec<u8>>,
    events: Events,
    /// Set by [`Self::answer_queries`].
    answering: Option<answers::QueryAnswering>,
    /// The screen has shown something other than blanks since titles have been reported, so a
    /// title is no longer one that precedes any drawing ([`Self::report_titles`]).
    drawn: bool,
    /// The normal screen is known to be as a reset leaves it: nothing drawn on it, nothing in its
    /// scrollback, its cursor home with the default pen. Behind the alternate screen, which
    /// `alacritty_terminal` does not let be read, only such a normal screen is one a saved screen
    /// rebuilds ([`Self::save`]). Watched while the normal screen is shown, and given up on the
    /// first byte that changes it, until a reset may have made it blank again.
    blank_normal: bool,
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
                // xterm.js, is left with `vtExtensions.kittyKeyboard` off, so there are no flags to
                // restore.
                Config {
                    scrolling_history: SCROLLBACK_LINES,
                    ..Config::default()
                },
                &size,
                events.clone(),
            ),
            parser: Processor::new(),
            unexposed: Unexposed::new(size.rows),
            unexposed_parser: Processor::new(),
            unended: Unended::default(),
            frozen: None,
            events,
            answering: None,
            drawn: false,
            blank_normal: true,
        }
    }

    /// Sets whether the screen has shown something since titles have been reported, for a
    /// restored one (`SavedScreen::drawn`).
    pub fn set_drawn(&mut self, drawn: bool) {
        self.drawn = drawn;
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.unended.advance(bytes);
        self.unexposed.left_alternate = false;
        self.unexposed_parser.advance(&mut self.unexposed, bytes);
        let leaves_alternate = self.unexposed.left_alternate;
        let mut rest = bytes;
        // A reset (RIS) may leave the normal screen blank again, which is looked for over the
        // whole screen right after each. One cut across two chunks is missed, and the screen then
        // stays counted as not blank, which only costs a saved screen its exactness.
        while let Some(at) = rest.windows(2).position(|pair| pair == b"\x1bc") {
            let (through, after) = rest.split_at(at + 2);
            self.parse(through, leaves_alternate);
            self.blank_normal = self.normal_is_blank();
            rest = after;
        }
        self.parse(rest, leaves_alternate);
    }

    /// Has the parsed terminal take `bytes` in, a byte at a time for as long as something has to
    /// be told after each: until the screen shows anything once titles are reported, so that a
    /// title is known to precede every drawing or not whatever reads the output was cut into (a
    /// byte draws something or completes a title, never both, and what it draws is on the
    /// cursor's row once it has); and while the normal screen is shown blank, so that it is known
    /// whether it still was when the program entered the alternate screen, or behind the
    /// alternate screen while blank, where `leaves_alternate` (the output leaves the alternate
    /// screen somewhere), so that what it draws on the normal screen before it enters the
    /// alternate one again counts.
    fn parse(&mut self, bytes: &[u8], leaves_alternate: bool) {
        let mut at = 0;
        while at < bytes.len() {
            let awaiting_drawing = !self.drawn && self.events.titles().is_some();
            let watching_normal = self.watches_normal() || (leaves_alternate && self.blank_normal);
            if !awaiting_drawing && !watching_normal {
                break;
            }
            self.parser
                .advance(&mut self.terminal, std::slice::from_ref(&bytes[at]));
            at += 1;
            if awaiting_drawing {
                if self.cursor_row_shows_anything() {
                    self.drawn = true;
                } else {
                    self.events.titles().iter_mut().for_each(Vec::clear);
                }
            }
            if watching_normal {
                self.note_normal();
            }
        }
        self.parser.advance(&mut self.terminal, &bytes[at..]);
    }

    /// Whether the normal screen is shown and still blank ([`Self::blank_normal`]).
    fn watches_normal(&self) -> bool {
        self.blank_normal && !self.terminal.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Notes whether the normal screen, blank before the output last taken in, still is. Output
    /// that entered the alternate screen left it as it was.
    fn note_normal(&mut self) {
        if !self.terminal.mode().contains(TermMode::ALT_SCREEN) {
            self.blank_normal = self.normal_still_blank();
        }
    }

    /// Whether the normal screen, shown and blank before the last sequence or character taken in,
    /// still is: nothing in its scrollback, its cursor as a reset leaves it, and nothing on the
    /// home cell. Drawing a character moves the cursor or leaves it waiting to wrap, a zero-width
    /// one drawn at home lands on the home cell, and so does a screen alignment test (DECALN);
    /// whatever else changes cells with the cursor home and the default pen leaves them blank,
    /// or scrolls a row into the scrollback.
    fn normal_still_blank(&self) -> bool {
        let grid = self.terminal.grid();
        grid.history_size() == 0
            && grid.cursor == Cursor::default()
            && grid[Line(0)][Column(0)] == Cell::default()
    }

    /// Whether the normal screen is shown and as a reset leaves it, every cell of it looked at.
    fn normal_is_blank(&self) -> bool {
        if self.terminal.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        let grid = self.terminal.grid();
        let blank = Cell::default();
        grid.history_size() == 0
            && grid.cursor == Cursor::default()
            && (0..grid.screen_lines() as i32).all(|row| {
                let cells = &grid[Line(row)];
                (0..grid.columns()).all(|col| cells[Column(col)] == blank)
            })
    }

    /// Keeps the title changes the output makes from now on, for [`Self::take_titles`], leaving out
    /// any made before the screen shows anything but blanks: a host that renders what the program
    /// draws can set a title of its own as it starts (conhost names the console after the program
    /// it runs), and the program has drawn nothing by then.
    pub fn report_titles(&mut self) {
        self.events.titles().get_or_insert_with(Vec::new);
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
        let size = Size {
            rows: usize::from(rows),
            cols: usize::from(cols.max(2)),
        };
        let changed =
            size.rows != self.terminal.screen_lines() || size.cols != self.terminal.columns();
        self.unexposed.resize(size.rows, changed);
        self.terminal.resize(size);
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
            lines.push(render_row(grid, Line(row), false));
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

#[cfg(test)]
mod tests;
