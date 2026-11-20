//! The composer requests written into a stand-in agent that draws the test provider's made-up
//! input box (`crate::adapter::test_provider::TestScreen`) and logs what it was given.

use std::time::{Duration, Instant};

use super::super::BoxIs;
use super::*;
use crate::session::paste_apart::NotSubmitted;
use crate::session::term::{self, CommandNotRecognised, Shape};
use crate::test_support::composer_agent::{agent, agent_with, Agent, AgentOptions};
use crate::test_support::PATIENCE;

/// Short caps, for a test of a wait that is meant to run out.
const QUICK: Timing = Timing {
    paste_taken: Duration::from_millis(600),
    unread: Duration::from_millis(900),
    submit_taken: Duration::from_millis(600),
    settle: Duration::from_millis(60),
};

fn images(names: &[&str]) -> Vec<String> {
    names
        .iter()
        .map(|name| format!("/pasted/{name}.png"))
        .collect()
}

fn paste(text: &str) -> Option<Input> {
    Some(Input::Paste(text.to_string()))
}

/// Each image is pasted alone and the next paste waits for its placeholder to show; the text comes
/// after the last one, and the Enter after it.
#[test]
fn images_are_pasted_apart_each_after_the_one_before_shows_then_the_text_then_enter() {
    let agent = agent("composer-sequence", "yes", "0.4", "");
    agent
        .live
        .write_images_message("m", &images(&["a", "b"]), paste("describe"), false)
        .unwrap();
    let started = Instant::now();
    while !agent.logged().iter().any(|line| line.starts_with("SENT")) {
        assert!(started.elapsed() < PATIENCE, "{:?}", agent.logged());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        agent.logged(),
        [
            "PASTE /pasted/a.png",
            "PASTE /pasted/b.png",
            "PASTE describe",
            "SENT [Image #1][Image #2]describe",
        ],
        "no paste came while a placeholder was still to show"
    );
}

/// A message of images alone has no text paste.
#[test]
fn a_message_of_images_alone_is_sent_without_a_text_paste() {
    let agent = agent("composer-images-only", "yes", "0.1", "");
    agent
        .live
        .write_images_message("m", &images(&["a"]), None, false)
        .unwrap();
    assert_eq!(agent.logged(), ["PASTE /pasted/a.png", "SENT [Image #1]"]);
}

/// A provider that declares images but never draws a placeholder does not hang the message: every
/// wait is bounded, and it ends as one written and left in the input box.
#[test]
fn placeholders_that_never_show_end_the_message_as_one_left_in_the_box() {
    let agent = agent("composer-no-placeholder", "no", "0", "");
    let started = Instant::now();
    let failure = agent
        .live
        .write_images_with("m", &images(&["a", "b"]), paste("hi"), false, QUICK)
        .unwrap_err();
    assert!(failure
        .error
        .get_ref()
        .is_some_and(|error| error.is::<NotSubmitted>()));
    assert!(started.elapsed() < Duration::from_secs(10));
    let logged = agent.logged();
    assert_eq!(
        logged,
        ["PASTE /pasted/a.png"],
        "nothing past the first paste"
    );
}

/// Nothing is pasted after someone else has written into the session since the message began.
#[test]
fn a_later_paste_is_refused_once_someone_else_has_written() {
    let agent = agent("composer-gated", "yes", "0.8", "");
    let live = agent.live.clone();
    let typist = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        live.write_terminal_input(b"x").unwrap();
    });
    let failure = agent
        .live
        .write_images_message("m", &images(&["a", "b"]), None, false)
        .unwrap_err();
    typist.join().unwrap();
    assert!(failure
        .error
        .get_ref()
        .is_some_and(|error| error.is::<NotSubmitted>()));
    assert!(!agent
        .logged()
        .iter()
        .any(|line| line.contains("b.png") || line.starts_with("SENT")));
}

/// A command is pasted as it is, stripped of control and invisible characters and with no guard
/// before its slash, and run by its own Enter.
#[test]
fn a_command_is_written_unguarded_and_sent() {
    let agent = agent("composer-command", "yes", "0", "");
    let route = term::send_shaped_with_id(
        &agent.live,
        "\u{200B}/status\u{1}",
        &Shape::Command,
        "m",
        false,
        &|| false,
    )
    .unwrap();
    assert_eq!(route, term::Route::Terminal);
    assert_eq!(agent.logged(), ["PASTE /status", "SENT /status"]);
}

/// A completion that offers the one written, or none, lets the Enter go.
#[test]
fn a_command_whose_completion_is_itself_or_absent_is_sent() {
    let agent = agent("composer-complete-same", "yes", "0", "status");
    agent.live.write_command("m", "/status", false).unwrap();
    assert_eq!(agent.logged(), ["PASTE /status", "SENT /status"]);
    // An argument shows no completion, and the one offered is not looked at against it.
    let agent = self::agent("composer-no-complete", "yes", "0", "");
    agent.live.write_command("m", "/model fast", false).unwrap();
    assert_eq!(agent.logged(), ["PASTE /model fast", "SENT /model fast"]);
}

/// A completion that offers another command than the one written would have the Enter run that
/// one: the box is cleared and the command refused, naming what was offered.
#[test]
fn a_command_whose_completion_is_another_is_cleared_and_refused() {
    let agent = agent("composer-complete-other", "yes", "0", "probecmd");
    let failure = agent.live.write_command("m", "/probe", false).unwrap_err();
    let refused = failure
        .error
        .get_ref()
        .and_then(|error| error.downcast_ref::<CommandNotRecognised>())
        .expect("a command not recognised");
    assert_eq!(refused.command, "/probe");
    assert_eq!(refused.offered, "/probecmd");
    assert_eq!(agent.logged(), ["PASTE /probe", "CLEARED"]);
    assert_eq!(
        agent.live.input_box(&agent.live.screen_text_undimmed()),
        Some(String::new())
    );
}

/// A server of the session's own that takes images, and keeps what it was offered with them.
struct ImagesServer {
    offered: std::sync::Mutex<Vec<(String, Vec<String>)>>,
}

impl term::MessageTransport for ImagesServer {
    fn send(&self, _: &str, _: &str) -> Result<term::Sent, PartialWrite> {
        panic!("a message with images or a command is not offered as plain text");
    }
    fn send_with_images(
        &self,
        text: &str,
        images: &[String],
        _: &str,
    ) -> Result<term::Sent, PartialWrite> {
        self.offered
            .lock()
            .unwrap()
            .push((text.to_string(), images.to_vec()));
        Ok(term::Sent::Queued)
    }
    fn attach(&self, _: term::TransportEvents) {}
    fn start_queue(&self) -> term::QueueStart {
        term::QueueStart::Nothing
    }
    fn stop(&self) {}
}

/// Files that exist under `agent`'s scratch, as the paths of saved images.
fn image_files(agent: &Agent, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .map(|name| {
            let path = agent.dir.join(format!("{name}.png"));
            std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
            path.to_string_lossy().into_owned()
        })
        .collect()
}

/// A command is never offered to the session's own server, whose queue would hand it to the model
/// as text: the terminal takes it.
#[test]
fn a_command_never_goes_through_the_agents_own_server() {
    let server = crate::test_support::AcceptingServer::new();
    let agent = agent_with(
        "composer-command-server",
        AgentOptions {
            transport: Some(server.clone()),
            ..AgentOptions::default()
        },
    );
    let route =
        term::send_shaped_with_id(&agent.live, "/status", &Shape::Command, "m", false, &|| {
            false
        })
        .unwrap();
    assert_eq!(route, term::Route::Terminal);
    assert!(server.offered().is_empty(), "{:?}", server.offered());
    assert_eq!(agent.logged(), ["PASTE /status", "SENT /status"]);
}

/// A message with images goes to a server that takes them, as its images and its text, and writes
/// nothing into the terminal; with no text, the text is empty.
#[test]
fn a_message_with_images_goes_to_a_server_that_takes_them() {
    let server = std::sync::Arc::new(ImagesServer {
        offered: Default::default(),
    });
    let agent = agent_with(
        "composer-images-server",
        AgentOptions {
            transport: Some(server.clone()),
            ..AgentOptions::default()
        },
    );
    let files = image_files(&agent, &["a", "b"]);
    for text in ["look", ""] {
        let route = term::send_shaped_with_id(
            &agent.live,
            text,
            &Shape::Images(files.clone()),
            "m",
            false,
            &|| false,
        )
        .unwrap();
        assert_eq!(route, term::Route::Queued);
    }
    assert_eq!(
        *server.offered.lock().unwrap(),
        [
            ("look".to_string(), files.clone()),
            (String::new(), files.clone())
        ]
    );
    assert!(
        agent.logged().is_empty(),
        "nothing was written to the terminal"
    );
}

/// A server that does not take images leaves the message to the terminal, written as images.
#[test]
fn a_message_with_images_a_server_declines_is_written_into_the_terminal() {
    let server = crate::test_support::AcceptingServer::new();
    let agent = agent_with(
        "composer-images-declined",
        AgentOptions {
            transport: Some(server.clone()),
            ..AgentOptions::default()
        },
    );
    let files = image_files(&agent, &["a"]);
    let route = term::send_shaped_with_id(
        &agent.live,
        "look",
        &Shape::Images(files.clone()),
        "m",
        false,
        &|| false,
    )
    .unwrap();
    assert_eq!(route, term::Route::Terminal);
    assert!(server.offered().is_empty());
    assert_eq!(
        agent.logged(),
        [
            format!("PASTE {}", files[0]),
            "PASTE look".to_string(),
            "SENT [Image #1]look".to_string()
        ]
    );
}

/// The text of a message with images is a message's: stripped, and a leading slash guarded so it
/// reaches the model as text rather than runs as a command.
#[test]
fn the_text_of_a_message_with_images_has_its_leading_slash_guarded() {
    let agent = agent("composer-images-slash", "yes", "0", "");
    let files = image_files(&agent, &["a"]);
    term::send_shaped_with_id(
        &agent.live,
        "\u{1}/status",
        &Shape::Images(files),
        "m",
        false,
        &|| false,
    )
    .unwrap();
    assert!(
        agent.logged().contains(&"PASTE  /status".to_string()),
        "{:?}",
        agent.logged()
    );
}

/// An image whose file is gone is refused as a message that could not be written, before anything
/// is offered to the server or written to the terminal.
#[test]
fn a_message_whose_image_file_is_gone_is_refused_before_anything_is_written() {
    let server = std::sync::Arc::new(ImagesServer {
        offered: Default::default(),
    });
    let agent = agent_with(
        "composer-images-gone",
        AgentOptions {
            transport: Some(server.clone()),
            ..AgentOptions::default()
        },
    );
    let mut files = image_files(&agent, &["a"]);
    files.push(agent.dir.join("gone.png").to_string_lossy().into_owned());
    let failure = term::send_shaped_with_id(
        &agent.live,
        "look",
        &Shape::Images(files),
        "m",
        false,
        &|| false,
    )
    .unwrap_err();
    assert_eq!(failure.written, 0);
    assert!(failure
        .error
        .get_ref()
        .is_some_and(|error| error.is::<term::Unwritable>()));
    assert!(server.offered.lock().unwrap().is_empty());
    assert!(agent.logged().is_empty());
}

/// Typing into the box, as the user does, so that it holds a draft.
fn draft(agent: &Agent, text: &str) {
    agent.live.write_terminal_input(text.as_bytes()).unwrap();
    let started = Instant::now();
    while agent
        .live
        .input_box(&agent.live.screen_text_undimmed())
        .as_deref()
        != Some(text)
    {
        assert!(started.elapsed() < PATIENCE, "the draft was not drawn");
        std::thread::sleep(Duration::from_millis(20));
    }
    agent.live.settle_input();
}

/// A message with images written over a draft the agent can stash goes in with the draft set
/// aside, and is waited on for the draft's restore only once its Enter is written: the agent puts
/// the draft back after the prompt it sends, not after the first paste.
#[test]
fn a_message_with_images_over_a_stashed_draft_is_submitted_before_the_restore_is_waited_for() {
    let agent = agent_with(
        "composer-stash-images",
        AgentOptions {
            stash: true,
            ..AgentOptions::default()
        },
    );
    draft(&agent, "hello");
    agent
        .live
        .write_images_message("m", &images(&["a"]), paste("describe"), false)
        .unwrap();
    assert_eq!(
        agent.logged(),
        [
            "STASHKEY",
            "PASTE /pasted/a.png",
            "PASTE describe",
            "SENT [Image #1]describe",
        ]
    );
    assert_eq!(
        agent
            .live
            .input_box(&agent.live.screen_text_undimmed())
            .as_deref(),
        Some("hello"),
        "the draft is back"
    );
    assert!(!agent.live.input_swap_active());
    assert!(
        !agent.live.stash_unsettled(),
        "the restore was seen, so the stash stays on"
    );
}

/// The same for a command: its Enter is written before the restore is waited for.
#[test]
fn a_command_over_a_stashed_draft_is_run_before_the_restore_is_waited_for() {
    let agent = agent_with(
        "composer-stash-command",
        AgentOptions {
            stash: true,
            ..AgentOptions::default()
        },
    );
    draft(&agent, "hello");
    agent.live.write_command("m", "/status", false).unwrap();
    assert_eq!(
        agent.logged(),
        ["STASHKEY", "PASTE /status", "SENT /status"]
    );
    assert!(!agent.live.stash_unsettled());
}

/// A long text after the images shows in the box as the provider's placeholder for a long paste,
/// which counts as the text shown: the Enter goes.
#[test]
fn a_long_text_the_agent_collapses_is_taken_as_shown_after_the_image_placeholders() {
    let agent = agent_with(
        "composer-collapse",
        AgentOptions {
            collapse: true,
            ..AgentOptions::default()
        },
    );
    agent
        .live
        .write_images_message("m", &images(&["a"]), paste("one\ntwo\nthree"), false)
        .unwrap();
    assert_eq!(
        agent.logged().last().map(String::as_str),
        Some("SENT [Image #1][Pasted text #1 +3 lines]")
    );
}

/// Placeholders the box held before the first image are not taken for the images'.
#[test]
fn a_placeholder_the_box_held_already_is_not_taken_for_an_image_s() {
    let agent = agent("composer-baseline", "yes", "0", "");
    let first = Input::Paste(images(&["a"])[0].clone());
    agent
        .live
        .write_message_input_as("m", std::slice::from_ref(&first), "", false)
        .unwrap();
    let writes = agent.live.input_writes();
    let held = Written {
        writes,
        box_before: Some("[Image #7]".to_string()),
        stashed: false,
    };
    let failure = agent
        .live
        .finish_images(&held, &images(&["a"]), None, QUICK)
        .unwrap_err();
    assert!(failure
        .error
        .get_ref()
        .is_some_and(|error| error.is::<NotSubmitted>()));
    let empty = Written {
        box_before: Some(String::new()),
        ..held
    };
    agent
        .live
        .finish_images(&empty, &images(&["a"]), None, QUICK)
        .unwrap();
}

/// A popup that comes up between the look at the completion and the Enter has the Enter refused
/// under the input lock, and nothing is pressed.
#[test]
fn an_enter_is_refused_when_a_completion_has_come_up_for_another_command() {
    let agent = agent("composer-late-popup", "yes", "0", "");
    agent
        .live
        .on_output(b"\x1b[2J\x1b[H\r\nINPUT> /probe\r\nCOMPLETE probecmd\r\n");
    let writes = agent.live.input_writes();
    let gated = Gated::after(&agent.live, writes, Some("probe"));
    assert!(!gated.press(&[Input::Enter]).unwrap());
    assert_eq!(gated.late.take().as_deref(), Some("probecmd"));
    assert_eq!(agent.live.input_writes(), writes);
    assert!(agent.logged().is_empty());
    // The command itself offered, or nothing, lets it through.
    agent
        .live
        .on_output(b"\x1b[2J\x1b[H\r\nINPUT> /probe\r\nCOMPLETE probe\r\n");
    assert!(gated.press(&[Input::Enter]).unwrap());
}

/// A command is never written as the box stands beside other text: where the box cannot be
/// emptied for it, it is refused to its sender, and nothing is written.
#[test]
fn a_command_is_never_written_into_a_box_as_it_stands() {
    let agent = agent("composer-beside-text", "yes", "0", "");
    draft(&agent, "hello");
    let tail = Tail {
        then: &|_| unreachable!("nothing was written"),
        records_prompt: false,
        needs_empty_box: true,
        images: 0,
        bytes: 0,
    };
    let writes = agent.live.input_writes();
    let failure = agent
        .live
        .write_message_into_box(
            &[Input::Paste("/status".into())],
            "/status",
            true,
            BoxIs::Unclearable {
                unsent: false,
                read_before: None,
            },
            Some(&tail),
        )
        .unwrap_err();
    assert_eq!(failure.written, 0);
    assert!(failure
        .error
        .get_ref()
        .is_some_and(|error| error.is::<term::Unwritable>()));
    assert_eq!(agent.live.input_writes(), writes);
    assert!(agent.logged().is_empty());
}

/// A command the agent's completion would run as another, written over a stashed draft: the clear
/// is allowed inside the replacement, the command is refused to its sender whole, and the stash is
/// restored with one key, instead of the command staying in the box and the draft in the stash.
#[test]
fn a_wrong_completion_over_a_stashed_draft_is_cleared_and_the_draft_restored() {
    let agent = agent_with(
        "composer-stash-wrong-completion",
        AgentOptions {
            stash: true,
            complete: "probecmd",
            ..AgentOptions::default()
        },
    );
    draft(&agent, "hello");
    let failure = agent.live.write_command("m", "/probe", false).unwrap_err();
    let refused = failure
        .error
        .get_ref()
        .and_then(|error| error.downcast_ref::<CommandNotRecognised>())
        .expect("a command not recognised, not a fragment");
    assert!(refused.cleared);
    assert_eq!(
        agent.logged(),
        ["STASHKEY", "PASTE /probe", "CLEARED", "STASHKEY"]
    );
    assert_eq!(
        agent
            .live
            .input_box(&agent.live.screen_text_undimmed())
            .as_deref(),
        Some("hello"),
        "the draft is back"
    );
    assert!(!agent.live.input_swap_active());
    assert!(!agent.live.stash_unsettled());
}

/// Whether a box shows the message written, as opposed to a draft the agent put back late, decides
/// whether an unconfirmed restore means the message was left unsent.
#[test]
fn only_a_box_showing_the_message_itself_is_taken_for_an_unsent_one() {
    let agent = agent("composer-shows-message", "yes", "0", "");
    draft(&agent, "hello");
    assert!(
        !agent.live.box_shows_the_message(0),
        "a draft is not the message"
    );
    let agent = self::agent("composer-shows-message-2", "yes", "0", "");
    agent
        .live
        .write_message_input_as("m", &[Input::Paste("/status".into())], "/status", false)
        .unwrap();
    let started = Instant::now();
    while !agent.live.box_shows_the_message(0) {
        assert!(started.elapsed() < PATIENCE, "the message was not seen");
        std::thread::sleep(Duration::from_millis(20));
    }
}
