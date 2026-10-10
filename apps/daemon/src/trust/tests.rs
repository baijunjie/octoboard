use std::path::{Path, PathBuf};

use super::*;
use crate::protocol::{Console, Origin, ProjectSource, SessionStatus};
use crate::session::spawn_reader_thread;
use crate::store::LOCAL_HOST_ID;
use crate::test_support::{watched_live_session, ScratchDir, StandIn, PATIENCE};

// What Claude Code 2.1.289 actually printed, captured from a real PTY in a directory it had
// never seen; only the directory's name and the account's plan were replaced. The screen as
// first drawn, the redraw after one Down, and what replaced the screen after Enter.
const SCREEN: &[u8] = include_bytes!("../../testdata/claude_trust_screen.bin");
const AFTER_DOWN: &[u8] = include_bytes!("../../testdata/claude_trust_after_down.bin");
const DISMISSED: &[u8] = include_bytes!("../../testdata/claude_trust_dismissed.bin");

// What Codex 0.161.0 printed in a git repository it had not been told to trust, captured from a
// real PTY; only the user's name, the model's name and the probe's scratch directory were
// replaced, each by text of the same length. The confirmation as first drawn, over the loading
// frame before it (whose redraw left the path with characters missing), the redraw after one
// Down, what replaced it after Enter, and the near miss: the review Codex raises for new hooks
// when it is launched without the hook-trust bypass.
const CODEX_SCREEN: &[u8] = include_bytes!("../../testdata/codex_trust_screen.bin");
const CODEX_AFTER_DOWN: &[u8] = include_bytes!("../../testdata/codex_trust_after_down.bin");
const CODEX_DISMISSED: &[u8] = include_bytes!("../../testdata/codex_trust_dismissed.bin");
const CODEX_HOOK_REVIEW: &[u8] = include_bytes!("../../testdata/codex_hook_review.bin");

// What Grok Build 1.0.50 printed in a folder holding an `AGENTS.md` it had not been told to
// trust: the confirmation as first drawn, and 2 KiB of the logo animation it keeps drawing
// while it waits, cut at escape sequences; only the user's name and the probe's scratch directory
// were replaced, each by text of the same length.
const GROK_SCREEN: &[u8] = include_bytes!("../../testdata/grok_trust_screen.bin");
const GROK_ANIMATION: &[u8] = include_bytes!("../../testdata/grok_trust_animation.bin");

/// What an answered screen is sent: the Down, then the Enter.
fn answer_keys() -> Vec<u8> {
    [DOWN, ENTER].concat()
}

/// The stand-ins cannot tell a carriage return from a newline (see [`take`]), so the keys are
/// pinned here instead.
#[test]
fn the_keys_are_the_bytes_the_screen_expects() {
    assert_eq!(DOWN, b"\x1b[B");
    assert_eq!(ENTER, b"\r");
    assert_eq!(YES, b"y");
}

/// Each agent's screen is recognised in its own first drawing and nowhere else: not in what
/// follows a key, not in Codex's hook review, and not in another agent's screen.
#[test]
fn each_agents_screen_is_recognised_in_its_first_drawing_alone() {
    for (screen, output, shown) in [
        (&CLAUDE, SCREEN, true),
        (&CODEX, CODEX_SCREEN, true),
        (&GROK, GROK_SCREEN, true),
        (&CODEX, CODEX_AFTER_DOWN, false),
        (&CODEX, CODEX_DISMISSED, false),
        (&CODEX, CODEX_HOOK_REVIEW, false),
        (&CODEX, SCREEN, false),
        (&GROK, SCREEN, false),
        (&CLAUDE, CODEX_SCREEN, false),
        (&GROK, GROK_ANIMATION, false),
    ] {
        assert_eq!(
            screen.is_shown_in(output),
            shown,
            "{:?} in {} bytes",
            screen.first_draw,
            output.len()
        );
    }
}

/// After Octoboard's own Enter the screen is gone once readable text that is not the screen's
/// own is drawn — a later screen may still say "No, exit". After keys the person typed, Codex's
/// screen counts as replaced only once its next screen is drawn after the last drawing of its own.
/// The empty frame Codex 0.161.0 answers an ignored key with (measured) and the redraw after a
/// Down are neither.
#[test]
fn a_screen_is_gone_only_when_something_else_is_drawn_in_its_place() {
    const IGNORED_KEY: &[u8] = b"\x1b[?2026h\x1b[39m\x1b[49m\x1b[0m\x1b[?2026l";
    let other_text = &b"some other text"[..];
    let down_then_enter = [CODEX_AFTER_DOWN, CODEX_DISMISSED].concat();
    let drawn_again = [CODEX_DISMISSED, CODEX_AFTER_DOWN].concat();
    for (screen, output, gone, replaced) in [
        (&CODEX, CODEX_DISMISSED, true, true),
        (&CODEX, &down_then_enter[..], false, true),
        (&CODEX, &drawn_again[..], false, false),
        (
            &CODEX,
            &[CODEX_DISMISSED, b"2. Quit"].concat()[..],
            true,
            false,
        ),
        (&CODEX, other_text, true, false),
        (&CODEX, IGNORED_KEY, false, false),
        (&CODEX, CODEX_AFTER_DOWN, false, false),
        (&CLAUDE, DISMISSED, true, true),
        (
            &CLAUDE,
            &b"Bypass mode: No, exit or Yes, I accept"[..],
            true,
            true,
        ),
        (&CLAUDE, AFTER_DOWN, false, false),
    ] {
        assert_eq!(screen.gone_in(output), gone, "{} bytes", output.len());
        assert_eq!(
            screen.replaced_in(output),
            replaced,
            "{} bytes",
            output.len()
        );
    }
}

/// Codex's cursor starts on the option that trusts, and a redraw of only the changed cells
/// moves it; Grok's screen has no cursor to read.
#[test]
fn codexs_cursor_is_read_off_its_latest_redraw_and_grok_has_none() {
    assert_eq!(CODEX.cursor_on(CODEX_SCREEN), Some(Cursor::Accept));
    assert_eq!(CODEX.cursor_on(CODEX_AFTER_DOWN), Some(Cursor::Decline));
    assert_eq!(
        CODEX.cursor_on(&[CODEX_SCREEN, CODEX_AFTER_DOWN].concat()),
        Some(Cursor::Decline)
    );
    assert_eq!(GROK.cursor_on(GROK_SCREEN), None);
}

/// A fixture's path as a shell script names it, quoted.
fn fixture(name: &str) -> String {
    format!("'{}/testdata/{name}'", env!("CARGO_MANIFEST_DIR"))
}

/// A path as a shell script names it, quoted.
fn quoted(path: &Path) -> String {
    format!("'{}'", path.display())
}

/// The start of a stand-in script: reads the three captures into `$SCREEN`, `$AFTER_DOWN` and
/// `$DISMISSED`, so that the rest of the conversation can use shell builtins alone. Every
/// exec of the stand-in's shell is a chance for endpoint-security software to stall it for
/// seconds, and a stall in the middle of the conversation lands inside `answer`'s own
/// timeouts, which a test cannot stretch; a stall here only delays the start, which the
/// tests' waits absorb. `read -d ''` takes a whole file (the captures hold no NUL), keeping its
/// final newlines.
fn load_captures() -> String {
    load(&[
        ("SCREEN", "claude_trust_screen.bin"),
        ("AFTER_DOWN", "claude_trust_after_down.bin"),
        ("DISMISSED", "claude_trust_dismissed.bin"),
    ])
}

/// [`load_captures`] for any captures, each into the variable named beside it.
fn load(captures: &[(&str, &str)]) -> String {
    captures
        .iter()
        .map(|(var, file)| format!("IFS= read -r -d '' {var} < {}", fixture(file)))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Codex's whole conversation: its screen, a read of the Enter, what replaces the screen, and
/// one more read, so that anything sent after the Enter shows up in what was received.
fn codex_script(received: &Path) -> String {
    codex_script_reading(received, 1)
}

/// [`codex_script`] reading `after` bytes once the screen is gone.
fn codex_script_reading(received: &Path, after: usize) -> String {
    let r = quoted(received);
    [
        load(&[
            ("SCREEN", "codex_trust_screen.bin"),
            ("DISMISSED", "codex_trust_dismissed.bin"),
        ]),
        show("SCREEN"),
        take(1, &r),
        show("DISMISSED"),
        take(after, &r),
        "sleep 30".to_string(),
    ]
    .join("; ")
}

/// Prints one of the captures [`load_captures`] read.
fn show(var: &str) -> String {
    format!("printf '%s' \"${var}\"")
}

/// Reads `count` bytes of what the stand-in is sent and appends them to `received`. `read -n`
/// is the builtin that takes bytes rather than a line; it is bash's, which is what the stand-in
/// runs under. While it reads, bash turns the terminal's `ICRNL`, `ISIG` and `IEXTEN` on,
/// so a carriage return arrives as a newline and the two cannot be told apart: the newline
/// is turned back into a `\r`, and that `DOWN` and `ENTER` are the bytes they should be is
/// pinned by `the_keys_are_the_bytes_the_screen_expects`. For the same reason the tests must
/// send no control character the terminal would act on (^C, ^Z, ^Y, ^T, ^V, ^O).
fn take(count: usize, received: &str) -> String {
    format!(
        "IFS= read -r -n {count} -d '' key; key=${{key//$'\\n'/$'\\r'}}; \
         printf '%s' \"$key\" >> {received}"
    )
}

#[test]
fn the_screen_is_recognised_although_the_raw_bytes_never_spell_its_words() {
    let raw = String::from_utf8_lossy(SCREEN);
    assert!(
        !raw.contains("trust this folder"),
        "words are cursor-positioned"
    );
    assert!(CLAUDE.is_shown_in(SCREEN));
    assert_eq!(CLAUDE.cursor_on(SCREEN), Some(Cursor::Decline));
}

#[test]
fn a_redraw_after_the_cursor_moves_reads_as_the_cursor_on_the_other_option() {
    assert_eq!(CLAUDE.cursor_on(AFTER_DOWN), Some(Cursor::Accept));
    // And the cursor of the whole picture is the latest one drawn, not the first.
    let both = [SCREEN, AFTER_DOWN].concat();
    assert_eq!(CLAUDE.cursor_on(&both), Some(Cursor::Accept));
}

#[test]
fn what_replaces_the_screen_is_not_the_screen() {
    assert!(!CLAUDE.is_shown_in(DISMISSED));
    assert!(CLAUDE.gone_in(DISMISSED));
    assert_eq!(CLAUDE.cursor_on(DISMISSED), None);
}

#[test]
fn quoted_prose_is_not_the_screen() {
    assert!(!CLAUDE.is_shown_in(b"the option is called \"No, exit\" and nothing else"));
    assert!(!CLAUDE.is_shown_in(b"Yes, I trust this folder"));
    assert!(!CLAUDE.is_shown_in(b"Claude Code asks: No, exit, or Yes, I trust this folder?"));
    // Both options in a row, but no cursor in front of the first.
    assert!(!CLAUDE.is_shown_in(b"No, exit\r\nYes, I trust this folder"));
    // The redraw after a Down is not a first draw either.
    assert!(!CLAUDE.is_shown_in(AFTER_DOWN));
}

/// A transcript can quote both options next to a cursor glyph, as a model printing a prompt
/// would; neither a numbered list nor words between them is the screen's first draw.
#[test]
fn a_transcript_quoting_both_options_beside_a_cursor_glyph_is_not_the_screen() {
    let numbered = "\u{276F} 1. No, exit\r\n  2. Yes, I trust this folder\r\n";
    assert!(!CLAUDE.is_shown_in(numbered.as_bytes()));
    let sentence = "\u{276F} No, exit or maybe Yes, I trust this folder";
    assert!(!CLAUDE.is_shown_in(sentence.as_bytes()));
    let reversed = "\u{276F} Yes, I trust this folder\r\n  No, exit\r\n";
    assert!(!CLAUDE.is_shown_in(reversed.as_bytes()));
}

#[test]
fn the_watch_ends_after_too_much_output_or_too_long() {
    let state = TrustState::new(Agent::Claude, None);
    let mut sightings = state.take_sightings().expect("receiver");
    state.feed(&vec![b'.'; WATCH_BYTES + 1], 0, 0);
    state.feed(SCREEN, 0, 0);
    assert!(sightings.try_recv().is_err(), "too late by output");
    assert!(!state.waiting());
    assert_eq!(
        state.window.lock().unwrap().window.capacity(),
        0,
        "the window is released"
    );

    let state = TrustState::with_limits(Agent::Claude, None, WATCH_BYTES, Duration::ZERO);
    let mut sightings = state.take_sightings().expect("receiver");
    std::thread::sleep(Duration::from_millis(5));
    state.feed(SCREEN, 0, 0);
    assert!(sightings.try_recv().is_err(), "too late by time");
}

#[test]
fn a_hook_releases_the_window() {
    let state = TrustState::new(Agent::Claude, None);
    state.feed(b"some output", 0, 0);
    state.note_hook();
    assert_eq!(state.window.lock().unwrap().window.capacity(), 0);
}

#[test]
fn the_event_is_named_trust_prompt_on_the_wire_and_names_the_agent() {
    let json = serde_json::to_value(Event::TrustPrompt {
        session: "s".into(),
        agent: Agent::Codex,
        project: "p".into(),
        path: "/x/y".into(),
        trust_dir: Some("/x".into()),
    })
    .unwrap();
    assert_eq!(json["type"], "trust_prompt");
    assert_eq!(json["agent"], "codex");
    assert_eq!(json["path"], "/x/y");
    assert_eq!(json["trust_dir"], "/x");
}

/// The screen arrives in however many reads the kernel makes of it, cut anywhere — inside an
/// escape sequence, inside a multi-byte character, between the pieces it is matched by.
#[test]
fn the_screen_is_sighted_once_however_it_is_cut_into_reads() {
    for (agent, screen, after) in [
        (Agent::Claude, SCREEN, AFTER_DOWN),
        (Agent::Codex, CODEX_SCREEN, CODEX_AFTER_DOWN),
        (Agent::Grok, GROK_SCREEN, GROK_ANIMATION),
    ] {
        for size in [1, 3, 7, 200, screen.len()] {
            let state = TrustState::new(agent, None);
            let mut sightings = state.take_sightings().expect("receiver");
            for chunk in screen.chunks(size) {
                state.feed(chunk, 0, 0);
            }
            // The redraws that follow, as a resize or a keypress would produce.
            state.feed(after, 0, 0);
            state.feed(screen, 0, 0);

            assert!(sightings.try_recv().is_ok(), "{agent:?}, reads of {size}");
            assert!(
                sightings.try_recv().is_err(),
                "{agent:?} sighted only once with reads of {size}"
            );
        }
    }
}

#[test]
fn nothing_is_sighted_before_the_screen_is_complete() {
    let state = TrustState::new(Agent::Claude, None);
    let mut sightings = state.take_sightings().expect("receiver");
    // Everything up to, and including, the first option.
    let cut = SCREEN
        .windows(4)
        .position(|window| window == b"exit")
        .expect("the first option");
    state.feed(&SCREEN[..cut], 0, 0);
    assert!(sightings.try_recv().is_err());
    state.feed(&SCREEN[cut..], 0, 0);
    assert!(sightings.try_recv().is_ok());
}

#[test]
fn each_agent_is_watched_for_its_own_screen_and_a_hook_ends_the_watch() {
    for agent in [Agent::Codex, Agent::Grok] {
        let state = TrustState::new(agent, None);
        let mut sightings = state.take_sightings().expect("receiver");
        state.feed(SCREEN, 0, 0);
        assert!(
            sightings.try_recv().is_err(),
            "{agent:?} does not show Claude Code's screen"
        );
        assert!(!state.claim_answer(), "{agent:?} is not answered for it");
    }

    let state = TrustState::new(Agent::Claude, None);
    let mut sightings = state.take_sightings().expect("receiver");
    state.feed(DISMISSED, 0, 0);
    state.note_hook();
    // Quoted by a session that is already running normally.
    state.feed(SCREEN, 0, 0);
    assert!(sightings.try_recv().is_err());
    assert!(!state.waiting());
}

#[test]
fn a_session_is_answered_at_most_once_and_only_after_the_screen_was_sighted() {
    let state = TrustState::new(Agent::Claude, None);
    assert!(!state.claim_answer(), "nothing sighted yet");
    state.feed(SCREEN, 0, 0);
    assert!(state.waiting());
    assert!(state.claim_answer());
    assert!(!state.waiting());
    assert!(!state.claim_answer());
}

fn console() -> Console {
    Console {
        id: "console-1".to_string(),
        name: "Console".to_string(),
        workdir: "/tmp/console-1".to_string(),
        console_session_agent: Agent::Claude,
        default_agent: Agent::Claude,
        claude_account_id: None,
        codex_account_id: None,
        grok_account_id: None,
        icon: None,
        created_at: 0,
    }
}

fn project(consent: bool) -> Project {
    Project {
        id: "project-1".to_string(),
        console_id: "console-1".to_string(),
        host_id: LOCAL_HOST_ID.to_string(),
        name: "Project".to_string(),
        path: "/work/project".to_string(),
        default_agent: None,
        source: ProjectSource::Local,
        remote_url: None,
        trust_consent: consent,
        pinned: false,
        tags: Vec::new(),
    }
}

fn session(id: &str, agent: Agent, role: Role, project_id: Option<&str>) -> Session {
    Session {
        id: id.to_string(),
        agent,
        agent_session_id: None,
        console_id: "console-1".to_string(),
        project_id: project_id.map(str::to_string),
        host_id: LOCAL_HOST_ID.to_string(),
        role,
        origin: Origin::User,
        title: "Session".to_string(),
        status: SessionStatus::Idle,
        has_conversation: false,
        bound_to: None,
        colour: None,
        ordinal: None,
        account_id: None,
        config_dir: None,
        pinned: false,
        started_at: 0,
        ended_at: None,
    }
}

#[test]
fn a_console_session_is_always_answered_and_a_project_only_with_the_users_consent() {
    let console_session = session("s", Agent::Claude, Role::Console, None);
    let project_session = session("s", Agent::Claude, Role::Project, Some("project-1"));
    let none: &[String] = &[];
    let work = ["/work".to_string()];
    assert!(consented(&console_session, None, none));
    assert!(consented(&project_session, Some(&project(true)), none));
    assert!(!consented(&project_session, Some(&project(false)), none));
    assert!(!consented(&project_session, None, none));
    // A trusted directory covers the project under it, one with no consent of its own included.
    assert!(consented(&project_session, Some(&project(false)), &work));
    assert!(!consented(
        &project_session,
        Some(&project(false)),
        &["/elsewhere".to_string()]
    ));
    assert!(!consented(&project_session, None, &work));
}

fn scratch(name: &str) -> ScratchDir {
    ScratchDir::new(&format!("trust-{name}"))
}

/// A stand-in for an agent at its trust screen, on a real PTY with its trust watch running:
/// prints what the script says to and records every byte it is sent, so a test sees exactly
/// what reached the terminal.
fn stand_in(id: &str, agent: Agent, script: &str) -> StandIn {
    // Raw first, so the fixtures reach the terminal as captured and the keys are read as sent.
    let live = watched_live_session(
        id,
        agent,
        120,
        32,
        &format!("stty raw -echo; {script}"),
        None,
    );
    spawn_reader_thread(live.clone(), 8 * 1024);
    live
}

/// Every byte the stand-in has been sent so far. The file exists, empty, as soon as the stand-in
/// starts waiting for input.
fn sent(received: &Path) -> Vec<u8> {
    std::fs::read(received).unwrap_or_default()
}

/// The stand-in's whole conversation: the screen, a read of the Down, the redraw, a read of the
/// Enter, the main screen, and then one more read, so that anything sent after the Enter shows
/// up in what was received.
fn full_script(received: &Path) -> String {
    conversation(received, 0)
}

/// [`full_script`] with a read of `held` bytes between the Down and the redraw. The stand-in
/// does not redraw until it has them, so a test that writes them once it sees the Down arrive
/// puts them between the Down and the Enter: the cursor cannot move, and the Enter cannot
/// follow, before they are in. The test still has to write them within `MOVE_TIMEOUT`, after
/// which the answer gives up waiting for the cursor.
fn conversation(received: &Path, held: usize) -> String {
    let r = quoted(received);
    let held_read = if held == 0 {
        ":".to_string()
    } else {
        take(held, &r)
    };
    [
        load_captures(),
        show("SCREEN"),
        take(3, &r),
        held_read,
        show("AFTER_DOWN"),
        take(1, &r),
        show("DISMISSED"),
        take(1, &r),
        "sleep 30".to_string(),
    ]
    .join("; ")
}

#[test]
fn answering_sends_a_down_and_then_an_enter_and_nothing_else() {
    let dir = scratch("answer");
    let received = dir.join("received");
    let live = stand_in("s", Agent::Claude, &full_script(&received));
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    answer(&live).expect("answered");

    // The stand-in is still reading, so a stray byte after the Enter would land in the file.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(sent(&received), answer_keys());
}

/// With the cursor already on "Yes", a Down would wrap to "No, exit" and the Enter after it
/// would quit the session. Nothing is written at all.
#[test]
fn nothing_is_sent_when_the_cursor_is_not_on_the_first_option() {
    let dir = scratch("cursor-on-yes");
    let received = dir.join("received");
    let script = [
        load_captures(),
        show("SCREEN"),
        show("AFTER_DOWN"),
        take(1, &quoted(&received)),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let live = stand_in("s", Agent::Claude, &script);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    let err = answer(&live).expect_err("refused");
    assert!(err.to_string().contains("starts on"), "{err}");

    assert!(sent(&received).is_empty(), "no key may have been sent");
}

/// The Down goes in and the cursor does not move, so the Enter that would follow is withheld.
#[test]
fn enter_is_withheld_when_the_cursor_does_not_move() {
    let dir = scratch("no-move");
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
    let live = stand_in("s", Agent::Claude, &script);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    let err = answer(&live).expect_err("not confirmed");
    assert!(err.to_string().contains("did not move"), "{err}");
    let coded = err.downcast_ref::<CodedError>().expect("a coded error");
    assert_eq!(coded.code, error_code::TRUST_ANSWER_FAILED);
    assert_eq!(
        coded.params["reason_code"],
        trust_reason::CURSOR_DID_NOT_MOVE
    );

    assert_eq!(sent(&received), DOWN);
}

/// An attached terminal reports focus and answers the agent's queries on its own, which says
/// nothing about the cursor: the answer still completes.
#[test]
fn a_terminals_own_traffic_does_not_stop_the_answer() {
    let dir = scratch("terminal-traffic");
    let received = dir.join("received");
    let chatter: &[u8] = b"\x1b[I\x1b[O\x1b[?1;2c";
    // The stand-in holds its redraw until the chatter is in.
    let script = conversation(&received, chatter.len());
    let live = stand_in("s", Agent::Claude, &script);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    let answering = {
        let live = live.clone();
        std::thread::spawn(move || answer(&live))
    };
    // After the Down, before the Enter: where a counted write would stop the answer.
    assert!(wait_for(PATIENCE, || sent(&received) == DOWN));
    live.write_input(chatter).expect("the terminal's traffic");
    answering.join().unwrap().expect("answered");

    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(sent(&received), [DOWN, chatter, ENTER].concat());
}

/// The count of foreign writes and the refusal that rests on it: a terminal's own traffic is not
/// counted, a typed byte is, and a key offered against a stale count is refused and never
/// reaches the terminal.
#[test]
fn a_key_is_written_only_while_nothing_else_has_written() {
    let dir = scratch("if-untouched");
    let received = dir.join("received");
    let script = format!("printf ready; {}; sleep 30", take(5, &quoted(&received)));
    let live = stand_in("s", Agent::Claude, &script);
    // Input written before the stand-in has set its terminal up would be thrown away.
    assert!(wait_for(PATIENCE, || live
        .recent_output(64)
        .starts_with(b"ready")));

    let before = live.input_writes();
    live.write_input(b"\x1b[I").expect("focus report");
    assert_eq!(
        live.input_writes(),
        before,
        "protocol traffic is not counted"
    );
    live.write_input(b"x").expect("typed");
    assert_eq!(live.input_writes(), before + 1);
    assert!(!live.write_input_if_untouched(before, b"Z").unwrap());
    assert!(live
        .write_input_if_untouched(live.input_writes(), b"\r")
        .unwrap());

    // The focus report, the typed byte and the key — not the key that was refused.
    assert!(wait_for(PATIENCE, || sent(&received).len() >= 5));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(sent(&received), b"\x1b[Ix\r");
}

/// Someone else types into the session after the Down went in — the user pressing a key in the
/// terminal, a message leaving the queue — so the cursor may no longer be where the Enter would
/// assume. The Enter is not sent.
#[test]
fn enter_is_withheld_when_something_else_writes_into_the_session_meanwhile() {
    let dir = scratch("foreign-write");
    let received = dir.join("received");
    // The stand-in holds its redraw until the stray byte is in.
    let script = conversation(&received, 1);
    let live = stand_in("s", Agent::Claude, &script);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    let answering = {
        let live = live.clone();
        std::thread::spawn(move || answer(&live))
    };
    assert!(wait_for(PATIENCE, || sent(&received) == DOWN));
    live.write_input(b"x").expect("the foreign write");

    let err = answering.join().unwrap().expect_err("withheld");
    assert!(err.to_string().contains("something else wrote"), "{err}");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(sent(&received), [DOWN, b"x"].concat(), "no Enter was sent");
}

/// A session that never showed its screen, or showed another agent's, is never typed at.
#[test]
fn nothing_is_sent_to_a_session_that_never_showed_its_own_screen() {
    let dir = scratch("never");
    let received = dir.join("received");
    let script = format!("echo hello; {}; sleep 30", take(1, &quoted(&received)));
    let claude = stand_in("a", Agent::Claude, &script);
    assert!(answer(&claude).is_err());

    let codex = stand_in("b", Agent::Codex, &script);
    codex.trust.feed(SCREEN, 0, 0);
    assert!(answer(&codex).is_err());

    std::thread::sleep(Duration::from_millis(300));
    assert!(sent(&received).is_empty());
}

/// A stand-in that shows the screen and takes the Down but never redraws, so the cursor never
/// moves. For [`start`], which fills in where it records what it is sent.
fn never_moves_script() -> String {
    [
        load_captures(),
        show("SCREEN"),
        take(3, "'%RECEIVED%'"),
        "sleep 30".to_string(),
    ]
    .join("; ")
}

fn state_with_project(name: &str) -> (Arc<AppState>, ScratchDir) {
    let (state, dir) = crate::test_support::app_state(&format!("trust-{name}"));
    state.store.insert_console(&console()).expect("console");
    state
        .store
        .insert_project(&project(false))
        .expect("project");
    (state, dir)
}

/// Starts a stand-in session of the given kind under a stored record and registers it, as a
/// launch would; the caller decides whether it is supervised.
fn start(state: &Arc<AppState>, dir: &Path, record: Session, script: &str) -> (StandIn, PathBuf) {
    let received = dir.join(format!("received-{}", record.id));
    state.store.insert_session(&record).expect("session");
    let live = stand_in(
        &record.id,
        record.agent,
        &script.replace("%RECEIVED%", &received.to_string_lossy()),
    );
    state.register_live(live.clone());
    (live, received)
}

/// Waits until the stand-in has printed the trust screen. Feeding the screen to `live.trust`
/// only makes the session look like it is waiting; `answer` reads the screen from what the
/// stand-in printed, and starts looking `SETTLE` after it is called, which a stand-in whose
/// shell is slow to start can miss.
async fn screen_printed(live: &StandIn) {
    let seen = Arc::clone(live);
    assert!(
        tokio::task::spawn_blocking(move || wait_for(PATIENCE, || {
            seen.trust.screen.is_shown_in(&seen.recent_output(WINDOW))
        }))
        .await
        .unwrap(),
        "the stand-in never printed the trust screen"
    );
}

/// Waits until the stand-in has been sent the Down and the Enter and has printed the screen
/// that replaces the trust screen. A test that ended at the Enter would drop the stand-in
/// while an answer running in the background was still waiting for that screen, and the
/// runtime would then wait out the answer's `DISMISS_TIMEOUT`.
async fn answered(live: &StandIn, received: &Path) {
    let seen = Arc::clone(live);
    let received = received.to_path_buf();
    assert!(
        tokio::task::spawn_blocking(move || wait_for(PATIENCE, || {
            sent(&received) == answer_keys() && seen.recent_output(DISMISSED.len()) == DISMISSED
        }))
        .await
        .unwrap(),
        "the keys must have been sent and the screen dismissed"
    );
}

#[tokio::test]
async fn a_console_sessions_screen_is_answered_without_asking() {
    let (state, dir) = state_with_project("console-session");
    let mut events = state.subscribe();
    let record = session("console-session-1", Agent::Claude, Role::Console, None);
    let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
    supervise(&state, &live);

    answered(&live, &received).await;
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, Event::TrustPrompt { .. }),
            "a console session is not asked about"
        );
    }
}

#[tokio::test]
async fn a_project_without_consent_is_asked_about_and_nothing_is_sent_until_confirmed() {
    let (state, dir) = state_with_project("ask");
    let mut events = state.subscribe();
    let record = session(
        "project-session-1",
        Agent::Claude,
        Role::Project,
        Some("project-1"),
    );
    let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
    supervise(&state, &live);

    let prompt = tokio::time::timeout(PATIENCE, async {
        loop {
            if let Event::TrustPrompt {
                session,
                project,
                path,
                ..
            } = events.recv().await.expect("event")
            {
                return (session, project, path);
            }
        }
    })
    .await
    .expect("the prompt");
    assert_eq!(
        prompt,
        (
            "project-session-1".to_string(),
            "project-1".to_string(),
            "/work/project".to_string()
        )
    );
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        sent(&received).is_empty(),
        "nothing is sent before the user agrees"
    );

    confirm(&state, "project-session-1", true, false)
        .await
        .expect("confirmed");

    assert_eq!(sent(&received), answer_keys());
    assert!(
        state
            .store
            .get_project("project-1")
            .unwrap()
            .unwrap()
            .trust_consent
    );
    let mut upserted = false;
    while let Ok(event) = events.try_recv() {
        upserted |= matches!(event, Event::ProjectUpserted { project } if project.trust_consent);
    }
    assert!(upserted, "clients are told the project is now consented");

    // Answered once: a second confirmation of the same screen is stale.
    assert!(confirm(&state, "project-session-1", true, false)
        .await
        .is_err());
}

#[tokio::test]
async fn a_consented_project_is_answered_without_a_prompt() {
    let (state, dir) = state_with_project("consented");
    state
        .store
        .set_project_trust_consent("project-1", true)
        .expect("consent");
    let mut events = state.subscribe();
    let record = session(
        "project-session-1",
        Agent::Claude,
        Role::Project,
        Some("project-1"),
    );
    let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
    supervise(&state, &live);

    answered(&live, &received).await;
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, Event::TrustPrompt { .. }));
    }
}

/// A go-ahead that names a session which is not a running project session at its own screen
/// records no consent and sends nothing.
#[tokio::test]
async fn a_confirmation_for_a_stale_or_foreign_session_changes_nothing() {
    let (state, dir) = state_with_project("refused");

    // Unknown.
    assert!(confirm(&state, "nobody", true, false).await.is_err());

    // Holding another agent's screen, which is not its own.
    let codex = session("codex-1", Agent::Codex, Role::Project, Some("project-1"));
    let (codex_live, codex_received) = start(&state, &dir, codex, "sleep 30");
    codex_live.trust.feed(SCREEN, 0, 0);
    let err = confirm(&state, "codex-1", true, false)
        .await
        .expect_err("refused");
    assert!(err.to_string().contains("any more"), "{err}");

    // Claude Code's, but not running.
    let gone = session("gone-1", Agent::Claude, Role::Project, Some("project-1"));
    state.store.insert_session(&gone).expect("session");
    let err = confirm(&state, "gone-1", true, false)
        .await
        .expect_err("refused");
    assert!(err.to_string().contains("not running"), "{err}");

    // Running, but its screen is not up.
    let quiet = session("quiet-1", Agent::Claude, Role::Project, Some("project-1"));
    let (_quiet_live, quiet_received) = start(&state, &dir, quiet, "sleep 30");
    let err = confirm(&state, "quiet-1", true, false)
        .await
        .expect_err("refused");
    assert!(err.to_string().contains("any more"), "{err}");

    // A console session's screen is not the user's to confirm.
    let console_session = session("console-session-1", Agent::Claude, Role::Console, None);
    let (console_session_live, _) = start(&state, &dir, console_session, "sleep 30");
    console_session_live.trust.feed(SCREEN, 0, 0);
    let err = confirm(&state, "console-session-1", true, false)
        .await
        .expect_err("refused");
    assert!(err.to_string().contains("console session"), "{err}");

    assert!(
        !state
            .store
            .get_project("project-1")
            .unwrap()
            .unwrap()
            .trust_consent
    );
    assert!(sent(&codex_received).is_empty() && sent(&quiet_received).is_empty());
}

fn consented_in_store(state: &AppState) -> bool {
    state
        .store
        .get_project("project-1")
        .unwrap()
        .unwrap()
        .trust_consent
}

/// A go-ahead without `remember` answers the screen and records nothing.
#[tokio::test]
async fn without_remember_the_screen_is_answered_and_no_consent_is_recorded() {
    let (state, dir) = state_with_project("no-remember");
    let record = session(
        "project-session-1",
        Agent::Claude,
        Role::Project,
        Some("project-1"),
    );
    let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
    live.trust.feed(SCREEN, 0, 0);
    screen_printed(&live).await;

    confirm(&state, "project-session-1", false, false)
        .await
        .expect("confirmed");
    assert_eq!(sent(&received), answer_keys());
    assert!(!consented_in_store(&state));
}

/// Consent follows the answer: a screen that could not be answered leaves nothing recorded, and
/// the user is told, whether or not the dialog they confirmed in is still open.
#[tokio::test]
async fn a_failed_answer_records_no_consent_and_tells_the_user() {
    let (state, dir) = state_with_project("failed-confirm");
    let mut events = state.subscribe();
    // The stand-in never moves its cursor.
    let script = never_moves_script();
    let record = session(
        "project-session-1",
        Agent::Claude,
        Role::Project,
        Some("project-1"),
    );
    let (live, _) = start(&state, &dir, record, &script);
    live.trust.feed(SCREEN, 0, 0);
    screen_printed(&live).await;

    let err = confirm(&state, "project-session-1", true, false)
        .await
        .expect_err("failed");
    assert!(err.to_string().contains("did not move"), "{err}");
    assert!(!consented_in_store(&state));
    let mut told = false;
    while let Ok(event) = events.try_recv() {
        told |= matches!(event, Event::SessionNotice { session, message, .. }
            if session == "project-session-1" && message.contains("Answer it in the terminal"));
    }
    assert!(told, "a notice, for a dialog that may be closed");
}

/// An automatic answer that fails is not silent either.
#[tokio::test]
async fn an_automatic_answer_that_fails_leaves_a_notice() {
    let (state, dir) = state_with_project("failed-auto");
    let mut events = state.subscribe();
    let script = never_moves_script();
    let record = session("console-session-1", Agent::Claude, Role::Console, None);
    let (live, _) = start(&state, &dir, record, &script);
    supervise(&state, &live);

    let notice = tokio::time::timeout(PATIENCE, async {
        loop {
            if let Event::SessionNotice {
                session, message, ..
            } = events.recv().await.expect("event")
            {
                return (session, message);
            }
        }
    })
    .await
    .expect("the notice");
    assert_eq!(notice.0, "console-session-1");
    assert!(
        notice.1.contains("Answer it in the terminal"),
        "{}",
        notice.1
    );
}

/// A client that was not there when the prompt was broadcast, or whose snapshot replaced it, is
/// told again about exactly the screens still waiting for a go-ahead.
#[tokio::test]
async fn a_snapshot_is_followed_by_the_prompts_still_waiting() {
    let (state, dir) = state_with_project("replay");
    let waiting = session("waiting-1", Agent::Claude, Role::Project, Some("project-1"));
    let (waiting_live, _) = start(&state, &dir, waiting, "sleep 30");
    let console_session = session("console-session-1", Agent::Claude, Role::Console, None);
    let (console_session_live, _) = start(&state, &dir, console_session, "sleep 30");
    let quiet = session("quiet-1", Agent::Claude, Role::Project, Some("project-1"));
    let (_quiet_live, _) = start(&state, &dir, quiet, "sleep 30");
    let answered = session(
        "answered-1",
        Agent::Claude,
        Role::Project,
        Some("project-1"),
    );
    let (answered_live, _) = start(&state, &dir, answered, "sleep 30");
    waiting_live.trust.feed(SCREEN, 0, 0);
    console_session_live.trust.feed(SCREEN, 0, 0);
    answered_live.trust.feed(SCREEN, 0, 0);
    assert!(answered_live.trust.claim_answer());

    let prompts = pending_prompts(&state);
    assert_eq!(prompts.len(), 1, "{prompts:?}");
    assert!(
        matches!(&prompts[0], Event::TrustPrompt { session, project, path, trust_dir, .. }
        if session == "waiting-1" && project == "project-1" && path == "/work/project"
            && trust_dir.as_deref() == Some("/work"))
    );

    // Once the project has consented the screen is answered, not asked about.
    state
        .store
        .set_project_trust_consent("project-1", true)
        .expect("consent");
    assert!(pending_prompts(&state).is_empty());
}

fn trusted_in_store(state: &AppState) -> Vec<String> {
    state.store.trusted_directories().unwrap()
}

/// Paths are compared component by component, after a lexical clean-up, and a trusted directory
/// covers itself and everything below it.
#[test]
fn a_trusted_directory_covers_itself_and_what_is_below_it_and_nothing_else() {
    let work = ["/work".to_string()];
    assert!(under_a_trusted_directory("/work", &work), "itself");
    assert!(under_a_trusted_directory("/work/project", &work), "a child");
    assert!(
        under_a_trusted_directory("/work/a/b/c", &work),
        "a descendant"
    );
    assert!(
        under_a_trusted_directory("/work/project/", &work),
        "a trailing slash"
    );
    assert!(
        under_a_trusted_directory("/work/./a/../project", &work),
        "lexical clean-up"
    );
    assert!(under_a_trusted_directory(
        "/work/project",
        &["/work/".to_string()]
    ));
    // Not a string prefix.
    assert!(!under_a_trusted_directory("/workspace/project", &work));
    assert!(!under_a_trusted_directory("/work2", &work));
    assert!(
        !under_a_trusted_directory("/Work/project", &work),
        "case differs"
    );
    // Not above it, not beside it, not escaping it.
    assert!(!under_a_trusted_directory("/", &work));
    assert!(!under_a_trusted_directory("/work/../other", &work));
    assert!(
        !under_a_trusted_directory("work/project", &work),
        "relative"
    );
    assert!(!under_a_trusted_directory("/work/project", &[]));
}

#[test]
fn the_parent_to_trust_is_never_the_root_the_home_directory_or_above_it() {
    let home = Path::new("/Users/me");
    assert_eq!(
        trustable_parent("/Users/me/Coding/app", home).unwrap(),
        Path::new("/Users/me/Coding")
    );
    assert_eq!(
        trustable_parent("/Users/me/Coding/app/", home).unwrap(),
        Path::new("/Users/me/Coding"),
        "a trailing slash"
    );
    // Directly in the home directory, in the root, and the root itself.
    for project in ["/Users/me/app", "/app", "/", "/Users/app"] {
        let err = trustable_parent(project, home).expect_err(project);
        let coded = err.downcast_ref::<CodedError>().expect("a coded error");
        assert_eq!(
            coded.code,
            error_code::TRUST_DIRECTORY_TOO_BROAD,
            "{project}"
        );
        assert!(err.to_string().contains("too broad"), "{err}");
    }
    assert!(trustable_parent("relative/app", home).is_err());
}

/// "Trust parent folder" answers this screen and, once that worked, records the
/// parent directory — and leaves the project's own consent unset.
#[tokio::test]
async fn confirming_for_the_parent_directory_records_it_after_the_answer() {
    let (state, dir) = state_with_project("confirm-parent");
    let mut events = state.subscribe();
    let record = session(
        "project-session-1",
        Agent::Claude,
        Role::Project,
        Some("project-1"),
    );
    let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
    live.trust.feed(SCREEN, 0, 0);
    screen_printed(&live).await;

    confirm(&state, "project-session-1", false, true)
        .await
        .expect("confirmed");
    assert_eq!(sent(&received), answer_keys());
    assert_eq!(trusted_in_store(&state), ["/work"]);
    assert!(
        !consented_in_store(&state),
        "the project's own consent is separate"
    );
    let mut told = false;
    while let Ok(event) = events.try_recv() {
        told |= matches!(event, Event::TrustedDirectoriesUpdated { trusted_directories }
            if trusted_directories == ["/work"]);
    }
    assert!(told, "clients are told");
}

#[tokio::test]
async fn nothing_is_trusted_when_the_answer_fails_or_is_refused() {
    let (state, dir) = state_with_project("confirm-parent-refused");
    // The stand-in never moves its cursor.
    let script = never_moves_script();
    let record = session(
        "project-session-1",
        Agent::Claude,
        Role::Project,
        Some("project-1"),
    );
    let (live, _) = start(&state, &dir, record, &script);
    live.trust.feed(SCREEN, 0, 0);
    screen_printed(&live).await;
    assert!(confirm(&state, "project-session-1", false, true)
        .await
        .is_err());
    assert!(trusted_in_store(&state).is_empty());

    // The same refusals as for one project: unknown, not at its own screen, not running, a
    // console session's.
    let codex = session("codex-1", Agent::Codex, Role::Project, Some("project-1"));
    let (_codex_live, _) = start(&state, &dir, codex, "sleep 30");
    let console_session = session("console-session-1", Agent::Claude, Role::Console, None);
    let (console_session_live, _) = start(&state, &dir, console_session, "sleep 30");
    console_session_live.trust.feed(SCREEN, 0, 0);
    let gone = session("gone-1", Agent::Claude, Role::Project, Some("project-1"));
    state.store.insert_session(&gone).expect("session");
    for id in ["nobody", "codex-1", "console-session-1", "gone-1"] {
        assert!(confirm(&state, id, true, true).await.is_err(), "{id}");
    }
    assert!(trusted_in_store(&state).is_empty());
}

/// A project directly under the home directory has nothing narrower to trust, so the request is
/// refused before anything is answered or sent.
#[tokio::test]
async fn a_parent_that_is_the_home_directory_is_refused_before_the_screen_is_answered() {
    let (state, dir) = state_with_project("confirm-home");
    let home = paths::home_dir();
    let mut broad = project(false);
    broad.id = "broad".to_string();
    broad.path = home.join("app").to_string_lossy().into_owned();
    state.store.insert_project(&broad).expect("project");
    let record = session(
        "project-session-1",
        Agent::Claude,
        Role::Project,
        Some("broad"),
    );
    let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
    live.trust.feed(SCREEN, 0, 0);
    screen_printed(&live).await;

    let err = confirm(&state, "project-session-1", false, true)
        .await
        .expect_err("refused");
    assert_eq!(
        err.downcast_ref::<CodedError>().map(|coded| coded.code),
        Some(error_code::TRUST_DIRECTORY_TOO_BROAD)
    );
    assert!(trusted_in_store(&state).is_empty());
    assert!(live.trust.waiting(), "the screen is still waiting");
    assert!(sent(&received).is_empty());
}

#[tokio::test]
async fn a_directory_can_be_removed_again_and_each_projects_own_consent_stays() {
    let (state, _dir) = state_with_project("remove");
    state
        .store
        .set_project_trust_consent("project-1", true)
        .expect("consent");
    state.store.add_trusted_directory("/work").expect("trusted");
    let mut events = state.subscribe();

    remove_trusted_directory(&state, "/work/").expect("removed");
    remove_trusted_directory(&state, "/work").expect("a repeat");
    assert!(trusted_in_store(&state).is_empty());
    assert!(consented_in_store(&state));

    let mut updates = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::TrustedDirectoriesUpdated {
            trusted_directories,
        } = event
        {
            updates.push(trusted_directories);
        }
    }
    assert_eq!(updates, [Vec::<String>::new()], "a repeat is not announced");
}

/// A screen of a project under a directory is answered without a prompt, and trusting the
/// directory answers the screens already waiting under it — and only those.
#[tokio::test]
async fn trusting_a_directory_answers_the_screens_waiting_under_it_and_no_others() {
    let (state, dir) = state_with_project("fan-out");
    let mut elsewhere = project(false);
    elsewhere.id = "project-2".to_string();
    elsewhere.path = "/other/project".to_string();
    state.store.insert_project(&elsewhere).expect("project");
    let mut events = state.subscribe();

    let under = session("under-1", Agent::Claude, Role::Project, Some("project-1"));
    let (under_live, under_received) =
        start(&state, &dir, under, &full_script(Path::new("%RECEIVED%")));
    supervise(&state, &under_live);
    let beside = session("beside-1", Agent::Claude, Role::Project, Some("project-2"));
    let (beside_live, beside_received) =
        start(&state, &dir, beside, &full_script(Path::new("%RECEIVED%")));
    supervise(&state, &beside_live);
    let mut asked = 0;
    tokio::time::timeout(PATIENCE, async {
        while asked < 2 {
            if let Event::TrustPrompt { .. } = events.recv().await.expect("event") {
                asked += 1;
            }
        }
    })
    .await
    .expect("both are asked about");

    add_trusted_directory(&state, Path::new("/work")).expect("trusted");
    answered(&under_live, &under_received).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        sent(&beside_received).is_empty(),
        "the one beside it is not"
    );
    assert_eq!(pending_prompts(&state).len(), 1, "and is still asked about");

    // A screen that comes up later under the directory is answered with no prompt.
    let later = session("later-1", Agent::Claude, Role::Project, Some("project-1"));
    let (later_live, later_received) =
        start(&state, &dir, later, &full_script(Path::new("%RECEIVED%")));
    supervise(&state, &later_live);
    answered(&later_live, &later_received).await;
}

/// Whether the refusal is one of the codes that leave the dialog open.
fn is_not_offerable(result: &Result<PathBuf>) -> bool {
    result.as_ref().is_err_and(|err| {
        err.downcast_ref::<CodedError>().is_some_and(|coded| {
            [
                error_code::TRUST_DIRECTORY_TOO_BROAD,
                error_code::TRUST_PATH_NOT_ABSOLUTE,
                error_code::TRUST_HOME_UNKNOWN,
            ]
            .contains(&coded.code)
        })
    })
}

/// The guard is not fooled by a spelling of the home directory the text comparison cannot see:
/// a symlink to it, a symlink to something containing it, or — on a volume that ignores letter
/// case, which a temporary directory here may or may not be on — the same name in another case.
#[test]
fn the_guard_sees_through_symlinks_and_letter_case() {
    let dir = scratch("guard");
    let home = dir.join("Home");
    std::fs::create_dir_all(home.join("app")).expect("home");
    std::os::unix::fs::symlink(&home, dir.join("link-to-home")).expect("link");
    std::os::unix::fs::symlink(&dir, dir.join("link-to-above")).expect("link");
    let under = |parent: PathBuf| parent.join("app").to_string_lossy().into_owned();

    // As written, for the baseline.
    assert!(is_not_offerable(&trustable_parent(
        &under(home.clone()),
        &home
    )));
    // A link to the home directory, and a link to a directory above it.
    assert!(is_not_offerable(&trustable_parent(
        &under(dir.join("link-to-home")),
        &home
    )));
    assert!(is_not_offerable(&trustable_parent(
        &under(dir.join("link-to-above")),
        &home
    )));
    // A sibling that is neither is still offered.
    std::fs::create_dir_all(dir.join("Elsewhere")).expect("sibling");
    assert!(trustable_parent(&under(dir.join("Elsewhere")), &home).is_ok());

    // Another letter case. On a volume that ignores case it is the home directory and is
    // refused; on one that does not it is a different folder, which is offered.
    let flipped = dir.join("hOME");
    if flipped.exists() {
        assert!(is_not_offerable(&trustable_parent(&under(flipped), &home)));
    } else {
        std::fs::create_dir_all(flipped.join("app")).expect("a different folder");
        assert!(trustable_parent(&under(flipped), &home).is_ok());
    }
}

/// The home directory may itself be given through a link whose real location lies under the
/// parent being offered: `/home -> /data/home`, `HOME=/home/me`, a project at `/data/app`.
#[test]
fn the_guard_sees_a_home_directory_reached_through_a_link() {
    let dir = scratch("guard-home-link");
    std::fs::create_dir_all(dir.join("data/home/me")).expect("real home");
    std::fs::create_dir_all(dir.join("data/app")).expect("project");
    std::os::unix::fs::symlink(dir.join("data/home"), dir.join("home")).expect("link");
    let home = dir.join("home/me");

    let project = dir.join("data/app").to_string_lossy().into_owned();
    assert!(
        is_not_offerable(&trustable_parent(&project, &home)),
        "`data` contains the real home"
    );
    // A folder that does not contain it is still offered.
    std::fs::create_dir_all(dir.join("other/app")).expect("other");
    let other = dir.join("other/app").to_string_lossy().into_owned();
    assert!(trustable_parent(&other, &home).is_ok());
}

/// With no usable home directory nothing can be checked against it, so nothing is offered —
/// and a project path that is not absolute says so rather than blaming the root or home.
#[test]
fn an_odd_home_directory_or_a_relative_project_path_offers_nothing() {
    for home in ["", "relative/home", "/"] {
        let err = trustable_parent("/work/project", Path::new(home)).expect_err(home);
        assert!(err.to_string().contains("home directory"), "{err}");
        assert!(is_not_offerable(&Err::<PathBuf, _>(err)), "{home:?}");
    }
    let err = trustable_parent("work/project", Path::new("/Users/me")).expect_err("relative");
    assert!(err.to_string().contains("not an absolute path"), "{err}");
    assert!(!err.to_string().contains("too broad"), "{err}");
    // Still a refusal the dialog stays open for.
    assert!(is_not_offerable(&Err::<PathBuf, _>(err)));
}

#[test]
fn dot_dot_in_a_project_path_is_folded_before_anything_is_compared() {
    let home = Path::new("/Users/me");
    assert_eq!(
        trustable_parent("/work/other/../project", home).unwrap(),
        Path::new("/work")
    );
    let work = ["/work".to_string()];
    assert!(under_a_trusted_directory("/work/other/../project", &work));
    // Climbing out of the directory is not under it.
    assert!(!under_a_trusted_directory("/work/../etc/project", &work));
    assert!(!under_a_trusted_directory("/work/../../project", &work));
}

/// The prompt carries the directory its button would trust, and none when that would be too
/// broad.
#[test]
fn a_prompt_names_the_directory_it_would_trust_unless_that_is_too_broad() {
    let mut inside = project(false);
    inside.path = "/work/project".to_string();
    let asking = session("s", Agent::Grok, Role::Project, Some("project-1"));
    assert!(matches!(prompt(&asking, &inside),
        Event::TrustPrompt { trust_dir, .. } if trust_dir.as_deref() == Some("/work")));
    let mut at_home = project(false);
    at_home.path = paths::home_dir().join("app").to_string_lossy().into_owned();
    assert!(matches!(
        prompt(&asking, &at_home),
        Event::TrustPrompt {
            trust_dir: None,
            ..
        }
    ));
}

/// The replay after a snapshot leaves out a screen whose project sits under a trusted
/// directory, as it does one whose project has consented.
#[tokio::test]
async fn the_replay_leaves_out_a_screen_under_a_trusted_directory() {
    let (state, dir) = state_with_project("replay-trusted");
    let record = session("waiting-1", Agent::Claude, Role::Project, Some("project-1"));
    let (live, _) = start(&state, &dir, record, "sleep 30");
    live.trust.feed(SCREEN, 0, 0);
    assert_eq!(pending_prompts(&state).len(), 1);

    state.store.add_trusted_directory("/work").expect("trusted");
    assert!(
        live.trust.waiting(),
        "still waiting; only the question is moot"
    );
    assert!(pending_prompts(&state).is_empty());
}

/// Trusting the directory from one session's dialog answers every other session's screen
/// waiting under it, whichever agent's it is; a repeat neither announces nor answers anything
/// again; a console session is left to its own rule.
#[tokio::test]
async fn one_sessions_go_ahead_answers_the_others_waiting_under_the_directory() {
    let (state, dir) = state_with_project("fan-out-end-to-end");
    let mut events = state.subscribe();
    let a = session("a-1", Agent::Claude, Role::Project, Some("project-1"));
    let (a_live, a_received) = start(&state, &dir, a, &full_script(Path::new("%RECEIVED%")));
    a_live.trust.feed(SCREEN, 0, 0);
    screen_printed(&a_live).await;
    let b = session("b-1", Agent::Claude, Role::Project, Some("project-1"));
    let (b_live, b_received) = start(&state, &dir, b, &full_script(Path::new("%RECEIVED%")));
    b_live.trust.feed(SCREEN, 0, 0);
    screen_printed(&b_live).await;
    let console_session = session("console-session-1", Agent::Claude, Role::Console, None);
    let (console_session_live, console_session_received) = start(
        &state,
        &dir,
        console_session,
        &full_script(Path::new("%RECEIVED%")),
    );
    console_session_live.trust.feed(SCREEN, 0, 0);
    screen_printed(&console_session_live).await;
    // Another agent's, at its own screen: the permission is the same for every agent.
    let codex = session("codex-1", Agent::Codex, Role::Project, Some("project-1"));
    let (codex_live, codex_received) =
        start(&state, &dir, codex, &codex_script(Path::new("%RECEIVED%")));
    screen_printed(&codex_live).await;

    confirm(&state, "a-1", false, true)
        .await
        .expect("confirmed");
    assert_eq!(sent(&a_received), answer_keys());
    answered(&b_live, &b_received).await;
    let pressed = tokio::task::spawn_blocking(move || {
        wait_for(PATIENCE, || {
            sent(&codex_received) == ENTER
                && codex_live.recent_output(CODEX_DISMISSED.len()) == CODEX_DISMISSED
        })
    });
    assert!(pressed.await.unwrap(), "Codex's own screen is pressed too");
    assert!(
        sent(&console_session_received).is_empty(),
        "a console session is not the fan-out's"
    );

    // A repeat: nothing announced, and nothing more is answered.
    let c = session("c-1", Agent::Claude, Role::Project, Some("project-1"));
    let (c_live, c_received) = start(&state, &dir, c, &full_script(Path::new("%RECEIVED%")));
    c_live.trust.feed(SCREEN, 0, 0);
    screen_printed(&c_live).await;
    add_trusted_directory(&state, Path::new("/work")).expect("a repeat");
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(sent(&c_received).is_empty());
    let mut announced = 0;
    while let Ok(event) = events.try_recv() {
        if matches!(event, Event::TrustedDirectoriesUpdated { .. }) {
            announced += 1;
        }
    }
    assert_eq!(announced, 1);
}

/// Codex's cursor starts on the option that trusts, so its screen is pressed with an Enter
/// alone, and nothing else is sent.
#[test]
fn codex_is_pressed_with_an_enter_and_nothing_else() {
    let dir = scratch("codex");
    let received = dir.join("received");
    let live = stand_in("s", Agent::Codex, &codex_script(&received));
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));

    answer(&live).expect("pressed");

    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(sent(&received), ENTER);
}

/// Codex runs no hook after its screen is answered in the terminal, so the person's own key is
/// what ends the wait: the prompt is not put to a client again, and a go-ahead for it is
/// refused as stale rather than tried and reported as a failure.
#[tokio::test]
async fn codex_answered_in_the_terminal_is_no_longer_waiting() {
    let (state, dir) = state_with_project("codex-in-terminal");
    let record = session("codex-1", Agent::Codex, Role::Project, Some("project-1"));
    let (live, received) = start(&state, &dir, record, &codex_script(Path::new("%RECEIVED%")));
    assert!(wait_for(PATIENCE, || live.trust.waiting()));
    assert_eq!(pending_prompts(&state).len(), 1);

    live.write_input(b"\r").expect("the person's Enter");

    assert!(pending_prompts(&state).is_empty());
    let err = confirm(&state, "codex-1", true, false)
        .await
        .expect_err("stale");
    assert_eq!(
        err.downcast_ref::<CodedError>().map(|coded| coded.code),
        Some(error_code::TRUST_NOT_WAITING)
    );
    assert!(!consented_in_store(&state));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(sent(&received), ENTER, "only the person's own key");
}

/// A message for a Codex session at its screen is held, since its trailing Enter would accept
/// the screen, and goes in once the press has.
#[tokio::test]
async fn a_message_waits_while_codex_is_at_its_screen_and_follows_the_press() {
    let (state, dir) = state_with_project("held-message");
    let message = crate::term::frame_message("hello");
    let record = session("codex-1", Agent::Codex, Role::Project, Some("project-1"));
    let script = codex_script_reading(Path::new("%RECEIVED%"), message.len());
    let (live, received) = start(&state, &dir, record, &script);
    assert!(wait_for(PATIENCE, || live.trust.waiting()));

    let delivery = crate::reporting::write_message(
        &state,
        "codex-1",
        "hello",
        crate::reporting::WhenBlocked::Queue,
    )
    .expect("accepted");
    assert!(matches!(delivery, crate::reporting::Delivery::Queued));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(sent(&received).is_empty(), "nothing reaches the screen");

    confirm(&state, "codex-1", false, false)
        .await
        .expect("pressed");
    let expected = [ENTER, &message].concat();
    let delivered =
        tokio::task::spawn_blocking(move || wait_for(PATIENCE, || sent(&received) == expected));
    assert!(delivered.await.unwrap(), "the message follows the press");
}

/// "Trust and continue" in one session's dialog presses the screens already waiting in the same
/// project, whichever agent shows them, and none in another project.
#[tokio::test]
async fn trusting_a_project_presses_its_other_waiting_screens_and_no_others() {
    let (state, dir) = state_with_project("project-fan-out");
    let mut elsewhere = project(false);
    elsewhere.id = "project-2".to_string();
    elsewhere.path = "/other/project".to_string();
    state.store.insert_project(&elsewhere).expect("project");
    let a = session("a-1", Agent::Claude, Role::Project, Some("project-1"));
    let (a_live, a_received) = start(&state, &dir, a, &full_script(Path::new("%RECEIVED%")));
    let same = session("codex-1", Agent::Codex, Role::Project, Some("project-1"));
    let (same_live, same_received) =
        start(&state, &dir, same, &codex_script(Path::new("%RECEIVED%")));
    let other = session("codex-2", Agent::Codex, Role::Project, Some("project-2"));
    let (other_live, other_received) =
        start(&state, &dir, other, &codex_script(Path::new("%RECEIVED%")));
    for live in [&a_live, &same_live, &other_live] {
        assert!(wait_for(PATIENCE, || live.trust.waiting()));
    }

    confirm(&state, "a-1", true, false)
        .await
        .expect("confirmed");
    assert_eq!(sent(&a_received), answer_keys());
    let pressed =
        tokio::task::spawn_blocking(move || wait_for(PATIENCE, || sent(&same_received) == ENTER));
    assert!(
        pressed.await.unwrap(),
        "the other agent's screen in the project"
    );
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(sent(&other_received).is_empty(), "not another project's");
}

/// A Codex session that shows no trust screen runs no hook until its first prompt, so what was
/// queued for it is let through when its watch ends.
#[tokio::test]
async fn a_message_for_codex_is_let_through_when_its_watch_ends_with_no_screen() {
    let (state, dir) = state_with_project("watch-ends");
    let message = crate::term::frame_message("hello");
    let record = session("codex-1", Agent::Codex, Role::Project, Some("project-1"));
    let script = format!(
        "printf 'ready'; {}; sleep 30",
        take(message.len(), "'%RECEIVED%'")
    );
    let (live, received) = start(&state, &dir, record, &script);
    live.trust.shorten_watch(Duration::from_millis(300));
    supervise(&state, &live);

    let delivery = crate::reporting::write_message(
        &state,
        "codex-1",
        "hello",
        crate::reporting::WhenBlocked::Queue,
    )
    .expect("accepted");
    assert!(matches!(delivery, crate::reporting::Delivery::Queued));
    let delivered =
        tokio::task::spawn_blocking(move || wait_for(PATIENCE, || sent(&received) == message));
    assert!(delivered.await.unwrap());
}

/// A Codex screen the person answers in the terminal lets the messages held for the session
/// through once it is seen to go, even when they type on into the next screen at once; a Down,
/// which only moves the cursor, keeps them held.
#[tokio::test]
async fn codex_answered_in_the_terminal_lets_held_messages_through_once_the_screen_goes() {
    let (state, dir) = state_with_project("codex-released");
    let message = crate::term::frame_message("hello");
    let r = "'%RECEIVED%'";
    let answered = session("codex-1", Agent::Codex, Role::Project, Some("project-1"));
    let (answered_live, answered_received) = start(
        &state,
        &dir,
        answered,
        &codex_script_reading(Path::new("%RECEIVED%"), 3 + message.len()),
    );
    let moved = session("codex-2", Agent::Codex, Role::Project, Some("project-1"));
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
    let (moved_live, moved_received) = start(&state, &dir, moved, &moved_script);
    for live in [&answered_live, &moved_live] {
        assert!(wait_for(PATIENCE, || live.trust.waiting()));
        supervise(&state, live);
        let delivery = crate::reporting::write_message(
            &state,
            &live.id,
            "hello",
            crate::reporting::WhenBlocked::Queue,
        )
        .expect("accepted");
        assert!(matches!(delivery, crate::reporting::Delivery::Queued));
    }

    // Enter, and typing on into the next screen at once.
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

/// Grok's screen pressed, but its entry never written: the go-ahead fails as not carried over,
/// the user is told, and no permission of Octoboard's is recorded.
#[tokio::test]
async fn a_grok_entry_that_never_appears_is_reported_and_records_nothing() {
    let (state, dir) = state_with_project("grok-not-carried");
    let mut events = state.subscribe();
    let received = dir.join("received");
    let user_store = dir.join("trusted_folders.toml");
    let script = [
        load(&[("SCREEN", "grok_trust_screen.bin")]),
        show("SCREEN"),
        take(1, &quoted(&received)),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let record = session("grok-1", Agent::Grok, Role::Project, Some("project-1"));
    state.store.insert_session(&record).expect("session");
    let live = watched_live_session(
        "grok-1",
        Agent::Grok,
        120,
        32,
        &format!("stty raw -echo; {script}"),
        Some(CarriedTrust {
            session_store: dir.join("session-store.toml"),
            user_store: user_store.clone(),
            folder: "/work/project".to_string(),
        }),
    );
    spawn_reader_thread(live.clone(), 8 * 1024);
    state.register_live(live.clone());
    assert!(wait_for(PATIENCE, || live.trust.waiting()));
    // Grok's `SessionStart`, right after the press, with nothing written to its store.
    let hook = {
        let (state, live, received) = (state.clone(), live.clone(), received.clone());
        tokio::task::spawn_blocking(move || {
            assert!(wait_for(PATIENCE, || sent(&received) == YES));
            on_hook(&state, &live);
        })
    };

    let err = confirm(&state, "grok-1", true, false)
        .await
        .expect_err("not carried over");
    hook.await.unwrap();
    assert_eq!(
        err.downcast_ref::<CodedError>().map(|coded| coded.code),
        Some(error_code::TRUST_NOT_CARRIED_OVER)
    );
    assert!(!consented_in_store(&state));
    assert!(trusted_in_store(&state).is_empty());
    assert!(!user_store.exists());
    let mut told = false;
    while let Ok(event) = events.try_recv() {
        told |= matches!(event, Event::SessionNotice { session, code, .. }
            if session == "grok-1" && code == notice_code::TRUST_NOT_CARRIED_OVER);
    }
    assert!(told, "the user is told");
}

/// Grok answered by the person in the terminal: the entry Grok wrote reaches the user's own
/// store all the same, and no permission of Octoboard's is recorded.
#[tokio::test]
async fn grok_answered_in_the_terminal_still_has_its_entry_carried() {
    let (state, dir) = state_with_project("grok-in-terminal");
    let received = dir.join("received");
    let session_store = dir.join("session-store.toml");
    let user_store = dir.join("trusted_folders.toml");
    let entry = "[folders.\"/work/project\"]\ntrusted = true\ndecided_at = 1791531653\n";
    let wrote = dir.join("grok-wrote");
    std::fs::write(&wrote, entry).expect("what Grok writes");
    let script = [
        load(&[("SCREEN", "grok_trust_screen.bin")]),
        format!("IFS= read -r -d '' WROTE < {}", quoted(&wrote)),
        show("SCREEN"),
        take(1, &quoted(&received)),
        format!("printf '%s' \"$WROTE\" > {}", quoted(&session_store)),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let record = session("grok-1", Agent::Grok, Role::Project, Some("project-1"));
    state.store.insert_session(&record).expect("session");
    let live = watched_live_session(
        "grok-1",
        Agent::Grok,
        120,
        32,
        &format!("stty raw -echo; {script}"),
        Some(CarriedTrust {
            session_store,
            user_store: user_store.clone(),
            folder: "/work/project".to_string(),
        }),
    );
    spawn_reader_thread(live.clone(), 8 * 1024);
    state.register_live(live.clone());
    assert!(wait_for(PATIENCE, || live.trust.waiting()));

    live.write_input(YES).expect("the person's key");
    on_hook(&state, &live);

    let carried = tokio::task::spawn_blocking(move || {
        wait_for(PATIENCE, || {
            std::fs::read_to_string(&user_store).is_ok_and(|text| text == entry)
        })
    });
    assert!(carried.await.unwrap(), "the entry reaches the user's store");
    assert!(!consented_in_store(&state));
    assert!(trusted_in_store(&state).is_empty());
}

/// Grok's first drawing leaves the recent output behind its logo animation, and the terminal
/// never goes quiet: it is still pressed, with `y`, and the entry Grok then writes into the
/// session's copy of its store is carried to the user's own as Grok wrote it, by the hook Grok
/// runs right after the press.
#[tokio::test]
async fn grok_is_pressed_behind_its_animation_and_its_entry_reaches_the_users_store() {
    let (state, dir) = state_with_project("grok-press");
    let received = dir.join("received");
    let session_store = dir.join("session-store.toml");
    let user_store = dir.join("trusted_folders.toml");
    let users = "[folders.\"/somewhere/else\"]\ntrusted = true\ndecided_at = 1789520034\n";
    std::fs::write(&user_store, users).expect("the user's store");
    std::fs::write(&session_store, users).expect("the session's copy");
    // As Grok 1.0.50 saves it: the whole store again, with the folder's new table last.
    let entry = "[folders.\"/work/project\"]\ntrusted = true\ndecided_at = 1791531653\n";
    let wrote = dir.join("grok-wrote");
    std::fs::write(&wrote, format!("{users}\n{entry}")).expect("what Grok writes");
    let script = [
        load(&[
            ("SCREEN", "grok_trust_screen.bin"),
            ("ANIMATION", "grok_trust_animation.bin"),
        ]),
        format!("IFS= read -r -d '' WROTE < {}", quoted(&wrote)),
        show("SCREEN"),
        "for frame in 1 2 3 4 5 6 7 8 9 10; do printf '%s' \"$ANIMATION\"; done".to_string(),
        take(1, &quoted(&received)),
        format!("printf '%s' \"$WROTE\" > {}", quoted(&session_store)),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let carried = CarriedTrust {
        session_store,
        user_store: user_store.clone(),
        folder: "/work/project".to_string(),
    };
    let live = watched_live_session(
        "s",
        Agent::Grok,
        120,
        32,
        &format!("stty raw -echo; {script}"),
        Some(carried),
    );
    spawn_reader_thread(live.clone(), 8 * 1024);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));
    assert!(wait_for(PATIENCE, || {
        live.output_total() as usize >= GROK_SCREEN.len() + 10 * GROK_ANIMATION.len()
    }));
    assert!(!GROK.is_shown_in(&live.recent_output(WINDOW)));
    // Grok's `SessionStart`, right after the key.
    let hook = {
        let (state, live, received) = (state.clone(), live.clone(), received.clone());
        tokio::task::spawn_blocking(move || {
            assert!(wait_for(PATIENCE, || sent(&received) == YES));
            on_hook(&state, &live);
        })
    };

    let pressing = live.clone();
    tokio::task::spawn_blocking(move || answer(&pressing))
        .await
        .unwrap()
        .expect("pressed");
    hook.await.unwrap();

    assert_eq!(sent(&received), YES);
    assert_eq!(
        std::fs::read_to_string(&user_store).expect("the user's store"),
        format!("{users}\n{entry}")
    );
}

/// Only Grok's entry changes in the user's store, exactly as Grok wrote it — added, or put in
/// place of a stale one — and every other byte stays; a store that cannot be read is left
/// alone.
#[test]
fn only_groks_entry_changes_in_the_users_store() {
    let entry = "[folders.\"/work/project\"]\ntrusted = true\ndecided_at = 1791531653\n";
    let others = "# mine\n[folders.\"/a\"]\ntrusted = true\n\n[folders.'/b']\ntrusted   =   true\n";
    assert_eq!(
        merge_entry(others, "/work/project", entry).unwrap(),
        format!("{others}\n{entry}")
    );
    let stale =
        "[folders.\"/a\"]\ntrusted = true\n\n[folders.\"/work/project\"]\ntrusted = false\n\
                 decided_at = 5\n\n# about b\n[folders.\"/b\"]\ntrusted = true\n";
    assert_eq!(
        merge_entry(stale, "/work/project", entry).unwrap(),
        stale.replace(
            "trusted = false\ndecided_at = 5\n",
            "trusted = true\ndecided_at = 1791531653\n"
        )
    );
    assert!(merge_entry("[folders\n", "/work/project", entry).is_err());
}
