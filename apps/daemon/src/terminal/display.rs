//! The display facts the daemon answers the program's colour and size queries with: what a
//! client's terminal shows, which the client reports (`display` frames on the terminal stream).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::vte::ansi::Rgb;

use crate::protocol::TerminalAppearance;
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

/// Used when there is nothing else to go on, with no client report anywhere and no launch
/// appearance: the web client's dark theme, and a cell of a 14 px monospace font.
impl Default for DisplayFacts {
    fn default() -> Self {
        Self::for_appearance(TerminalAppearance::Dark)
    }
}

impl DisplayFacts {
    /// The colours of the web client's terminal theme for `appearance` (`XTERM_THEMES` in
    /// `packages/ui/src/terminal/xtermThemes.ts`), and a cell of a 14 px monospace font.
    pub fn for_appearance(appearance: TerminalAppearance) -> Self {
        let rgb = |r, g, b| Rgb { r, g, b };
        let (foreground, background) = match appearance {
            TerminalAppearance::Dark => (rgb(0xe4, 0xe4, 0xe6), rgb(0x1e, 0x1f, 0x22)),
            TerminalAppearance::Light => (rgb(0x38, 0x3a, 0x42), rgb(0xfa, 0xfa, 0xfa)),
        };
        Self {
            foreground,
            background,
            cursor: rgb(0x80, 0x80, 0x80),
            cell: (8, 17),
        }
    }

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

    /// These colours with the default cell: a cell size is a fact of one client's display, which
    /// no other terminal has any reason to take.
    fn colors_only(self) -> Self {
        Self {
            cell: Self::default().cell,
            ..self
        }
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

/// The colours last reported by any client of any terminal, kept by the daemon for all of them. A
/// terminal uses them only when its launch appearance is not newer.
#[derive(Debug, Default)]
pub struct DisplayFallback {
    /// The colours of the display most recently reported by any client of any terminal, and when.
    last_report: Mutex<Option<(Instant, DisplayFacts)>>,
}

impl DisplayFallback {
    fn record(&self, display: DisplayFacts) {
        *self
            .last_report
            .lock()
            .expect("display fallback lock poisoned") =
            Some((Instant::now(), display.colors_only()));
    }

    fn last_report(&self) -> Option<(Instant, DisplayFacts)> {
        *self
            .last_report
            .lock()
            .expect("display fallback lock poisoned")
    }
}

/// Where one terminal takes its display from while none of its clients has reported one: the
/// appearance its launch was made for or the daemon's last report from any terminal, whichever is
/// more recent, else the default.
#[derive(Debug, Default)]
pub struct DisplaySeed {
    shared: Arc<DisplayFallback>,
    launch: Option<(Instant, DisplayFacts)>,
}

impl DisplaySeed {
    /// For a terminal started now for `appearance`, if the launch names one.
    pub fn new(shared: Arc<DisplayFallback>, appearance: Option<TerminalAppearance>) -> Self {
        Self {
            shared,
            launch: appearance
                .map(|appearance| (Instant::now(), DisplayFacts::for_appearance(appearance))),
        }
    }

    /// Notes a display a client of this terminal reported, for every terminal's fallback.
    pub fn record(&self, display: DisplayFacts) {
        self.shared.record(display);
    }

    /// The display to answer with while no client of the terminal has reported one.
    pub fn fallback(&self) -> DisplayFacts {
        match (self.launch, self.shared.last_report()) {
            (Some((launched, launch)), Some((reported, report))) => {
                if reported > launched {
                    report
                } else {
                    launch
                }
            }
            (Some((_, display)), None) | (None, Some((_, display))) => display,
            (None, None) => DisplayFacts::default(),
        }
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
    use std::sync::Arc;

    use alacritty_terminal::vte::ansi::Rgb;

    use super::{DisplayFacts, DisplayFallback, DisplaySeed};
    use crate::protocol::TerminalAppearance;
    use crate::test_support::next_millisecond;

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    /// A display a client would report, with a cell of its own.
    fn reported(foreground: &str, background: &str) -> DisplayFacts {
        DisplayFacts::reported(foreground, background, "#101010", Some((9, 20))).expect("a display")
    }

    fn reported_light() -> DisplayFacts {
        reported("#383a42", "#fafafa")
    }

    fn reported_dark() -> DisplayFacts {
        reported("#e4e4e6", "#1e1f22")
    }

    #[test]
    fn each_appearance_has_the_colours_of_the_web_clients_theme() {
        let light = DisplayFacts::for_appearance(TerminalAppearance::Light);
        assert_eq!(light.foreground, rgb(0x38, 0x3a, 0x42));
        assert_eq!(light.background, rgb(0xfa, 0xfa, 0xfa));
        assert_eq!(light.cursor, rgb(0x80, 0x80, 0x80));
        assert!(!light.dark());
        let dark = DisplayFacts::for_appearance(TerminalAppearance::Dark);
        assert_eq!(dark.foreground, rgb(0xe4, 0xe4, 0xe6));
        assert_eq!(dark.background, rgb(0x1e, 0x1f, 0x22));
        assert_eq!(dark.cursor, rgb(0x80, 0x80, 0x80));
        assert!(dark.dark());
        assert!(light.colors_differ(&dark));
        assert_eq!(light.cell, dark.cell, "the same default cell");
        assert_eq!(light.cell, DisplayFacts::default().cell);
    }

    #[test]
    fn the_default_display_is_the_dark_theme() {
        assert_eq!(
            DisplayFacts::default(),
            DisplayFacts::for_appearance(TerminalAppearance::Dark)
        );
    }

    #[test]
    fn a_seed_with_nothing_to_go_on_gives_the_dark_theme() {
        let seed = DisplaySeed::new(Arc::default(), None);
        assert_eq!(seed.fallback(), DisplayFacts::default());
        assert_eq!(DisplaySeed::default().fallback(), DisplayFacts::default());
    }

    #[test]
    fn a_seed_launched_for_an_appearance_gives_that_theme_before_any_report() {
        for appearance in [TerminalAppearance::Light, TerminalAppearance::Dark] {
            let seed = DisplaySeed::new(Arc::default(), Some(appearance));
            assert_eq!(seed.fallback(), DisplayFacts::for_appearance(appearance));
        }
    }

    #[tokio::test]
    async fn a_seed_with_no_appearance_gives_the_last_report_of_any_terminal() {
        let shared = Arc::new(DisplayFallback::default());
        let reporting = DisplaySeed::new(shared.clone(), None);
        let other = DisplaySeed::new(shared.clone(), None);
        reporting.record(reported_light());
        assert!(!other.fallback().dark());
        assert!(
            !reporting.fallback().dark(),
            "the terminal that reported shares the daemon's last report"
        );

        next_millisecond().await;
        reporting.record(reported_dark());
        assert!(other.fallback().dark(), "the most recent report");
        let unrelated = DisplaySeed::new(Arc::default(), None);
        assert_eq!(
            unrelated.fallback(),
            DisplayFacts::default(),
            "another daemon-wide fallback is not shared"
        );
    }

    #[tokio::test]
    async fn only_the_colours_of_a_report_are_kept_never_its_cell() {
        let shared = Arc::new(DisplayFallback::default());
        let seed = DisplaySeed::new(shared.clone(), None);
        let display = reported_light();
        assert_ne!(display.cell, DisplayFacts::default().cell);
        seed.record(display);
        let fallback = seed.fallback();
        assert_eq!(fallback.foreground, display.foreground);
        assert_eq!(fallback.background, display.background);
        assert_eq!(fallback.cursor, display.cursor);
        assert_eq!(fallback.cell, DisplayFacts::default().cell);

        next_millisecond().await;
        let launched = DisplaySeed::new(shared, Some(TerminalAppearance::Dark));
        assert_eq!(
            launched.fallback(),
            DisplayFacts::for_appearance(TerminalAppearance::Dark)
        );
    }

    #[tokio::test]
    async fn a_launch_newer_than_the_last_report_wins() {
        let shared = Arc::new(DisplayFallback::default());
        DisplaySeed::new(shared.clone(), None).record(reported_dark());
        next_millisecond().await;
        let launched = DisplaySeed::new(shared, Some(TerminalAppearance::Light));
        assert_eq!(
            launched.fallback(),
            DisplayFacts::for_appearance(TerminalAppearance::Light)
        );
    }

    #[tokio::test]
    async fn a_report_newer_than_the_launch_wins() {
        let shared = Arc::new(DisplayFallback::default());
        let launched = DisplaySeed::new(shared.clone(), Some(TerminalAppearance::Light));
        next_millisecond().await;
        DisplaySeed::new(shared, None).record(reported_dark());
        let fallback = launched.fallback();
        assert!(fallback.dark());
        assert_eq!(fallback.background, rgb(0x1e, 0x1f, 0x22));
        assert_eq!(fallback.cell, DisplayFacts::default().cell);
    }

    #[tokio::test]
    async fn the_launch_and_the_last_report_are_weighed_again_at_each_look() {
        let shared = Arc::new(DisplayFallback::default());
        let launched = DisplaySeed::new(shared.clone(), Some(TerminalAppearance::Light));
        let reporting = DisplaySeed::new(shared, None);
        assert!(!launched.fallback().dark(), "no report yet");
        next_millisecond().await;
        reporting.record(reported_dark());
        assert!(launched.fallback().dark());
        next_millisecond().await;
        reporting.record(reported_light());
        assert!(!launched.fallback().dark(), "the later report decides");
    }

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
