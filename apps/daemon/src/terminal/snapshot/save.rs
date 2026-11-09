//! The screen a planned restart saves for the next daemon.

use std::fmt::Write;

use alacritty_terminal::grid::{Charsets, Cursor, Dimensions};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::color::COUNT;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{CharsetIndex, Handler, Processor};

use super::cells::write_pen;
use super::paint::place;
use super::{ImmediateUpdates, TerminalSnapshot};
use crate::terminal::saved_screen::SavedScreen;

impl TerminalSnapshot {
    /// The bytes that take a fresh terminal of this one's size, which answers queries and keeps
    /// status reports as this one does, to the state this one is in, as [`SavedScreen`] describes
    /// them, with this terminal's size. Marked inexact where this terminal is in a state the bytes
    /// do not rebuild ([`Self::left_out`]); the caller adds whether the screen was inexact
    /// already. `None` while the output is in the middle of a sequence too long to keep.
    pub fn save(&self) -> Option<SavedScreen> {
        let unended = self.unended.bytes()?;
        let mut bytes = self.write_paint(usize::MAX, true).into_bytes();
        let left_out = self
            .left_out()
            .or_else(|| self.write_repeat(&mut bytes).err());
        if let Some(what) = left_out {
            tracing::info!(
                what,
                "the saved screen leaves terminal state out; it is restored as inexact"
            );
        }
        self.write_answering(&mut bytes);
        // What the parsers took in of the unended sequence, then what the query scanner held
        // back after it: fed in that order, the restored scanner passes the first on as the
        // original did, and holds the rest back again. Only after a kept status report that
        // interrupted the sequence does it differ: the report is gone, so the sequence it
        // interrupted is read as unbroken.
        bytes.extend_from_slice(unended);
        bytes.extend_from_slice(self.held_back());
        Some(SavedScreen {
            rows: self.terminal.screen_lines() as u16,
            cols: self.terminal.columns() as u16,
            bytes,
            inexact: left_out.is_some(),
            drawn: self.drawn,
        })
    }

    /// What of this terminal's state the saved bytes do not rebuild, and would change what later
    /// output does or shows, if anything: the normal screen behind the alternate one, unless it is
    /// known to be blank (`blank_normal`); a character set other than ASCII designated or in use;
    /// tab stops changed; colours of the palette changed; a title stack the paint does not push
    /// again; a cursor outside the scroll region in origin mode, which the paint places on the
    /// region's edge; on the normal screen, a cursor the alternate screen keeps saved, which
    /// `alacritty_terminal` does not let be read.
    fn left_out(&self) -> Option<&'static str> {
        let mode = *self.terminal.mode();
        let grid = self.terminal.grid();
        if mode.contains(TermMode::ALT_SCREEN) && !self.blank_normal {
            return Some("the normal screen behind the alternate one");
        }
        if !mode.contains(TermMode::ALT_SCREEN) && self.unexposed.alternate_saved_cursor {
            return Some("the cursor the alternate screen keeps saved");
        }
        if self.unexposed.active_charset != CharsetIndex::G0
            || grid.cursor.charsets != Charsets::default()
            || grid.saved_cursor.charsets != Charsets::default()
        {
            return Some("the character sets");
        }
        if self.unexposed.tabs_changed {
            return Some("the tab stops");
        }
        let colors = self.terminal.colors();
        if (0..COUNT).any(|index| colors[index].is_some()) {
            return Some("the colour palette changes");
        }
        // Only entries pushed before any title was set have none, and with no title now none of
        // them has one (see `write_title_stack`).
        let titles = &self.unexposed.titles;
        if titles
            .iter()
            .skip_while(|title| title.is_none())
            .any(Option::is_none)
            || (self.unexposed.title.is_none() && titles.iter().any(Option::is_some))
        {
            return Some("the title stack");
        }
        if mode.contains(TermMode::ORIGIN) {
            let (top, bottom) = self.unexposed.region.unwrap_or((0, grid.screen_lines()));
            let within = |cursor: &Cursor<Cell>| {
                usize::try_from(cursor.point.line.0).is_ok_and(|line| (top..bottom).contains(&line))
            };
            if !within(&grid.cursor) || !within(&grid.saved_cursor) {
                return Some("a cursor outside the scroll region in origin mode");
            }
        }
        None
    }

    /// Makes the character a repeat (REP, `CSI b`) draws again in the restored terminal this
    /// one's, by appending to `bytes`, which take a fresh terminal to this one's screen, what
    /// draws that character and takes the screen back. The parser keeps the character last drawn
    /// for good, so nothing but drawing it sets it, and drawing the paint's cells has left the
    /// last of those. Drawn on three blank cells off the margins, with the default pen and insert
    /// mode off, whatever its width; the three are erased, which with the default pen leaves them
    /// blank again, and the cursor is placed and given its pen once more. On a normal screen as a
    /// reset leaves it, with no cursor saved, it is drawn on the alternate screen instead, which
    /// is cleared as it is entered again, so that the normal one stays known to be blank. What is
    /// left out instead where that cannot be done.
    fn write_repeat(&self, bytes: &mut Vec<u8>) -> Result<(), &'static str> {
        const LEFT_OUT: &str = "the character a repeat draws again";
        if last_drawn(bytes) == self.unexposed.preceding {
            return Ok(());
        }
        // A restored parser that drew something has a character to repeat, and one that drew
        // nothing does not get one by anything else.
        let Some(c) = self.unexposed.preceding else {
            return Err(LEFT_OUT);
        };
        let grid = self.terminal.grid();
        // Placing a cursor waiting to wrap draws its cell again, after anything drawn here.
        if grid.cursor.input_needs_wrap {
            return Err(LEFT_OUT);
        }
        // Drawing it would count as the restored screen's first drawing where this one has shown
        // nothing since titles were reported.
        if !self.drawn && self.events.titles().is_some() && !matches!(c, ' ' | '\t') {
            return Err(LEFT_OUT);
        }
        let mode = *self.terminal.mode();
        if !mode.contains(TermMode::ALT_SCREEN)
            && self.normal_is_blank()
            && grid.saved_cursor == Cursor::default()
        {
            // Entering the alternate screen saves the normal screen's cursor, here as it was.
            bytes.extend_from_slice(format!("\x1b[?1049h{c}\x1b[?1049l").as_bytes());
            return Ok(());
        }
        let (top, bottom, origin) = match (mode.contains(TermMode::ORIGIN), self.unexposed.region) {
            (true, Some((start, end))) => (start, end, start),
            _ => (0, grid.screen_lines(), 0),
        };
        let blank = Cell::default();
        let columns = grid.columns();
        let spot = (top..bottom).find_map(|row| {
            let cells = &grid[Line(row as i32)];
            (1..columns.saturating_sub(2))
                .find(|&col| (col - 1..=col + 1).all(|col| cells[Column(col)] == blank))
                .map(|col| (row - origin + 1, col + 1))
        });
        let Some((row, col)) = spot else {
            return Err(LEFT_OUT);
        };
        let mut output = String::new();
        let insert = mode.contains(TermMode::INSERT);
        if insert {
            output.push_str("\x1b[4l");
        }
        write_pen(&mut output, &Cursor::default());
        write!(output, "\x1b[{row};{col}H{c}\x1b[{row};{}H\x1b[3X", col - 1)
            .expect("writing a String");
        place(&mut output, grid, &grid.cursor, origin);
        if insert {
            output.push_str("\x1b[4h");
        }
        bytes.extend_from_slice(output.as_bytes());
        Ok(())
    }
}

/// The character a terminal that takes `bytes` in from a fresh parser draws last, which a repeat
/// would draw again.
fn last_drawn(bytes: &[u8]) -> Option<char> {
    struct LastDrawn(Option<char>);
    impl Handler for LastDrawn {
        fn input(&mut self, c: char) {
            self.0 = Some(c);
        }
    }
    let mut drawn = LastDrawn(None);
    Processor::<ImmediateUpdates>::new().advance(&mut drawn, bytes);
    drawn.0
}

#[cfg(test)]
mod tests;
