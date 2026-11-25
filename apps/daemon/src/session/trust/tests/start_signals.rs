//! What a provider's trust screen declares beyond the first three agents' (`Screen::others`,
//! `StartSignal::InputBox`), read off screens of a made-up agent: the test-only provider, given a
//! screen of each shape by
//! `crate::test_support::test_provider::declaring_trust_screen`. The real agents' screens have
//! their own tests beside the captures they were measured from.
//!
//! Polls are driven on tokio's paused clock, one `RELEASE_POLL` at a time.

use super::*;
use crate::test_support::test_provider::{declaring_trust_screen, Declared};

/// The made-up agent's trust screen, drawn over a cleared screen.
const TRUST_TEXT: &[u8] =
    b"\x1b[2J\x1b[HTrust this folder?\r\n> 1. Yes, trust it\r\n  2. No, exit\r\n";
/// Its input box, as the test-only provider's screen reader reads one, drawn over a cleared screen.
const INPUT_BOX_TEXT: &[u8] = b"\x1b[2J\x1b[HINPUT> \r\n";
/// Something else on a cleared screen: neither its input box nor its trust screen.
const STARTING_TEXT: &[u8] = b"\x1b[2J\x1b[HStarting up...\r\n";

/// A trust screen of the made-up agent that starts as `start_signal` says and lets its messages
/// through after `hook_fallback` where it has one.
const fn made_up(start_signal: StartSignal, hook_fallback: Option<Duration>) -> Screen {
    Screen {
        first_draw: &[&["Trustthisfolder?", "1.Yes,trustit"]],
        accept: Some("1.Yes,trustit"),
        others: &["2.No,exit"],
        cursor: None,
        press: Press::Yes,
        next_screen: None,
        input_ends_wait: true,
        start_signal,
        hook_fallback,
        waits_for_paste_mode: false,
    }
}

/// An agent whose input box alone shows its startup, with no bound on the wait.
static BOX_ONLY: Screen = made_up(StartSignal::InputBox, None);
/// The same, its messages let through after the hook fallback while no screen has been sighted.
static BOX_WITH_FALLBACK: Screen = made_up(StartSignal::InputBox, Some(HOOK_FALLBACK));
/// An agent that runs a hook at start and has the hook fallback.
static HOOK_WITH_FALLBACK: Screen = made_up(StartSignal::Hook, Some(HOOK_FALLBACK));

#[test]
fn a_rendered_title_agent_without_a_confirmation_releases_at_its_input_box() {
    static TITLE_ONLY: Screen = Screen {
        first_draw: &[],
        accept: None,
        others: &[],
        ..made_up(StartSignal::TerminalTitle, None)
    };
    let _declared = declaring_trust_screen(&TITLE_ONLY);
    let watch = TrustState::new(AgentId::TEST, None);
    watch.read_the_screen();
    watch.note_title();
    watch.feed_screen(|| "Starting up...".to_owned(), 0, 14);
    assert!(watch.holds_writes());
    watch.feed_screen(|| "INPUT> ".to_owned(), 0, 21);
    assert!(!watch.holds_writes());
    assert!(!watch.waiting());
}

/// A session of the test-only provider, declaring `screen`, with its watch looking for the screen
/// and a message held for it.
struct Starting {
    state: Arc<AppState>,
    live: StandIn,
    _dir: ScratchDir,
    _declared: Declared<&'static Screen>,
}

impl Starting {
    fn new(name: &str, screen: &'static Screen) -> Self {
        let declared = declaring_trust_screen(screen);
        let (state, dir) = state_with_project(name);
        let record = session("s", AgentId::TEST, SessionKind::Project, Some("project-1"));
        state.store.insert_session(&record).expect("session");
        let live = watched_live_session("s", AgentId::TEST, 120, 32, "sleep 30", None);
        state.register_live(live.clone());
        state.queue_message("s", "held until the agent is up");
        assert!(live.trust.holds_writes());
        Self {
            state,
            live,
            _dir: dir,
            _declared: declared,
        }
    }

    /// Whether the message queued at the start has been let through: the hold is over and the
    /// release, which writes in a blocking task that holds the clock still until it is done, has
    /// emptied the queue.
    async fn released(&self) -> bool {
        if self.live.trust.holds_writes() {
            return false;
        }
        for _ in 0..4 {
            if self.state.outbox.len("s") == 0 {
                return true;
            }
            tokio::time::sleep(RELEASE_POLL).await;
        }
        self.state.outbox.len("s") == 0
    }

    /// Whether the message queued at the start is still waiting.
    fn held(&self) -> bool {
        self.live.trust.holds_writes() && self.state.outbox.len("s") == 1
    }
}

/// Lets `count` polls of the release watches pass, each run to its end before the next.
async fn polls(count: u32) {
    for _ in 0..count {
        tokio::time::advance(RELEASE_POLL).await;
        crate::test_support::tasks_idle().await;
    }
}

/// An agent whose start signal is its input box has its messages held until the box is seen with
/// the screen not up at two looks a poll apart: neither a screen without the box nor one passing
/// frame of it releases them, and a frame that is gone again by the next look starts the count
/// over. Started by `supervise`, as a launch starts it.
#[tokio::test(start_paused = true)]
async fn an_input_box_agents_messages_are_released_at_two_looks_of_the_box_a_poll_apart() {
    let starting = Starting::new("box-two-looks", &BOX_ONLY);
    let (state, live) = (&starting.state, &starting.live);
    supervise(state, live);
    crate::test_support::tasks_idle().await;

    live.on_output(STARTING_TEXT);
    polls(3).await;
    assert!(starting.held(), "no box is on the screen");

    live.on_output(INPUT_BOX_TEXT);
    polls(1).await;
    assert!(starting.held(), "one look at the box is not enough");

    live.on_output(STARTING_TEXT);
    polls(1).await;
    live.on_output(INPUT_BOX_TEXT);
    polls(1).await;
    assert!(
        starting.held(),
        "a box that went in between starts the count over"
    );

    polls(1).await;
    assert!(starting.released().await, "the box at two looks in a row");
}

/// The hold of an input-box agent is not over for a screen that was sighted, nor for Octoboard's
/// press of it, nor for the screen's going: only the box with the screen not up, or a hook, ends
/// it.
#[test]
fn an_input_box_agents_hold_outlasts_a_sighted_screen_its_press_and_its_dismissal() {
    let _declared = declaring_trust_screen(&BOX_ONLY);
    let watch = TrustState::new(AgentId::TEST, None);
    assert!(watch.holds_writes());
    watch.feed(TRUST_TEXT, 0, TRUST_TEXT.len() as u64);
    assert!(watch.sighted.load(Ordering::Acquire));
    assert!(watch.holds_writes());
    watch.pressed.store(true, Ordering::Release);
    watch.dismissed.store(true, Ordering::Release);
    assert!(watch.holds_writes(), "pressed and dismissed");

    // Still the screen on the parsed screen: not the start.
    assert!(watch
        .start_at_input_box("INPUT> \nTrust this folder?\n1. Yes, trust it", true)
        .is_none());
    assert!(watch.holds_writes());
    let started = watch.start_at_input_box("INPUT> ", true).expect("started");
    assert!(
        !started.answered_in_terminal,
        "Octoboard's press answered it"
    );
    assert!(!watch.holds_writes());

    let hooked = TrustState::new(AgentId::TEST, None);
    hooked.feed(TRUST_TEXT, 0, TRUST_TEXT.len() as u64);
    assert!(hooked.holds_writes());
    assert!(hooked.note_hook().released, "a hook still releases");
    assert!(!hooked.holds_writes());
}

/// A screen the person answered in the terminal, which raised the hand when it was sighted, has the
/// hand lowered when the box comes, and the answer is told.
#[tokio::test(start_paused = true)]
async fn a_screen_answered_in_the_terminal_lowers_the_raised_hand_when_the_box_comes() {
    let starting = Starting::new("box-answered-in-terminal", &BOX_ONLY);
    let (state, live) = (&starting.state, &starting.live);
    live.on_output(TRUST_TEXT);
    assert!(live.trust.waiting());
    live.trust.needs_user.store(true, Ordering::Release);
    assert!(live.trust.needs_user());
    release_when_the_input_box_comes(state, live);
    crate::test_support::tasks_idle().await;

    live.on_output(INPUT_BOX_TEXT);
    polls(3).await;

    assert!(starting.released().await);
    assert!(!live.trust.needs_user(), "the hand came down");
    assert!(live.trust.dismissed.load(Ordering::Acquire));
    assert!(!live.trust.waiting());
}

/// With a hook fallback the messages are let through after it while no screen was sighted, and a
/// sighted screen keeps them held past it: neither its going nor anything but the box ends that.
#[tokio::test(start_paused = true)]
async fn the_hook_fallback_of_an_input_box_agent_lets_through_only_while_no_screen_was_sighted() {
    let starting = Starting::new("box-fallback-open", &BOX_WITH_FALLBACK);
    supervise(&starting.state, &starting.live);
    crate::test_support::tasks_idle().await;
    tokio::time::advance(HOOK_FALLBACK).await;
    crate::test_support::tasks_idle().await;
    assert!(starting.released().await, "nothing was sighted");

    let starting = Starting::new("box-fallback-sighted", &BOX_WITH_FALLBACK);
    supervise(&starting.state, &starting.live);
    crate::test_support::tasks_idle().await;
    starting.live.on_output(TRUST_TEXT);
    tokio::time::advance(HOOK_FALLBACK).await;
    crate::test_support::tasks_idle().await;
    assert!(starting.held(), "a sighted screen holds past the fallback");

    // Its going is not what an input-box agent's hold waits for.
    starting.live.on_output(STARTING_TEXT);
    polls(4).await;
    assert!(starting.held());

    starting.live.on_output(INPUT_BOX_TEXT);
    polls(3).await;
    assert!(
        starting.released().await,
        "its box, once the screen is gone"
    );
}

/// A provider's own tests replay a capture through the matcher the watch and the press use
/// (`reading_of`): the screen first drawn is shown and up, with no cursor read where the screen
/// has none, and what follows it reads as gone and replaced.
#[test]
fn a_capture_is_read_through_the_matcher_the_watch_uses() {
    assert_eq!(
        reading_of(&BOX_ONLY, TRUST_TEXT),
        Reading {
            shown: true,
            up: true,
            cursor_on_accept: None,
            gone: false,
            replaced: false,
        }
    );
    assert_eq!(
        reading_of(&BOX_ONLY, INPUT_BOX_TEXT),
        Reading {
            shown: false,
            up: false,
            cursor_on_accept: None,
            gone: true,
            replaced: true,
        }
    );
    static WITH_CURSOR: Screen = Screen {
        cursor: Some('>'),
        press: Press::Enter,
        ..made_up(StartSignal::Hook, None)
    };
    assert_eq!(
        reading_of(
            &WITH_CURSOR,
            b"Trust this folder?\r\n> 1. Yes, trust it\r\n  2. No, exit"
        )
        .cursor_on_accept,
        Some(true)
    );
    assert_eq!(
        reading_of(
            &WITH_CURSOR,
            b"Trust this folder?\r\n  1. Yes, trust it\r\n> 2. No, exit"
        )
        .cursor_on_accept,
        Some(false)
    );
}

/// A sighted screen seen to go, for an agent whose hook cannot be relied on, releases its messages
/// at once, lowers the raised hand and counts as dismissed.
#[tokio::test(start_paused = true)]
async fn a_sighted_screen_seen_to_go_releases_at_once() {
    let starting = Starting::new("sighted-gone", &HOOK_WITH_FALLBACK);
    let (state, live) = (&starting.state, &starting.live);
    live.on_output(TRUST_TEXT);
    live.trust.needs_user.store(true, Ordering::Release);
    release_when_the_sighted_screen_goes(state, live);
    crate::test_support::tasks_idle().await;

    live.on_output(INPUT_BOX_TEXT);
    polls(3).await;

    assert!(starting.released().await);
    assert!(live.trust.dismissed.load(Ordering::Acquire));
    assert!(!live.trust.needs_user());
}

/// A screen with several other options and a cursor, the first of them where the cursor starts.
static OPTIONS: Screen = Screen {
    first_draw: &[&["Trustthisfolder?", "Yes,trust"]],
    accept: Some("Yes,trust"),
    // The second begins with the text of the option that trusts.
    others: &["No,exit", "Yes,trustforthissessiononly"],
    cursor: Some('>'),
    press: Press::DownThenEnter,
    next_screen: None,
    input_ends_wait: true,
    start_signal: StartSignal::Hook,
    hook_fallback: None,
    waits_for_paste_mode: false,
};

/// A cursor is read against every other option before the one that trusts, so an option whose text
/// begins with the trusting one's is the cursor on another option, not on it; and a cursor on
/// nothing the screen draws, or none, reads as neither.
#[test]
fn the_cursor_reads_as_on_another_option_before_it_reads_as_on_the_one_that_trusts() {
    for (output, expected) in [
        (
            "Trust this folder?\r\n> No, exit\r\n  Yes, trust",
            Some(Cursor::Other),
        ),
        ("> Yes, trust for this session only", Some(Cursor::Other)),
        ("> Yes, trust", Some(Cursor::Accept)),
        (
            "> No, exit\r\n  Yes, trust\r\n> Yes, trust",
            Some(Cursor::Accept),
        ),
        ("> Something else", None),
        ("  Yes, trust", None),
    ] {
        assert_eq!(OPTIONS.cursor_on(output.as_bytes()), expected, "{output:?}");
    }
    // The option drawn in inverse video, where no glyph says: one option alone is the cursor's.
    for (inverse, expected) in [
        ("No, exit", Some(Cursor::Other)),
        ("Yes, trust", Some(Cursor::Accept)),
        ("No, exit and Yes, trust", None),
        ("nothing", None),
    ] {
        assert_eq!(
            OPTIONS.cursor_in_inverse(inverse.as_bytes()),
            expected,
            "{inverse:?}"
        );
    }
}

/// A Down from the option the cursor starts on, another one, then an Enter once the cursor is seen
/// on the option that trusts, and nothing at all when the cursor starts on the option that trusts,
/// where the Down would move it off.
#[test]
fn down_then_enter_presses_the_option_below_where_the_cursor_starts() {
    let _declared = declaring_trust_screen(&OPTIONS);
    let started_on_another = |dir: &ScratchDir| {
        let received = dir.join("received");
        let r = quoted(&received);
        let script = [
            "printf 'Trust this folder?\\r\\n> No, exit\\r\\n  Yes, trust\\r\\n  Yes, trust for this session only\\r\\n'".to_string(),
            take(3, &r),
            "printf '\\r\\n  No, exit\\r\\n> Yes, trust\\r\\n'".to_string(),
            take(1, &r),
            "printf '\\033[2J\\033[HWelcome\\r\\n'".to_string(),
            take(1, &r),
            "sleep 30".to_string(),
        ]
        .join("; ");
        (stand_in("s", AgentId::TEST, &script), received)
    };
    let dir = scratch("down-then-enter");
    let (live, received) = started_on_another(&dir);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));
    answer(&live).expect("answered");
    assert_input_through_marker(&live, &received, &answer_keys());

    let dir = scratch("down-then-enter-on-accept");
    let received = dir.join("received");
    let script = [
        "printf 'Trust this folder?\\r\\n  No, exit\\r\\n> Yes, trust\\r\\n  Yes, trust for this session only\\r\\n'".to_string(),
        take(1, &quoted(&received)),
        "sleep 30".to_string(),
    ]
    .join("; ");
    let live = stand_in("s", AgentId::TEST, &script);
    let mut sightings = live.trust.take_sightings().expect("receiver");
    assert!(wait_for(PATIENCE, || sightings.try_recv().is_ok()));
    let err = answer(&live).expect_err("refused");
    assert!(err.to_string().contains("starts on"), "{err}");
    assert!(sent(&received).is_empty(), "no key may have been sent");
}
