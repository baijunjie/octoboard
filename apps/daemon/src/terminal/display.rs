//! The display facts the daemon answers the program's colour and size queries with: what a
//! client's terminal shows, which the client reports (`display` frames on the terminal stream).

use alacritty_terminal::vte::ansi::Rgb;

use crate::terminal::snapshot::{MAX_COLS, MAX_ROWS};

/// The largest cell taken from a client: the text area of the largest terminal in pixels, which a
/// size answer writes, has to fit in 16 bits.
const MOST_CELL: (u16, u16) = (u16::MAX / MAX_COLS, u16::MAX / MAX_ROWS);

/// The colours and cell size of a terminal's display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayFacts {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    /// A cell's width and height in pixels.
    pub cell: (u16, u16),
}

/// Until a client has reported its display: the web client's dark theme, and a cell of a 14 px
/// monospace font.
impl Default for DisplayFacts {
    fn default() -> Self {
        Self {
            foreground: Rgb {
                r: 0xe4,
                g: 0xe4,
                b: 0xe6,
            },
            background: Rgb {
                r: 0x1e,
                g: 0x1f,
                b: 0x22,
            },
            cursor: Rgb {
                r: 0x80,
                g: 0x80,
                b: 0x80,
            },
            cell: (8, 17),
        }
    }
}

impl DisplayFacts {
    /// The display a client reports: colours as `#rrggbb`, and its cell size where it knows it,
    /// the default's otherwise, at most [`MOST_CELL`]. `None` when a colour is not one.
    pub fn reported(
        foreground: &str,
        background: &str,
        cursor: &str,
        cell: Option<(u16, u16)>,
    ) -> Option<Self> {
        Some(Self {
            foreground: parse_color(foreground)?,
            background: parse_color(background)?,
            cursor: parse_color(cursor)?,
            cell: cell
                .filter(|&(width, height)| width > 0 && height > 0)
                .map(|(width, height)| (width.min(MOST_CELL.0), height.min(MOST_CELL.1)))
                .unwrap_or(Self::default().cell),
        })
    }

    /// Whether the colour scheme is dark: the background darker than the foreground.
    pub fn dark(&self) -> bool {
        luminance(self.background) < luminance(self.foreground)
    }

    /// Whether `other` shows different colours, which is what a program that subscribed to
    /// colour scheme updates is told of.
    pub fn colors_differ(&self, other: &Self) -> bool {
        (self.foreground, self.background, self.cursor)
            != (other.foreground, other.background, other.cursor)
    }
}

fn parse_color(color: &str) -> Option<Rgb> {
    let hex = color.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some(Rgb {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
    })
}

/// Relative luminance in the sense of WCAG, enough to tell the lighter of two colours.
fn luminance(color: Rgb) -> f64 {
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

#[cfg(test)]
mod tests {
    use super::DisplayFacts;

    #[test]
    fn a_reported_display_is_read_and_its_scheme_told_dark_or_light() {
        let dark = DisplayFacts::reported("#e4e4e6", "#1e1f22", "#808080", Some((9, 18))).unwrap();
        assert!(dark.dark());
        assert_eq!(dark.cell, (9, 18));
        let light = DisplayFacts::reported("#383a42", "#FAFAFA", "#808080", None).unwrap();
        assert!(!light.dark());
        assert_eq!(light.cell, DisplayFacts::default().cell, "the default cell");
        assert!(light.colors_differ(&dark));
        assert!(DisplayFacts::default().dark());
    }

    #[test]
    fn a_colour_that_is_not_one_is_refused() {
        for color in ["e4e4e6", "#e4e4e", "#e4e4e6f", "#gggggg", "rgb:e4/e4/e6"] {
            assert!(
                DisplayFacts::reported(color, "#000000", "#000000", None).is_none(),
                "{color}"
            );
        }
        assert_eq!(
            DisplayFacts::reported("#000000", "#ffffff", "#000000", Some((0, 18)))
                .unwrap()
                .cell,
            DisplayFacts::default().cell,
            "an empty cell is not a size"
        );
        assert_eq!(
            DisplayFacts::reported("#000000", "#ffffff", "#000000", Some((u16::MAX, u16::MAX)))
                .unwrap()
                .cell,
            (65, 131),
            "a cell the largest terminal's text area in pixels would not fit 16 bits with"
        );
    }
}
