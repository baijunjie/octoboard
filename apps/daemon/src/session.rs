//! One running agent process: its PTY (`crate::pty`), and what is the agent's own on top of it —
//! the agent, its title, its folder-trust watch and the count of writes into its input.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::protocol::Agent;
use crate::pty::{NewPty, PtyCore, PtyOwner, Spawned};
use crate::ptyio;
use crate::trust::{CarriedTrust, TrustState};

pub use crate::pty::spawn_reader_thread;

/// Graceful teardown: agents are asked to exit with `SIGTERM` and only killed if they do not.
/// This is not politeness — Grok's `Stop` hook fires with `reason: "shutdown"` on a `SIGTERM`
/// teardown and not at all on a kill, and repeatedly killing Claude Code mid-startup trips its own
/// renderer-fallback counter, which is machine-level state it keeps about itself.
const AGENT_STOP_SIGNAL: libc::c_int = libc::SIGTERM;

#[derive(Default)]
struct TitleObserver {
    parser: crate::terminal_title::Parser,
    latest: Option<Option<String>>,
    state: Weak<crate::state::AppState>,
    runtime: Option<tokio::runtime::Handle>,
}

impl TitleObserver {
    fn publish(&self, session: &LiveSession) {
        if let (Some(state), Some(title)) = (self.state.upgrade(), &self.latest) {
            if state.record_terminal_title(session, title.as_deref()) {
                // Output arrives on the PTY reader thread, outside the Tokio runtime. A write
                // must not block that reader while the agent is still draining its output.
                if let Some(runtime) = &self.runtime {
                    let id = session.id.clone();
                    runtime.spawn_blocking(move || {
                        if let Ok(record) = state.session_record(&id) {
                            state.flush_outbox(&id, state.agent_status(&record));
                        }
                    });
                }
            }
        }
    }
}

/// A running agent session. Everything about its process and PTY is its [`PtyCore`]'s, reached
/// through it directly (it dereferences to it); what is here is the agent's own.
pub struct LiveSession {
    pub id: String,
    pub agent: Agent,
    pty: PtyCore,
    title: Mutex<TitleObserver>,
    action_required: AtomicBool,
    /// Permission hooks can describe automatic reviews without a dialog. Visible action prompts
    /// are observed separately, since this setting does not rule out manual approvals.
    pub resolves_approvals_itself: bool,
    /// Watches the session's output for its agent's folder-trust confirmation.
    pub trust: TrustState,
    /// How many times anything but the trust path has written into this session's input. An
    /// attached terminal's own protocol replies (focus reports, answers to the agent's terminal
    /// queries) are not counted, because they cannot move a cursor. Changed only under the PTY's
    /// input lock, so that the trust path's check of the count and its key are one step that no
    /// other writer can fall between. See [`Self::write_input`].
    input_writes: AtomicU64,
}

impl std::ops::Deref for LiveSession {
    type Target = PtyCore;

    fn deref(&self) -> &PtyCore {
        &self.pty
    }
}

impl PtyOwner for LiveSession {
    fn pty(&self) -> &PtyCore {
        &self.pty
    }

    fn on_output(&self, data: &[u8]) {
        LiveSession::on_output(self, data);
    }
}

/// Everything one running session is built from. A struct rather than a parameter list: these are
/// all values the launch produced, and half of them are indistinguishable by type.
pub struct NewSession {
    pub id: String,
    pub agent: Agent,
    pub spawned: Spawned,
    /// Per-session scratch directory (Grok's `GROK_HOME` symlink farm), removed when the session's
    /// process is gone.
    pub scratch_dir: Option<std::path::PathBuf>,
    pub resolves_approvals_itself: bool,
    /// Grok's: where the trust it records when its confirmation is pressed is carried from and to;
    /// see `adapter::LaunchPlan::carried_trust`.
    pub carried_trust: Option<CarriedTrust>,
}

impl LiveSession {
    pub fn new(session: NewSession) -> Self {
        Self {
            pty: PtyCore::new(NewPty {
                label: session.id.clone(),
                spawned: session.spawned,
                scratch_dir: session.scratch_dir,
                kill_session_leftovers: true,
            }),
            id: session.id,
            agent: session.agent,
            title: Mutex::new(TitleObserver::default()),
            action_required: AtomicBool::new(false),
            resolves_approvals_itself: session.resolves_approvals_itself,
            trust: TrustState::new(session.agent, session.carried_trust),
            input_writes: AtomicU64::new(0),
        }
    }

    /// Output may already have arrived before the launch is registered. Bind and publish the
    /// retained observation under the same lock as feeding, so an older title cannot win later.
    pub(crate) fn observe_titles(&self, state: &Arc<crate::state::AppState>) {
        let mut observer = self.title.lock().expect("title observer lock poisoned");
        observer.state = Arc::downgrade(state);
        observer.runtime = tokio::runtime::Handle::try_current().ok();
        observer.publish(self);
    }

    pub(crate) fn needs_user_action(&self) -> bool {
        self.action_required.load(Ordering::Acquire)
    }

    pub(crate) fn observe_action_title(&self, title: Option<&str>) -> bool {
        let waiting = self.agent == Agent::Codex
            && title.is_some_and(crate::terminal_title::codex_action_required);
        self.action_required.swap(waiting, Ordering::AcqRel) != waiting
    }

    /// Called from the PTY reader thread for every chunk read. Blocks while a slow client catches
    /// up; see [`PtyCore::push_output`].
    pub fn on_output(&self, data: &[u8]) {
        {
            let mut observer = self.title.lock().expect("title observer lock poisoned");
            if let Some(title) = observer.parser.feed(data) {
                // A title that only spun since the last one changes nothing, so it does not reach
                // the store's lock.
                let changed = observer.latest.as_ref() != Some(&title);
                observer.latest = Some(title.clone());
                if changed {
                    observer.publish(self);
                }
            }
        }
        self.pty.push_output(data, |total| {
            // The count without its lock: a writer holds that lock while the agent may be waiting
            // for this very thread to drain its output.
            self.trust
                .feed(data, self.input_writes.load(Ordering::Acquire), total);
        });
    }

    /// Writes raw input. The error says how many bytes went in, because a caller deciding whether
    /// to retry has to know: whatever was accepted is already in the child's input buffer.
    ///
    /// Counted: every writer but the trust path comes through here, which is how the trust path
    /// learns that someone else has typed into the session since it last looked. A write made up
    /// only of what an attached terminal sends by itself — focus reports, replies to the agent's
    /// terminal queries, mouse movement, wheel turns and button releases — is not counted: Claude
    /// Code turns focus reporting on as it draws its trust screen, and Grok Build turns on
    /// reporting every mouse movement as it draws its own, so an attached terminal writes that
    /// traffic all the time and none of it answers a screen. A mouse button press is counted: it
    /// is the person acting.
    pub fn write_input(&self, data: &[u8]) -> Result<(), ptyio::PartialWrite> {
        let input = self.pty.lock_input();
        if self.pty.is_frozen() {
            return Err(ptyio::PartialWrite {
                written: 0,
                error: io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the session output has been archived",
                ),
            });
        }
        if !is_terminal_protocol(data) {
            self.input_writes.fetch_add(1, Ordering::AcqRel);
        }
        self.pty.write_locked(&input, data)
    }

    /// Writes `data` only if nobody else has written into the input since the count read as
    /// `expected`, and says whether it did. For the trust path alone: its own keys are not counted,
    /// and the check and the write happen under the lock every other writer takes.
    pub(crate) fn write_input_if_untouched(
        &self,
        expected: u64,
        data: &[u8],
    ) -> Result<bool, ptyio::PartialWrite> {
        let input = self.pty.lock_input();
        if self.input_writes.load(Ordering::Acquire) != expected {
            return Ok(false);
        }
        self.pty.write_locked(&input, data).map(|()| true)
    }

    /// How many writes anyone else has made into this session's input so far.
    pub fn input_writes(&self) -> u64 {
        // Under the lock, so that the count is never read in the middle of a write.
        let _input = self.pty.lock_input();
        self.input_writes.load(Ordering::Acquire)
    }

    /// Asks the agent to exit, escalating to a kill only if it does not, with its screen frozen
    /// first so that what it draws on the way out does not replace it. **Blocks**; see
    /// [`PtyCore::stop`].
    pub fn terminate(&self) {
        if self.pty.poll_exit() {
            return;
        }
        self.pty.freeze_output();
        self.pty.stop(AGENT_STOP_SIGNAL);
    }
}

/// Whether a chunk written to a session's input is nothing but a terminal's own protocol traffic:
/// focus in/out (`CSI I`, `CSI O`), device attributes (`CSI … c`), cursor position (`CSI … R`),
/// mode reports (`CSI … $ y`), window reports (`CSI … t`), status reports (`CSI … n`), mouse
/// movement, wheel and button-release reports (`CSI < b;x;y M` with the motion bit, 32, or the
/// wheel bit, 64, set in `b`, and `CSI < … m`; the older `CSI M` with three bytes alike), and
/// operating system or device control string replies. Conservative: an empty chunk, any other
/// byte, any other final byte — the arrow keys included — a mouse button press, and a sequence cut
/// short all say no.
///
/// Movement and the wheel are let through because Grok Build reports every mouse movement while
/// its trust screen is up, and a pointer passing or scrolling over the terminal is not the person
/// answering it. Where mouse reporting is off, as at Codex's screen, the terminal turns the wheel
/// into arrow keys, which are counted.
fn is_terminal_protocol(chunk: &[u8]) -> bool {
    if chunk.is_empty() {
        return false;
    }
    let mut at = 0;
    while at < chunk.len() {
        if chunk[at] != 0x1b {
            return false;
        }
        match chunk.get(at + 1) {
            // The older mouse report: `M` and three bytes of button and position.
            Some(b'[') if chunk.get(at + 2) == Some(&b'M') => {
                if chunk.len() < at + 6 || chunk[at + 3..at + 6].iter().any(|byte| *byte < 0x20) {
                    return false;
                }
                // A release is button 3; movement has the motion bit, the wheel its own bit.
                let button = chunk[at + 3] - 0x20;
                if button & (32 | 64) == 0 && button & 3 != 3 {
                    return false;
                }
                at += 6;
            }
            Some(b'[') => {
                at += 2;
                let start = at;
                while at < chunk.len() && (0x20..=0x3f).contains(&chunk[at]) {
                    at += 1;
                }
                let Some(&final_byte) = chunk.get(at) else {
                    return false;
                };
                let known = match final_byte {
                    b'I' | b'O' | b'c' | b'R' | b't' | b'n' => true,
                    b'y' => chunk[start..at].contains(&b'$'),
                    b'm' => chunk.get(start) == Some(&b'<'),
                    b'M' => {
                        chunk.get(start) == Some(&b'<')
                            && std::str::from_utf8(&chunk[start + 1..at])
                                .ok()
                                .and_then(|params| params.split(';').next()?.parse::<u32>().ok())
                                .is_some_and(|button| button & (32 | 64) != 0)
                    }
                    _ => false,
                };
                if !known {
                    return false;
                }
                at += 1;
            }
            Some(&opener @ (b']' | b'P')) => {
                at += 2;
                // Up to a string terminator (an escape and a backslash), or for an operating
                // system command a bell as well.
                // Nothing but printable bytes in between: a control byte is typed input, not a reply.
                loop {
                    match chunk.get(at) {
                        None => return false,
                        Some(0x07) if opener == b']' => {
                            at += 1;
                            break;
                        }
                        Some(0x1b) if chunk.get(at + 1) == Some(&b'\\') => {
                            at += 2;
                            break;
                        }
                        Some(byte) if *byte < 0x20 => return false,
                        Some(_) => at += 1,
                    }
                }
            }
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::is_terminal_protocol;

    #[test]
    fn only_a_terminals_own_protocol_traffic_is_told_apart_from_input() {
        for protocol in [
            &b"\x1b[I"[..],
            b"\x1b[O",
            b"\x1b[I\x1b[O",
            b"\x1b[?1;2c",
            b"\x1b[>0;276;0c",
            b"\x1b[24;80R",
            b"\x1b[?2004;2$y",
            b"\x1b[8;32;120t",
            b"\x1b[0n",
            b"\x1b]11;rgb:0000/0000/0000\x07",
            b"\x1b]11;rgb:0000/0000/0000\x1b\\",
            b"\x1bP>|xterm.js\x1b\\",
            b"\x1b[I\x1b[?1;2c\x1b]10;rgb:ffff/ffff/ffff\x07",
            b"\x1b[<35;12;7M",
            b"\x1b[<0;12;7m",
            b"\x1b[M#,'",
            b"\x1b[MC,'",
            b"\x1b[<64;12;7M",
            b"\x1b[Ma,'",
        ] {
            assert!(is_terminal_protocol(protocol), "{protocol:?}");
        }
        for input in [
            &b""[..],
            b"\r",
            b"x",
            b"\x1b[B",
            b"\x1b[A",
            b"\x1b[C",
            b"\x1b[3~",
            b"\x1b",
            b"\x1b[",
            b"\x1b[I\x1b[B",
            b"\x1b[Ix",
            b"\x1b[I\r",
            b"\x1b[?2004;2y",
            b"\x1b[1",
            b"\x1b]11;rgb:0000",
            b"\x1b]11;rgb\r:0000\x07",
            b"\x1b]11;rgb\x1b[B:0000\x07",
            b"\x1bP>|xterm\x07.js\x1b\\",
            b"\x1bP>|xterm.js",
            b"\x1bOB",
            b"\x1b[35;12;7M",
            b"\x1b[M#",
            b"\x1b[<35;12;7",
            b"\x1b[<0;12;7M",
            b"\x1b[<0;12;7M\x1b[<0;12;7m",
            b"\x1b[M ,'",
        ] {
            assert!(!is_terminal_protocol(input), "{input:?}");
        }
    }
}
