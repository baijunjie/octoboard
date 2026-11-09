//! The paint a client attaching to a terminal starts from.

use std::fmt::Write;

use alacritty_terminal::grid::{Charsets, Cursor, Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::CursorShape;

use super::cells::{push_cell, render_row, sgr, write_link, write_pen, Attributes};
use super::TerminalSnapshot;

impl TerminalSnapshot {
    /// The input and screen modes this terminal is in but the alternate screen, which moves no
    /// cursor and changes no screen content, as set or reset from a freshly reset terminal.
    fn write_modes(&self, output: &mut String) {
        let mode = *self.terminal.mode();
        for (flag, set) in [
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
    }

    /// The first paint of a client attaching to this terminal: escape sequences that take any
    /// terminal of the same size to the screen this one shows now, so that the program's live
    /// output can follow it. It resets the client's terminal and clears it, then writes up to
    /// `history` of the normal screen's scrollback rows, which scroll into the client's own
    /// scrollback, and the normal screen's rows and cursor, then, when the program is on the
    /// alternate screen, enters it and writes its rows, and then the scroll region, the saved and
    /// current cursor with the pen each draws with, the modes, the cursor style and the title. It
    /// asks the client nothing.
    ///
    /// Takes `&mut self` only to read the normal screen behind the alternate one
    /// ([`Self::normal_behind_alternate`]), which leaves the terminal as it was.
    ///
    /// Not painted, and so different on a client that attached after them:
    /// - the normal screen hidden behind the alternate one after the width changed while the
    ///   alternate one was shown, until the program enters it again (`normal_reflowed`), so such
    ///   a client shows a blank normal screen once the program leaves the alternate one;
    /// - the character sets, tab stops and colour palette changes, which programs leave at their
    ///   defaults when idle;
    /// - the kitty keyboard flags, which the daemon's parser does not keep;
    /// - a saved cursor above or below the scroll region in origin mode, which lands on the
    ///   region's edge;
    /// - the titles the program pushed, which a saved screen restores
    ///   ([`Self::write_title_stack`]).
    pub fn paint(&mut self, history: usize) -> Vec<u8> {
        let (normal, _) = self.normal_behind_alternate(history);
        self.write_paint(history, false, normal.as_deref())
            .into_bytes()
    }

    /// The bytes that write the normal screen behind the alternate one, with up to `history` rows
    /// of its scrollback, where it is shown and the normal one is not known to be blank
    /// (`blank_normal`), for a paint or a saved screen to write before it enters the alternate
    /// screen (see `write_normal_behind`), and what of it the bytes leave out, if anything: all of
    /// it after a change of width (below) or where its saved cursor was lost, or only the character
    /// sets of its cursor, which the bytes do not designate. Leaves the terminal as it was.
    ///
    /// `alacritty_terminal` keeps the hidden grid to itself, but lets the screens be swapped and
    /// the shown grid be replaced: leaving the alternate screen keeps both as they are, and
    /// entering it again clears the alternate grid, gives it the normal screen's cursor and saves
    /// that cursor on the normal screen. The alternate grid copied beforehand puts back all of
    /// that but the last, which entering the alternate screen did already and only a change of
    /// width while it was shown can have undone since (reflow moves the cursor and only clamps
    /// the saved one); after one (`normal_reflowed`) the normal screen is not read. Swapping also
    /// swaps the kitty keyboard mode stacks and sets the mode from the top of the shown one, which
    /// changes nothing only because `kitty_keyboard` stays off (see [`TerminalSnapshot::new`]).
    pub(super) fn normal_behind_alternate(
        &mut self,
        history: usize,
    ) -> (Option<String>, Option<&'static str>) {
        if !self.terminal.mode().contains(TermMode::ALT_SCREEN) || self.blank_normal {
            return (None, None);
        }
        if self.normal_reflowed {
            return (
                None,
                Some("the normal screen behind the alternate one, reflowed"),
            );
        }
        // Only the alternate grid, which has no scrollback, is copied; the normal one is written
        // while it is shown.
        let alternate = self.terminal.grid().clone();
        self.terminal.swap_alt();
        let normal = self.terminal.grid();
        let lost_saved_cursor = normal.saved_cursor != normal.cursor;
        let written = if lost_saved_cursor {
            (None, Some("the normal screen's saved cursor"))
        } else {
            let mut output = String::new();
            write_normal_behind(&mut output, normal, history);
            let charsets = normal.cursor.charsets != Charsets::default();
            (Some(output), charsets.then_some("the character sets"))
        };
        self.terminal.swap_alt();
        *self.terminal.grid_mut() = alternate;
        if lost_saved_cursor {
            // Unreachable while `normal_reflowed` is kept right: entering again has just saved
            // the cursor over the one the normal screen had saved.
            tracing::warn!(
                "reading the normal screen behind the alternate one lost its saved cursor"
            );
        }
        written
    }

    /// [`Self::paint`], or where `saving`, the paint a saved screen starts with: the titles the
    /// program pushed are pushed again right after the reset, and neither the rows of a normal
    /// screen as a reset leaves it nor a saved cursor as a reset leaves it are written, so that the
    /// restored terminal is known to be in that state too (see `blank_normal`), which writing them
    /// would hide. `normal` is what writes the normal screen behind the alternate one, written
    /// before the alternate screen is entered ([`write_normal_behind`]).
    pub(super) fn write_paint(&self, history: usize, saving: bool, normal: Option<&str>) -> String {
        let mode = *self.terminal.mode();
        let grid = self.terminal.grid();
        let mut output = String::from("\x1bc");
        if saving {
            self.write_title_stack(&mut output);
        }
        let history = if mode.contains(TermMode::ALT_SCREEN) {
            if let Some(normal) = normal {
                output.push_str(normal);
            }
            output.push_str("\x1b[?1049h");
            if normal.is_some() {
                // Entering gave the alternate screen the normal one's cursor, which its rows are
                // not written from, and cleared it with that cursor's background, which the rows
                // leave in the blanks they do not write: both are reset first.
                output.push_str("\x1b[H");
                write_pen(&mut output, &Cursor::default());
                output.push_str("\x1b[2J");
            }
            0
        } else {
            history.min(grid.history_size())
        };
        if !(saving && self.normal_is_blank()) {
            write_rows(&mut output, grid, history);
        }
        let origin = self.write_region(&mut output);
        // A saved screen leaves a saved cursor as the reset left it alone: saving it again would
        // count, on the alternate screen, as one the program saved there (`left_out`).
        if !(saving && grid.saved_cursor == Cursor::default()) {
            place(&mut output, grid, &grid.saved_cursor, origin);
            output.push_str("\x1b7");
        }
        place(&mut output, grid, &grid.cursor, origin);
        // Set only now: in insert mode, drawing a cell again to place a cursor would shift the row.
        for (flag, set) in [
            (TermMode::INSERT, "\x1b[4h"),
            (TermMode::LINE_FEED_NEW_LINE, "\x1b[20h"),
        ] {
            if mode.contains(flag) {
                output.push_str(set);
            }
        }
        self.write_modes(&mut output);
        if let Some(style) = self.unexposed.cursor_style {
            let blinking = u8::from(!style.blinking);
            let shape = match style.shape {
                CursorShape::Underline => 3,
                CursorShape::Beam => 5,
                CursorShape::Block | CursorShape::HollowBlock | CursorShape::Hidden => 1,
            };
            write!(output, "\x1b[{} q", shape + blinking).expect("writing a String");
        }
        if let Some(title) = &self.unexposed.title {
            write!(output, "\x1b]2;{title}\x07").expect("writing a String");
        }
        output
    }

    /// Pushes the titles the program pushed (`CSI 22 t`) onto a reset terminal's empty stack, each
    /// made the title first, oldest first. Only the entries pushed before any title was set are
    /// without one, and a reset terminal has none to push for them; the title the last leaves is
    /// replaced by the current one at the end of the paint. A stack this does not rebuild is one
    /// [`Self::save`] says it leaves out.
    fn write_title_stack(&self, output: &mut String) {
        for title in &self.unexposed.titles {
            if let Some(title) = title {
                write!(output, "\x1b]2;{title}\x07").expect("writing a String");
            }
            output.push_str("\x1b[22t");
        }
    }

    /// Writes the scroll region and origin mode, and returns the row cursor positions count from:
    /// the region's top in origin mode, else the screen's.
    fn write_region(&self, output: &mut String) -> usize {
        let top = match self.unexposed.region {
            Some((start, end)) => {
                write!(output, "\x1b[{};{}r", start + 1, end).expect("writing a String");
                start
            }
            None => 0,
        };
        if self.terminal.mode().contains(TermMode::ORIGIN) {
            output.push_str("\x1b[?6h");
            top
        } else {
            0
        }
    }
}

/// Writes the last `history` rows of `grid`'s scrollback and its screen's rows, one after another
/// with soft wraps kept, as [`TerminalSnapshot::output`] does, so that the client can reflow them.
/// Every row is written, blank or not, so that the last ones land on the screen and those before
/// them in the scrollback. A soft-wrapped row fills the width, and the next row goes on by
/// wrapping.
fn write_rows(output: &mut String, grid: &Grid<Cell>, history: usize) {
    let first = -(history as i32);
    let mut wrapped = false;
    for row in first..grid.screen_lines() as i32 {
        if row > first && !wrapped {
            output.push_str("\r\n");
        }
        let (line, next_wrapped) = render_row(grid, Line(row), true);
        if wrapped && line.is_empty() {
            // Only a character wraps onto the row, and a blank one is a cleared cell.
            output.push(' ');
        }
        output.push_str(&line);
        wrapped = next_wrapped;
    }
}

/// Writes the normal screen `normal`, hidden behind the alternate one, onto the normal screen of a
/// reset terminal: up to `history` rows of its scrollback, its screen's rows and its cursor with
/// its pen. Entering the alternate screen right after keeps them as they are, saving that cursor
/// on the normal screen as entering it did there (`TerminalSnapshot::normal_behind_alternate` has
/// made sure the two are still the same), and leaving it shows them again. The scroll region and
/// origin mode are only written after this, so the cursor is placed from the screen's top.
fn write_normal_behind(output: &mut String, normal: &Grid<Cell>, history: usize) {
    write_rows(output, normal, history.min(normal.history_size()));
    place(output, normal, &normal.cursor, 0);
}

/// Moves the client's cursor to where `cursor` is, rows counted from `origin`, and gives it the
/// pen `cursor` draws with. A cursor past the last column, waiting to wrap with the next
/// character, is only reached by drawing that column again, a wide glyph there from its first
/// column.
pub(super) fn place(output: &mut String, grid: &Grid<Cell>, cursor: &Cursor<Cell>, origin: usize) {
    let line = cursor.point.line;
    let mut column = cursor.point.column.0;
    let cells = &grid[line];
    if cursor.input_needs_wrap {
        let last = grid.columns() - 1;
        column = if cells[Column(last)].flags.contains(Flags::WIDE_CHAR_SPACER) {
            last - 1
        } else {
            last
        };
    }
    let row = usize::try_from(line.0).unwrap_or(0).saturating_sub(origin);
    write!(output, "\x1b[{};{}H", row + 1, column + 1).expect("writing a String");
    if cursor.input_needs_wrap {
        let cell = &cells[Column(column)];
        output.push_str(&sgr(&Attributes::from(cell)));
        write_link(output, cell.hyperlink().as_ref());
        push_cell(output, cell);
    }
    write_pen(output, cursor);
}

#[cfg(test)]
pub(super) mod tests;
