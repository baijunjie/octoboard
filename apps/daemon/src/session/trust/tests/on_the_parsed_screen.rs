//! The trust screen read off the session's parsed screen, as a host that renders what the
//! program draws has it read (`TrustState::reads_screen`), over the captures the output's reading
//! is tested with: first drawings, cursors and dismissals read as they do off the output, except
//! where the output's order decides, which the parsed screen cannot see and which is why a host
//! whose output can be read still reads the output.

use super::*;
use crate::terminal::pty::INITIAL_SIZE;
use crate::terminal::snapshot::TerminalSnapshot;

/// The empty frame Codex 0.161.0 answers a key it ignores with (measured).
const IGNORED_KEY: &[u8] = b"\x1b[?2026h\x1b[39m\x1b[49m\x1b[0m\x1b[?2026l";

// What the inbox console host's repaint of Claude Code 2.1.319 and Codex 0.166.0 left on the
// screen (`testdata/conhost-captures.md`): each capture holds the output from the start, up to
// its stage, so one is replayed alone, never after another. After the person's answer, the next
// screen is the fresh input box (Claude Code) and the empty composer (Codex). They were taken
// at 120 columns and 40 rows, and are read on a screen of that size.
const CONHOST_SIZE: (u16, u16) = (40, 120);
const CONHOST_SCREEN: &[u8] =
    include_bytes!("../../../../testdata/conhost_claude_trust_screen.bin");
const CONHOST_AFTER_DOWN: &[u8] =
    include_bytes!("../../../../testdata/conhost_claude_trust_after_down.bin");
const CONHOST_STARTED: &[u8] =
    include_bytes!("../../../../testdata/conhost_claude_input_placeholder.bin");
const CONHOST_CODEX_SCREEN: &[u8] =
    include_bytes!("../../../../testdata/conhost_codex_trust_screen.bin");
const CONHOST_CODEX_AFTER_DOWN: &[u8] =
    include_bytes!("../../../../testdata/conhost_codex_trust_after_down.bin");
const CONHOST_CODEX_STARTED: &[u8] =
    include_bytes!("../../../../testdata/conhost_codex_fresh_empty.bin");

/// A parsed screen of the size a session starts at, each of `frames` taken in as a read of its own.
fn parsed(frames: &[&[u8]]) -> TerminalSnapshot {
    let mut terminal = TerminalSnapshot::new(INITIAL_SIZE.rows, INITIAL_SIZE.cols);
    for frame in frames {
        terminal.process(frame);
    }
    terminal
}

/// The text of the parsed screen `frames` leave, as the trust path reads it.
fn shown(frames: &[&[u8]]) -> Vec<u8> {
    parsed(frames).visible_text().into_bytes()
}

/// The inverse-video cells of the parsed screen `frames` leave.
fn inverse(frames: &[&[u8]]) -> Vec<u8> {
    parsed(frames).visible_text_inverse().into_bytes()
}

/// `capture` with Codex's cursor glyph drawn as a space, as a host that loses the glyph would draw
/// it: the option the cursor is on is then told only by its inverse video.
fn without_glyph(capture: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(capture)
        .replace('\u{203A}', " ")
        .into_bytes()
}

/// A watch reading the parsed screen, fed by its own parsed screen one read at a time.
struct ScreenWatch {
    state: TrustState,
    terminal: TerminalSnapshot,
}

impl ScreenWatch {
    fn new(agent: AgentId) -> Self {
        Self::sized(agent, INITIAL_SIZE.rows, INITIAL_SIZE.cols)
    }

    /// A watch of a parsed screen of `rows` and `cols`, as the capture's own terminal had.
    fn sized(agent: AgentId, rows: u16, cols: u16) -> Self {
        let state = TrustState::new(agent, None);
        state.read_the_screen();
        Self {
            state,
            terminal: TerminalSnapshot::new(rows, cols),
        }
    }

    fn read(&mut self, chunk: &[u8]) {
        self.terminal.process(chunk);
        let terminal = &self.terminal;
        self.state
            .on_output(chunk, || terminal.visible_text(), 0, 0);
    }
}

/// The first drawings the output's reading recognises, and the near misses it does not, are what
/// the parsed screen recognises.
#[test]
fn the_parsed_screen_recognises_each_first_drawing_the_output_does() {
    for (screen, output, expected) in [
        (&CLAUDE, SCREEN, true),
        (&CODEX, CODEX_SCREEN, true),
        (&CODEX, CODEX_FOLDER_ACCESS, true),
        (&GROK, GROK_SCREEN, true),
        (&CODEX, CODEX_AFTER_DOWN, false),
        (&CODEX, CODEX_DISMISSED, false),
        (&CODEX, CODEX_HOOK_REVIEW, false),
        (&CODEX, SCREEN, false),
        (&GROK, SCREEN, false),
        (&CLAUDE, CODEX_SCREEN, false),
        (&GROK, GROK_ANIMATION, false),
        (&CLAUDE, AFTER_DOWN, false),
        (&CLAUDE, DISMISSED, false),
    ] {
        assert_eq!(screen.is_shown_in(output), expected, "the output");
        assert_eq!(
            screen.is_shown_in(&shown(&[output])),
            expected,
            "{:?} on the parsed screen of {} bytes",
            screen.first_draw,
            output.len()
        );
    }
    let prose: [&[u8]; 6] = [
        b"the option is called \"No, exit\" and nothing else",
        b"Claude Code asks: No, exit, or Yes, I trust this folder?",
        b"No, exit\r\nYes, I trust this folder",
        "\u{276F} 1. No, exit\r\n  2. Yes, I trust this folder\r\n".as_bytes(),
        "\u{276F} No, exit or maybe Yes, I trust this folder".as_bytes(),
        "\u{276F} Yes, I trust this folder\r\n  No, exit\r\n".as_bytes(),
    ];
    for text in prose {
        assert!(!CLAUDE.is_shown_in(text));
        assert!(!CLAUDE.is_shown_in(&shown(&[text])), "{text:?}");
    }
    let screen = std::str::from_utf8(CODEX_FOLDER_ACCESS).unwrap();
    for piece in [
        "Folder access",
        "Trust this folder?",
        "› 1. Trust and continue",
        "2. Back to Agent Command Center",
    ] {
        let missing = screen.replace(piece, "");
        assert!(!CODEX.is_shown_in(&shown(&[missing.as_bytes()])), "{piece}");
    }
}

/// The cursor is read off the glyph wherever it now stands on the parsed screen, as the output's
/// latest redraw has it.
#[test]
fn the_cursor_on_the_parsed_screen_is_where_the_latest_redraw_left_it() {
    for (screen, frames, cursor) in [
        (&CLAUDE, &[SCREEN][..], Some(Cursor::Decline)),
        (&CLAUDE, &[AFTER_DOWN][..], Some(Cursor::Accept)),
        (&CLAUDE, &[SCREEN, AFTER_DOWN][..], Some(Cursor::Accept)),
        (&CLAUDE, &[DISMISSED][..], None),
        (&CODEX, &[CODEX_SCREEN][..], Some(Cursor::Accept)),
        (&CODEX, &[CODEX_AFTER_DOWN][..], Some(Cursor::Decline)),
        (
            &CODEX,
            &[CODEX_SCREEN, CODEX_AFTER_DOWN][..],
            Some(Cursor::Decline),
        ),
        (&CODEX, &[CODEX_FOLDER_ACCESS][..], Some(Cursor::Accept)),
        (
            &CODEX,
            &[
                CODEX_FOLDER_ACCESS,
                "› 2. Back to Agent Command Center".as_bytes(),
            ][..],
            Some(Cursor::Decline),
        ),
        (&GROK, &[GROK_SCREEN][..], None),
    ] {
        assert_eq!(screen.cursor_on(&frames.concat()), cursor, "the output");
        assert_eq!(
            screen.cursor_on(&shown(frames)),
            cursor,
            "the parsed screen"
        );
    }
}

/// A screen whose cursor has moved since it was first drawn is still up on the parsed screen,
/// though its first drawing is no longer there to be found, so a press that finds the cursor
/// moved says so, as it does reading the output; once the screen is replaced it is not up.
#[test]
fn a_screen_whose_cursor_moved_is_still_up_on_the_parsed_screen() {
    for (screen, frames, up) in [
        (&CLAUDE, &[SCREEN][..], true),
        (&CLAUDE, &[SCREEN, AFTER_DOWN][..], true),
        (&CLAUDE, &[SCREEN, AFTER_DOWN, DISMISSED][..], false),
        (&CODEX, &[CODEX_SCREEN][..], true),
        (&CODEX, &[CODEX_SCREEN, CODEX_AFTER_DOWN][..], true),
        (&CODEX, &[CODEX_SCREEN, CODEX_DISMISSED][..], false),
        (&CODEX, &[CODEX_HOOK_REVIEW][..], false),
        (&GROK, &[GROK_SCREEN, GROK_ANIMATION][..], true),
    ] {
        assert_eq!(
            screen.is_up_in(&shown(frames)),
            up,
            "{} frames",
            frames.len()
        );
    }
    assert!(!CLAUDE.is_shown_in(&shown(&[SCREEN, AFTER_DOWN])));
}

/// Where no glyph marks the option the cursor is on, the parsed screen tells it by the option
/// drawn in inverse video, as Codex draws it. The glyph says first, and a screen that marks
/// nothing in inverse video, such as Claude Code's, is not read by it.
#[test]
fn the_option_drawn_in_inverse_video_is_where_the_cursor_is() {
    let screen = without_glyph(CODEX_SCREEN);
    let after_down = without_glyph(CODEX_AFTER_DOWN);
    assert_eq!(
        CODEX.cursor_on(&shown(&[&screen])),
        None,
        "no glyph to read"
    );
    assert_eq!(
        CODEX.cursor_in_inverse(&inverse(&[&screen])),
        Some(Cursor::Accept)
    );
    assert_eq!(
        CODEX.cursor_in_inverse(&inverse(&[&screen, &after_down])),
        Some(Cursor::Decline)
    );
    assert_eq!(
        CODEX.cursor_in_inverse(&inverse(&[CODEX_SCREEN])),
        Some(Cursor::Accept),
        "the glyph's row is inverse too"
    );
    assert_eq!(
        CODEX.cursor_in_inverse(&inverse(&[CODEX_HOOK_REVIEW])),
        None
    );
    assert_eq!(CODEX.cursor_in_inverse(&inverse(&[CODEX_DISMISSED])), None);
    assert_eq!(CLAUDE.cursor_in_inverse(&inverse(&[SCREEN])), None);
    assert_eq!(GROK.cursor_in_inverse(&inverse(&[GROK_SCREEN])), None);
}

/// After Octoboard's own key, the parsed screen shows the screen gone once it has changed since
/// the key and holds none of the screen's text; a frame Codex draws for a key it ignores changes
/// nothing, and a screen already gone as the key was sent is gone only once something new is
/// drawn. After the person's keys, Codex's screen is replaced once its next screen is up with none
/// of the screen's own text left, either option included.
#[test]
fn the_parsed_screen_shows_the_screen_gone_by_what_changed_since() {
    for (screen, before, after, gone, replaced) in [
        (
            &CODEX,
            &[CODEX_SCREEN][..],
            &[CODEX_DISMISSED][..],
            true,
            true,
        ),
        (
            &CODEX,
            &[CODEX_SCREEN][..],
            &[IGNORED_KEY][..],
            false,
            false,
        ),
        (
            &CODEX,
            &[CODEX_SCREEN][..],
            &[CODEX_AFTER_DOWN][..],
            false,
            false,
        ),
        (
            &CODEX,
            &[CODEX_SCREEN][..],
            &[CODEX_DISMISSED, CODEX_AFTER_DOWN][..],
            false,
            false,
        ),
        (
            &CODEX,
            &[CODEX_SCREEN][..],
            &[CODEX_DISMISSED, b"2. Quit"][..],
            true,
            false,
        ),
        (
            &CODEX,
            &[CODEX_SCREEN][..],
            &[CODEX_DISMISSED, b"2. Back to Agent Command Center"][..],
            true,
            false,
        ),
        (
            &CODEX,
            &[CODEX_SCREEN][..],
            &[CODEX_DISMISSED, CODEX_FOLDER_ACCESS][..],
            false,
            false,
        ),
        (&CODEX, &[CODEX_DISMISSED][..], &[][..], false, true),
        (&CODEX, &[][..], &[&b"some other text"[..]][..], true, false),
        (
            &CLAUDE,
            &[SCREEN, AFTER_DOWN][..],
            &[DISMISSED][..],
            true,
            true,
        ),
        (&CLAUDE, &[SCREEN][..], &[AFTER_DOWN][..], false, false),
        (
            &CLAUDE,
            &[][..],
            &[&b"Bypass mode: No, exit or Yes, I accept"[..]][..],
            true,
            true,
        ),
    ] {
        let at_key = shown(before);
        let now = shown(&[before, after].concat());
        assert_eq!(screen.gone_from(&at_key, &now), gone, "gone");
        assert_eq!(screen.replaced_on(&now), replaced, "replaced");
    }
}

/// The inbox console host's repaint of each screen reads on the parsed screen as the macOS
/// captures do: the screen is recognised while its first option holds the cursor, the cursor is
/// found by Claude Code's glyph and by Codex's inverse row (with the glyph or without it), and the
/// screen is gone once the next one is up.
#[test]
fn the_inbox_console_host_s_repaint_of_each_screen_reads_as_the_macos_capture_does() {
    let screen_of = |capture: &[u8]| {
        let mut terminal = TerminalSnapshot::new(CONHOST_SIZE.0, CONHOST_SIZE.1);
        terminal.process(capture);
        terminal
    };
    let shown = |capture: &[u8]| screen_of(capture).visible_text().into_bytes();
    let inverse = |capture: &[u8]| screen_of(capture).visible_text_inverse().into_bytes();
    for (screen, capture, first, cursor, up) in [
        (&CLAUDE, CONHOST_SCREEN, true, Some(Cursor::Decline), true),
        (
            &CLAUDE,
            CONHOST_AFTER_DOWN,
            false,
            Some(Cursor::Accept),
            true,
        ),
        (&CLAUDE, CONHOST_STARTED, false, None, false),
        (
            &CODEX,
            CONHOST_CODEX_SCREEN,
            true,
            Some(Cursor::Accept),
            true,
        ),
        (
            &CODEX,
            CONHOST_CODEX_AFTER_DOWN,
            false,
            Some(Cursor::Decline),
            true,
        ),
        (&CODEX, CONHOST_CODEX_STARTED, false, None, false),
    ] {
        let text = shown(capture);
        assert_eq!(screen.is_shown_in(&text), first, "first drawing");
        assert_eq!(screen.cursor_on(&text), cursor, "cursor");
        assert_eq!(screen.is_up_in(&text), up, "up");
    }
    for (capture, cursor) in [
        (CONHOST_CODEX_SCREEN, Some(Cursor::Accept)),
        (CONHOST_CODEX_AFTER_DOWN, Some(Cursor::Decline)),
        (CONHOST_CODEX_STARTED, None),
    ] {
        assert_eq!(CODEX.cursor_in_inverse(&inverse(capture)), cursor);
    }
    let glyphless = without_glyph(CONHOST_CODEX_AFTER_DOWN);
    assert_eq!(CODEX.cursor_on(&shown(&glyphless)), None);
    assert_eq!(
        CODEX.cursor_in_inverse(&inverse(&glyphless)),
        Some(Cursor::Decline)
    );
    for (screen, at_key, started) in [
        (&CLAUDE, CONHOST_AFTER_DOWN, CONHOST_STARTED),
        (&CODEX, CONHOST_CODEX_SCREEN, CONHOST_CODEX_STARTED),
    ] {
        let now = shown(started);
        assert!(screen.gone_from(&shown(at_key), &now), "gone");
        assert!(screen.replaced_on(&now), "replaced");
    }
    assert!(!CODEX.is_shown_in(&shown(CONHOST_SCREEN)));
    assert!(!CLAUDE.is_shown_in(&shown(CONHOST_CODEX_SCREEN)));
}

/// The parsed screen is read after every read of the output, so the screen is sighted once
/// however it is cut into reads, and not before it is complete.
#[test]
fn the_parsed_screen_is_sighted_once_however_the_screen_is_cut_into_reads() {
    let initial = (INITIAL_SIZE.rows, INITIAL_SIZE.cols);
    for (agent, screen, after, (rows, cols)) in [
        (AgentId::CLAUDE, SCREEN, AFTER_DOWN, initial),
        (AgentId::CODEX, CODEX_SCREEN, CODEX_AFTER_DOWN, initial),
        (
            AgentId::CODEX,
            CODEX_FOLDER_ACCESS,
            CODEX_AFTER_DOWN,
            initial,
        ),
        (AgentId::GROK, GROK_SCREEN, GROK_ANIMATION, initial),
        (
            AgentId::CLAUDE,
            CONHOST_SCREEN,
            CONHOST_AFTER_DOWN,
            CONHOST_SIZE,
        ),
        (
            AgentId::CODEX,
            CONHOST_CODEX_SCREEN,
            CONHOST_CODEX_AFTER_DOWN,
            CONHOST_SIZE,
        ),
    ] {
        for size in [1, 3, 7, 200, screen.len()] {
            let mut watch = ScreenWatch::sized(agent, rows, cols);
            let mut sightings = watch.state.take_sightings().expect("receiver");
            for chunk in screen.chunks(size) {
                watch.read(chunk);
            }
            watch.read(after);
            watch.read(screen);
            assert!(sightings.try_recv().is_ok(), "{agent:?}, reads of {size}");
            assert!(
                sightings.try_recv().is_err(),
                "{agent:?} sighted only once with reads of {size}"
            );
            assert!(watch.state.window.lock().unwrap().is_empty());
        }
    }

    let mut watch = ScreenWatch::new(AgentId::CLAUDE);
    let mut sightings = watch.state.take_sightings().expect("receiver");
    let cut = SCREEN
        .windows(4)
        .position(|window| window == b"exit")
        .expect("the first option");
    watch.read(&SCREEN[..cut]);
    assert!(sightings.try_recv().is_err());
    watch.read(&SCREEN[cut..]);
    assert!(sightings.try_recv().is_ok());

    // Codex's composer ends the watch (its title does not, where the parsed screen is read:
    // `codex_on_the_parsed_screen_ends_its_watch_at_its_composer_rather_than_its_title`), and a
    // hook ends any agent's.
    let mut watch = ScreenWatch::new(AgentId::CODEX);
    let mut sightings = watch.state.take_sightings().expect("receiver");
    watch.read(CODEX_STARTED);
    watch.read(CODEX_SCREEN);
    assert!(sightings.try_recv().is_err());
    let mut watch = ScreenWatch::new(AgentId::CLAUDE);
    let mut sightings = watch.state.take_sightings().expect("receiver");
    watch.state.note_hook();
    watch.read(SCREEN);
    assert!(sightings.try_recv().is_err());
}

/// Where the output's order is what decides, the parsed screen, holding only what is up now,
/// decides otherwise: a first drawing and the redraw after a Down that arrive in one read leave
/// only the redraw on it, so nothing is sighted, and a redraw of the options after Octoboard's key
/// followed by the next screen leaves only the next screen, so the screen counts as gone. This is
/// why a host whose output can be read reads the output.
#[test]
fn the_parsed_screen_decides_otherwise_where_the_order_of_the_output_matters() {
    let one_read = [SCREEN, AFTER_DOWN].concat();
    let output = TrustState::new(AgentId::CLAUDE, None);
    let mut output_sightings = output.take_sightings().expect("receiver");
    output.feed(&one_read, 0, 0);
    assert!(output_sightings.try_recv().is_ok(), "the output sights it");
    let mut screen = ScreenWatch::new(AgentId::CLAUDE);
    let mut screen_sightings = screen.state.take_sightings().expect("receiver");
    screen.read(&one_read);
    assert!(
        screen_sightings.try_recv().is_err(),
        "the parsed screen does not"
    );

    let redrawn_then_replaced = [CODEX_AFTER_DOWN, CODEX_DISMISSED].concat();
    assert!(!CODEX.gone_in(&redrawn_then_replaced), "the output");
    assert!(
        CODEX.gone_from(
            &shown(&[CODEX_SCREEN]),
            &shown(&[CODEX_SCREEN, &redrawn_then_replaced])
        ),
        "the parsed screen"
    );
}

/// [`stand_in`] reading its trust screen off the parsed screen from the start.
fn screen_stand_in(id: &str, agent: AgentId, script: &str) -> StandIn {
    let script = format!("stty raw -echo; {script}");
    let live = watched_live_session(id, agent, 120, 32, &script, None);
    live.trust.read_the_screen();
    spawn_reader_thread(live.clone(), 8 * 1024);
    live
}

/// Claude Code's screen read off the parsed screen is pressed as it is reading the output: a Down,
/// an Enter once the cursor is seen on the option that trusts, and nothing else.
#[test]
fn claude_on_the_parsed_screen_is_pressed_with_a_down_and_an_enter() {
    let dir = scratch("screen-answer");
    let received = dir.join("received");
    let live = screen_stand_in("s", AgentId::CLAUDE, &full_script(&received));
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    answer(&live).expect("answered");

    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(sent(&received), answer_keys());
}

/// With the cursor moved to "Yes" before the press, the parsed screen still shows the screen up,
/// and nothing is written, the refusal naming the cursor as reading the output does.
#[test]
fn claude_on_the_parsed_screen_is_not_pressed_with_the_cursor_moved() {
    let dir = scratch("screen-cursor-on-yes");
    let received = dir.join("received");
    let r = quoted(&received);
    let script = [
        load_captures(),
        show("SCREEN"),
        take(1, &r),
        show("AFTER_DOWN"),
        take(1, &r),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let live = screen_stand_in("s", AgentId::CLAUDE, &script);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));
    // The person's key, which the stand-in answers with the redraw after a Down.
    live.write_input(b"j").expect("the person's key");
    assert!(wait_for(PATIENCE, || CLAUDE
        .cursor_on(live.screen_text().as_bytes())
        == Some(Cursor::Accept)));

    let err = answer(&live).expect_err("refused");
    assert!(err.to_string().contains("starts on"), "{err}");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(sent(&received), b"j", "no key of Octoboard's");
}

/// The Down goes in and the parsed screen never shows the cursor move, so the Enter is withheld.
#[test]
fn claude_on_the_parsed_screen_withholds_the_enter_when_the_cursor_does_not_move() {
    let dir = scratch("screen-no-move");
    let received = dir.join("received");
    let r = quoted(&received);
    let script = [
        load_captures(),
        show("SCREEN"),
        take(3, &r),
        take(1, &r),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let live = screen_stand_in("s", AgentId::CLAUDE, &script);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    let err = answer(&live).expect_err("not confirmed");
    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(
        coded.params["reason_code"],
        trust_reason::CURSOR_DID_NOT_MOVE
    );
    assert_eq!(sent(&received), DOWN);
}

/// Codex's screen read off the parsed screen is pressed with an Enter alone, and the press counts
/// once the parsed screen shows its next screen.
#[test]
fn codex_on_the_parsed_screen_is_pressed_with_an_enter_and_nothing_else() {
    let dir = scratch("screen-codex");
    let received = dir.join("received");
    let live = screen_stand_in("s", AgentId::CODEX, &codex_script(&received));
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    answer(&live).expect("pressed");

    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(sent(&received), ENTER);
}

/// Codex's screen without its glyph is still pressed where the parsed screen shows the option that
/// trusts in inverse video.
#[test]
fn codex_on_the_parsed_screen_is_pressed_by_its_inverse_video_without_a_glyph() {
    let dir = scratch("screen-codex-inverse");
    let received = dir.join("received");
    let screen = dir.join("screen.bin");
    std::fs::write(&screen, without_glyph(CODEX_SCREEN)).expect("the glyphless screen");
    let r = quoted(&received);
    let script = [
        read_capture("SCREEN", &quoted(&screen)),
        load(&[("DISMISSED", "codex_trust_dismissed.bin")]),
        show("SCREEN"),
        take(1, &r),
        show("DISMISSED"),
        take(1, &r),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let live = screen_stand_in("s", AgentId::CODEX, &script);
    // Without the glyph its first drawing is not there to sight; the press is what is tested.
    assert!(wait_for(PATIENCE, || CODEX.is_up_in(live.screen_text().as_bytes())));
    live.trust.feed_screen(
        || parsed(&[CODEX_SCREEN]).visible_text(),
        live.input_writes(),
        live.output_total(),
    );

    answer(&live).expect("pressed");

    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(sent(&received), ENTER);
}

/// A Codex screen the person answers in the terminal lets the messages held for the session
/// through once the parsed screen shows its next screen, even when they type on into it at once;
/// a Down, which only moves the cursor, keeps them held.
#[tokio::test]
async fn codex_answered_in_the_terminal_on_the_parsed_screen_lets_held_messages_through() {
    let (state, dir) = state_with_project("screen-codex-released");
    let message = crate::platform::term::encode(&crate::session::term::frame_message(
        AgentId::CODEX,
        "hello",
    ));
    let r = "'%RECEIVED%'";
    let start = |id: &str, script: &str| {
        let record = session(id, AgentId::CODEX, SessionKind::Project, Some("project-1"));
        let received = dir.join(format!("received-{id}"));
        state.store.insert_session(&record).expect("session");
        let live = screen_stand_in(
            id,
            AgentId::CODEX,
            &script.replace("%RECEIVED%", &received.to_string_lossy()),
        );
        state.register_live(live.clone());
        (live, received)
    };
    let (answered_live, answered_received) = start(
        "codex-1",
        &codex_script_reading(Path::new("%RECEIVED%"), 3 + message.len()),
    );
    let moved_script = [
        load(&[
            ("SCREEN", "codex_trust_screen.bin"),
            ("AFTER_DOWN", "codex_trust_after_down.bin"),
        ]),
        show("SCREEN"),
        take(3, r),
        show("AFTER_DOWN"),
        take(1, r),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let (moved_live, moved_received) = start("codex-2", &moved_script);
    for live in [&answered_live, &moved_live] {
        assert!(wait_for(PATIENCE, || live.trust.waiting()));
        supervise(&state, live);
        let delivery = crate::messaging::delivery::write_message(
            &state,
            &live.id,
            "hello",
            crate::messaging::delivery::WhenBlocked::Queue,
        )
        .expect("accepted");
        assert!(matches!(
            delivery,
            crate::messaging::delivery::Delivery::Queued
        ));
    }

    answered_live
        .write_input(ENTER)
        .expect("the person's Enter");
    answered_live
        .write_input(b"abc")
        .expect("the person's typing");
    moved_live.write_input(DOWN).expect("the person's Down");

    let expected = [ENTER, b"abc", &message].concat();
    let delivered = tokio::task::spawn_blocking(move || {
        wait_for(PATIENCE, || sent(&answered_received) == expected)
    });
    assert!(delivered.await.unwrap(), "released once the screen went");
    assert_eq!(sent(&moved_received), DOWN, "still held at the screen");
}

/// A Codex session past its startup (0.163.0), at its first empty composer.
const CODEX_STARTED: &[u8] = include_bytes!("../../../../testdata/codex_fresh_empty.bin");

/// A watched stand-in of `agent` with nothing reading its output, read as a host that renders what
/// the program draws reads it, so the test feeds it.
fn rendered_watch(agent: AgentId) -> StandIn {
    let live = watched_live_session("s", agent, 140, 45, "sleep 30", None);
    live.read_as_rendered();
    live
}

/// Where the parsed screen is read, Codex's watch ends at the composer it draws once its session
/// has started, not at a title, which the host can send at a moment of its own: until then a screen
/// that comes is still sighted, and afterwards none is.
#[test]
fn codex_on_the_parsed_screen_ends_its_watch_at_its_composer_rather_than_its_title() {
    let live = rendered_watch(AgentId::CODEX);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    live.on_output(b"login\x1b]0;project\x07");
    assert!(live.trust.holds_writes(), "a title ends nothing");
    live.on_output(CODEX_STARTED);
    assert!(!live.trust.holds_writes(), "the composer ends the watch");
    live.on_output(CODEX_SCREEN);
    assert!(sightings.try_recv().is_err(), "too late by the composer");

    let live = rendered_watch(AgentId::CODEX);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    live.on_output(b"login\x1b]0;project\x07");
    live.on_output(CODEX_SCREEN);
    assert!(sightings.try_recv().is_ok(), "sighted after the title");
    assert!(live.trust.waiting());
}

/// Claude Code's capture at its fresh prompt (2.1.310) without the bracketed paste it turns on, as
/// a host that keeps bracketed paste to itself would pass it on.
fn claude_prompt_without_paste_mode() -> Vec<u8> {
    String::from_utf8_lossy(include_bytes!(
        "../../../../testdata/claude_input_placeholder.bin"
    ))
    .replace("\x1b[?2004h", "")
    .replace("\x1b[?2004l", "")
    .into_bytes()
}

/// Where the parsed screen is read, Claude Code's input box drawn after its `SessionStart` hook
/// says it reads its input, as bracketed paste does; one drawn before the hook says nothing, and
/// bracketed paste still does on its own.
#[test]
fn claude_on_the_parsed_screen_reads_its_input_once_its_box_is_up_after_session_start() {
    let state = TrustState::new(AgentId::CLAUDE, None);
    state.read_the_screen();
    state.note_session_start();
    state.note_hook();
    assert!(state.holds_writes());
    assert!(state.watches_input_box());
    state.note_input_box(false);
    assert!(state.holds_writes(), "no box yet");
    state.note_input_box(true);
    assert!(!state.holds_writes());
    assert!(!state.follows_paste_mode());
    assert!(!state.watches_input_box());

    let state = TrustState::new(AgentId::CLAUDE, None);
    state.read_the_screen();
    state.note_input_box(true);
    state.note_hook();
    assert!(
        state.holds_writes(),
        "a box before SessionStart says nothing"
    );
    state.note_paste_mode(true);
    assert!(!state.holds_writes(), "bracketed paste still does");
}

/// Where the output is read, the `SessionStart` hook and the input box change nothing: only
/// bracketed paste says Claude Code reads its input.
#[test]
fn claude_reading_the_output_waits_for_bracketed_paste_alone() {
    let state = TrustState::new(AgentId::CLAUDE, None);
    state.read_the_output();
    state.note_session_start();
    assert!(!state.watches_input_box());
    state.note_input_box(true);
    state.note_hook();
    assert!(state.holds_writes());
    state.note_paste_mode(true);
    assert!(!state.holds_writes());
}

/// A session read as a host that renders what the program draws, whose bracketed paste never
/// shows: its `SessionStart` hook with the input box already up lets its messages through, and so
/// does the box drawn after the hook.
#[test]
fn claude_s_session_start_with_its_input_box_up_releases_its_messages_on_the_parsed_screen() {
    let prompt = claude_prompt_without_paste_mode();

    let live = rendered_watch(AgentId::CLAUDE);
    live.on_output(&prompt);
    assert!(!live.bracketed_paste_on());
    on_session_start(&live);
    assert!(live.trust.note_hook().released);
    assert!(!live.trust.holds_writes());

    let live = rendered_watch(AgentId::CLAUDE);
    on_session_start(&live);
    assert!(!live.trust.note_hook().released);
    assert!(live.trust.holds_writes(), "nothing drawn yet");
    live.on_output(&prompt);
    assert!(!live.trust.holds_writes(), "the box drawn after the hook");
}
