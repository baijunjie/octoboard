use super::redelivery::MAX_REDELIVERIES;
use super::*;
use crate::protocol::AgentId;
use crate::session::dialogs::{Dialog, WrittenMessage};
use std::cell::Cell;
use std::sync::Arc;

/// An agent mid-turn, or one that reports no prompts at all: nothing written is waited on.
struct MidTurn;

impl PromptSignal for MidTurn {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        false
    }
    fn wait_past(&self, _before: u64) -> bool {
        unreachable!("nothing is waited on mid-turn")
    }
    fn mark_unsent(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn mark_not_submitted(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {
        unreachable!("nothing is checked mid-turn")
    }
    fn trust_holds_writes(&self) -> bool {
        false
    }
}

impl Outbox {
    /// Queues `text` for `id`, has it written and puts it back held
    /// ([`Self::redeliver_held`]), with no PTY behind the write: the queue a session is left
    /// with after an interrupt cleared the message its agent gave back, for a test of what a
    /// caller answers of it.
    pub(crate) fn queue_held(&self, id: &str, text: &str) {
        let ticket = self.push(id, text).expect("accepted");
        self.drain_with(id, &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
        assert!(self.redeliver_held(id, &format!("launch:{ticket}")));
    }

    /// How many drains have been started, whatever they found to write.
    pub(crate) fn drains_started(&self) -> u64 {
        self.drains.load(Ordering::Acquire)
    }
}

/// A message cleared from the input box during the write of the next one is put back at the
/// front of the queue and written again right after that one, under a new ticket and with its
/// own text; only the latest written into the session, and once for each time it was written.
#[test]
fn a_cleared_message_is_written_again_right_after_the_one_that_cleared_it() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    let second = outbox.push("s", "two").expect("accepted");
    outbox.push("s", "three").expect("accepted");
    let id = |ticket: Ticket| format!("launch:{ticket}");
    assert!(!outbox.redeliver("t", &id(first)), "another session");
    assert!(!outbox.redeliver("s", "no ticket"));
    assert!(!outbox.redeliver("s", &id(second)), "not written yet");

    let mut written = Vec::new();
    let drain = outbox.drain_with(
        "s",
        &mut |ticket, text, _| {
            written.push(text.to_string());
            // The write of the second message clears the first, still the latest written.
            if ticket == second {
                assert!(outbox.redeliver("s", &id(first)));
                assert!(!outbox.redeliver("s", &id(first)), "once a write");
            }
            Ok(Route::Terminal)
        },
        &MidTurn,
    );
    assert_eq!(drain, Drain::Clear);
    assert_eq!(written, ["two", "one", "three"]);
    assert!(
        !outbox.redeliver("s", &id(first)),
        "a later message has been written since"
    );
}

/// A message written again goes under a new ticket, and the session that sent it is still found
/// as its sender by the id that new ticket is written under, so a cancel or a turn's end that
/// loses it after it is written again still tells that session.
#[test]
fn a_message_written_again_keeps_its_sender() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    outbox.note_sender(first, "sender");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    let id = |ticket: Ticket| format!("launch:{ticket}");
    assert!(outbox.redeliver_held("s", &id(first)));
    outbox.push("s", "two").expect("accepted");

    let mut written = Vec::new();
    outbox.drain_with(
        "s",
        &mut |ticket, text, _| {
            written.push((ticket, text.to_string()));
            Ok(Route::Terminal)
        },
        &MidTurn,
    );
    let again = written
        .iter()
        .find(|(_, text)| text == "one")
        .map(|(ticket, _)| *ticket)
        .expect("written again");
    assert_ne!(again, first, "under a new ticket");
    assert_eq!(outbox.sender_of(&id(again)).as_deref(), Some("sender"));
    let two = written
        .iter()
        .find(|(_, text)| text == "two")
        .map(|(ticket, _)| *ticket)
        .expect("written");
    assert_eq!(outbox.sender_of(&id(two)), None, "sent by nobody");
}

/// The message whose write cleared the one before, put back because nothing of it went in, is
/// still written before the one queued again.
#[test]
fn a_message_put_back_stays_ahead_of_the_one_it_cleared() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    outbox.push("s", "two").expect("accepted");
    let drain = outbox.drain_with(
        "s",
        &mut |_, _, _| {
            assert!(outbox.redeliver("s", &format!("launch:{first}")));
            Err(PartialWrite {
                written: 0,
                error: std::io::Error::from(std::io::ErrorKind::WouldBlock),
            })
        },
        &MidTurn,
    );
    assert_eq!(drain, Drain::Pending);
    let mut written = Vec::new();
    outbox.drain_with(
        "s",
        &mut |_, text, _| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        },
        &MidTurn,
    );
    assert_eq!(written, ["two", "one"]);
}

/// A message put back held is taken by no drain, however often one runs, until another message
/// is taken for the session, and is then written right after that one; it still counts as
/// queued, and a drain that finds only it ends clear rather than going round.
#[test]
fn a_message_put_back_held_waits_for_the_next_message_and_follows_it() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    assert!(outbox.redeliver_held("s", &format!("launch:{first}")));
    assert!(
        !outbox.redeliver_held("s", &format!("launch:{first}")),
        "once a write"
    );
    let mut written = Vec::new();
    let drain = |outbox: &Outbox, written: &mut Vec<String>| {
        outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(Route::Terminal)
            },
            &MidTurn,
        )
    };
    for _ in 0..2 {
        assert_eq!(drain(&outbox, &mut written), Drain::Clear);
        assert!(written.is_empty(), "{written:?}");
        assert_eq!(outbox.len("s"), 1, "held, and still queued");
    }
    outbox.push("s", "two").expect("accepted");
    outbox.push("s", "three").expect("accepted");
    assert_eq!(drain(&outbox, &mut written), Drain::Clear);
    assert_eq!(written, ["two", "one", "three"]);
    assert_eq!(outbox.len("s"), 0);
}

/// A held message goes where the queue goes: taken with it for a successor or the next daemon,
/// as a plain message, and forgotten with it; and it counts against the bound as any message
/// written again.
#[test]
fn a_held_message_is_taken_and_forgotten_with_its_queue_and_counts_against_the_bound() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    assert!(outbox.redeliver_held("s", &format!("launch:{first}")));
    let taken: Vec<String> = outbox.take("s").into_iter().map(|(text, _)| text).collect();
    assert_eq!(taken, ["one"]);

    let again = outbox.push("s", "one").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    assert!(outbox.redeliver_held("s", &format!("launch:{again}")));
    assert_eq!(outbox.forget("s"), 1);
    assert_eq!(outbox.len("s"), 0);

    outbox.push("s", "one").expect("accepted");
    for round in 0..=MAX_REDELIVERIES {
        let mut wrote = Vec::new();
        // Held from the round before, it waits for a message to follow, and is written last.
        if round > 0 {
            outbox.push("s", "next").expect("accepted");
        }
        outbox.drain_with(
            "s",
            &mut |ticket, text, _| {
                if text == "one" {
                    wrote.push(ticket);
                }
                Ok(Route::Terminal)
            },
            &MidTurn,
        );
        assert_eq!(wrote.len(), 1, "round {round}");
        let held = outbox.redeliver_held("s", &format!("launch:{}", wrote[0]));
        assert_eq!(held, round < MAX_REDELIVERIES, "round {round}");
    }
}

/// A session whose queue holds only a held message has nothing pending: no drain will take it,
/// so whoever asks whether the session is busy is told it is not, while the count of what is
/// queued still shows it.
#[test]
fn a_held_message_alone_is_queued_but_not_pending() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    assert!(outbox.redeliver_held("s", &format!("launch:{first}")));
    assert_eq!(outbox.len("s"), 1);
    assert_eq!(outbox.takeable("s"), 0);
    assert!(!outbox.is_pending("s"));
    outbox.push("s", "two").expect("accepted");
    assert_eq!(
        outbox.takeable("s"),
        1,
        "held until one is taken, not until one is queued"
    );
    assert!(outbox.is_pending("s"));
}

/// A message written again keeps its count when it is written and put back under the same
/// ticket, as one a dialog discarded is, so the bound does not start over; and the count goes
/// with the ticket when it leaves the queue, by whatever way.
#[test]
fn a_count_of_writes_again_stays_with_its_ticket_until_the_ticket_leaves() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    assert!(outbox.redeliver("s", &format!("launch:{first}")));
    let count = |outbox: &Outbox| {
        outbox
            .redeliveries
            .lock()
            .unwrap()
            .values()
            .copied()
            .collect::<Vec<u8>>()
    };
    assert_eq!(count(&outbox), [1]);
    // Written, then put back at the front as the same ticket: read again, it still counts one.
    let (ticket, text, _) = outbox.pop("s").expect("queued again");
    outbox.note_latest("s", ticket, &text, false, false);
    outbox.push_front("s", ticket, text.clone(), None);
    outbox.note_latest("s", ticket, &text, false, false);
    assert_eq!(count(&outbox), [1]);
    assert!(
        outbox.redeliver("s", &format!("launch:{ticket}")),
        "once more"
    );
    assert_eq!(count(&outbox).len(), 2, "the old ticket is still queued");
    // Both leave: one taken for a successor, the other written.
    outbox.take("s");
    assert!(count(&outbox).is_empty(), "{:?}", count(&outbox));

    let again = outbox.push("s", "one").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    assert!(outbox.redeliver("s", &format!("launch:{again}")));
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Queued), &MidTurn);
    assert!(count(&outbox).is_empty(), "taken by the agent's own queue");
}

/// A message of the user's, which goes past a hold for the account's usage limit, still does
/// when it is written again.
#[test]
fn a_message_past_the_usage_limit_is_written_again_past_it() {
    let outbox = Outbox::default();
    let first = outbox.push_past_usage_limit("s", "mine").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    assert!(outbox.redeliver("s", &format!("launch:{first}")));
    assert!(
        outbox.pop_past_usage_limit("s").is_some(),
        "taken past the hold"
    );
}

/// A message given back and cleared every time is written again [`MAX_REDELIVERIES`] times and
/// no more, and one that is more than text never: its text alone would not be the message.
#[test]
fn a_message_is_written_again_a_bounded_number_of_times() {
    let outbox = Outbox::default();
    outbox.push("s", "one").expect("accepted");
    let mut latest = None;
    let write = |outbox: &Outbox| {
        let mut wrote = Vec::new();
        outbox.drain_with(
            "s",
            &mut |ticket, text, _| {
                wrote.push((ticket, text.to_string()));
                Ok(Route::Terminal)
            },
            &MidTurn,
        );
        wrote
    };
    for round in 0..=MAX_REDELIVERIES {
        let wrote = write(&outbox);
        assert_eq!(wrote.len(), 1, "round {round}");
        assert_eq!(wrote[0].1, "one");
        assert_ne!(Some(wrote[0].0), latest, "a ticket of its own each time");
        latest = Some(wrote[0].0);
        let again = outbox.redeliver("s", &format!("launch:{}", wrote[0].0));
        assert_eq!(again, round < MAX_REDELIVERIES, "round {round}");
    }
    assert_eq!(outbox.len("s"), 0);

    let shaped = outbox
        .push_shaped("s", "/compact", Shape::Command, false)
        .expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    assert!(!outbox.redeliver("s", &format!("launch:{shaped}")));
}

/// A drain whose session takes up a message given back while it was written, as a running
/// session does, and keeps what came of each.
struct TakesGiveBacksUp<'a> {
    outbox: &'a Outbox,
    became: std::cell::RefCell<Vec<Returned>>,
}

impl PromptSignal for TakesGiveBacksUp<'_> {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        false
    }
    fn wait_past(&self, _before: u64) -> bool {
        unreachable!("nothing is waited on mid-turn")
    }
    fn mark_unsent(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn mark_not_submitted(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {
        unreachable!("nothing is checked mid-turn")
    }
    fn trust_holds_writes(&self) -> bool {
        false
    }
    fn given_back_while_written(&self, ticket: Ticket) {
        let became = self
            .outbox
            .redeliver_returned("s", &format!("launch:{ticket}"));
        self.became.borrow_mut().push(became);
    }
}

/// A message the session's server or channel took and the agent gave back before the write had
/// returned, when the queue does not hold it as taken yet, is not lost: the give-back is kept
/// while the message is written and taken up once the write is over, so the message is queued
/// again, held. The give-back comes from inside the write itself, which is the only place it can
/// come from before the take is noted.
#[test]
fn a_message_given_back_while_it_is_written_is_queued_again_once_the_write_is_over() {
    let outbox = Outbox::default();
    let ticket = outbox.push("s", "one").expect("accepted");
    let prompts = TakesGiveBacksUp {
        outbox: &outbox,
        became: std::cell::RefCell::new(Vec::new()),
    };
    let mut during = Vec::new();
    let drained = outbox.drain_with(
        "s",
        &mut |ticket, _, _| {
            during.push(outbox.redeliver_returned("s", &format!("launch:{ticket}")));
            Ok(Route::Queued)
        },
        &prompts,
    );
    assert_eq!(drained, Drain::Clear);
    assert_eq!(during, [Returned::Writing]);
    assert_eq!(*prompts.became.borrow(), [Returned::Again]);
    assert_eq!(outbox.len("s"), 1, "queued again");
    assert_eq!(
        outbox.takeable("s"),
        0,
        "held until another message is taken"
    );
    // Given back once: nothing more comes of naming it again.
    assert_eq!(
        outbox.redeliver_returned("s", &format!("launch:{ticket}")),
        Returned::Unknown
    );
}

/// A give-back that names a message being written into the terminal comes to nothing: no server
/// or channel took it, so there is nothing of it to give back, during the write or after it.
#[test]
fn a_give_back_of_a_message_written_into_the_terminal_comes_to_nothing() {
    let outbox = Outbox::default();
    let ticket = outbox.push("s", "one").expect("accepted");
    let prompts = TakesGiveBacksUp {
        outbox: &outbox,
        became: std::cell::RefCell::new(Vec::new()),
    };
    outbox.drain_with(
        "s",
        &mut |ticket, _, _| {
            let _ = outbox.redeliver_returned("s", &format!("launch:{ticket}"));
            Ok(Route::Terminal)
        },
        &prompts,
    );
    assert!(prompts.became.borrow().is_empty());
    assert_eq!(outbox.len("s"), 0);
    assert_eq!(
        outbox.redeliver_returned("s", &format!("launch:{ticket}")),
        Returned::Unknown
    );
}

/// The id a message is written under outlives the daemon run in an agent's own queue, so a
/// later run's tickets must start where no earlier run reached: from the clock, in
/// microseconds since the Unix epoch.
#[test]
fn an_outbox_seeded_from_the_clock_numbers_its_tickets_from_the_time_it_was_made() {
    let micros = || {
        u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after the epoch")
                .as_micros(),
        )
        .unwrap()
    };
    let before = micros();
    let outbox = Outbox::seeded_from_clock();
    let first = outbox.push("s", "one").expect("accepted");
    let second = outbox.push("s", "two").expect("accepted");
    let after = micros();
    assert!(
        (before..=after).contains(&first),
        "{first} is not between {before} and {after}"
    );
    assert_eq!(second, first + 1, "tickets still follow one another");
}

/// An agent that reports each prompt it sends and is at its prompt, whose reports are never
/// awaited by a test that does not expect to.
struct ReportsPrompts;

impl PromptSignal for ReportsPrompts {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        true
    }
    fn wait_past(&self, _before: u64) -> bool {
        false
    }
    fn mark_unsent(&self, _before: u64) {}
    fn mark_not_submitted(&self, _before: u64) {}
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {}
    fn trust_holds_writes(&self) -> bool {
        false
    }
}

/// A slash command reports no prompt when it is one of the agent's own, so a command written
/// and out of the box is written, where a message the agent does not report sending is unsent.
#[test]
fn a_command_is_not_waited_on_for_a_prompt_report_a_message_is() {
    let outbox = Outbox::default();
    outbox
        .push_shaped("s", "/status", Shape::Command, true)
        .expect("accepted");
    let outcome = outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &ReportsPrompts);
    assert_eq!(outcome, Drain::Clear);

    let message = outbox.push("s", "hello").expect("accepted");
    let outcome = outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &ReportsPrompts);
    assert_eq!(outcome, Drain::Unsent(message));
}

/// [`ReportsPrompts`] whose provider declares an empty box after an unconfirmed write to mean the
/// message went elsewhere, whose input box reads empty after every write, and that records whether
/// a message was marked unsent.
#[derive(Default)]
struct EmptiesItsBox {
    marked_unsent: std::cell::Cell<bool>,
}

impl PromptSignal for EmptiesItsBox {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        true
    }
    fn wait_past(&self, _before: u64) -> bool {
        false
    }
    fn mark_unsent(&self, _before: u64) {
        self.marked_unsent.set(true);
    }
    fn mark_not_submitted(&self, _before: u64) {}
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {}
    fn trust_holds_writes(&self) -> bool {
        false
    }
    fn write_went_elsewhere(&self) -> bool {
        true
    }
}

/// A message the agent does not report sending, whose input box then reads empty, went
/// elsewhere: it is told as such, never marked unsent (nothing of it is in the box to wait on or
/// press Enter on), not written again, and the message behind it is still queued, for a drain of
/// its own.
#[test]
fn a_message_unsent_with_its_box_empty_went_elsewhere_and_holds_nothing() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "hello").expect("accepted");
    outbox.push("s", "behind").expect("accepted");
    let prompts = EmptiesItsBox::default();
    let mut written = Vec::new();
    let outcome = outbox.drain_with(
        "s",
        &mut |_, text, _| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        },
        &prompts,
    );
    assert_eq!(outcome, Drain::WentElsewhere(first));
    assert!(!prompts.marked_unsent.get());
    assert_eq!(written, ["hello"]);
    assert_eq!(outbox.len("s"), 1);
}

/// A slash command the agent would have run another way fails alone with the command it
/// offered, and the queue behind it goes on.
#[test]
fn a_command_the_agent_would_run_as_another_is_refused_with_what_it_offered() {
    let outbox = Outbox::default();
    let refused = outbox
        .push_shaped("s", "/probe", Shape::Command, true)
        .expect("accepted");
    let next = outbox.push("s", "next").expect("accepted");
    let mut attempts = Vec::new();
    let outcome = outbox.drain_with(
        "s",
        &mut |ticket, _, _| {
            attempts.push(ticket);
            if ticket == refused {
                Err(PartialWrite {
                    written: 1,
                    error: std::io::Error::other(term::CommandNotRecognised {
                        command: "/probe".into(),
                        offered: "/probecmd".into(),
                        cleared: true,
                        reason: "another".into(),
                    }),
                })
            } else {
                Ok(Route::Queued)
            }
        },
        &MidTurn,
    );
    assert_eq!(outcome, Drain::Clear);
    assert_eq!(attempts, [refused, next]);
    let announced = outbox.refusals_to_announce("s");
    assert_eq!(announced.len(), 1);
    assert_eq!(
        announced[0].1.kind,
        RefusalKind::OtherCommand("/probe".into(), "/probecmd".into())
    );
    assert_eq!(
        outbox.take_refusal(refused),
        Some(Refused {
            reason: "another".to_string(),
            kind: RefusalKind::OtherCommand("/probe".to_string(), "/probecmd".to_string()),
        })
    );
    assert_eq!(
        outbox.shape_of(refused),
        None,
        "the shape goes with the message"
    );
}

/// A message with images whose files are gone (the scratch directory went with the process) is
/// refused to its sender and the queue goes on; nothing is written into the terminal.
#[test]
fn a_message_whose_images_are_gone_is_refused_and_the_queue_goes_on() {
    let live = crate::test_support::idle_stand_in("outbox-images-gone");
    let outbox = Outbox::default();
    let before = live.input_writes();
    let gone = outbox
        .push_shaped(
            "outbox-images-gone",
            "look",
            Shape::Images(vec!["/no/such/dir/paste-1.png".to_string()]),
            true,
        )
        .expect("accepted");
    assert_eq!(outbox.drain(&live, &MidTurn), Drain::Clear);
    let announced = outbox.refusals_to_announce("outbox-images-gone");
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].1.kind, RefusalKind::Unwritable);
    let refused = outbox.take_refusal(gone).expect("refused");
    assert_eq!(refused.kind, RefusalKind::Unwritable);
    assert!(refused.reason.contains("image"), "{}", refused.reason);
    assert_eq!(live.input_writes(), before);
}

/// A message with images or a slash command cannot be saved as text for the next daemon: the
/// seal drops it, and keeps the plain messages around it in order.
#[test]
fn sealing_drops_the_messages_that_are_more_than_text() {
    let outbox = Outbox::default();
    outbox.push("s", "one").expect("accepted");
    let shaped = outbox
        .push_shaped("s", "/status", Shape::Command, true)
        .expect("accepted");
    outbox
        .push_shaped("s", "", Shape::Images(vec!["/a.png".into()]), false)
        .expect("accepted");
    outbox.push("s", "two").expect("accepted");
    let sealed = outbox.seal(Duration::ZERO);
    let texts: Vec<&str> = sealed[0]
        .1
        .iter()
        .map(|(_, text, _)| text.as_str())
        .collect();
    assert_eq!(texts, ["one", "two"]);
    assert_eq!(outbox.shape_of(shaped), None);
}

/// What a hold takes to forward as text leaves out the messages that are more than text, and
/// says how many.
#[test]
fn taking_a_queue_to_forward_leaves_out_and_counts_the_shaped_messages() {
    let outbox = Outbox::default();
    outbox.push("s", "one").expect("accepted");
    outbox
        .push_shaped("s", "/status", Shape::Command, true)
        .expect("accepted");
    outbox.push("s", "two").expect("accepted");
    let (taken, shaped) = outbox.take_counting_shaped("s");
    let texts: Vec<String> = taken.into_iter().map(|(text, _)| text).collect();
    assert_eq!(texts, ["one", "two"]);
    assert_eq!(shaped, 1);
    assert_eq!(outbox.len("s"), 0);
}

/// A message the agent's own queue refused outright fails alone: it is not written again, its
/// sender can be told why, and the queue behind it goes on.
#[test]
fn a_refused_message_fails_alone_and_the_queue_goes_on() {
    let outbox = Outbox::default();
    let refused = outbox.push("s", "refused").expect("accepted");
    let next = outbox.push("s", "next").expect("accepted");
    let mut attempts = Vec::new();
    let outcome = outbox.drain_with(
        "s",
        &mut |ticket, text, _| {
            attempts.push(ticket);
            if text == "refused" {
                Err(PartialWrite {
                    written: 0,
                    error: std::io::Error::other(crate::session::term::RefusedByAgent(
                        "input too long".into(),
                    )),
                })
            } else {
                Ok(Route::Queued)
            }
        },
        &MidTurn,
    );
    assert_eq!(outcome, Drain::Clear);
    assert_eq!(attempts, [refused, next]);
    // Another flush of the session, one that lost the drain claim (a hook's, the server's
    // readiness), announces it before its sender looks: the sender still finds it, by its
    // ticket, and so is never told the message was delivered.
    let announced = outbox.refusals_to_announce("s");
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].0, refused);
    assert!(outbox.refusals_to_announce("s").is_empty());
    assert!(outbox.refusals_to_announce("other").is_empty());
    assert!(outbox
        .take_refusal(refused)
        .is_some_and(|refused| refused.reason.contains("input too long")));
    assert_eq!(outbox.take_refusal(refused), None);
    assert_eq!(outbox.take_refusal(next), None);
}

/// An announced refusal is kept for its sender while a drain of its session runs, however
/// long that drain goes on, and dropped once it has been left that long with none running.
#[test]
fn a_refusal_outlasts_the_drain_its_sender_waits_on() {
    for draining in [true, false] {
        let outbox = Outbox::default();
        let long_ago = Instant::now()
            .checked_sub(REFUSAL_KEPT + Duration::from_secs(1))
            .unwrap();
        outbox.refused.lock().unwrap().insert(
            7,
            Refusal {
                session: "s".into(),
                refused: Refused {
                    reason: "input too long".into(),
                    kind: RefusalKind::Agent,
                },
                announced: Some(long_ago),
            },
        );
        let claim = draining.then(|| outbox.claim("s").unwrap());
        drop(claim);
        assert!(outbox.refusals_to_announce("other").is_empty());
        assert_eq!(outbox.take_refusal(7).is_some(), draining);
    }
}

/// [`MidTurn`] for a session whose writes are held for its account's usage limit, or not.
struct UsageLimited(Cell<bool>);

impl PromptSignal for UsageLimited {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        false
    }
    fn wait_past(&self, _before: u64) -> bool {
        unreachable!("nothing is waited on mid-turn")
    }
    fn mark_unsent(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn mark_not_submitted(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {
        unreachable!("nothing is checked mid-turn")
    }
    fn trust_holds_writes(&self) -> bool {
        false
    }
    fn usage_limit_held(&self) -> bool {
        self.0.get()
    }
}

/// A message of the user's put back held stays held under a usage-limit hold: going past that
/// hold does not have a drain take it by itself, nor does a message the limit holds beside it;
/// it follows the next message that is taken.
#[test]
fn a_held_message_is_not_taken_for_going_past_a_usage_limit_hold() {
    let outbox = Outbox::default();
    let limited = UsageLimited(Cell::new(true));
    let mut written = Vec::new();
    let mut write = |_, text: &str, _| {
        written.push(text.to_string());
        Ok(Route::Terminal)
    };
    let mine = outbox.push_past_usage_limit("s", "mine").expect("accepted");
    outbox.drain_with("s", &mut write, &limited);
    assert!(outbox.redeliver_held("s", &format!("launch:{mine}")));

    outbox.drain_with("s", &mut write, &limited);
    outbox.push("s", "peer");
    outbox.drain_with("s", &mut write, &limited);
    assert_eq!(outbox.len("s"), 2);

    outbox.push_past_usage_limit("s", "mine too");
    outbox.drain_with("s", &mut write, &limited);
    assert_eq!(outbox.len("s"), 1, "the peer's waits for the limit");
    drop(write);
    assert_eq!(written, ["mine", "mine too", "mine"]);
}

/// While a usage limit holds the session's writes, only the user's messages go, in their
/// order, past the ones held; once it lets go the held ones follow in theirs.
#[test]
fn a_usage_limit_hold_lets_only_the_user_s_messages_past() {
    let outbox = Outbox::default();
    let held = UsageLimited(Cell::new(true));
    outbox.push("s", "first");
    outbox.push_past_usage_limit("s", "mine");
    outbox.push("s", "second");
    outbox.push_past_usage_limit("s", "mine too");
    let mut written = Vec::new();
    let mut write = |_, text: &str, _| {
        written.push(text.to_string());
        Ok(Route::Terminal)
    };
    assert_eq!(
        outbox.drain_with("s", &mut write, &held),
        Drain::HeldForUsageLimit
    );
    assert_eq!(outbox.len("s"), 2);
    assert!(outbox.past_usage_limit.lock().unwrap().is_empty());
    assert_eq!(
        outbox.drain_with("s", &mut write, &held),
        Drain::HeldForUsageLimit
    );
    held.0.set(false);
    assert_eq!(outbox.drain_with("s", &mut write, &held), Drain::Clear);
    // Nothing held and nothing of the user's: an empty queue is clear, as ever.
    held.0.set(true);
    assert_eq!(outbox.drain_with("s", &mut write, &held), Drain::Clear);
    assert_eq!(written, ["mine", "mine too", "first", "second"]);
}

/// What a hold of the server noted of a message's grace goes once the message leaves the
/// queue.
#[test]
fn a_grace_restart_is_forgotten_once_the_message_is_written() {
    let outbox = Outbox::default();
    outbox.push("s", "message");
    let mut held = true;
    let mut write = |_, _: &str, _| {
        if std::mem::take(&mut held) {
            Err(PartialWrite {
                written: 0,
                error: std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    crate::session::term::HeldByServer("not yet".into()),
                ),
            })
        } else {
            Ok(Route::Queued)
        }
    };
    assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
    assert_eq!(outbox.grace_restarted.lock().unwrap().len(), 1);
    assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Clear);
    assert!(outbox.grace_restarted.lock().unwrap().is_empty());
}

/// A hold of the session's own server starts a message's draft grace again only once, so a
/// session flipping between that hold and another cannot keep the message waiting past it.
#[test]
fn a_hold_of_the_server_starts_the_grace_again_once() {
    let outbox = Outbox::default();
    outbox.push("s", "message");
    let held = || {
        Err(PartialWrite {
            written: 0,
            error: std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                crate::session::term::HeldByServer("not yet".into()),
            ),
        })
    };
    let front = |outbox: &Outbox| outbox.queues.lock().unwrap()["s"].front().unwrap().2;
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| held(), &MidTurn),
        Drain::Pending
    );
    assert_eq!(front(&outbox), None);
    // The next attempt starts its grace; a second hold keeps it.
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| held(), &MidTurn),
        Drain::Pending
    );
    assert!(front(&outbox).is_some());
}

/// A message the agent's own server holds back has not waited for a draft: its grace starts
/// afresh, so a terminal that takes it later still waits for the user's draft. Other holds of
/// nothing written keep the grace they began.
#[test]
fn a_message_held_by_the_server_keeps_its_whole_draft_grace() {
    for (held_by_server, keeps) in [(true, false), (false, true)] {
        let outbox = Outbox::default();
        outbox.push("s", "message");
        let outcome = outbox.drain_with(
            "s",
            &mut |_, _, _| {
                Err(PartialWrite {
                    written: 0,
                    error: if held_by_server {
                        std::io::Error::new(
                            std::io::ErrorKind::WouldBlock,
                            crate::session::term::HeldByServer("not yet".into()),
                        )
                    } else {
                        std::io::Error::new(std::io::ErrorKind::WouldBlock, "composer busy")
                    },
                })
            },
            &MidTurn,
        );
        assert_eq!(outcome, Drain::Pending);
        let queues = outbox.queues.lock().unwrap();
        let (_, _, protection_started) = queues["s"].front().unwrap();
        assert_eq!(protection_started.is_some(), keeps);
    }
}

/// [`MidTurn`] with the session's input held, as for a draft being stashed or a fragment.
struct HeldInput;

impl PromptSignal for HeldInput {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        false
    }
    fn wait_past(&self, _before: u64) -> bool {
        unreachable!("nothing is written while the input is held")
    }
    fn mark_unsent(&self, _before: u64) {
        unreachable!("nothing is written while the input is held")
    }
    fn mark_not_submitted(&self, _before: u64) {
        unreachable!("nothing is written while the input is held")
    }
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {
        unreachable!("nothing is checked while the input is held")
    }
    fn trust_holds_writes(&self) -> bool {
        false
    }
    fn input_held(&self) -> bool {
        true
    }
}

/// An agent at its prompt that sends a written message, or none while `sends` is off, and is
/// busy with a turn once anything is sent. [`Self::prompt_sent`] is any prompt reported sent,
/// the agent's own or one from elsewhere, such as the user pressing Enter.
struct AtPrompt {
    sent: Cell<u64>,
    sends: Cell<bool>,
    waits: Cell<u32>,
    unsent: Cell<Option<u64>>,
    /// The message marked unsent was left in the box, its submit key pressed or held back.
    not_submitted: Cell<bool>,
    busy: Cell<bool>,
    dialog: Cell<Option<Dialog>>,
    /// What the screen has shown so far.
    sighting: Cell<Sighting>,
    /// A dialog that comes up while the next message is waited on, and whether it is gone
    /// again by the time the wait ends.
    dialog_on_wait: Cell<Option<(Dialog, bool)>>,
    /// Screen watches in force, and the most that ever were.
    watching: Cell<i32>,
    watched_during_wait: Cell<bool>,
    trust_holds: Cell<bool>,
    recovering_input: Cell<bool>,
}

impl AtPrompt {
    fn new(sends: bool) -> Self {
        Self {
            sent: Cell::new(0),
            sends: Cell::new(sends),
            waits: Cell::new(0),
            unsent: Cell::new(None),
            not_submitted: Cell::new(false),
            busy: Cell::new(false),
            dialog: Cell::new(None),
            sighting: Cell::new(Sighting::default()),
            dialog_on_wait: Cell::new(None),
            watching: Cell::new(0),
            watched_during_wait: Cell::new(false),
            trust_holds: Cell::new(false),
            recovering_input: Cell::new(false),
        }
    }

    /// The screen now shows `dialog`, or none.
    fn show(&self, dialog: Option<Dialog>) {
        self.dialog.set(dialog);
        self.sighting.set(self.sighting.get().after(dialog));
    }

    fn prompt_sent(&self) {
        self.sent.set(self.sent.get() + 1);
        self.busy.set(true);
    }
}

impl PromptSignal for AtPrompt {
    fn sent(&self) -> u64 {
        self.sent.get()
    }

    fn input_recovery_pending(&self) -> bool {
        self.recovering_input.get()
    }
    fn expects_send(&self) -> bool {
        !self.busy.get()
    }
    fn wait_past(&self, before: u64) -> bool {
        self.waits.set(self.waits.get() + 1);
        self.watched_during_wait.set(self.watching.get() > 0);
        if let Some((dialog, gone_again)) = self.dialog_on_wait.take() {
            self.show(Some(dialog));
            if gone_again {
                self.show(None);
            }
        }
        if self.sends.get() {
            self.prompt_sent();
        }
        self.sent.get() > before
    }
    fn mark_unsent(&self, before: u64) {
        self.unsent.set(Some(before));
    }
    fn mark_not_submitted(&self, before: u64) {
        self.unsent.set(Some(before));
        self.not_submitted.set(true);
    }
    fn unsent_waiting(&self) -> bool {
        self.unsent
            .get()
            .is_some_and(|before| self.sent.get() <= before)
    }
    fn sighting(&self) -> Sighting {
        self.sighting.get()
    }
    fn watch_screen(&self, on: bool) {
        self.watching
            .set(self.watching.get() + if on { 1 } else { -1 });
    }
    fn trust_holds_writes(&self) -> bool {
        self.trust_holds.get()
    }
}

fn dialog(written: WrittenMessage) -> Dialog {
    Dialog {
        code: "test_dialog",
        written,
        kind: crate::session::dialogs::DialogKind::Question,
    }
}

fn partial(written: usize) -> PartialWrite {
    PartialWrite {
        written,
        error: std::io::Error::new(std::io::ErrorKind::TimedOut, "no room"),
    }
}

#[test]
fn a_reserved_first_message_preserves_ticket_order_and_cancel_keeps_later_messages() {
    let outbox = Outbox::default();
    outbox.pause("s");
    let first = outbox.reserve_first("s").unwrap();
    assert!(
        outbox.reserve_first("s").is_err(),
        "the first place is already reserved"
    );
    let report = outbox.push("s", "child report").unwrap();
    let bootstrap = outbox.push_first(first, "bootstrap").unwrap();
    assert!(bootstrap < report);
    outbox.resume("s");
    let mut written = Vec::new();
    assert_eq!(
        outbox.drain_with(
            "s",
            &mut |ticket, text, _| {
                written.push((ticket, text.to_string()));
                Ok(Route::Terminal)
            },
            &MidTurn
        ),
        Drain::Clear
    );
    assert_eq!(
        written,
        [
            (bootstrap, "bootstrap".to_string()),
            (report, "child report".to_string())
        ]
    );
    outbox.pause("cancelled");
    let cancelled = outbox.reserve_first("cancelled").unwrap();
    outbox.push("cancelled", "external input");
    outbox.cancel_first(cancelled);
    outbox.resume("cancelled");
    assert_eq!(
        outbox.take("cancelled"),
        [("external input".to_string(), None)]
    );
}

#[test]
fn everything_queued_is_written_in_order() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    outbox.push("s", "second");
    let mut written = Vec::new();
    assert_eq!(
        outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(Route::Terminal)
            },
            &MidTurn
        ),
        Drain::Clear
    );
    assert_eq!(written, vec!["first", "second"]);
    assert_eq!(outbox.len("s"), 0);
}

/// A paused queue is written only once every pause on it has ended, so two that overlap do not
/// let it go early.
#[test]
fn a_paused_queue_waits_for_the_last_pause_to_end() {
    let outbox = Outbox::default();
    outbox.push("s", "held");
    outbox.pause("s");
    outbox.pause("s");
    let mut written = Vec::new();
    let mut drain = |outbox: &Outbox| {
        outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(Route::Terminal)
            },
            &MidTurn,
        )
    };
    assert_eq!(drain(&outbox), Drain::Pending);
    outbox.resume("s");
    assert_eq!(drain(&outbox), Drain::Pending);
    outbox.resume("s");
    assert_eq!(drain(&outbox), Drain::Clear);
    assert_eq!(written, vec!["held"]);
}

/// A message queued while a drain is already under way is written by that drain rather than left
/// for a later one. This does not reach the outer re-claim loop — landing between the final pop
/// and the release needs a second thread, and the argument for that window is the ordering noted
/// in `drain_with` rather than something a test can arrange.
#[test]
fn a_message_queued_during_a_drain_is_written_by_it() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    let mut written = Vec::new();
    let mut pushed_late = false;
    assert_eq!(
        outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                if !pushed_late {
                    pushed_late = true;
                    // Stands in for a producer arriving as this pass finishes: by the time the
                    // drainer re-reads the queue the message is in it.
                    outbox.push("s", "late arrival");
                }
                Ok(Route::Terminal)
            },
            &MidTurn
        ),
        Drain::Clear
    );
    assert_eq!(written, vec!["first", "late arrival"]);
}

/// A message kept in the store is queued once per row however often it is pushed, its row is
/// handed on once it is written, a queue that is forgotten or taken leaves the row alone, and a
/// queue reported lost counts as done.
#[test]
fn a_kept_message_is_queued_once_and_its_row_goes_when_it_is_written_or_lost() {
    let outbox = Outbox::default();
    assert!(outbox.push_kept("s", "kept", 7).is_some());
    assert!(outbox.push_kept("s", "kept", 7).is_none());
    assert_eq!(outbox.len("s"), 1);
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn),
        Drain::Clear
    );
    assert_eq!(outbox.take_written(), [7]);
    // Written, the row may be queued again by a later resume that still finds it.
    assert!(outbox.push_kept("s", "kept", 7).is_some());

    // Taken, it reports its row and is no longer queued.
    let taken = outbox.take("s");
    assert_eq!(taken, [("kept".to_string(), Some(7))]);
    assert!(outbox.push_kept("s", "kept", 7).is_some());
    outbox.forget("s");
    assert!(outbox.take_written().is_empty());

    // Lost: reported to the sender as undelivered, so its row and those behind it go.
    outbox.push_kept("s", "first", 1);
    outbox.push_kept("s", "second", 2);
    outbox.push("s", "plain");
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Err(partial(3)), &MidTurn),
        Drain::Lost
    );
    let mut rows = outbox.take_written();
    rows.sort();
    assert_eq!(rows, [1, 2]);
}

/// [`MidTurn`] once a planned stop has stopped the writes, after the first message written.
struct StopsAfterFirst(Cell<u32>);

impl PromptSignal for StopsAfterFirst {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        false
    }
    fn wait_past(&self, _before: u64) -> bool {
        unreachable!("nothing is waited on mid-turn")
    }
    fn mark_unsent(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn mark_not_submitted(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {
        unreachable!("nothing is checked mid-turn")
    }
    fn trust_holds_writes(&self) -> bool {
        false
    }
    fn writes_stopped(&self) -> bool {
        let looked = self.0.get();
        self.0.set(looked + 1);
        looked > 0
    }
}

/// A drain under way when a planned stop stops the writes finishes the message it is writing
/// and begins no other: the rest stays queued, whole and in order, for the next daemon.
#[test]
fn a_drain_stops_between_messages_once_the_writes_are_stopped() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    outbox.push("s", "second");
    outbox.push("s", "third");
    let mut written = Vec::new();
    assert_eq!(
        outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(Route::Terminal)
            },
            &StopsAfterFirst(Cell::new(0))
        ),
        Drain::Pending
    );
    assert_eq!(written, vec!["first"]);
    let sealed = outbox.seal(Duration::ZERO);
    assert_eq!(sealed.len(), 1);
    let texts: Vec<&str> = sealed[0]
        .1
        .iter()
        .map(|(_, text, _)| text.as_str())
        .collect();
    assert_eq!(texts, ["second", "third"]);
}

/// [`MidTurn`] whose writes stop when the flag is raised, as a planned stop's barrier does.
struct Barrier(Arc<AtomicBool>);

impl PromptSignal for Barrier {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        false
    }
    fn wait_past(&self, _before: u64) -> bool {
        unreachable!("nothing is waited on mid-turn")
    }
    fn mark_unsent(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn mark_not_submitted(&self, _before: u64) {
        unreachable!("nothing is checked mid-turn")
    }
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {
        unreachable!("nothing is checked mid-turn")
    }
    fn trust_holds_writes(&self) -> bool {
        false
    }
    fn writes_stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// A message the agent's own server declines after the handover barrier rose is not written
/// into the terminal: it goes back to the front of its queue, and the seal takes it with the
/// rest, in order, for the next daemon.
#[test]
fn a_message_declined_after_the_barrier_rose_is_put_back_and_sealed_in_order() {
    let barrier = Arc::new(AtomicBool::new(false));
    let raised = barrier.clone();
    let server =
        crate::test_support::DecliningServer::new(move || raised.store(true, Ordering::Release));
    let live = crate::test_support::idle_stand_in_of("declined-after-barrier", AgentId::CODEX)
        .with_transport(server.clone());
    let outbox = Outbox::default();
    let first = outbox.push_kept(&live.id, "first", 7).unwrap();
    outbox.push(&live.id, "second");
    let before = live.input_writes();

    assert_eq!(
        outbox.drain(&live, &Barrier(barrier.clone())),
        Drain::Pending
    );
    assert_eq!(live.input_writes(), before, "something was written");
    assert_eq!(server.offered().len(), 1, "only the first was offered");
    assert_eq!(outbox.len(&live.id), 2);

    let sealed = outbox.seal(Duration::ZERO);
    assert_eq!(sealed.len(), 1);
    let texts: Vec<&str> = sealed[0]
        .1
        .iter()
        .map(|(_, text, _)| text.as_str())
        .collect();
    assert_eq!(texts, ["first", "second"]);
    assert_eq!(sealed[0].1[0].0, first);
    assert_eq!(sealed[0].1[0].2, Some(7), "the kept row went with it");
}

/// Before the barrier, a message the server declines is still written into the terminal and the
/// queue goes on. The stand-in is of an agent whose paste and Enter go in together on every
/// host, so the write waits on no input box the stand-in never draws.
#[test]
fn a_message_declined_before_the_barrier_is_written_into_the_terminal() {
    let barrier = Arc::new(AtomicBool::new(false));
    let server = crate::test_support::DecliningServer::new(|| {});
    let live = crate::test_support::idle_stand_in("declined-before-barrier")
        .with_transport(server.clone());
    let outbox = Outbox::default();
    outbox.push(&live.id, "first");
    let before = live.input_writes();

    assert_eq!(outbox.drain(&live, &Barrier(barrier)), Drain::Clear);
    assert_eq!(live.input_writes(), before + 1);
    assert_eq!(server.offered().len(), 1);
    assert_eq!(outbox.len(&live.id), 0);
}

/// Sealing takes every queue whole, kept rows with them, and leaves the rows unqueued for the
/// next daemon to keep; from then on the outbox takes no message by the sealed push.
#[test]
fn sealing_takes_every_queue_and_refuses_what_comes_after() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "first").unwrap();
    outbox.push_kept("s", "kept", 7);
    outbox.push("u", "other");
    assert!(!outbox.is_sealed());
    let mut sealed = outbox.seal(Duration::ZERO);
    sealed.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(sealed[0].0, "s");
    assert_eq!(sealed[0].1[0], (first, "first".to_string(), None));
    assert_eq!(sealed[0].1[1].1, "kept");
    assert_eq!(sealed[0].1[1].2, Some(7));
    assert_eq!(sealed[1].0, "u");
    assert!(outbox.is_sealed());
    assert!(!outbox.is_row_queued(7));
    assert_eq!(outbox.len("s"), 0);
    assert_eq!(outbox.push_unless_sealed("s", "late"), None);
    assert_eq!(outbox.push_kept("s", "kept late", 8), None);
    // Nor by the plain push, once the queues are taken: nothing would ever write it.
    assert_eq!(outbox.push("s", "after"), None);
    assert!(!outbox.is_row_queued(8));
    assert_eq!(outbox.len("s"), 0);
}

/// A message whose write the handover refused while the queues were being sealed is put back
/// and taken with the rest, since the seal waits for the drain still writing.
#[test]
fn a_message_put_back_while_the_seal_waits_is_taken_with_the_rest() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    outbox.push("s", "second");
    // Borrowed, so that the drain's thread takes the borrow and the channel's sender with it.
    let outbox = &outbox;
    let (writing, being_written) = std::sync::mpsc::channel();
    let sealed = std::thread::scope(|scope| {
        let draining = scope.spawn(move || {
            outbox.drain_with(
                "s",
                &mut |_, _, _| {
                    // The message is off the queue and being written: the seal starts now,
                    // and the write goes on until it has, so the seal finds it still writing.
                    writing.send(()).ok();
                    crate::session::wait::wait_for(crate::test_support::PATIENCE, || {
                        outbox.is_sealed()
                    });
                    Err(partial(0))
                },
                &MidTurn,
            )
        });
        being_written
            .recv_timeout(crate::test_support::PATIENCE)
            .expect("the drain never began its write");
        let sealed = outbox.seal(crate::test_support::PATIENCE);
        assert_eq!(draining.join().unwrap(), Drain::Pending);
        sealed
    });
    let texts: Vec<&str> = sealed[0]
        .1
        .iter()
        .map(|(_, text, _)| text.as_str())
        .collect();
    assert_eq!(texts, ["first", "second"]);
}

/// Nothing went in, so the session could not take it: it stays at the front, and the one behind
/// it must not be let past.
#[test]
fn a_message_that_could_not_be_written_stays_at_the_front() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    outbox.push("s", "second");
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Err(partial(0)), &MidTurn),
        Drain::Pending
    );
    assert_eq!(outbox.len("s"), 2);

    let mut written = Vec::new();
    assert_eq!(
        outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(Route::Terminal)
            },
            &MidTurn
        ),
        Drain::Clear
    );
    assert_eq!(written, vec!["first", "second"]);
}

/// A half-written message leaves an unterminated paste in the session's input line, so retrying
/// it would deliver the fragment twice and writing the next one would append it inside the
/// fragment. Both it and the queue behind it go, and the loss is reported.
#[test]
fn a_half_written_message_takes_the_queue_with_it_and_is_reported() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    outbox.push("s", "second");
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Err(partial(3)), &MidTurn),
        Drain::Lost
    );
    assert_eq!(outbox.len("s"), 0);
}

/// A message written in part while the draft was stashed leaves a session that holds every
/// later write until a prompt is sent after it, so only that message is lost, its row with it:
/// the queue behind it waits, rows kept, refused meanwhile, and is written in order once the
/// session takes writes again.
#[test]
fn a_held_fragment_loses_only_its_own_message() {
    let outbox = Outbox::default();
    let first = outbox.push_kept("s", "first", 1).unwrap();
    outbox.push_kept("s", "second", 2);
    outbox.push("s", "third");
    let held = || PartialWrite {
        written: 3,
        error: std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            crate::session::HeldFragment("no room".to_string()),
        ),
    };
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Err(held()), &MidTurn),
        Drain::Fragmented(first)
    );
    assert_eq!(outbox.len("s"), 2);
    assert_eq!(outbox.take_written(), [1]);
    assert!(outbox.is_row_queued(2));
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Err(partial(0)), &MidTurn),
        Drain::Pending
    );
    let mut written = Vec::new();
    assert_eq!(
        outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(Route::Terminal)
            },
            &MidTurn
        ),
        Drain::Clear
    );
    assert_eq!(written, vec!["second", "third"]);
    assert_eq!(outbox.take_written(), [2]);
}

/// A message refused while the session's input is held keeps none of its draft grace, so once
/// the hold ends it gets the whole grace again rather than having the draft cleared out of its
/// way.
#[test]
fn a_hold_on_the_input_spends_none_of_a_message_s_draft_grace() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    let (ticket, text, _) = outbox.pop("s").unwrap();
    outbox.push_front("s", ticket, text, Some(Instant::now() - DRAFT_WAIT));
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Err(partial(0)), &HeldInput),
        Drain::Pending
    );
    let mut past = None;
    assert_eq!(
        outbox.drain_with(
            "s",
            &mut |_, _, past_grace| {
                past = Some(past_grace);
                Ok(Route::Terminal)
            },
            &MidTurn
        ),
        Drain::Clear
    );
    assert_eq!(past, Some(false));
}

#[test]
fn only_one_drainer_at_a_time_owns_a_session() {
    let outbox = Outbox::default();
    let claim = outbox.claim("s");
    assert!(claim.is_some());
    assert!(outbox.claim("s").is_none());
    drop(claim);
    assert!(outbox.claim("s").is_some());
}

#[test]
fn a_drain_whose_write_panics_leaves_the_session_drainable() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        outbox.drain_with("s", &mut |_, _, _| panic!("the write failed"), &MidTurn)
    }));
    assert!(panicked.is_err());
    assert!(outbox.claim("s").is_some());
}

#[test]
fn a_drain_a_concurrent_drainer_already_owns_reports_pending() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    let _claim = outbox.claim("s").unwrap();
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn),
        Drain::Pending
    );
    assert_eq!(outbox.len("s"), 1);
}

#[test]
fn forgetting_a_session_reports_what_it_threw_away() {
    let outbox = Outbox::default();
    outbox.push("s", "one");
    outbox.push("s", "two");
    assert_eq!(outbox.forget("s"), 2);
    assert_eq!(outbox.len("s"), 0);
    assert_eq!(outbox.forget("s"), 0);
}

/// A message an idle agent sends goes through, and what follows it in the same pass is not
/// waited on: the agent is busy with the first and queues the rest itself.
#[test]
fn a_message_the_agent_sends_is_written_and_the_rest_follow_unchecked() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    outbox.push("s", "second");
    let agent = AtPrompt::new(true);
    let mut written = Vec::new();
    let outcome = outbox.drain_with(
        "s",
        &mut |_, text, _| {
            written.push(text.to_string());
            Ok(Route::Terminal)
        },
        &agent,
    );
    assert_eq!(outcome, Drain::Clear);
    assert_eq!(written, vec!["first", "second"]);
    assert_eq!(agent.waits.get(), 1);
}

/// A message the agent does not send is reported by its ticket and not put back, and nothing
/// more is written — not even a message queued afterwards — until the agent next reports a
/// prompt sent.
#[test]
fn a_message_the_agent_does_not_send_holds_the_queue_until_the_next_prompt() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "first").unwrap();
    outbox.push("s", "second");
    let agent = AtPrompt::new(false);
    let mut written = Vec::new();
    let mut write = |_: Ticket, text: &str, _: bool| {
        written.push(text.to_string());
        Ok(Route::Terminal)
    };
    assert_eq!(
        outbox.drain_with("s", &mut write, &agent),
        Drain::Unsent(first)
    );
    outbox.push("s", "third");
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
    assert_eq!(outbox.len("s"), 2);

    agent.prompt_sent();
    agent.sends.set(true);
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
    assert_eq!(written, vec!["first", "second", "third"]);
}

/// A message the write itself left in the input box, its submit key pressed or held back, is
/// unsent, for an agent whose prompts the outbox does not wait on as for one it does: not lost,
/// and the queue behind it held until a prompt is sent.
#[test]
fn a_message_left_in_the_box_by_its_submit_key_is_unsent_and_holds_the_queue() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "first").unwrap();
    outbox.push("s", "second");
    let agent = AtPrompt::new(false);
    agent.busy.set(true);
    let mut written = Vec::new();
    let mut write = |_: Ticket, text: &str, _: bool| {
        written.push(text.to_string());
        if text == "first" {
            return Err(PartialWrite {
                written: 20,
                error: std::io::Error::other(crate::session::NotSubmitted),
            });
        }
        Ok(Route::Terminal)
    };
    assert_eq!(
        outbox.drain_with("s", &mut write, &agent),
        Drain::Unsent(first)
    );
    assert!(agent.unsent_waiting());
    assert!(
        agent.not_submitted.get(),
        "a message left in its input box by the write is marked so, for no retry to press it"
    );
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
    assert_eq!(outbox.len("s"), 1);

    agent.prompt_sent();
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
    assert_eq!(written, vec!["first", "second"]);
}

/// Nothing is written while a dialog is up, and the queue goes on once it is gone.
#[test]
fn nothing_is_written_while_a_dialog_is_up() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    let agent = AtPrompt::new(true);
    agent.show(Some(dialog(WrittenMessage::Draft)));
    let mut written = Vec::new();
    let mut write = |_: Ticket, text: &str, _: bool| {
        written.push(text.to_string());
        Ok(Route::Terminal)
    };
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
    assert_eq!(outbox.len("s"), 1);

    agent.show(None);
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
    assert_eq!(written, vec!["first"]);
}

/// A trust screen sighted while a drain runs stops it at the next message.
#[test]
fn nothing_is_written_while_the_trust_screen_holds_writes() {
    let outbox = Outbox::default();
    outbox.push("s", "first");
    let agent = AtPrompt::new(true);
    agent.trust_holds.set(true);
    let mut written = Vec::new();
    let mut write = |_: Ticket, text: &str, _: bool| {
        written.push(text.to_string());
        Ok(Route::Terminal)
    };
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
    assert_eq!(outbox.len("s"), 1);
    assert!(written.is_empty());
}

/// A dialog that came up around a write threw the message away: it goes back at the front
/// under its ticket, the agent is not marked as holding an unsent message, and the message
/// behind it is not let past. The dialog is gone again by the end of the wait, as one a write
/// closes is, and the screen was being watched throughout.
#[test]
fn a_message_a_dialog_threw_away_goes_back_at_the_front() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "first").unwrap();
    outbox.push("s", "second");
    let agent = AtPrompt::new(false);
    agent
        .dialog_on_wait
        .set(Some((dialog(WrittenMessage::Discarded), true)));
    let mut written = Vec::new();
    let mut write = |_: Ticket, text: &str, _: bool| {
        written.push(text.to_string());
        Ok(Route::Terminal)
    };
    assert_eq!(
        outbox.drain_with("s", &mut write, &agent),
        Drain::Discarded(first)
    );
    assert!(agent.watched_during_wait.get());
    assert_eq!(agent.watching.get(), 0);
    assert_eq!(outbox.len("s"), 2);
    assert!(!agent.unsent_waiting());

    agent.sends.set(true);
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
    assert_eq!(written, vec!["first", "first", "second"]);
}

/// A view that took the message has delivered it, to the wrong place: it is not put back, no
/// unsent mark holds the queue, and the message behind it waits for the view to be left.
#[test]
fn a_message_a_view_took_is_not_put_back() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "first").unwrap();
    outbox.push("s", "second");
    let agent = AtPrompt::new(false);
    agent
        .dialog_on_wait
        .set(Some((dialog(WrittenMessage::Diverted), false)));
    let mut written = Vec::new();
    let mut write = |_: Ticket, text: &str, _: bool| {
        written.push(text.to_string());
        Ok(Route::Terminal)
    };
    assert_eq!(
        outbox.drain_with("s", &mut write, &agent),
        Drain::Diverted(first)
    );
    assert_eq!(outbox.len("s"), 1);
    assert!(!agent.unsent_waiting());

    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Pending);
    agent.show(None);
    agent.sends.set(true);
    assert_eq!(outbox.drain_with("s", &mut write, &agent), Drain::Clear);
    assert_eq!(written, vec!["first", "second"]);
}

/// A draft a dialog kept (the idle resume dialog) is in the input box, so it counts as unsent.
#[test]
fn a_message_left_as_a_draft_by_a_dialog_is_unsent() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "first").unwrap();
    let agent = AtPrompt::new(false);
    agent
        .dialog_on_wait
        .set(Some((dialog(WrittenMessage::Draft), false)));
    assert_eq!(
        outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &agent),
        Drain::Unsent(first)
    );
    assert_eq!(outbox.len("s"), 0);
}
#[test]
fn a_written_message_with_pending_draft_recovery_is_not_retried_or_marked_unsent() {
    let outbox = Outbox::default();
    outbox.push("s", "already written");
    outbox.push("s", "still queued");
    let agent = AtPrompt::new(false);
    agent.recovering_input.set(true);
    let mut written = Vec::new();
    assert_eq!(
        outbox.drain_with(
            "s",
            &mut |_, text, _| {
                written.push(text.to_string());
                Ok(Route::Terminal)
            },
            &agent
        ),
        Drain::Pending
    );
    assert_eq!(written, ["already written"]);
    assert_eq!(outbox.len("s"), 1);
    assert!(!agent.unsent_waiting());
    assert_eq!(agent.waits.get(), 0);
}

/// An agent whose own queue left a message's acceptance unconfirmed: it holds later messages
/// for [`DRAFT_WAIT`] after the mark, or until a prompt is reported, as a session does.
#[derive(Default)]
struct UncertainQueue {
    sent: Cell<u64>,
    mark: Cell<Option<(u64, Instant)>>,
    marked_unsent: Cell<bool>,
}

impl PromptSignal for UncertainQueue {
    fn sent(&self) -> u64 {
        self.sent.get()
    }
    fn expects_send(&self) -> bool {
        false
    }
    fn wait_past(&self, _before: u64) -> bool {
        unreachable!("nothing is waited on")
    }
    fn mark_unsent(&self, _before: u64) {
        self.marked_unsent.set(true);
    }
    fn mark_not_submitted(&self, _before: u64) {
        self.marked_unsent.set(true);
    }
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn mark_submission_uncertain(&self, before: u64) {
        self.mark.set(Some((before, Instant::now())));
    }
    fn submission_uncertain(&self) -> bool {
        self.mark
            .get()
            .is_some_and(|(before, at)| self.sent.get() <= before && at.elapsed() < DRAFT_WAIT)
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {}
    fn trust_holds_writes(&self) -> bool {
        false
    }
}

fn uncertain(written: usize) -> PartialWrite {
    PartialWrite {
        written,
        error: std::io::Error::other(crate::session::term::UncertainSubmission(
            "connection closed after add".into(),
        )),
    }
}

/// A message whose acceptance is uncertain is never written again, by the queue or the
/// terminal (no unsent mark, which would retry its submit key), even where its error says
/// nothing went in; later messages wait until a prompt is reported or the hold runs out.
#[test]
fn uncertain_native_acceptance_preserves_later_messages_without_resending() {
    for written in [1, 0] {
        for released_by_prompt in [true, false] {
            let outbox = Outbox::default();
            let first = outbox.push("s", "first").expect("accepted");
            outbox.push("s", "second");
            let agent = UncertainQueue::default();
            let mut attempts = Vec::new();
            let outcome = outbox.drain_with(
                "s",
                &mut |ticket, text, _| {
                    attempts.push((ticket, text.to_string()));
                    Err(uncertain(written))
                },
                &agent,
            );
            assert_eq!(outcome, Drain::Uncertain(first));
            assert_eq!(attempts, [(first, "first".to_string())]);
            assert_eq!(outbox.len("s"), 1);
            assert!(!agent.marked_unsent.get());
            assert_eq!(
                outbox.drain_with("s", &mut |_, _, _| panic!("held"), &agent),
                Drain::Pending
            );
            if released_by_prompt {
                agent.sent.set(agent.sent.get() + 1);
            } else {
                let (before, _) = agent.mark.get().unwrap();
                agent.mark.set(Some((before, Instant::now() - DRAFT_WAIT)));
            }
            assert_eq!(
                outbox.drain_with(
                    "s",
                    &mut |ticket, text, _| {
                        attempts.push((ticket, text.to_string()));
                        Ok(Route::Queued)
                    },
                    &agent
                ),
                Drain::Clear
            );
            assert_eq!(attempts[1].1, "second");
        }
    }
}

/// A draft left in the box holds a message through its grace only: past it, with no stash to
/// take the draft, the box is cleared and the message written into it alone.
#[test]
fn an_unsubmitted_draft_is_cleared_out_of_the_way_after_the_grace_period() {
    let outbox = Outbox::default();
    let live = crate::test_support::fake_live_session(
        "bounded-draft",
        crate::protocol::AgentId::CLAUDE,
        120,
        40,
        "sleep 30",
    )
    .with_composer();
    live.on_output(include_bytes!(
        "../../../testdata/claude_input_restored.bin"
    ));
    live.write_terminal_input(b"unfinished user input").unwrap();
    // Typed well before: text typed just now may not be drawn yet, which holds even a message
    // past its grace.
    live.settle_input();
    outbox.push(&live.id, "automatic message");
    let before = live.input_writes();
    assert_eq!(outbox.drain(&live, &MidTurn), Drain::Pending);
    assert_eq!(live.input_writes(), before);
    let queued = outbox.pop(&live.id).unwrap();
    outbox.push_front(
        &live.id,
        queued.0,
        queued.1,
        Some(Instant::now() - DRAFT_WAIT),
    );
    let agent = live.clone();
    let screen = std::thread::spawn(move || {
        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while agent.input_writes() == before {
            assert!(Instant::now() < deadline, "no key cleared the box");
            std::thread::sleep(Duration::from_millis(2));
        }
        // The Ctrl-U shows: the box is empty.
        let rule = "\u{2500}".repeat(60);
        agent.on_output(format!("\x1b[2J\x1b[30;1H{rule}\r\n\u{276f} \r\n{rule}").as_bytes());
    });
    assert_eq!(outbox.drain(&live, &MidTurn), Drain::Clear);
    screen.join().unwrap();
    assert_eq!(
        live.input_writes(),
        before + 2,
        "one Ctrl-U, then the message"
    );
    assert_eq!(outbox.len(&live.id), 0);
}

#[test]
fn draft_wait_expires_for_the_same_ticket_across_retries() {
    let outbox = Outbox::default();
    let ticket = outbox.push("s", "automatic").unwrap();
    let mut allowances = Vec::new();
    let mut write = |seen, _: &str, past_grace| {
        assert_eq!(seen, ticket);
        allowances.push(past_grace);
        Err(PartialWrite {
            written: 0,
            error: std::io::ErrorKind::WouldBlock.into(),
        })
    };
    assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
    let queued = outbox.pop("s").unwrap();
    assert!(queued.2.is_some());
    outbox.push_front("s", queued.0, queued.1, Some(Instant::now() - DRAFT_WAIT));
    assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
    assert_eq!(outbox.drain_with("s", &mut write, &MidTurn), Drain::Pending);
    assert_eq!(allowances, [false, true, true]);
}

/// Several messages written into the terminal and cleared together are each written again:
/// put back latest first, they come out in the order they were written in, right after the
/// message whose write cleared them.
#[test]
fn several_cleared_messages_are_written_again_in_their_order() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    let second = outbox.push("s", "two").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    let third = outbox.push("s", "three").expect("accepted");
    let id = |ticket: Ticket| format!("launch:{ticket}");
    let mut written = Vec::new();
    outbox.drain_with(
        "s",
        &mut |ticket, text, _| {
            written.push(text.to_string());
            if ticket == third {
                assert!(outbox.redeliver("s", &id(second)));
                assert!(outbox.redeliver("s", &id(first)));
                assert!(!outbox.redeliver("s", &id(first)), "once a write");
            }
            Ok(Route::Terminal)
        },
        &MidTurn,
    );
    assert_eq!(written, ["three", "one", "two"]);
}

/// A message put back because its agent dropped it at a cancel is held as one an interrupt
/// cleared is, and leaves the queue again where the agent reports its text sent while it is
/// still held. Once another message has been taken, it follows that one and is withdrawn no
/// more: a prompt with its text is then its own write.
#[test]
fn a_dropped_message_is_withdrawn_only_while_it_is_held() {
    let outbox = Outbox::default();
    let first = outbox.push("s", "one").expect("accepted");
    let second = outbox.push("s", "two").expect("accepted");
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &MidTurn);
    let id = |ticket: Ticket| format!("launch:{ticket}");
    assert!(outbox.redeliver_dropped("s", &id(second)));
    assert!(outbox.redeliver_dropped("s", &id(first)));
    assert!(!outbox.redeliver_dropped("s", &id(first)), "once a cancel");
    assert_eq!((outbox.len("s"), outbox.takeable("s")), (2, 0));

    assert!(!outbox.withdraw_dropped("t", |text| text == "two"));
    assert!(!outbox.withdraw_dropped("s", |text| text == "three"));
    assert!(outbox.withdraw_dropped("s", |text| text == "two"));
    assert!(!outbox.withdraw_dropped("s", |text| text == "two"));
    assert_eq!(outbox.len("s"), 1);

    // Another message is taken: the one held follows it, and is the queue's to write.
    outbox.push("s", "next").expect("accepted");
    let mut written = Vec::new();
    outbox.drain_with(
        "s",
        &mut |_, text, _| {
            written.push(text.to_string());
            if text == "next" {
                assert!(
                    !outbox.withdraw_dropped("s", |text| text == "one"),
                    "held no longer"
                );
            }
            Ok(Route::Terminal)
        },
        &MidTurn,
    );
    assert_eq!(written, ["next", "one"]);
}

/// A message an interrupt cleared, put back held, is not one the agent dropped: a prompt
/// reported with its text withdraws nothing.
#[test]
fn a_message_held_for_another_reason_is_never_withdrawn() {
    let outbox = Outbox::default();
    outbox.queue_held("s", "one");
    assert!(!outbox.withdraw_dropped("s", |text| text == "one"));
    assert_eq!(outbox.len("s"), 1);
}

/// A message dropped at every cancel is put back as often as a message is written again, and
/// no more.
#[test]
fn a_dropped_message_is_put_back_a_bounded_number_of_times() {
    let outbox = Outbox::default();
    let mut ticket = outbox.push("s", "one").expect("accepted");
    for round in 0..=MAX_REDELIVERIES {
        let mut wrote = None;
        outbox.push("s", "other").expect("accepted");
        outbox.drain_with(
            "s",
            &mut |written, text, _| {
                if text == "one" {
                    wrote = Some(written);
                }
                Ok(Route::Terminal)
            },
            &MidTurn,
        );
        ticket = wrote.unwrap_or(ticket);
        assert_eq!(
            outbox.redeliver_dropped("s", &format!("launch:{ticket}")),
            round < MAX_REDELIVERIES,
            "round {round}"
        );
    }
}

/// An agent at its prompt, as `expects` and `elsewhere` say, that records what the outbox tells
/// it of the composer's messages.
#[derive(Default)]
struct ComposerWatch {
    expects: bool,
    elsewhere: bool,
    told: std::cell::RefCell<Vec<(Ticket, ComposerLeft)>>,
}

impl PromptSignal for ComposerWatch {
    fn sent(&self) -> u64 {
        0
    }
    fn expects_send(&self) -> bool {
        self.expects
    }
    fn wait_past(&self, _before: u64) -> bool {
        false
    }
    fn mark_unsent(&self, _before: u64) {}
    fn mark_not_submitted(&self, _before: u64) {}
    fn unsent_waiting(&self) -> bool {
        false
    }
    fn sighting(&self) -> Sighting {
        Sighting::default()
    }
    fn watch_screen(&self, _on: bool) {}
    fn trust_holds_writes(&self) -> bool {
        false
    }
    fn write_went_elsewhere(&self) -> bool {
        self.elsewhere
    }
    fn composer_outcome(&self, ticket: Ticket, left: ComposerLeft) {
        self.told.borrow_mut().push((ticket, left));
    }
}

/// What became of a message handed over from the composer's queue is told once, as it is known,
/// for each way a drain leaves a message; a message nobody marked is told of to no one.
#[test]
fn a_composers_message_is_told_as_written_unsent_or_gone_elsewhere() {
    let cases: [(&str, ComposerWatch, Route, ComposerLeft); 4] = [
        (
            "terminal",
            ComposerWatch::default(),
            Route::Terminal,
            ComposerLeft::Written,
        ),
        (
            "queued",
            ComposerWatch {
                expects: true,
                ..Default::default()
            },
            Route::Queued,
            ComposerLeft::Written,
        ),
        (
            "unsent",
            ComposerWatch {
                expects: true,
                ..Default::default()
            },
            Route::Terminal,
            ComposerLeft::Unsent,
        ),
        (
            "elsewhere",
            ComposerWatch {
                expects: true,
                elsewhere: true,
                ..Default::default()
            },
            Route::Terminal,
            ComposerLeft::WentElsewhere("hello".to_string()),
        ),
    ];
    for (name, watch, route, left) in cases {
        let outbox = Outbox::default();
        let ticket = outbox.push("s", "hello").expect("accepted");
        outbox.mark_composer(ticket);
        outbox.drain_with("s", &mut |_, _, _| Ok(route), &watch);
        assert_eq!(*watch.told.borrow(), [(ticket, left)], "{name}");
        assert!(!outbox.holds_composer("s", ticket), "{name}: written");
    }

    // A message nobody marked is told of to no one.
    let outbox = Outbox::default();
    outbox.push("s", "plain").expect("accepted");
    let watch = ComposerWatch::default();
    outbox.drain_with("s", &mut |_, _, _| Ok(Route::Terminal), &watch);
    assert!(watch.told.borrow().is_empty());
}

/// A message the agent's own queue refuses is told as refused, with the reason and who refused.
#[test]
fn a_composers_message_the_agent_refused_is_told_as_refused() {
    let outbox = Outbox::default();
    let ticket = outbox.push("s", "too long").expect("accepted");
    outbox.mark_composer(ticket);
    let watch = ComposerWatch::default();
    outbox.drain_with(
        "s",
        &mut |_, _, _| {
            Err(PartialWrite {
                written: 0,
                error: std::io::Error::other(crate::session::term::RefusedByAgent(
                    "input too long".into(),
                )),
            })
        },
        &watch,
    );
    let told = watch.told.borrow();
    assert_eq!(told.len(), 1);
    let ComposerLeft::Refused(refused) = &told[0].1 else {
        panic!("not told as refused: {told:?}");
    };
    assert_eq!(told[0].0, ticket);
    assert_eq!(refused.kind, RefusalKind::Agent);
    assert!(
        refused.reason.contains("input too long"),
        "{}",
        refused.reason
    );
}

/// A message of the composer's still in the queue can be pulled out alone: the messages around
/// it stay, in order; one a drain has taken to write cannot, and is still the composer's until
/// the drain is done with it.
#[test]
fn a_composers_message_is_withdrawn_alone_until_a_drain_takes_it() {
    let outbox = Outbox::default();
    outbox.push("s", "a").expect("accepted");
    let b = outbox.push("s", "b").expect("accepted");
    outbox.push("s", "c").expect("accepted");
    outbox.mark_composer(b);
    assert!(
        !outbox.withdraw_queued("other", b),
        "another session's queue"
    );
    assert!(outbox.holds_composer("s", b));
    assert!(outbox.withdraw_queued("s", b));
    assert!(!outbox.withdraw_queued("s", b), "once");
    assert!(!outbox.holds_composer("s", b));
    assert_eq!(outbox.len("s"), 2);

    let d = outbox.push("s", "d").expect("accepted");
    outbox.mark_composer(d);
    let mut written = Vec::new();
    outbox.drain_with(
        "s",
        &mut |ticket, text, _| {
            written.push(text.to_string());
            if ticket == d {
                assert!(!outbox.withdraw_queued("s", d), "taken to be written");
                assert!(outbox.holds_composer("s", d), "still being written");
            }
            Ok(Route::Terminal)
        },
        &MidTurn,
    );
    assert_eq!(written, ["a", "c", "d"]);
    assert!(!outbox.holds_composer("s", d));
}

/// A composer's message is never carried elsewhere as text: a planned stop's seal and a hold's
/// take leave it out (the composer queue keeps it), and the messages around it are carried as
/// they were; the take does not count it among the messages that were more than text.
#[test]
fn a_composers_message_is_left_out_of_what_is_carried_elsewhere() {
    let outbox = Outbox::default();
    outbox.push("s", "one").expect("accepted");
    let composer = outbox.push("s", "from the composer").expect("accepted");
    outbox.mark_composer(composer);
    outbox.push("s", "two").expect("accepted");
    let sealed = outbox.seal(Duration::ZERO);
    let texts: Vec<&str> = sealed[0]
        .1
        .iter()
        .map(|(_, text, _)| text.as_str())
        .collect();
    assert_eq!(texts, ["one", "two"]);

    let outbox = Outbox::default();
    outbox.push("s", "one").expect("accepted");
    let composer = outbox.push("s", "from the composer").expect("accepted");
    outbox.mark_composer(composer);
    outbox.push("s", "two").expect("accepted");
    let (taken, shaped) = outbox.take_counting_shaped("s");
    let texts: Vec<String> = taken.into_iter().map(|(text, _)| text).collect();
    assert_eq!(texts, ["one", "two"]);
    assert_eq!(shaped, 0);
    assert_eq!(outbox.len("s"), 0);
}

/// A message queued as the composer's is marked in the same step, so a drain tells its outcome
/// with no mark added after; while the drain writes it, it is held as the composer's, from the
/// moment it is taken off the queue.
#[test]
fn a_message_pushed_as_the_composers_is_marked_and_held_while_written() {
    let outbox = Outbox::default();
    let ticket = outbox
        .push_composer("s", "hello", None, false)
        .expect("accepted");
    assert!(outbox.holds_composer("s", ticket));
    let watch = ComposerWatch::default();
    let mut held_while_written = false;
    outbox.drain_with(
        "s",
        &mut |_, _, _| {
            held_while_written = outbox.holds_composer("s", ticket);
            Ok(Route::Terminal)
        },
        &watch,
    );
    assert!(held_while_written, "held while the drain writes it");
    assert_eq!(*watch.told.borrow(), [(ticket, ComposerLeft::Written)]);
    assert!(!outbox.holds_composer("s", ticket));
}

/// A composer's message a drain could not write yet and put back stays the queue's throughout:
/// it is found on the queue once back, and told of to no one.
#[test]
fn a_composers_message_put_back_by_a_drain_stays_held() {
    let outbox = Outbox::default();
    let ticket = outbox
        .push_composer("s", "later", None, false)
        .expect("accepted");
    let watch = ComposerWatch::default();
    outbox.drain_with(
        "s",
        &mut |_, _, _| {
            Err(PartialWrite {
                written: 0,
                error: std::io::Error::from(std::io::ErrorKind::WouldBlock),
            })
        },
        &watch,
    );
    assert!(outbox.holds_composer("s", ticket));
    assert!(watch.told.borrow().is_empty());
}
