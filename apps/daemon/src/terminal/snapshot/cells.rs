//! Cells and rows of a parsed terminal written back as the escape sequences that draw them.

use std::fmt::Write;

use alacritty_terminal::grid::{Cursor, Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// What a cell is drawn with, as SGR sets it.
#[derive(PartialEq, Eq)]
pub(super) struct Attributes {
    fg: Color,
    bg: Color,
    underline: Option<Color>,
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
            underline: cell.underline_color(),
            flags: cell.flags
                & !(Flags::WRAPLINE
                    | Flags::WIDE_CHAR
                    | Flags::WIDE_CHAR_SPACER
                    | Flags::LEADING_WIDE_CHAR_SPACER),
        }
    }
}

/// One row of `grid` as text with SGR styles, without its trailing blanks unless it soft-wraps,
/// and whether it does; with `links`, with its OSC 8 hyperlinks.
pub(super) fn render_row(grid: &Grid<Cell>, row: Line, links: bool) -> (String, bool) {
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
                    || (links && cell.hyperlink().is_some())
            })
            .map_or(0, |col| col + 1)
    };
    let mut output = String::new();
    let mut previous = Attributes::default();
    let mut link: Option<Hyperlink> = None;
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
            output.push_str(&sgr(&attrs));
            previous = attrs;
        }
        if links {
            let cell_link = cell.hyperlink();
            if cell_link != link {
                write_link(&mut output, cell_link.as_ref());
                link = cell_link;
            }
        }
        push_cell(&mut output, cell);
    }
    if link.is_some() {
        write_link(&mut output, None);
    }
    if previous != Attributes::default() {
        output.push_str("\x1b[0m");
    }
    (output, wrapped)
}

/// A cell's character, with any combining characters on it.
pub(super) fn push_cell(output: &mut String, cell: &Cell) {
    // Alacritty marks the first cell crossed by a tab with a literal tab. Its remaining
    // columns already exist in the grid, so replay it as one space rather than moving twice.
    output.push(if cell.c == '\t' { ' ' } else { cell.c });
    if let Some(chars) = cell.zerowidth() {
        output.extend(chars);
    }
}

/// Opens the OSC 8 hyperlink `link`, or closes the open one.
pub(super) fn write_link(output: &mut String, link: Option<&Hyperlink>) {
    match link {
        Some(link) => write!(output, "\x1b]8;id={};{}\x1b\\", link.id(), link.uri())
            .expect("writing a String"),
        None => output.push_str("\x1b]8;;\x1b\\"),
    }
}

/// The SGR sequence that sets exactly `attrs`, from a reset.
pub(super) fn sgr(attrs: &Attributes) -> String {
    let mut output = String::from("\x1b[0");
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
    if let Some(color) = attrs.underline {
        write_color(&mut output, color, 58);
    }
    output.push('m');
    output
}

/// The pen `cursor` draws with: the SGR attributes of its template cell, and its hyperlink or
/// none.
pub(super) fn write_pen(output: &mut String, cursor: &Cursor<Cell>) {
    output.push_str(&sgr(&Attributes::from(&cursor.template)));
    write_link(output, cursor.template.hyperlink().as_ref());
}

/// The SGR parameters that set `color` for `code`: 38 the foreground, 48 the background, 58 the
/// underline, which has no named colours of its own.
fn write_color(output: &mut String, color: Color, code: u8) {
    match color {
        Color::Named(NamedColor::Foreground | NamedColor::Background) => {}
        // SGR sets the underline colour only from the palette or as RGB.
        Color::Named(_) if code == 58 => {}
        Color::Named(named) => {
            // The sixteen named colours are written as SGR names them (30–37 and 90–97 for the
            // foreground, 40–47 and 100–107 for the background), which a terminal parses back as
            // named rather than as palette entries. The other named colours (the cursor's, the
            // dim ones) are the renderer's own and never set on a cell.
            let base = u16::from(code - 8);
            match named as u16 {
                index @ 0..=7 => write!(output, ";{}", base + index).expect("writing a String"),
                index @ 8..=15 => {
                    write!(output, ";{}", base + 60 + index - 8).expect("writing a String")
                }
                _ => {}
            }
        }
        Color::Indexed(index) => write!(output, ";{code};5;{index}").expect("writing a String"),
        Color::Spec(rgb) => {
            write!(output, ";{code};2;{};{};{}", rgb.r, rgb.g, rgb.b).expect("writing a String")
        }
    }
}
