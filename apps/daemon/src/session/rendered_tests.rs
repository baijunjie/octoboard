//! A session read as a host that renders what the program draws reads it
//! (`LiveSession::host_renders`, forced here by `LiveSession::read_as_rendered`): its titles come
//! off the parsed screen's title changes and say what they say off the output, once something has
//! been drawn, and its input box reads empty when it shows only the provider's placeholder.

use std::time::Instant;

use crate::protocol::{AgentId, Event, SessionStatus};
use crate::session::LiveSession;
use crate::state::test_support::insert_record;
use crate::test_support::{app_state, fake_live_session, StandIn};

/// What a session draws before the title captures, which keep only the title sequences.
const DRAWN: &[u8] = b"$ ";

/// A stand-in session of `agent`, read as a host that renders what the program draws reads it.
fn rendered(id: &str, agent: AgentId) -> StandIn {
    let live = fake_live_session(id, agent, 80, 24, "sleep 30");
    live.read_as_rendered();
    live
}

/// `output` taken in one byte at a time, as each title completes in a read of its own on a PTY.
fn replay(live: &LiveSession, output: &[u8]) {
    for byte in output {
        live.on_output(std::slice::from_ref(byte));
    }
}

/// What the session's titles have left it with: the latest title, its glyph, and what it said
/// about the turns.
fn read(live: &LiveSession, now: Instant) -> (Option<Option<String>>, Option<char>, u64, bool) {
    let observer = live.title.lock().unwrap();
    (
        observer.latest.clone(),
        observer.parser.leading_glyph(),
        observer.activity.stops(),
        observer.activity.shows_running(now),
    )
}

/// Every title capture leaves a session read off the parsed screen where it leaves one read off
/// the output: the same title, glyph, turns ended and turn running, which is what the task title,
/// Codex's "Action Required" (below), the title activity and the cancel read off a title are made
/// of.
#[test]
fn a_session_takes_the_same_titles_off_the_parsed_screen() {
    for (agent, captures) in [
        (
            AgentId::CLAUDE,
            &[
                &include_bytes!("../../testdata/claude_title_idle.bin")[..],
                include_bytes!("../../testdata/claude_title_cancel.bin"),
                include_bytes!("../../testdata/claude_title_permission.bin"),
            ][..],
        ),
        (
            AgentId::CODEX,
            &[
                &include_bytes!("../../testdata/codex_working_title.bin")[..],
                include_bytes!("../../testdata/codex_approval_title.bin"),
                include_bytes!("../../testdata/codex_working_title.bin"),
            ][..],
        ),
        (
            AgentId::GROK,
            &[
                &include_bytes!("../../testdata/grok_title_tool.bin")[..],
                include_bytes!("../../testdata/grok_title_turn_end.bin"),
                include_bytes!("../../testdata/grok_title_cancel_before_output.bin"),
            ][..],
        ),
    ] {
        let output = fake_live_session("output", agent, 80, 24, "sleep 30");
        let screen = rendered("screen", agent);
        for live in [&output, &screen] {
            live.on_output(DRAWN);
        }
        for capture in captures {
            replay(&output, capture);
            replay(&screen, capture);
            let now = Instant::now();
            let read_off_the_output = read(&output, now);
            assert!(read_off_the_output.0.is_some(), "{agent:?}");
            assert_eq!(read(&screen, now), read_off_the_output, "{agent:?}");
        }
    }
}

/// The session's title comes off the parsed screen once the program has drawn something; a title
/// set before that, as a console host names its console at start, is not taken.
#[test]
fn the_session_s_title_comes_off_the_parsed_screen_once_something_is_drawn() {
    let (state, dir) = app_state("rendered-terminal-title");
    insert_record(&state, &dir, "s", SessionStatus::Idle);
    let live = rendered("s", AgentId::CLAUDE);
    state.register_live(live.clone());
    let mut events = state.subscribe();
    live.on_output(b"\x1b]0;C:\\Program Files\\nodejs\\node.exe\x07");
    assert!(events.try_recv().is_err(), "the host's own title");
    live.on_output(b"drawn\x1b]2;First task\x07");
    let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
        panic!("expected a title update")
    };
    assert_eq!(session.terminal_title.as_deref(), Some("First task"));
}

/// Codex's "Action Required" title read off the parsed screen raises the hand as it does off the
/// output.
#[test]
fn codex_s_action_title_off_the_parsed_screen_waits_on_the_user() {
    let (state, dir) = app_state("rendered-action-title");
    insert_record(&state, &dir, "s", SessionStatus::Working);
    let mut record = state.session_record("s").unwrap();
    record.agent = AgentId::CODEX;
    state.save_session(&record).unwrap();
    let live = rendered("s", AgentId::CODEX);
    state.register_live(live.clone());
    live.on_output(DRAWN);
    replay(
        &live,
        include_bytes!("../../testdata/codex_approval_title.bin"),
    );
    assert!(live.needs_user_action());
    assert_eq!(state.agent_status(&record), SessionStatus::WaitingUser);
    replay(
        &live,
        include_bytes!("../../testdata/codex_working_title.bin"),
    );
    assert!(!live.needs_user_action());
}

/// An input box that shows nothing but its provider's placeholder is empty where the host renders
/// what the program draws, whatever the placeholder is drawn with; elsewhere it is empty only with
/// the placeholder's faint cells blanked. A draft reads the same on both.
#[test]
fn a_placeholder_is_an_empty_input_box_where_the_host_renders() {
    let replayed = |bytes: &[u8], rows: u16, cols: u16| {
        let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(rows, cols);
        screen.process(bytes);
        (screen.visible_text(), screen.visible_text_undimmed())
    };
    for (agent, capture, rows, cols, placeholder) in [
        (
            AgentId::CLAUDE,
            &include_bytes!("../../testdata/claude_input_placeholder.bin")[..],
            40,
            120,
            "Try \"write a test for <filepath>\"",
        ),
        (
            AgentId::CLAUDE,
            include_bytes!("../../testdata/claude_input_queued.bin"),
            40,
            120,
            "Press up to edit queued messages",
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/codex_fresh_empty.bin"),
            45,
            140,
            "Ask Codex to do anything",
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/codex_hooked_empty.bin"),
            45,
            140,
            "Ask Codex to do anything",
        ),
    ] {
        let output = fake_live_session("output", agent, 80, 24, "sleep 30");
        let screen = rendered("screen", agent);
        let (plain, undimmed) = replayed(capture, rows, cols);
        // The plain screen stands for a host that drew the faint cells plain.
        assert_eq!(output.input_box(&plain).as_deref(), Some(placeholder));
        assert_eq!(
            screen.input_box(&plain).as_deref(),
            Some(""),
            "{placeholder}"
        );
        assert_eq!(
            screen.input_box_rows(&plain),
            output.input_box_rows(&undimmed),
            "{placeholder}: the same rows as with the faint cells blanked"
        );
        for live in [&output, &screen] {
            assert_eq!(
                live.input_box(&undimmed).as_deref(),
                Some(""),
                "{placeholder}"
            );
        }
    }
    for (agent, capture, rows, cols, draft) in [
        (
            AgentId::CLAUDE,
            &include_bytes!("../../testdata/claude_input_queued_draft.bin")[..],
            40,
            120,
            "draft words",
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/codex_fresh_draft.bin"),
            45,
            140,
            "FRESH_DRAFT_ONE\nFRESH_DRAFT_TWO",
        ),
        (
            AgentId::GROK,
            include_bytes!("../../testdata/grok_composer_draft.bin"),
            40,
            120,
            "draft text",
        ),
    ] {
        let screen = rendered("screen", agent);
        let (plain, undimmed) = replayed(capture, rows, cols);
        for text in [&plain, &undimmed] {
            assert_eq!(screen.input_box(text).as_deref(), Some(draft));
        }
    }
}

/// Only a box whose one row of text is the placeholder reads empty: text typed around words of the
/// same shape, or a row of text below them, is a draft.
#[test]
fn a_draft_shaped_like_a_placeholder_is_still_a_draft() {
    let claude = rendered("claude", AgentId::CLAUDE);
    let rule = "\u{2500}".repeat(40);
    let boxed = |rows: &str| format!("{rule}\n\u{276F} {rows}\n{rule}\n");
    for (rows, text) in [
        ("Try \"fix lint errors\"", ""),
        ("Try \"fix lint errors\" now", "Try \"fix lint errors\" now"),
        ("Try \"\"", "Try \"\""),
        (
            "Try \"fix lint errors\"\n  and then",
            "Try \"fix lint errors\"\nand then",
        ),
        ("Press up to edit queued messages", ""),
        (
            "Press up to edit queued messages!",
            "Press up to edit queued messages!",
        ),
    ] {
        assert_eq!(
            claude.input_box(&boxed(rows)).as_deref(),
            Some(text),
            "{rows:?}"
        );
    }
    let codex = rendered("codex", AgentId::CODEX);
    let composer = |rows: &str| format!("\u{203A} {rows}\n\n  100% context left\n");
    assert_eq!(
        codex
            .input_box(&composer("Ask Codex to do anything"))
            .as_deref(),
        Some("")
    );
    assert_eq!(
        codex
            .input_box(&composer("Ask Codex to do anything else"))
            .as_deref(),
        Some("Ask Codex to do anything else")
    );
}

/// What the captures of the inbox console host's repaint (`testdata/conhost-captures.md`) were
/// overwritten to: the folder Codex names in its titles.
const CONHOST_FOLDER: &str = "cebw-pbqrk-vaobk";

/// A session's titles read off the parsed screen of each capture of the inbox console host's
/// repaint, which holds the program's output from its start: the glyph and the running turn are
/// read as they are off the program's own titles. The title Codex's host sets from the
/// program's path, which comes after the first frame is drawn, is not one of them: the trust
/// screen, which has no other, leaves the session with none.
#[test]
fn the_titles_of_the_inbox_console_host_s_repaint_read_off_the_parsed_screen() {
    let name = "Greeting in two words";
    let codex_title = format!("Say hi | {CONHOST_FOLDER}");
    for (agent, capture, title, glyph, running) in [
        (
            AgentId::CLAUDE,
            &include_bytes!("../../testdata/conhost_claude_trust_screen.bin")[..],
            None,
            None,
            false,
        ),
        (
            AgentId::CLAUDE,
            include_bytes!("../../testdata/conhost_claude_input_placeholder.bin"),
            Some("Claude Code"),
            Some('\u{2733}'),
            false,
        ),
        (
            AgentId::CLAUDE,
            include_bytes!("../../testdata/conhost_claude_turn_running.bin"),
            Some("Claude Code"),
            Some('\u{25D1}'),
            true,
        ),
        (
            AgentId::CLAUDE,
            include_bytes!("../../testdata/conhost_claude_turn_finished.bin"),
            Some(name),
            Some('\u{2733}'),
            false,
        ),
        (
            AgentId::CLAUDE,
            include_bytes!("../../testdata/conhost_claude_cancel_running.bin"),
            Some(name),
            Some('\u{25D1}'),
            true,
        ),
        (
            AgentId::CLAUDE,
            include_bytes!("../../testdata/conhost_claude_input_restored.bin"),
            Some(name),
            Some('\u{2733}'),
            false,
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/conhost_codex_trust_screen.bin"),
            None,
            None,
            false,
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/conhost_codex_fresh_empty.bin"),
            Some(CONHOST_FOLDER),
            None,
            false,
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/conhost_codex_turn_running.bin"),
            Some(CONHOST_FOLDER),
            Some('\u{283C}'),
            true,
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/conhost_codex_working_title.bin"),
            Some(codex_title.as_str()),
            Some('\u{2834}'),
            true,
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/conhost_codex_interrupted.bin"),
            Some(codex_title.as_str()),
            None,
            false,
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/conhost_codex_turn_finished.bin"),
            Some(codex_title.as_str()),
            None,
            false,
        ),
    ] {
        let live = rendered("screen", agent);
        replay(&live, capture);
        let (latest, leading, _, shows_running) = read(&live, Instant::now());
        assert_eq!(
            latest.as_ref().map(|title| title.as_deref()),
            title.map(Some),
            "{agent:?} {title:?}"
        );
        assert_eq!(leading, glyph, "{agent:?} {title:?}");
        assert_eq!(shows_running, running, "{agent:?} {title:?}");
    }
}

/// The inbox console host's repaint of an input box, read as a host that renders what the program
/// draws reads it: the placeholder, drawn faint and kept faint by the host, is an empty box whether
/// the faint cells are blanked or not, and a draft or a restored prompt is its text.
#[test]
fn the_inbox_console_host_s_repaint_of_an_input_box_reads_off_the_parsed_screen() {
    let replayed = |bytes: &[u8]| {
        let mut screen = crate::terminal::snapshot::TerminalSnapshot::new(40, 120);
        screen.process(bytes);
        (screen.visible_text(), screen.visible_text_undimmed())
    };
    for (agent, capture, text) in [
        (
            AgentId::CLAUDE,
            &include_bytes!("../../testdata/conhost_claude_input_placeholder.bin")[..],
            "",
        ),
        (
            AgentId::CLAUDE,
            include_bytes!("../../testdata/conhost_claude_input_pasted.bin"),
            "say hi in two words",
        ),
        (
            AgentId::CLAUDE,
            include_bytes!("../../testdata/conhost_claude_input_restored.bin"),
            "write a 400 word story about a lighthouse keeper",
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/conhost_codex_fresh_empty.bin"),
            "",
        ),
        (
            AgentId::CODEX,
            include_bytes!("../../testdata/conhost_codex_fresh_draft.bin"),
            "say hi in two words",
        ),
    ] {
        let screen = rendered("screen", agent);
        let (plain, undimmed) = replayed(capture);
        // Codex's footer names its directory the Windows way, which only such a host reads.
        let read = (agent != AgentId::CODEX || windows_paths()).then_some(text);
        for shown in [&plain, &undimmed] {
            assert_eq!(
                screen.input_box(shown).as_deref(),
                read,
                "{agent:?} {text:?}"
            );
        }
    }
}

/// Whether this host's paths are Windows ones, where Codex's footer as the inbox console host's
/// captures show it is read.
fn windows_paths() -> bool {
    crate::platform::names::PATH_STYLE == crate::platform::names::PathStyle::Windows
}
