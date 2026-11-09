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

fn painted(original: &mut TerminalSnapshot, history: usize) -> TerminalSnapshot {
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
        let mut replayed = replay(capture, rows, cols);
        assert_same_state(&painted(&mut replayed, SCROLLBACK_LINES), &replayed);
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
    let painted = painted(&mut original, 100);
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
    assert_same_state(&painted(&mut original, SCROLLBACK_LINES), &original);

    let paint = painted(&mut original, 5);
    assert_eq!(paint.terminal.grid().history_size(), 5);
    assert_eq!(paint.visible_text(), original.visible_text());
    assert_eq!(painted(&mut original, 0).terminal.grid().history_size(), 0);
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
    let painted = painted(&mut original, 0);
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
    let painted = painted(&mut original, 0);
    assert_eq!(painted.unexposed.region, None);
    assert_same_state(&painted, &original);
}

/// What a program does after a client was painted while it was on the alternate screen: leaves it
/// (no output, so that the normal screen's scrollback is still what the paint carried), leaves it
/// and writes on the normal screen, leaves it and restores the cursor, or draws and goes back and
/// forth.
const AFTER_THE_ALTERNATE_SCREEN: [&[u8]; 4] = [
    b"\x1b[?1049l",
    b"\x1b[?1049lnormal again",
    b"\x1b[?1049l\x1b8restored",
    b"app\x1b[?1049l\x1b[2bx\x1b[?1049h\x1b[?1049l\x1b8y",
];

/// The history a client is painted with, besides none at all: all there is, and fewer rows than
/// the normal screen's scrollback holds in most cases below.
const HISTORIES: [usize; 3] = [SCROLLBACK_LINES, 3, 0];

/// Asserts that `client` is in the state `original` is in, naming `context` and the first line
/// that differs; the scrollback, the history size line and the rows in it are left out where
/// `scrollback` is false.
fn assert_same_state_in(
    client: &TerminalSnapshot,
    original: &TerminalSnapshot,
    scrollback: bool,
    context: &str,
) {
    let keep = |line: &String| scrollback || !(line.starts_with("history") || is_scrollback(line));
    let client: Vec<String> = client.state().into_iter().filter(keep).collect();
    let original: Vec<String> = original.state().into_iter().filter(keep).collect();
    for (at, (got, want)) in client.iter().zip(&original).enumerate() {
        assert_eq!(got, want, "{context}: line {at} of the state differs");
    }
    assert_eq!(
        client.len(),
        original.len(),
        "{context}: the states differ in length"
    );
}

fn is_scrollback(line: &str) -> bool {
    line.trim_start().starts_with('-')
}

/// Asserts that a client painted from the terminal `build` makes, with `history` rows, is in the
/// state the original is in on the alternate screen; that it has as many rows of the normal
/// screen's scrollback as the original's, up to `history`, once the program leaves the alternate
/// screen; and that both go to the same state, the normal screen's rows, cursor, pen and saved
/// cursor included, on what the program does next.
fn assert_client_goes_on_as_original(
    build: impl Fn() -> TerminalSnapshot,
    history: usize,
    context: &str,
) {
    for after in AFTER_THE_ALTERNATE_SCREEN {
        let context = format!("{context}, history {history}, then {:?}", after);
        let mut original = build();
        let mut client = painted(&mut original, history);
        assert_same_state_in(&client, &original, true, &context);

        // The scrollback of the normal screen is in play once the program leaves the alternate
        // one; the output that follows may scroll both further.
        let mut left = build();
        left.process(b"\x1b[?1049l");
        let available = left.terminal.grid().history_size();
        let mut left_client = painted(&mut build(), history);
        left_client.process(b"\x1b[?1049l");
        assert_eq!(
            left_client.terminal.grid().history_size(),
            history.min(available),
            "{context}: the client's scrollback rows"
        );
        assert_same_state_in(&left_client, &left, history >= available, &context);

        original.process(after);
        client.process(after);
        assert_same_state_in(&client, &original, history >= available, &context);
    }
}

fn assert_client_matches_for_every_history(build: impl Fn() -> TerminalSnapshot, context: &str) {
    for history in HISTORIES {
        assert_client_goes_on_as_original(&build, history, context);
    }
}

fn assert_client_matches_for_every_history_replaying(before: &[u8]) {
    assert_client_matches_for_every_history(
        || replay(before, 6, 20),
        &format!("{:?}", String::from_utf8_lossy(before)),
    );
}

/// A client painted while the program is on the alternate screen over a drawn normal screen shows
/// the same screen and, once the program leaves it, the same normal screen: its scrollback past
/// the screen's height, a moved cursor, a coloured or hyperlinked pen, a cursor waiting to wrap,
/// wide characters and a saved cursor.
#[test]
fn a_client_painted_on_the_alternate_screen_gets_the_drawn_normal_screen_behind_it() {
    for before in [
        &b"history\r\nmore history\r\n\x1b[?1049h\x1b[Happ"[..],
        b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\nten\r\n$ \x1b[?1049h\x1b[2;3Happ",
        b"text\x1b[3;4H\x1b[?1049happ",
        b"text\x1b7\x1b[3;4Hmore\x1b[?1049happ",
        b"\x1b[1;31mred bold\x1b[?1049happ",
        b"\x1b[3;4;38;2;1;2;3;48;5;9mtruecolour pen\x1b[?1049happ",
        b"\x1b]8;;https://example.com\x07link\x1b[?1049happ",
        b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07 plain\x1b[?1049happ",
        b"abcdefghijklmnopqrst\x1b[?1049happ",
        b"abcdefghijklmnopqrstu\x1b[?1049happ",
        b"abcdefghijklmnopqrst\x1b[1;32m\x1b[?1049happ",
        "wide \u{754c}\u{754c}\u{754c}\r\n\u{754c}e\u{301}\x1b[?1049happ".as_bytes(),
        "0123456789012345678\u{754c}\x1b[?1049happ".as_bytes(),
    ] {
        assert_client_matches_for_every_history_replaying(before);
    }
}

/// Entering the alternate screen clears it with the pen of the normal screen's cursor, so a
/// normal screen whose pen has a background colour is hidden behind an alternate screen of that
/// colour until the program clears it; the client's alternate screen is the same either way.
#[test]
fn a_background_pen_on_the_normal_screen_is_painted_with_and_without_the_program_clearing() {
    for before in [
        &b"\x1b[41mred\x1b[?1049happ"[..],
        b"\x1b[1;31;44mred on blue\r\nmore\x1b[?1049h\x1b[2;3Happ",
        b"\x1b[41mred\x1b[?1049h\x1b[0m\x1b[2Japp",
        b"\x1b[1;31;44mred on blue\r\nmore\x1b[?1049h\x1b[0m\x1b[2J\x1b[2;3Happ",
        b"\x1b[1;31;44mred on blue\r\nmore\x1b[?1049h\x1b[2Japp",
    ] {
        assert_client_matches_for_every_history_replaying(before);
    }
}

/// The program's pen, saved cursor and scroll region on the alternate screen come through next to
/// the normal screen's, which are not the same.
#[test]
fn the_alternate_screens_own_cursor_pen_and_saved_cursor_are_kept_next_to_the_normal_ones() {
    for before in [
        &b"one\r\ntwo\r\n\x1b[32mnormal\x1b[?1049h\x1b[2;5r\x1b[?6h\x1b[3;3H\x1b[1;35mSAVED\x1b7\x1b[0m\x1b[1;1Happ"[..],
        b"one\r\ntwo\r\nnormal\x1b[?1049h\x1b[4;4H\x1b7\x1b[5;5H\x1b]8;;https://example.com\x07app",
    ] {
        assert_client_matches_for_every_history_replaying(before);
    }
}

/// A normal screen that was never drawn on, from the start or since a reset, is painted as before.
#[test]
fn a_blank_normal_screen_behind_the_alternate_one_is_painted_as_before() {
    for before in [
        &b"\x1b[?1049h\x1b[2;3Happ"[..],
        b"drawn\r\n\x1bc\x1b[?1049h\x1b[2;3Happ",
        b"\x1b]2;a title\x07\x1b[?25l\x1b[?1049happ",
    ] {
        assert_client_matches_for_every_history_replaying(before);
    }
}

/// Painting leaves the terminal as it was: it is the state a fresh replay of the same bytes is in,
/// on the alternate screen and after the program leaves it, and goes on the same way, also with
/// a cursor the program saved on the alternate screen.
#[test]
fn painting_behind_the_alternate_screen_leaves_the_terminal_unchanged() {
    for before in [
        &b"history\r\nmore history\r\n\x1b[?1049h\x1b[Happ"[..],
        b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\n\x1b[31mtext\x1b[3;4H\x1b[?1049happ",
        b"abcdefghijklmnopqrst\x1b[?1049happ",
        b"\x1b]8;;https://example.com\x07link\x1b[?1049h\x1b[2;3Happ",
        b"\x1b[2;5r\x1b[?6h\x1b[2;3Htext\x1b[?1049happ",
        b"text\x1b[3;4H\x1b[?1049h\x1b[2;2H\x1b7\x1b[5;5Happ",
        b"text\x1b[3;4H\x1b[?1049h\x1b[2;2H\x1b[1;31m\x1b7\x1b[0m\x1b[5;5Happ",
        b"\x1b[?1049h\x1b[2;3Happ",
    ] {
        for history in HISTORIES {
            let mut original = replay(before, 6, 20);
            let _ = original.paint(history);
            let mut fresh = replay(before, 6, 20);
            let context = format!("{:?}, history {history}", String::from_utf8_lossy(before));
            assert_same_state_in(&original, &fresh, true, &context);
            for after in [
                &b" more on the alternate screen"[..],
                b"\x1b8\x1b[?1049l\x1b8restored",
                b"\x1b[?1049lnormal again",
                b"\x1b[?1049l\x1b8restored",
                b"\x1b8\x1b[?1049l\x1b8restored again\x1b[2b",
            ] {
                original.process(after);
                fresh.process(after);
                assert_same_state_in(&original, &fresh, true, &context);
            }
        }
    }
}

/// A character set other than the default on the cursor of the normal screen behind the
/// alternate one is not painted (a paint never restores character sets), but the normal screen
/// still is: once the program leaves the alternate screen, the client has the original's rows,
/// cursor and scrollback, and painting leaves the terminal as it was.
#[test]
fn a_client_painted_over_a_normal_screen_with_a_character_set_still_gets_the_normal_screen() {
    for before in [
        &b"drawn\r\n\x1b(0q\x1b[?1049h\x1b(Bapp"[..],
        b"drawn\r\n\x1b(0lqk\x1b[3;4H\x1b[?1049h\x1b(Bapp",
        b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\n\x1b(0lqk\x1b[?1049h\x1b(Bapp",
    ] {
        for history in HISTORIES {
            let context = format!("{:?}, history {history}", String::from_utf8_lossy(before));
            let mut left = replay(before, 6, 20);
            left.process(b"\x1b[?1049l");
            let available = left.terminal.grid().history_size();

            let mut original = replay(before, 6, 20);
            let mut client = painted(&mut original, history);
            client.process(b"\x1b[?1049l");
            assert_eq!(
                client.terminal.grid().history_size(),
                history.min(available),
                "{context}: the client's scrollback rows"
            );
            assert_same_state_in(&client, &left, history >= available, &context);

            // Painting left the live terminal as a fresh replay of the same bytes is.
            let mut fresh = replay(before, 6, 20);
            assert_same_state_in(&original, &fresh, true, &context);
            original.process(b"\x1b[?1049l");
            fresh.process(b"\x1b[?1049l");
            assert_same_state_in(&original, &fresh, true, &context);
        }
    }
}

/// Painting twice gives the same bytes, since the first leaves nothing behind.
#[test]
fn painting_twice_gives_the_same_bytes() {
    let mut original = replay(
        b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\n\x1b[31mtext\x1b[3;4H\x1b[?1049happ",
        6,
        20,
    );
    let first = original.paint(SCROLLBACK_LINES);
    assert_eq!(original.paint(SCROLLBACK_LINES), first);
}

/// A change of width while the alternate screen is shown may have reflowed the normal screen
/// behind it, which a paint cannot rebuild: it is left out, so the client shows a blank normal
/// screen once the program leaves the alternate one, and painting still leaves the terminal as it
/// was. A change of height alone does not, and neither does a change of width before the
/// alternate screen was entered.
#[test]
fn a_width_change_behind_the_alternate_screen_leaves_the_normal_screen_out_of_the_paint() {
    let before = b"one\r\ntwo\r\nthree\r\n$ prompt text\x1b[?1049happ";
    for (rows, cols) in [(6, 30), (6, 12), (8, 12)] {
        for history in HISTORIES {
            let mut original = replay(before, 6, 20);
            original.resize(rows, cols);
            let mut client = painted(&mut original, history);
            let context = format!("{rows}x{cols}, history {history}");
            assert_same_state_in(&client, &original, true, &context);
            assert_eq!(client.terminal.grid().history_size(), 0);

            let mut fresh = replay(before, 6, 20);
            fresh.resize(rows, cols);
            assert_same_state_in(&original, &fresh, true, &context);

            client.process(b"\x1b[?1049l");
            original.process(b"\x1b[?1049l");
            fresh.process(b"\x1b[?1049l");
            assert_same_state_in(&original, &fresh, true, &context);
            assert_eq!(
                client.visible_text().trim(),
                "",
                "{context}: the normal screen is blank"
            );
            assert_eq!(client.terminal.grid().history_size(), 0, "{context}");
            assert!(
                !original.visible_text().trim().is_empty(),
                "{context}: the original's own normal screen is not"
            );
        }
    }
}

/// A change of height alone keeps the normal screen in the paint, and so does a change of width
/// while the normal screen is shown.
#[test]
fn a_height_change_or_a_width_change_before_entering_keeps_the_normal_screen_in_the_paint() {
    let before = b"one\r\ntwo\r\nthree\r\n$ prompt text\x1b[?1049happ";
    assert_client_matches_for_every_history(
        || {
            let mut terminal = replay(before, 6, 20);
            terminal.resize(9, 20);
            terminal
        },
        "taller",
    );
    assert_client_matches_for_every_history(
        || {
            let mut terminal = replay(before, 6, 20);
            terminal.resize(4, 20);
            terminal
        },
        "shorter",
    );
    assert_client_matches_for_every_history(
        || {
            let mut terminal = replay(b"one\r\ntwo\r\n$ text", 6, 20);
            terminal.resize(6, 30);
            terminal.process(b"\x1b[?1049happ");
            terminal
        },
        "resized before entering",
    );
}

/// Once the program enters the alternate screen again after the width changed, in a later chunk
/// than the resize or in the one that leaves it, the paint carries the normal screen again.
#[test]
fn entering_the_alternate_screen_again_after_a_width_change_paints_the_normal_screen_again() {
    let before = b"one\r\ntwo\r\n$ prompt text\x1b[?1049happ";
    for (what, chunks) in [
        (
            "left and entered in later chunks",
            &[&b"\x1b[?1049l"[..], b"more\x1b[?1049happ"][..],
        ),
        (
            "left and entered in one chunk",
            &[&b"\x1b[?1049lnormal\x1b[?1049happ"[..]][..],
        ),
        (
            "reset and entered in one chunk after drawing",
            &[&b"\x1bcnormal text\r\nmore\x1b[?1049happ"[..]][..],
        ),
    ] {
        assert_client_matches_for_every_history(
            || {
                let mut terminal = replay(before, 6, 20);
                terminal.resize(6, 30);
                for chunk in chunks {
                    terminal.process(chunk);
                }
                terminal
            },
            what,
        );
    }
}
