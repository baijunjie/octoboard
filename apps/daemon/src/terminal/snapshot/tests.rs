use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

use super::TerminalSnapshot;
use crate::terminal::display::DisplayFacts;
use crate::terminal::queries::Query;

pub(super) fn replay(output: &[u8], rows: u16, cols: u16) -> TerminalSnapshot {
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

fn answering(rows: u16, cols: u16) -> TerminalSnapshot {
    let mut terminal = TerminalSnapshot::new(rows, cols);
    terminal.answer_queries(false);
    terminal
}

fn light() -> DisplayFacts {
    DisplayFacts::reported("#383a42", "#fafafa", "#101010", Some((9, 20))).expect("a display")
}

#[test]
fn each_query_is_answered_from_the_parsed_terminal_and_left_out_of_the_delivered_output() {
    let mut terminal = answering(24, 80);
    let taken = terminal.take_in(b"ab\x1b[6ncd\x1b[c");
    assert_eq!(taken.delivered, b"abcd");
    assert_eq!(taken.answers, b"\x1b[1;3R\x1b[?6c");
    assert_eq!(
        taken.asked,
        [
            (Query::CursorPosition, true),
            (Query::DeviceAttributes, true)
        ]
    );
    assert!(
        terminal.visible_text().starts_with("abcd "),
        "the output is parsed"
    );

    let cell = DisplayFacts::default().cell;
    for (query, answer) in [
        (&b"\x1b[0c"[..], "\x1b[?6c".to_string()),
        (b"\x1b[5n", "\x1b[0n".to_string()),
        (b"\x1b[18t", "\x1b[8;24;80t".to_string()),
        (
            b"\x1b[14t",
            format!("\x1b[4;{};{}t", 24 * cell.1, 80 * cell.0),
        ),
        (b"\x1b[16t", format!("\x1b[6;{};{}t", cell.1, cell.0)),
        (b"\x1b[?2004$p", "\x1b[?2004;2$y".to_string()),
        (b"\x1b[4$p", "\x1b[4;2$y".to_string()),
        (b"\x1b[?996n", "\x1b[?997;1n".to_string()),
        (
            b"\x1b]10;?\x07",
            "\x1b]10;rgb:e4e4/e4e4/e6e6\x07".to_string(),
        ),
        (
            b"\x1b]11;?\x1b\\",
            "\x1b]11;rgb:1e1e/1f1f/2222\x1b\\".to_string(),
        ),
        (
            b"\x1b]12;?\x07",
            "\x1b]12;rgb:8080/8080/8080\x07".to_string(),
        ),
    ] {
        let taken = terminal.take_in(query);
        assert_eq!(
            String::from_utf8_lossy(&taken.answers),
            answer,
            "{:?}",
            String::from_utf8_lossy(query)
        );
        assert!(taken.delivered.is_empty());
    }
    let secondary = terminal.take_in(b"\x1b[>c").answers;
    assert!(secondary.starts_with(b"\x1b[>0;"), "{secondary:?}");

    let _ = terminal.take_in(b"\x1b[?2004h");
    assert_eq!(terminal.take_in(b"\x1b[?2004$p").answers, b"\x1b[?2004;1$y");
}

#[test]
fn colour_and_size_answers_follow_the_display_and_a_colour_the_program_set() {
    let mut terminal = answering(10, 40);
    assert!(
        terminal.set_display(light()).is_empty(),
        "nobody subscribed"
    );
    assert_eq!(
        terminal.take_in(b"\x1b]11;?\x07").answers,
        b"\x1b]11;rgb:fafa/fafa/fafa\x07"
    );
    assert_eq!(terminal.take_in(b"\x1b[?996n").answers, b"\x1b[?997;2n");
    assert_eq!(terminal.take_in(b"\x1b[14t").answers, b"\x1b[4;200;360t");
    assert_eq!(terminal.take_in(b"\x1b[16t").answers, b"\x1b[6;20;9t");

    let taken = terminal.take_in(b"\x1b]11;rgb:12/34/56\x07\x1b]11;?\x07");
    assert_eq!(
        taken.delivered, b"\x1b]11;rgb:12/34/56\x07",
        "setting a colour is output"
    );
    assert_eq!(taken.answers, b"\x1b]11;rgb:1212/3434/5656\x07");
}

#[test]
fn a_query_the_parsed_terminal_cannot_answer_is_delivered_for_a_client() {
    let mut terminal = answering(24, 80);
    for query in [
        &b"\x1b]4;1;?\x07"[..],
        b"\x1b[?9999$p",
        b"\x1b[?12$p",
        b"\x1b[?u",
        b"\x1b[>q",
        b"\x1bP$qm\x1b\\",
    ] {
        let taken = terminal.take_in(query);
        assert_eq!(
            taken.delivered,
            query,
            "{:?}",
            String::from_utf8_lossy(query)
        );
        assert!(
            taken.answers.is_empty(),
            "{:?}",
            String::from_utf8_lossy(query)
        );
    }
    assert_eq!(
        terminal.take_in(b"\x1b[?9999$p").asked,
        [(
            Query::Mode {
                private: true,
                mode: 9999
            },
            false
        )]
    );
}

#[test]
fn where_the_host_answers_the_reports_they_are_delivered_and_the_rest_still_answered() {
    let mut terminal = TerminalSnapshot::new(24, 80);
    terminal.answer_queries(true);
    let reports = b"\x1b[c\x1b[>c\x1b[5n\x1b[6n";
    let taken = terminal.take_in(reports);
    assert_eq!(taken.delivered, reports);
    assert!(taken.answers.is_empty());
    assert_eq!(taken.asked.len(), 4);
    assert_eq!(
        terminal.take_in(b"\x1b]11;?\x07").answers,
        b"\x1b]11;rgb:1e1e/1f1f/2222\x07"
    );
}

#[test]
fn a_program_subscribed_to_colour_scheme_updates_is_told_when_the_colours_change() {
    let mut terminal = answering(24, 80);
    assert_eq!(terminal.take_in(b"\x1b[?2031$p").answers, b"\x1b[?2031;2$y");
    let taken = terminal.take_in(b"\x1b[?2031h");
    assert_eq!(taken.delivered, b"\x1b[?2031h", "a mode set is output");
    assert_eq!(terminal.take_in(b"\x1b[?2031$p").answers, b"\x1b[?2031;1$y");

    assert_eq!(terminal.set_display(light()), b"\x1b[?997;2n");
    assert!(terminal.set_display(light()).is_empty(), "the same colours");
    let larger = DisplayFacts {
        cell: (12, 24),
        ..light()
    };
    assert!(
        terminal.set_display(larger).is_empty(),
        "only the cell changed"
    );
    assert_eq!(
        terminal.set_display(DisplayFacts::default()),
        b"\x1b[?997;1n"
    );

    let _ = terminal.take_in(b"\x1b[?2031l");
    assert!(terminal.set_display(light()).is_empty(), "unsubscribed");

    let _ = terminal.take_in(b"\x1b[?2031h");
    let _ = terminal.take_in(b"\x1bc");
    assert_eq!(
        terminal.take_in(b"\x1b[?2031$p").answers,
        b"\x1b[?2031;2$y",
        "a reset unsubscribes"
    );
}

#[test]
fn a_query_cut_across_chunks_is_answered_once_whole_and_never_delivered() {
    let mut terminal = answering(24, 80);
    let first = terminal.take_in(b"ab\x1b[");
    assert_eq!(first.delivered, b"ab");
    assert!(first.answers.is_empty());
    let second = terminal.take_in(b"6ncd");
    assert_eq!(second.delivered, b"cd");
    assert_eq!(second.answers, b"\x1b[1;3R");
}

#[test]
fn without_answering_every_byte_is_delivered_and_nothing_answered() {
    let mut terminal = TerminalSnapshot::new(24, 80);
    let taken = terminal.take_in(b"\x1b[c\x1b[6n");
    assert_eq!(taken.delivered, b"\x1b[c\x1b[6n");
    assert!(taken.answers.is_empty() && taken.asked.is_empty());
}

#[test]
fn captured_agent_screens_stay_readable_after_exit_cleanup() {
    for capture in [
        &include_bytes!("../../../testdata/claude_trust_screen.bin")[..],
        &include_bytes!("../../../testdata/grok_trust_screen.bin")[..],
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
        assert!(!screen
            .terminal
            .mode()
            .intersects(TermMode::ALT_SCREEN | TermMode::MOUSE_MODE | TermMode::BRACKETED_PASTE));
        terminal.process(b"\x1b[?1049l\x1b[2J\x1b[HResume this session with: exit");
        terminal.freeze();
        assert_eq!(terminal.output(), frozen);
    }
}

#[test]
fn normal_history_survives_raw_ring_eviction_and_alternate_view_is_independent() {
    let mut terminal = TerminalSnapshot::new(3, 40);
    terminal.process(b"retained history\r\nsecond\r\nthird\r\nfourth");
    let mut ring = crate::terminal::ringbuf::RingBuffer::new(crate::terminal::fan::RING_CAPACITY);
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
    terminal.process("\x1b[1;3;4;38;2;1;2;3;48;5;42m界e\u{301}ABCDEF\x1b[0m\r\nlast".as_bytes());
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

fn keeping_program_status(host_answers_reports: bool) -> TerminalSnapshot {
    let mut terminal = TerminalSnapshot::new(24, 80);
    terminal.answer_queries(host_answers_reports);
    terminal.keep_program_status();
    terminal
}

#[test]
fn the_program_status_support_query_is_answered_where_status_is_kept_on_every_host() {
    for host_answers_reports in [false, true] {
        let mut terminal = keeping_program_status(host_answers_reports);
        let taken = terminal.take_in(b"a\x1b]7501;?\x1b\\b\x1b]7501;?\x07");
        assert_eq!(taken.delivered, b"ab");
        assert_eq!(
            taken.answers,
            [
                &b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\"[..],
                b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x07",
            ]
            .concat()
        );
    }

    // Where nothing keeps the reports, the query is the clients' to answer, and none does.
    let mut terminal = answering(24, 80);
    let taken = terminal.take_in(b"\x1b]7501;?\x07");
    assert_eq!(taken.delivered, b"\x1b]7501;?\x07");
    assert!(taken.answers.is_empty());
}

#[test]
fn status_reports_move_the_root_record_and_are_left_out_of_the_delivered_output() {
    use crate::terminal::program_status::ReportedState;

    let mut terminal = keeping_program_status(false);
    let taken = terminal.take_in(
        b"x\x1b]7501;state=working\x07\x1b]7501;state=working:progress=5\x07\
          \x1b]7501;state=done:id=child\x1b\\\x1b]7501;state=blocked:kind=permission\x07y",
    );
    assert_eq!(taken.delivered, b"xy");
    assert!(taken.answers.is_empty());
    assert_eq!(
        taken.program_status,
        [Some(ReportedState::Working), Some(ReportedState::Blocked)],
        "only a move of the root's state counts"
    );

    // An unknown state, and a report past a limit, change nothing.
    let taken = terminal.take_in(
        b"\x1b]7501;state=asleep\x07\x1b]7501;state=idle:app=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\x07",
    );
    assert!(taken.delivered.is_empty());
    assert!(taken.program_status.is_empty());

    let taken = terminal.take_in(b"\x1b]7501;state=clear\x07");
    assert_eq!(taken.program_status, [None]);
    let _ = terminal.take_in(b"\x1b]7501;state=error\x07");
    assert_eq!(
        terminal.take_in(b"\x1bc").program_status,
        [None],
        "a reset forgets"
    );

    // Where nothing keeps them, reports reach the clients untouched.
    let mut terminal = answering(24, 80);
    let taken = terminal.take_in(b"\x1b]7501;state=working\x07");
    assert_eq!(taken.delivered, b"\x1b]7501;state=working\x07");
    assert!(taken.program_status.is_empty());
}

#[test]
fn the_root_record_s_state_is_readable_as_it_stands() {
    use crate::terminal::program_status::ReportedState;

    let mut terminal = keeping_program_status(false);
    assert_eq!(terminal.program_status_root(), None);
    let _ = terminal.take_in(b"\x1b]7501;state=working\x07");
    assert_eq!(terminal.program_status_root(), Some(ReportedState::Working));
    // A child's record is not the root's.
    let _ = terminal.take_in(b"\x1b]7501;state=done:id=child\x07");
    assert_eq!(terminal.program_status_root(), Some(ReportedState::Working));
    let _ = terminal.take_in(b"\x1b]7501;state=blocked:kind=question\x07");
    assert_eq!(terminal.program_status_root(), Some(ReportedState::Blocked));
    let _ = terminal.take_in(b"\x1b]7501;state=clear\x07");
    assert_eq!(terminal.program_status_root(), None);
    let _ = terminal.take_in(b"\x1b]7501;state=idle\x07");
    assert_eq!(terminal.program_status_root(), Some(ReportedState::Idle));

    // Where nothing keeps the reports, there is no record.
    let mut terminal = answering(24, 80);
    let _ = terminal.take_in(b"\x1b]7501;state=working\x07");
    assert_eq!(terminal.program_status_root(), None);
    assert_eq!(TerminalSnapshot::new(24, 80).program_status_root(), None);
}

#[test]
fn a_prompt_start_ends_working_and_blocked_records_and_still_reaches_the_clients() {
    use crate::terminal::program_status::ReportedState;

    let mut terminal = keeping_program_status(false);
    let _ = terminal.take_in(b"\x1b]7501;state=working\x07\x1b]7501;state=blocked:id=a\x07");
    // In order with the reports around it, and cut across reads.
    assert!(terminal
        .take_in(b"\x1b]7501;state=done:id=b\x07\x1b]13")
        .program_status
        .is_empty());
    let taken = terminal.take_in(b"3;A\x07\x1b]7501;state=working\x07");
    assert_eq!(taken.delivered, b"\x1b]133;A\x07", "the prompt is output");
    assert_eq!(
        taken.program_status,
        [None, Some(ReportedState::Working)],
        "the root went at the prompt, then came back with the report after it"
    );

    // A soft reset (DECSTR) forgets nothing.
    assert!(terminal.take_in(b"\x1b[!p").program_status.is_empty());

    // A root that is done stays at a prompt.
    let _ = terminal.take_in(b"\x1b]7501;state=done\x07");
    assert!(terminal
        .take_in(b"\x1b]133;A;cl=m\x1b\\")
        .program_status
        .is_empty());

    // The end of the process forgets the running records without reporting a move.
    let _ = terminal.take_in(b"\x1b]7501;state=working\x07");
    terminal.end_program_status();
    let taken = terminal.take_in(b"\x1b]7501;state=working\x07");
    assert_eq!(
        taken.program_status,
        [Some(ReportedState::Working)],
        "the working root was forgotten, so a new one moves it"
    );
}

#[test]
fn a_query_not_allowed_is_dropped_and_not_recorded_as_asked() {
    use crate::terminal::program_status::ReportedState;

    let mut terminal = keeping_program_status(false);
    let taken = terminal.take_in_allowing(
        b"a\x1b[6nb\x1b[?2031h\x1b]7501;state=working\x07\x1b[c",
        |query| *query == Query::DeviceAttributes,
    );
    assert_eq!(
        taken.delivered, b"ab\x1b[?2031h",
        "neither answered nor delivered"
    );
    assert_eq!(taken.answers, b"\x1b[?6c");
    assert_eq!(taken.asked, [(Query::DeviceAttributes, true)]);
    assert_eq!(
        taken.program_status,
        [Some(ReportedState::Working)],
        "a status report is no query"
    );
    assert_eq!(
        terminal.take_in(b"\x1b[?2031$p").answers,
        b"\x1b[?2031;1$y",
        "and neither are the private modes set"
    );
}
