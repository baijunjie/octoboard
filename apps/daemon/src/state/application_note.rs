//! What the desktop application tells the daemon over the daemon's standard input, which the
//! application owns the write end of (`--parent-pid`): before every quit it writes the line
//! `quit` without waiting for an answer. A daemon that then finds the application gone makes an
//! ordinary stop; one that finds it gone with no such line (a crash, a kill, the update restart,
//! which writes nothing) makes a planned stop, leaving its terminals to the next launch.
//!
//! The line is written before the application exits, so it is in the pipe by the time the
//! application's pid is gone, but the reader may not have taken it yet: the decision waits,
//! bounded, until the reader has seen the end of the input.
//!
//! A quit said while a planned stop is asked for or under way asks for the ordinary stop at once,
//! the application still running: it takes the place of the planned stop, or escalates it (see
//! `super::stop`), and a planned stop asked for after it is made ordinary. It is the one word that
//! reaches a daemon in a planned stop whatever the host, which accepts no new connection by then.
//!
//! Every quit also leaves a record in a file, before the line: a daemon that reads it at its start,
//! whether or not it follows the application, ends the holders it names
//! (`crate::holder::quit_targets`), which covers a quit this note never reached.

use std::io::{BufRead, ErrorKind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{AppState, StopKind};

/// The line the application writes before it quits.
const QUIT_LINE: &str = "quit";

/// How long the reader waits before reading again when the input has nothing to give yet.
const NOT_READY_PAUSE: Duration = Duration::from_millis(50);

/// How long the decision on a gone application waits for the reader to reach the end of the
/// input, which a pipe whose writer is gone reaches at once.
pub(crate) const INPUT_END_WAIT: Duration = Duration::from_secs(1);

/// What the application said, as the reader of its input has seen it.
pub struct ApplicationNote {
    quit: AtomicBool,
    input_ended: tokio::sync::watch::Sender<bool>,
}

impl Default for ApplicationNote {
    fn default() -> Self {
        Self {
            quit: AtomicBool::new(false),
            input_ended: tokio::sync::watch::channel(false).0,
        }
    }
}

/// Starts the thread that reads the application's input from the daemon's standard input. It is
/// the only reader of it once a key given with `--key-stdin` has been taken.
pub(crate) fn spawn_reader(state: Arc<AppState>) {
    let spawned = std::thread::Builder::new()
        .name("application-note".to_string())
        .spawn(move || read_notes(&state, std::io::stdin().lock()));
    if let Err(err) = spawned {
        tracing::warn!(%err, "starting the reader of the application's input failed; a quit is not told from a crash");
    }
}

/// Reads `input` line by line until its end or a read error that is not worth retrying, noting
/// what the application said, and at the end that nothing more will come. A line that is not text
/// is skipped, and an interrupted or not yet ready read is tried again, so that one failed read
/// does not end the reading, with the quit note still to come. **Blocks.**
fn read_notes(state: &AppState, mut input: impl BufRead) {
    let mut line = Vec::new();
    loop {
        match input.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                if std::str::from_utf8(&line).is_ok_and(|text| text.trim() == QUIT_LINE) {
                    state.note_application_quit();
                }
                line.clear();
            }
            // What was read of the line so far stays in `line`: the next read goes on with it.
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if err.kind() == ErrorKind::WouldBlock => std::thread::sleep(NOT_READY_PAUSE),
            Err(err) => {
                tracing::warn!(%err, "reading the application's input failed; it is taken to have ended");
                break;
            }
        }
    }
    state.note_application_input_ended();
}

impl AppState {
    /// Notes that the application said it is quitting; during a planned stop, asked for or under
    /// way, asks for the ordinary stop now, which escalates it.
    pub fn note_application_quit(&self) {
        self.application_note.quit.store(true, Ordering::Release);
        if self.planned_stop_asked() {
            self.request_shutdown();
        }
    }

    /// Notes that the application's input has ended, so that no line is still to come.
    pub fn note_application_input_ended(&self) {
        self.application_note.input_ended.send_replace(true);
    }

    /// Whether the application said it is quitting.
    pub fn application_said_quit(&self) -> bool {
        self.application_note.quit.load(Ordering::Acquire)
    }

    /// Asks for the stop the application's being gone calls for, once the reader has seen the end
    /// of its input or `wait` has passed: an ordinary stop when it said it was quitting, a planned
    /// stop with no successor when it did not. The ordinary stop takes the place of a planned stop
    /// asked for and not yet begun, or escalates one begun; the planned one never replaces a stop
    /// asked for already, so an ordinary stop never turns planned. A quit read only after the wait
    /// ran out, between the look at the note and the planned stop asked for, still makes the stop
    /// ordinary. Returns the kind asked for last.
    pub async fn stop_for_gone_application(&self, wait: Duration) -> StopKind {
        let mut ended = self.application_note.input_ended.subscribe();
        // A timeout reads as the input not having ended: whatever was noted by now decides.
        let _ = tokio::time::timeout(wait, ended.wait_for(|ended| *ended)).await;
        if !self.application_said_quit() {
            self.request_planned_stop();
            // The reader may have noted a quit after the look above and before the planned stop
            // was asked for, when it found none to escalate.
            if !self.application_said_quit() {
                return StopKind::Planned { successor: false };
            }
        }
        self.request_shutdown();
        StopKind::Ordinary
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::test_support::{app_state, PATIENCE};

    #[test]
    fn a_quit_line_is_noted_and_the_end_of_the_input_after_it() {
        let (state, _dir) = app_state("application-note-quit");
        read_notes(&state, Cursor::new("other\r\nquit\r\n"));
        assert!(state.application_said_quit());
        assert!(*state.application_note.input_ended.borrow());
    }

    #[test]
    fn an_input_with_no_quit_line_is_only_noted_as_ended() {
        let (state, _dir) = app_state("application-note-silent");
        read_notes(&state, Cursor::new("quitting\nnot quit\n"));
        assert!(!state.application_said_quit());
        assert!(*state.application_note.input_ended.borrow());
    }

    /// A line that is not text is skipped, and the lines after it are still read.
    #[test]
    fn a_line_that_is_not_text_is_skipped() {
        let (state, _dir) = app_state("application-note-unreadable");
        read_notes(&state, Cursor::new(b"\xff\xfe\nquit\n".to_vec()));
        assert!(state.application_said_quit());
        assert!(*state.application_note.input_ended.borrow());
    }

    /// An input that fails now and then: the reads that are interrupted or not ready are tried
    /// again, mid-line included, and a failure that is neither ends the input.
    struct Flaky(Vec<std::io::Result<&'static [u8]>>);

    impl std::io::Read for Flaky {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.is_empty() {
                return Ok(0);
            }
            match self.0.remove(0) {
                Ok(bytes) => {
                    buf[..bytes.len()].copy_from_slice(bytes);
                    Ok(bytes.len())
                }
                Err(err) => Err(err),
            }
        }
    }

    fn failing(kind: ErrorKind) -> std::io::Result<&'static [u8]> {
        Err(std::io::Error::from(kind))
    }

    #[test]
    fn a_read_that_is_interrupted_or_not_ready_is_tried_again() {
        let (state, _dir) = app_state("application-note-retried");
        let input = Flaky(vec![
            Ok(&b"qu"[..]),
            failing(ErrorKind::Interrupted),
            failing(ErrorKind::WouldBlock),
            Ok(&b"it\n"[..]),
        ]);
        read_notes(&state, std::io::BufReader::new(input));
        assert!(state.application_said_quit());
        assert!(*state.application_note.input_ended.borrow());
    }

    #[test]
    fn a_read_that_fails_for_good_ends_the_input() {
        let (state, _dir) = app_state("application-note-broken");
        let input = Flaky(vec![
            Ok(&b"other\n"[..]),
            failing(ErrorKind::BrokenPipe),
            Ok(&b"quit\n"[..]),
        ]);
        read_notes(&state, std::io::BufReader::new(input));
        assert!(!state.application_said_quit());
        assert!(*state.application_note.input_ended.borrow());
    }

    #[tokio::test]
    async fn a_gone_application_that_said_quit_makes_an_ordinary_stop() {
        let (state, _dir) = app_state("application-note-gone-quit");
        state.note_application_quit();
        state.note_application_input_ended();
        let kind = state.stop_for_gone_application(PATIENCE).await;
        assert_eq!(kind, StopKind::Ordinary);
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
    }

    #[tokio::test]
    async fn a_gone_application_that_said_nothing_makes_a_planned_stop() {
        let (state, _dir) = app_state("application-note-gone-silent");
        state.note_application_input_ended();
        let kind = state.stop_for_gone_application(PATIENCE).await;
        assert_eq!(kind, StopKind::Planned { successor: false });
        assert_eq!(
            state.await_stop().await,
            StopKind::Planned { successor: false }
        );
    }

    /// The note already in the pipe is read before the decision, which waits for the reader.
    #[tokio::test(start_paused = true)]
    async fn the_decision_waits_for_the_reader_to_reach_the_end_of_the_input() {
        let (state, _dir) = app_state("application-note-wait");
        let (kind, ()) = tokio::join!(state.stop_for_gone_application(PATIENCE), async {
            // On the paused clock the decision is waiting once every task is idle, short of its
            // bound running out.
            crate::test_support::tasks_idle().await;
            state.note_application_quit();
            state.note_application_input_ended();
        });
        assert_eq!(kind, StopKind::Ordinary);
    }

    /// An input that never ends (an application whose pipe something else holds open) is waited on
    /// only so long.
    #[tokio::test(start_paused = true)]
    async fn the_wait_for_the_end_of_the_input_is_bounded() {
        let (state, _dir) = app_state("application-note-bounded");
        let kind = state
            .stop_for_gone_application(Duration::from_millis(50))
            .await;
        assert_eq!(kind, StopKind::Planned { successor: false });
    }

    /// A quit said during a planned stop asks for the ordinary stop at once, which escalates it;
    /// one said with no stop asked for asks for none until the application is gone.
    #[tokio::test]
    async fn a_quit_said_during_a_planned_stop_escalates_it() {
        let (state, _dir) = app_state("application-note-escalates");
        read_notes(&state, Cursor::new("quit\n"));
        assert!(!state.ordinary_stop_asked());
        assert!(!state.planned_stop_asked());

        let (state, _dir) = app_state("application-note-escalates-planned");
        state.request_planned_stop();
        assert_eq!(
            state.await_stop().await,
            StopKind::Planned { successor: false }
        );
        read_notes(&state, Cursor::new("quit\n"));
        assert!(state.ordinary_stop_asked());
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
    }

    /// A stop asked for already stays the one made: an ordinary stop is not made planned by an
    /// application that is gone with no note.
    #[tokio::test]
    async fn an_ordinary_stop_asked_for_already_stays_ordinary() {
        let (state, _dir) = app_state("application-note-ordinary-stays");
        state.request_shutdown();
        state.note_application_input_ended();
        state.stop_for_gone_application(PATIENCE).await;
        assert_eq!(state.await_stop().await, StopKind::Ordinary);
    }
}
