//! An agent's CLI run in the background on a pseudo-terminal of its own, for a CLI that asks its
//! question only where its stdin and stdout are both a terminal and takes pipes for "no": the
//! [`TerminalProcess`], which hands over what the CLI prints as [`Line`]s while it runs, a prompt
//! it waits at included, and writes one line back ([`TerminalProcess::answer`]). The terminal is
//! the platform's (`platform::term`), the one a session's program runs on; nothing here is a
//! second one. It is bounded and ended as a CLI on pipes is (see [`super::Process`]).
//!
//! What differs from pipes, which a flow has to know:
//!
//! - A terminal has one output, so no line says it came from stderr.
//! - The terminal echoes what is written into it, so the answer comes back as a line.
//! - A line the CLI has not ended is handed over as it stands once the output has been silent for
//!   [`PROMPT_SETTLE`], marked [`Line::unfinished`]; what the CLI prints after it is a line of its
//!   own. Empty lines are not handed over.
//! - A line ends at a line feed, a carriage return, and a sequence that puts the cursor somewhere
//!   else (a position, a row, a column, up or down); a cursor-forward sequence reads as one space;
//!   every other escape sequence is dropped and the text on either side of it joined
//!   ([`LineCutter`]). On a host whose terminal backend repaints the screen
//!   (`TermFacts::host_renders`) the lines are that repaint's: rows it reaches by moving the
//!   cursor are lines of their own, but a row can arrive in pieces or again, so a flow tells a
//!   prompt by a phrase it holds, never by the whole line, and asks once for it.
//! - Nobody answers the CLI's terminal queries (device attributes, cursor position, status). A
//!   session's terminal has the daemon or a client answer them; this one has neither, so a CLI
//!   that queries its terminal waits out its own timeout, and on a host that holds the console
//!   until a query is answered (`TermFacts::host_waits_on_queries`) stalls for that long each.
//! - The CLI leads the terminal's session, so when it exits the system hangs up whatever it left
//!   in the terminal's foreground, and the terminal closes once the [`TerminalProcess`] is
//!   dropped. A CLI on pipes leaves such a helper running.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use super::{
    end_session, end_session_on_drop, line_channel, take_last_words, Line, EXIT_POLL, LINE_LIMIT,
};
use crate::platform::proc::{ChildHandle, REAP_GRACE};
use crate::platform::term::{self, Input, Pty, PtyReader, Size};
use crate::terminal::plain_text::strip_escapes;

/// How long the terminal's output has to stay silent for a line the CLI has not ended to be
/// handed over as it stands: the prompt of a CLI that waits for an answer. Longer than the pause
/// between two writes of one line, short beside the time a person takes to answer.
const PROMPT_SETTLE: Duration = Duration::from_millis(150);

/// The size of the terminal a CLI is run on: wide, so that neither the terminal nor a host that
/// repaints it wraps a prompt or a URL in the middle.
const SIZE: Size = Size {
    rows: 50,
    cols: 250,
};

/// The most an answer may hold, in bytes: a word or a short line, never a paste.
const ANSWER_LIMIT: usize = 1024;

/// How many chunks of the terminal's output wait for the task that cuts them into lines. The
/// reader waits for room, so nothing is lost between the terminal and the lines; the lines
/// themselves are bounded as a CLI's on pipes are.
const CHUNK_BACKLOG: usize = 64;

/// A CLI running on a pseudo-terminal of its own, leading the terminal's session, with what it
/// prints arriving on [`Self::lines`] (escape sequences stripped, trimmed). A cancel
/// ([`Self::stop`]) kills the whole session and hangs the terminal up, and so does dropping it, as
/// a flow's end by a timeout or the daemon's exit does. A CLI that exits by itself
/// ([`TerminalChild::exited`]) is only reaped.
pub(crate) struct TerminalProcess {
    pub(crate) child: TerminalChild,
    pty: Arc<Pty>,
    pub(crate) lines: mpsc::Receiver<Line>,
}

/// The CLI of a [`TerminalProcess`], apart from its lines so that a flow can wait for its exit
/// and its next line at once.
pub(crate) struct TerminalChild {
    /// The CLI as the platform kills, sweeps and reaps it; nothing else waits on it. Shared with
    /// the task a drop leaves the reaping to.
    handle: Arc<ChildHandle>,
    /// Names the CLI in the log of a session sweep: the agent's name.
    label: &'static str,
    /// How the CLI ended, once it has been reaped: its pid may be someone else's from then on,
    /// so nothing is sent through `handle` any more.
    ended: Option<Ended>,
}

/// How a CLI that has been reaped ended.
#[derive(Clone, Copy)]
struct Ended {
    /// Whether it exited with status 0; `None` when the host could not say.
    success: Option<bool>,
}

impl TerminalChild {
    /// Waits for the CLI to exit by itself, reaps it and says whether it exited with status 0;
    /// `None` when the host could not say. Its session is left as it is. Safe to drop and call
    /// again, and answers the same once the CLI has been reaped.
    pub(crate) async fn exited(&mut self) -> Option<bool> {
        loop {
            if let Some(ended) = self.ended {
                return ended.success;
            }
            match self.handle.exited_unreaped() {
                Some(false) => tokio::time::sleep(EXIT_POLL).await,
                Some(true) => {
                    let success = self.handle.exited_successfully();
                    self.reap(success);
                }
                // The host cannot tell, which it also says of a child already reaped.
                None => self.reap(None),
            }
        }
    }

    fn reap(&mut self, success: Option<bool>) {
        if let Err(err) = self.handle.reap_if_exited() {
            tracing::warn!(
                process = self.label,
                pid = self.handle.pid().0,
                %err,
                "reaping a CLI that ran on a terminal failed"
            );
        }
        self.ended = Some(Ended { success });
    }

    /// The CLI as the platform kills and sweeps it, for a test to look at.
    #[cfg(all(test, unix))]
    pub(crate) fn handle(&self) -> Arc<ChildHandle> {
        self.handle.clone()
    }
}

impl TerminalProcess {
    /// Starts `command` on a terminal of its own. Only its program, arguments, variables and
    /// working directory are taken ([`on_a_terminal`]). **Blocks** for the start, as
    /// [`super::Process::spawn`] does, and has to be called on the runtime.
    pub(crate) fn spawn(
        command: &std::process::Command,
        label: &'static str,
    ) -> std::io::Result<Self> {
        Self::spawn_sized(command, label, SIZE)
    }

    pub(crate) fn spawn_sized(
        command: &std::process::Command,
        label: &'static str,
        size: Size,
    ) -> std::io::Result<Self> {
        let spawned = term::spawn(on_a_terminal(command), size, None)
            .map_err(|err| std::io::Error::other(format!("{err:#}")))?;
        let child = TerminalChild {
            handle: Arc::new(spawned.child),
            label,
            ended: None,
        };
        let pty = Arc::new(spawned.pty);
        let (chunks_tx, chunks) = mpsc::channel(CHUNK_BACKLOG);
        let (tx, lines) = line_channel();
        // Made before the reader starts, so that a reader that cannot be started is dropped with
        // it: `Drop` kills the CLI and hangs the terminal up.
        let process = Self { child, pty, lines };
        read_terminal(process.pty.reader(), chunks_tx)?;
        tokio::spawn(cut_into_lines(chunks, tx));
        Ok(process)
    }

    /// Writes `line` and the Enter that submits it into the terminal, as the answer to what the
    /// CLI asked. Refused, with nothing written, when it holds a control character or is longer
    /// than [`ANSWER_LIMIT`]: an answer is one line of text.
    pub(crate) async fn answer(&self, line: &str) -> std::io::Result<()> {
        if line.len() > ANSWER_LIMIT || line.chars().any(char::is_control) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "an answer is one short line of text",
            ));
        }
        self.submit(Input::Text(line.to_string())).await
    }

    pub(crate) async fn paste_secret(
        &self,
        secret: &crate::protocol::SecretLoginText,
    ) -> std::io::Result<()> {
        self.submit(Input::Paste(secret.as_str().to_owned())).await
    }

    async fn submit(&self, answer: Input) -> std::io::Result<()> {
        let pty = self.pty.clone();
        let input = [answer, Input::Enter];
        // The write waits for a terminal whose program is not reading, for a few seconds at most.
        let written = tokio::task::spawn_blocking(move || {
            let _writing = pty
                .input_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pty.write_input(&input)
        })
        .await
        .map_err(std::io::Error::other)?;
        written.map_err(|partial| partial.error)
    }

    /// Kills the CLI's whole session, waits for the CLI to be gone and hangs the terminal up, as
    /// [`super::Process::stop`] does for a CLI on pipes. A CLI already reaped is left alone.
    pub(crate) async fn stop(&mut self) {
        if self.child.ended.is_none() {
            end_session(&self.child.handle, self.child.label).await;
            self.child.reap(None);
        }
        let pty = self.pty.clone();
        // Closing a console waits for its host on Windows.
        let _ = tokio::task::spawn_blocking(move || pty.hang_up()).await;
    }

    /// Takes what the CLI printed last into `last_words`, a moment's worth, as
    /// [`super::Process::last_words`] does.
    pub(crate) async fn last_words(&mut self, last_words: &mut String) {
        take_last_words(&mut self.lines, last_words).await;
    }
}

impl Drop for TerminalProcess {
    fn drop(&mut self) {
        let killed = self.child.ended.is_none().then(|| {
            end_session_on_drop(&self.child.handle, self.child.label);
            self.child.handle.clone()
        });
        // Nothing else reaps a CLI on a terminal, so the one just killed is waited for, for a
        // moment, and reaped; then the hang-up stops the reader and lets the terminal go. On the
        // runtime both are handed to a blocking thread: the wait blocks, and closing a console
        // waits for its host on Windows.
        let pty = self.pty.clone();
        let finish = move || {
            if let Some(child) = killed {
                reap_killed(&child);
            }
            pty.hang_up();
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn_blocking(finish);
            }
            Err(_) => finish(),
        }
    }
}

/// Reaps `child`, which has just been killed, once it has exited, waiting [`REAP_GRACE`] at most:
/// a process wedged in the kernel is left as the one zombie that costs. **Blocks.**
fn reap_killed(child: &ChildHandle) {
    let deadline = std::time::Instant::now() + REAP_GRACE;
    while child.exited_unreaped() == Some(false) && std::time::Instant::now() < deadline {
        std::thread::sleep(EXIT_POLL);
    }
    if let Err(err) = child.reap_if_exited() {
        tracing::debug!(pid = child.pid().0, %err, "reaping a CLI dropped on a terminal failed");
    }
}

/// `command` as a terminal starts it: the same program, arguments and working directory, over the
/// variables the command sets and no others. A `std::process::Command` does not say whether it
/// starts over the daemon's environment; every command of an account's CLI
/// (`adapter::account_cli`) starts over the shell snapshot alone, and a login must never inherit
/// the daemon's own environment by an oversight here.
fn on_a_terminal(command: &std::process::Command) -> term::Command {
    let mut on_terminal = term::Command::new(command.get_program());
    on_terminal.args(command.get_args()).env_clear();
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            on_terminal.env(key, value);
        }
    }
    if let Some(dir) = command.get_current_dir() {
        on_terminal.cwd(dir);
    }
    on_terminal
}

/// Reads the terminal on a thread of its own, handing each chunk to `chunks`, until the terminal
/// ends, its reader is stopped ([`Pty::hang_up`]) or nothing takes the chunks any more.
fn read_terminal(reader: PtyReader, chunks: mpsc::Sender<Vec<u8>>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("login-terminal-reader".to_string())
        .spawn(move || {
            let mut buf = vec![0u8; 4096];
            while let Ok(Some(read)) = reader.read_chunk(&mut buf) {
                if read == 0 || chunks.blocking_send(buf[..read].to_vec()).is_err() {
                    return;
                }
            }
        })
        .map(drop)
}

/// Cuts the terminal's output into lines for `tx` as it arrives, handing over a line the CLI has
/// not ended once the output has been silent for [`PROMPT_SETTLE`], and what is left when the
/// terminal ends. Past the backlog of `tx` the newest lines are dropped, as a CLI's on pipes are.
async fn cut_into_lines(mut chunks: mpsc::Receiver<Vec<u8>>, tx: mpsc::Sender<Line>) {
    let mut cutter = LineCutter::default();
    let mut lines = Vec::new();
    loop {
        // `None` is the output having been silent for long enough with a line under way.
        let next = if cutter.is_empty() {
            Some(chunks.recv().await)
        } else {
            tokio::time::timeout(PROMPT_SETTLE, chunks.recv())
                .await
                .ok()
        };
        let ended = matches!(next, Some(None));
        match next {
            Some(Some(chunk)) => cutter.feed(&chunk, &mut lines),
            Some(None) | None => lines.extend(cutter.take_unfinished()),
        }
        for line in lines.drain(..) {
            if let Err(mpsc::error::TrySendError::Closed(_)) = tx.try_send(line) {
                return;
            }
        }
        if ended {
            return;
        }
    }
}

/// The bytes of the line a terminal is in the middle of, at most [`LINE_LIMIT`] of them, and how
/// far into an escape sequence its output is.
#[derive(Default)]
struct LineCutter {
    line: Vec<u8>,
    /// Whether the line was longer than [`LINE_LIMIT`] and `line` is only its start.
    cut: bool,
    sequence: Sequence,
}

/// Where the terminal's output stands in an escape sequence, which can arrive in pieces.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Sequence {
    #[default]
    None,
    /// After the escape character.
    Escape,
    /// In a control sequence (`ESC [`), before its final byte.
    Control,
}

impl LineCutter {
    /// Whether no line is under way.
    fn is_empty(&self) -> bool {
        self.line.is_empty()
    }

    /// Takes `bytes` of the terminal's output, pushing every line they end onto `lines`. A line
    /// ends at a line feed or a carriage return (a terminal ends a line with both, and a CLI that
    /// redraws a line in place returns the carriage alone) and at a control sequence that puts
    /// the cursor somewhere else, which is how a host that repaints the screen goes from one row
    /// to the next. A cursor-forward sequence, which such a host writes for a run of spaces, is
    /// one space. Every other control sequence is dropped here; the escapes that are not control
    /// sequences (a title, a charset) are left for [`strip_escapes`].
    fn feed(&mut self, bytes: &[u8], lines: &mut Vec<Line>) {
        for &byte in bytes {
            match (self.sequence, byte) {
                (Sequence::None, 0x1b) => self.sequence = Sequence::Escape,
                (Sequence::None, b'\n' | b'\r') => lines.extend(self.take(false)),
                (Sequence::None, _) => self.push(byte),
                (Sequence::Escape, b'[') => self.sequence = Sequence::Control,
                (Sequence::Escape, _) => {
                    self.sequence = Sequence::None;
                    self.push(0x1b);
                    self.push(byte);
                }
                // Parameters and intermediates come before the one final byte.
                (Sequence::Control, 0x20..=0x3f) => {}
                (Sequence::Control, _) => {
                    self.sequence = Sequence::None;
                    match byte {
                        // Position, row, column, next and previous line, up and down.
                        b'H' | b'f' | b'd' | b'G' | b'E' | b'F' | b'A' | b'B' => {
                            lines.extend(self.take(false));
                        }
                        b'C' => self.push(b' '),
                        _ => {}
                    }
                }
            }
        }
    }

    fn push(&mut self, byte: u8) {
        if self.line.len() < LINE_LIMIT {
            self.line.push(byte);
        } else {
            self.cut = true;
        }
    }

    /// The line under way as it stands, for a CLI that has stopped printing in the middle of it.
    /// A sequence the output stopped in the middle of is taken up where it stands by what comes
    /// next.
    fn take_unfinished(&mut self) -> Option<Line> {
        self.take(true)
    }

    /// The line under way, with the cutter ready for the next; `None` for one that says nothing.
    fn take(&mut self, unfinished: bool) -> Option<Line> {
        let bytes = std::mem::take(&mut self.line);
        let cut = std::mem::take(&mut self.cut);
        // Decoded lossily, as a CLI's lines on pipes are.
        let text = strip_escapes(&String::from_utf8_lossy(&bytes))
            .trim()
            .to_string();
        (!text.is_empty()).then_some(Line {
            text,
            stderr: false,
            cut,
            unfinished,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod with_a_stand_in {
        use super::*;
        use crate::accounts::background_cli::Process;
        use crate::test_support::{
            assert_stops_running, read_pid_file, KillOnDrop, ScratchDir, PATIENCE,
        };

        /// What the stand-in asks where it has a terminal.
        const PROMPT: &str = "Keep the token in a file? (y/N)";

        /// A stand-in login CLI that asks [`PROMPT`] only where its stdin and stdout are both a
        /// terminal, as the CLI this mode was built for does, and takes pipes for "no". It writes its
        /// pid to `pid_file` first, and after a "y" exits 0 having said `kept`.
        #[cfg(unix)]
        fn asks_only_a_terminal(pid_file: &std::path::Path) -> std::process::Command {
            use crate::test_support::{bash, SHELL_PID};

            let script = format!(
                "echo {SHELL_PID} > '{pid}.tmp'; mv '{pid}.tmp' '{pid}'; \
             if [ -t 0 ] && [ -t 1 ]; then \
               printf '{PROMPT} '; read answer; \
               if [ \"$answer\" = y ]; then echo kept; exit 0; fi; \
               echo 'not kept'; exit 1; \
             fi; \
             echo 'no terminal to ask on'; exit 1",
                pid = pid_file.display()
            );
            let mut command = std::process::Command::new(bash());
            // Set one by one, so that the terminal's command, which starts over the variables the
            // command sets alone, has what the shell needs on every host.
            command.args(["-c", &script]).envs(std::env::vars_os());
            command
        }

        #[cfg(windows)]
        fn asks_only_a_terminal(pid_file: &std::path::Path) -> std::process::Command {
            use crate::test_support::fixture::Steps;

            let mut command = Steps::new()
                .write_pid(pid_file)
                .ask(
                    PROMPT,
                    None,
                    Steps::new().print("kept").exit(),
                    Steps::new().print("not kept").fail(),
                    Steps::new().say("no terminal to ask on").fail(),
                )
                .program("terminal-question")
                .command();
            command.envs(std::env::vars_os());
            command
        }

        /// The lines the CLI says until one holds `phrase`, that one included.
        async fn until_line(process: &mut TerminalProcess, phrase: &str) -> Vec<Line> {
            let mut seen = Vec::new();
            let found = tokio::time::timeout(PATIENCE, async {
                while let Some(line) = process.lines.recv().await {
                    let found = line.text.contains(phrase);
                    seen.push(line);
                    if found {
                        return true;
                    }
                }
                false
            })
            .await;
            let texts: Vec<&str> = seen.iter().map(|line| line.text.as_str()).collect();
            assert!(
                matches!(found, Ok(true)),
                "{phrase:?} never came: {texts:?}"
            );
            seen
        }

        async fn exited(process: &mut TerminalProcess) -> Option<bool> {
            tokio::time::timeout(PATIENCE, process.child.exited())
                .await
                .expect("the stand-in never exited")
        }

        #[tokio::test]
        async fn a_cli_on_a_terminal_asks_its_question_and_takes_the_answer_written_back() {
            for (answer, said, success) in [("y", "kept", true), ("n", "not kept", false)] {
                let dir = ScratchDir::new("terminal-cli-asks");
                let command = asks_only_a_terminal(&dir.join("pid"));
                let mut process = TerminalProcess::spawn(&command, "stand-in").expect("it starts");

                let asked = until_line(&mut process, PROMPT)
                    .await
                    .pop()
                    .expect("a line");
                if !crate::platform::term::facts().host_renders {
                    assert!(asked.unfinished, "the prompt came as an ended line");
                }
                eprintln!("prompt unfinished={}: {:?}", asked.unfinished, asked.text);
                assert!(!asked.stderr);

                process.answer(answer).await.expect("the answer is written");
                let after = until_line(&mut process, said).await;
                let lines: Vec<_> = after
                    .iter()
                    .map(|line| (&line.text, line.unfinished))
                    .collect();
                eprintln!("answer output: {lines:?}");
                assert_eq!(after.last().unwrap().text, said);
                #[cfg(unix)]
                assert!(!after.last().unwrap().unfinished);
                assert_eq!(exited(&mut process).await, Some(success), "{answer}");
                // Reaped, and answering the same from then on.
                assert_eq!(process.child.exited().await, Some(success));
            }
        }

        /// The difference the mode exists for: the same CLI on pipes never asks.
        #[tokio::test]
        async fn the_same_cli_on_pipes_is_never_asked() {
            let dir = ScratchDir::new("terminal-cli-pipes");
            let command = asks_only_a_terminal(&dir.join("pid"));
            let mut process = Process::spawn(command, "stand-in", true).expect("it starts");
            let status = tokio::time::timeout(PATIENCE, process.child.wait())
                .await
                .expect("the stand-in never exited")
                .expect("its status");
            assert!(!status.success());
            let mut last_words = String::new();
            process.last_words(&mut last_words).await;
            assert!(
                last_words.ends_with("no terminal to ask on"),
                "{last_words:?}"
            );
        }

        #[tokio::test]
        async fn an_answer_that_is_not_one_short_line_is_refused_unwritten() {
            let dir = ScratchDir::new("terminal-cli-answer");
            let command = asks_only_a_terminal(&dir.join("pid"));
            let mut process = TerminalProcess::spawn(&command, "stand-in").expect("it starts");
            until_line(&mut process, PROMPT).await;

            // Each of these, had it reached the CLI, would have been taken for a "no".
            for refused in ["n\nx", "n\r", "\u{1b}[A", &"n".repeat(ANSWER_LIMIT + 1)] {
                let err = process.answer(refused).await.expect_err("refused");
                assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
                assert_eq!(err.to_string(), "an answer is one short line of text");
            }
            // Nothing reached the CLI: the one answer it reads is the yes written now.
            process.answer("y").await.expect("the answer is written");
            until_line(&mut process, "kept").await;
            assert_eq!(exited(&mut process).await, Some(true));
        }

        /// A CLI dropped on a terminal is reaped, not left a zombie until the daemon exits: once it
        /// is, the host can say nothing of the child any more.
        #[cfg(unix)]
        #[tokio::test]
        async fn a_cli_dropped_on_a_terminal_is_reaped() {
            let dir = ScratchDir::new("terminal-cli-reaped");
            let command = asks_only_a_terminal(&dir.join("pid"));
            let mut process = TerminalProcess::spawn(&command, "stand-in").expect("it starts");
            until_line(&mut process, PROMPT).await;
            let child = process.child.handle();
            assert_eq!(child.exited_unreaped(), Some(false));

            drop(process);
            let deadline = tokio::time::Instant::now() + PATIENCE;
            while child.exited_unreaped().is_some() {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the CLI was killed but never reaped"
                );
                tokio::time::sleep(EXIT_POLL).await;
            }
        }

        #[tokio::test]
        async fn a_cli_on_a_terminal_that_is_stopped_or_dropped_is_ended() {
            for stopped in [true, false] {
                let dir = ScratchDir::new("terminal-cli-ended");
                let pid_file = dir.join("pid");
                let command = asks_only_a_terminal(&pid_file);
                let mut process = TerminalProcess::spawn(&command, "stand-in").expect("it starts");
                until_line(&mut process, PROMPT).await;
                let pid = read_pid_file(&pid_file).expect("the stand-in wrote its pid");
                let mut guard = KillOnDrop(vec![pid]);
                if stopped {
                    process.stop().await;
                } else {
                    drop(process);
                }
                assert_stops_running(pid);
                guard.0.clear();
            }
        }
    }

    /// What a terminal wrote, in the chunks it arrived in, and the lines it is cut into, each
    /// with whether it is unfinished.
    type Case<'a> = (&'a str, &'a [&'a [u8]], Vec<(String, bool)>);

    fn cut(chunks: &[&[u8]]) -> Vec<(String, bool)> {
        let mut cutter = LineCutter::default();
        let mut lines = Vec::new();
        for chunk in chunks {
            cutter.feed(chunk, &mut lines);
        }
        lines.extend(cutter.take_unfinished());
        lines
            .into_iter()
            .map(|line| (line.text, line.unfinished))
            .collect()
    }

    #[test]
    fn a_terminals_output_is_cut_at_its_line_ends_and_the_rest_is_unfinished() {
        let ended = |text: &str| (text.to_string(), false);
        let cases: [Case<'_>; 9] = [
            (
                "a line ends at the carriage return and line feed a terminal writes",
                &[b"one\r\ntwo\r\n"],
                vec![ended("one"), ended("two")],
            ),
            (
                "a line that arrives in pieces is one line, and escapes go",
                &[b"\x1b[1mfir", b"st\x1b[0m\r\n"],
                vec![ended("first")],
            ),
            (
                "a line redrawn in place is a line for each drawing",
                &[b"10%\r50%\r\n"],
                vec![ended("10%"), ended("50%")],
            ),
            (
                "a row reached by a cursor position is a line of its own, as a repaint writes it",
                &[b"\x1b[1;1HContinue? (y/N) y\x1b[2;1Hkept\x1b[3;1H"],
                vec![ended("Continue? (y/N) y"), ended("kept")],
            ),
            (
                "a cursor move down, to a row or to a column ends the line too",
                &[b"one\x1b[1Btwo\x1b[5dthree\x1b[1Gfour\r\n"],
                vec![ended("one"), ended("two"), ended("three"), ended("four")],
            ),
            (
                "a cursor-forward sequence is the space it stands for",
                &[b"Keep\x1b[1Cthe\x1b[3Ctoken?\r\n"],
                vec![ended("Keep the token?")],
            ),
            (
                "a sequence that arrives in pieces is read as one",
                &[b"a\x1b", b"[2;", b"1Hb\x1b[", b"1Cc\r\n"],
                vec![ended("a"), ended("b c")],
            ),
            (
                "erasing, colours and a title cut nothing and leave no text",
                &[b"\x1b]0;a title\x07\x1b[2K\x1b[32mgreen\x1b[0m\x1b[K text\r\n"],
                vec![ended("green text")],
            ),
            (
                "what the output stops in the middle of is unfinished",
                &[b"done\r\nContinue? (y/N) "],
                vec![ended("done"), ("Continue? (y/N)".to_string(), true)],
            ),
        ];
        for (name, chunks, expected) in cases {
            assert_eq!(cut(chunks), expected, "{name}");
        }
    }

    #[test]
    fn a_line_of_a_terminal_past_the_limit_is_cut_and_the_next_one_whole() {
        let mut cutter = LineCutter::default();
        let mut lines = Vec::new();
        cutter.feed(&b"x".repeat(LINE_LIMIT + 7), &mut lines);
        cutter.feed(b"\nnext\n", &mut lines);
        let got: Vec<(usize, bool)> = lines
            .iter()
            .map(|line| (line.text.len(), line.cut))
            .collect();
        assert_eq!(got, [(LINE_LIMIT, true), (4, false)]);
        // A cut line is nobody's last words, on a terminal as on pipes.
        assert_eq!(lines.remove(0).into_words(), None);
    }

    #[test]
    fn a_command_goes_onto_a_terminal_over_its_own_variables_alone() {
        let mut command = std::process::Command::new("/some/cli");
        command
            .args(["login", "--device"])
            .env("HOME", "/home/me")
            .env_remove("GONE")
            .current_dir("/home/me");
        let plain = on_a_terminal(&command).plain();
        assert_eq!(plain.get_program(), "/some/cli");
        assert_eq!(
            plain.get_args().collect::<Vec<_>>(),
            ["login", "--device"].map(std::ffi::OsStr::new)
        );
        assert_eq!(
            plain.get_envs().collect::<Vec<_>>(),
            [(
                std::ffi::OsStr::new("HOME"),
                Some(std::ffi::OsStr::new("/home/me"))
            )]
        );
        assert_eq!(
            plain.get_current_dir(),
            Some(std::path::Path::new("/home/me"))
        );
    }

    /// The daemon's own environment does not reach a CLI on a terminal: seen in what a process
    /// prints of its environment.
    #[cfg(unix)]
    #[test]
    fn a_command_on_a_terminal_does_not_inherit_the_daemons_environment() {
        let mut command = std::process::Command::new("/usr/bin/env");
        command.env("ONLY", "this");
        let output = on_a_terminal(&command).plain().output().expect("env runs");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.lines().any(|line| line == "ONLY=this"), "{text}");
        assert!(
            !text.lines().any(|line| line.starts_with("PATH=")),
            "{text}"
        );
    }
}
