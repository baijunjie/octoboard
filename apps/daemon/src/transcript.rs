//! Watching a Claude Code session's transcript for the one thing its hooks never report: the user
//! rejecting a permission prompt or declining its own `AskUserQuestion`.
//!
//! The permission-prompt path moves a session to `SessionStatus::WaitingUser` and then, on a
//! decline, falls silent — measured against the real CLI (2.1.274 and 2.1.286): no `Stop`, no
//! `Notification`, nothing. `PermissionDenied` exists in the binary's strings but never fires. The
//! only machine-readable trace is in the transcript JSONL at the payload's `transcript_path`: a
//! `tool_result` with `is_error: true` whose text begins "The user doesn't want to proceed with
//! this tool use", immediately followed by a user entry reading `[Request interrupted by user for
//! tool use]`. Either one alone says the pending decision was resolved by a rejection or an
//! interrupt — see `docs/agent-cli-reference.md`, section "## Claude Code", for the full
//! measurement. Cancelling an `AskUserQuestion` with Esc writes the same two records, field for
//! field — measured on 2.1.286 — and falls just as silent afterwards: no hook event either.
//!
//! On 2.1.289 the records and the recovery were confirmed in the running application rather than by
//! the same event-by-event measurement: a declined prompt wrote the record, and the session left
//! `WaitingUser` 484 ms later — a delay that fits this watch's poll and not a hook, which arrives at
//! once.
//!
//! The question list has no plain "No": its nearest exit, `4. Chat about this`, writes a
//! `tool_result` that starts with the same rejected-tool-use prefix but continues differently,
//! with no interrupt line after it, and the turn actually carries on — `PostToolBatch` and `Stop`
//! still fire. Matching on the prefix alone therefore catches that exit too, briefly reporting
//! `Idle` while the turn is in fact still running; the next `PostToolBatch` corrects it within one
//! step. That is an accepted trade: requiring the interrupt marker as well would narrow the match
//! and give up the redundancy that is this module's whole defence against the transcript format
//! changing under it.
//!
//! [`scan_for_rejection`] is the pure half: given a transcript path and an offset, what lines were
//! added and do any of them carry one of those two markers. [`watch_for_rejection`] is what calls
//! it on a timer, from the moment a hook puts a Claude Code session into `WaitingUser`, until the
//! rejection is found and applied or the watch is no longer owed anything.
//!
//! The transcript is Claude Code's own and is rewritten on every upgrade, so every line is parsed
//! on its own and a line that fails to parse, or parses into a shape this does not recognise, is
//! skipped rather than treated as an error. Nothing here may abort the watch over a transcript it
//! does not understand.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::protocol::SessionStatus;
use crate::state::AppState;

/// The start of the `tool_result` text Claude Code gives a rejected tool call.
const REJECTED_TOOL_USE_PREFIX: &str = "The user doesn't want to proceed with this tool use";

/// The user entry Claude Code writes immediately after the rejection `tool_result`.
const INTERRUPTED_MARKER: &str = "[Request interrupted by user for tool use]";

/// How often the watch checks the transcript for new lines. Fast enough that the hand comes down
/// close to when the person actually answers, cheap enough to keep polling for as long as they
/// take to decide.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// What one scan of a transcript found.
struct Scan {
    /// A rejection or interrupt record was among the lines read.
    rejected: bool,
    /// Where the next scan should start: the offset just past the last complete line read. A line
    /// still being written — one with no trailing newline yet — is left for the next scan, so this
    /// never splits a line in two.
    offset: u64,
}

/// Reads whatever full lines were appended to the transcript at `path` since `offset`, and reports
/// whether any of them records a rejected tool call or an interrupted one. An error here is an I/O
/// error opening or reading the file — a transcript that cannot be read at all — not a line that
/// fails to parse, which is tolerated silently.
fn scan_for_rejection(path: &Path, offset: u64) -> io::Result<Scan> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut added = Vec::new();
    file.read_to_end(&mut added)?;

    let mut consumed = 0usize;
    let mut rejected = false;
    while let Some(end) = added[consumed..].iter().position(|&byte| byte == b'\n') {
        let line = &added[consumed..consumed + end];
        consumed += end + 1;
        if is_rejection_line(line) {
            rejected = true;
        }
    }
    Ok(Scan {
        rejected,
        offset: offset + consumed as u64,
    })
}

/// Whether one transcript line is a rejection or interrupt record. Any line that is not valid JSON,
/// or valid JSON this does not recognise the shape of, reads as `false` rather than erroring —
/// that is what lets an unrelated line, or a future transcript format, pass through unremarked.
///
/// Most lines never reach the JSON parser at all: on the common path — the user answered `Yes`, and
/// the session sits in `WaitingUser` for the rest of the approved tool's run, per
/// `docs/agent-cli-reference.md`, section "## Claude Code" — the appended line is that tool's own
/// completed `tool_result`, which for a `Bash` with a lot of output or a `Read` of a big file runs
/// to hundreds of KB or more.
/// A plain byte-substring check for either marker is cheap and rules out such a line before paying
/// for a DOM parse of it; only a line that actually contains one of the markers is parsed, and the
/// structural check below (`is_error: true`, `starts_with`, exact equality) requires the full shape
/// as well as the text, so a tool that merely prints one of these strings still cannot match. The
/// byte check assumes a marker appears in the serialized line exactly as it does decoded — true
/// today, since both markers are plain printable ASCII and Claude Code's `JSON.stringify` escapes
/// none of it, but a marker written out with a JSON escape would slip past this check even though
/// the structural check below, which matches the decoded string, would still have caught it.
fn is_rejection_line(line: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(line) else {
        return false;
    };
    if !text.contains(REJECTED_TOOL_USE_PREFIX) && !text.contains(INTERRUPTED_MARKER) {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<Value>(line) else {
        return false;
    };
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return false;
    };
    content.iter().any(is_rejection_marker)
}

/// Whether one block of a transcript entry's content is either marker.
fn is_rejection_marker(block: &Value) -> bool {
    match block.get("type").and_then(Value::as_str) {
        Some("tool_result") => {
            block.get("is_error").and_then(Value::as_bool) == Some(true)
                && tool_result_text(block)
                    .is_some_and(|text| text.starts_with(REJECTED_TOOL_USE_PREFIX))
        }
        Some("text") => block.get("text").and_then(Value::as_str) == Some(INTERRUPTED_MARKER),
        _ => false,
    }
}

/// A `tool_result` block's text, however its `content` is shaped: a plain string in every
/// transcript this was measured against, but content blocks are Claude Code's general shape for
/// text elsewhere, so that form is read too rather than assumed away.
fn tool_result_text(block: &Value) -> Option<&str> {
    match block.get("content")? {
        Value::String(text) => Some(text.as_str()),
        Value::Array(blocks) => blocks
            .iter()
            .find_map(|block| block.get("text").and_then(Value::as_str)),
        _ => None,
    }
}

/// Starts watching `session_id`'s transcript for the rejection record that is the only trace left
/// of a declined permission prompt or a declined `AskUserQuestion` — both measured to write the
/// same record, see the module doc. Call this only once a hook has actually put the session into
/// `WaitingUser` for one of those two reasons — a session that resolves its own approvals never
/// reaches `WaitingUser` at all, and Codex and Grok report a decline through their own events and
/// must get no watcher.
///
/// Spawned, never awaited: the hook response has to return inside the adapters' few-second
/// timeout, and a person may sit at the prompt for far longer than that.
///
/// The transcript's current length is read before anything else, so a rejection left over from an
/// earlier turn is never mistaken for this one's. A fresh generation is taken from
/// `state.begin_transcript_watch` before spawning, so a newer watch started for the same session
/// retires this one.
pub fn watch_for_rejection(state: &Arc<AppState>, session_id: &str, transcript_path: Option<&str>) {
    let Some(transcript_path) = transcript_path else {
        tracing::debug!(
            session = %session_id,
            "no transcript path on a pending decision; it cannot be watched for a rejection"
        );
        return;
    };
    let path = PathBuf::from(transcript_path);
    let offset = match std::fs::metadata(&path) {
        Ok(meta) => meta.len(),
        Err(err) => {
            tracing::debug!(
                session = %session_id, %err,
                "the transcript could not be read; it cannot be watched for a rejection"
            );
            return;
        }
    };

    let generation = state.begin_transcript_watch(session_id);
    let state = state.clone();
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let mut offset = offset;
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            if !state.transcript_watch_current(&session_id, generation) {
                return; // A newer watch for this session has taken over.
            }
            if state.live_session(&session_id).is_none() {
                return; // The process is gone.
            }
            match state.store.get_session(&session_id) {
                Ok(Some(session)) if session.status == SessionStatus::WaitingUser => {}
                // A real hook event already moved the session on, or it is gone from the store
                // outright — either way, nothing here is still owed.
                Ok(_) => return,
                Err(err) => {
                    // Transient, same as a metadata read failing below — retry rather than
                    // abandoning the watch and leaving the hand up with nothing in the log.
                    tracing::debug!(
                        session = %session_id, %err,
                        "reading the session failed while watching its transcript for a rejection"
                    );
                    continue;
                }
            }

            let len = match std::fs::metadata(&path) {
                Ok(meta) => meta.len(),
                Err(_) => continue, // Transient — the transcript may not exist between writes.
            };
            if len < offset {
                // An offset left past end-of-file would never again satisfy the `len > offset`
                // check below, so the watch would poll forever without ever scanning again. Reset
                // forward to the new length — never rescan from zero, which could re-read an old
                // record still in the file as a fresh rejection — and let the watch recover on the
                // next append. Length alone cannot tell this apart from a transcript replaced
                // outright by a longer, unrelated file, which would resume the scan mid-record;
                // unreachable here since the path is per-session and uuid-named.
                tracing::debug!(
                    session = %session_id, old_offset = offset, new_len = len,
                    "the transcript shrank; resetting the watch offset to its new length"
                );
                offset = len;
                continue;
            }
            if len == offset {
                // A cheap short-circuit, not a separately observable branch: skipping the scan
                // here and letting `scan_for_rejection` run over zero added bytes behave the
                // same from outside, so this only saves the file open and seek.
                continue;
            }

            let scan = match scan_for_rejection(&path, offset) {
                Ok(scan) => scan,
                Err(err) => {
                    tracing::warn!(
                        session = %session_id, %err,
                        "reading the transcript failed while watching it for a rejection"
                    );
                    return;
                }
            };
            offset = scan.offset;
            if !scan.rejected {
                continue;
            }
            if state.transcript_watch_current(&session_id, generation) {
                // Conditional on the session still waiting, in one step: the scan above read the
                // status, and between that read and this write a real hook can have moved the
                // session on, which an unconditional write would then lose.
                match state.apply_reported_status_if(
                    &session_id,
                    SessionStatus::WaitingUser,
                    SessionStatus::Idle,
                ) {
                    Ok(true) => {
                        tracing::debug!(
                            session = %session_id,
                            "a transcript rejection was found; the hand is lowered"
                        );
                        // Mirrors the release the hook path pairs with every status change: a
                        // message queued while the session was `WaitingUser` is still queued, and
                        // nothing else is coming to release it. `flush_outbox`'s gate asks for a
                        // hook-reported state, which this `Idle` is not, but both states it
                        // accepts are reached here anyway: a genuine rejection leaves the agent
                        // at an empty prompt, and the `Chat about this` exit this match
                        // deliberately over-catches leaves it working.
                        state.spawn_flush_outbox(&session_id, SessionStatus::Idle);
                    }
                    Ok(false) => {
                        tracing::debug!(
                            session = %session_id,
                            "a transcript rejection was found, but the session had already moved on"
                        );
                    }
                    Err(err) => {
                        tracing::debug!(
                            session = %session_id, %err,
                            "lowering the hand after a transcript rejection failed"
                        );
                    }
                }
            }
            return;
        }
    });
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    use super::*;
    use crate::protocol::{Agent, Console, Origin, Role, Session};
    use crate::session::{LiveSession, NewSession};
    use crate::store::Store;

    /// A real rejection record, trimmed to the two lines that carry the markers, captured from a
    /// permission-prompt decline on Claude Code 2.1.274; a capture of the same decline on 2.1.286
    /// was byte-for-byte the same shape, only the ids differing.
    const REJECTION: &str = include_str!("../testdata/transcript_rejection.jsonl");

    /// A normal completed tool call's `tool_result`, from the same capture, `is_error` absent: the
    /// ordinary case a rejection must not be confused with.
    const COMPLETED_TOOL_RESULT: &str = include_str!("../testdata/transcript_tool_result.jsonl");

    /// The same two records, from an `AskUserQuestion` cancelled with Esc on Claude Code 2.1.286 —
    /// field for field identical to a declined permission prompt's, down to `toolDenialKind`.
    const QUESTION_DECLINE: &str = include_str!("../testdata/transcript_question_decline.jsonl");

    /// The question list's own `4. Chat about this` exit, from the same capture: a `tool_result`
    /// that starts with the same rejected-tool-use prefix but continues differently, with no
    /// interrupt line following it — the shape the prefix match deliberately over-catches, see the
    /// module doc.
    const QUESTION_CHAT_EXIT: &str =
        include_str!("../testdata/transcript_question_chat_exit.jsonl");

    /// A scratch directory for one test, removed when whatever holds it is dropped. Dereferences
    /// to the path, so it is passed as one. Owning the removal rather than leaving it to each
    /// test's last statement is what makes a failing assertion clean up too.
    ///
    /// The name carries the pid and a per-process counter, so two directories never collide within
    /// one run. Across runs a pid can be recycled, which is why the directory is also removed
    /// before being created: a run killed outright leaves its directory behind, and a leftover
    /// `octoboard.db` in it would fail `Store::insert_console`'s unique constraint.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(name: &str) -> Self {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "octoboardd-transcript-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(&dir).expect("temporary directory");
            Self(dir)
        }
    }

    impl std::ops::Deref for ScratchDir {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    /// A transcript path inside a scratch directory that goes away with it. Dereferences to the
    /// path, so it is passed as one.
    struct Scratch {
        _dir: ScratchDir,
        path: PathBuf,
    }

    impl std::ops::Deref for Scratch {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.path
        }
    }

    fn temp_path(name: &str) -> Scratch {
        let dir = ScratchDir::new(name);
        Scratch {
            path: dir.join("transcript.jsonl"),
            _dir: dir,
        }
    }

    fn write_transcript(path: &Path, content: &str) {
        let mut file = File::create(path).expect("transcript file");
        file.write_all(content.as_bytes())
            .expect("write transcript");
    }

    #[test]
    fn a_real_rejection_record_is_found() {
        let path = temp_path("rejection");
        write_transcript(&path, REJECTION);
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(scan.rejected);
        assert_eq!(scan.offset, REJECTION.len() as u64);
    }

    #[test]
    fn a_normal_completed_tool_result_does_not_match() {
        let path = temp_path("completed");
        write_transcript(&path, COMPLETED_TOOL_RESULT);
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(!scan.rejected);
    }

    /// The question-decline fixture is structurally identical to the permission-rejection one, so
    /// this is the regression anchor for the measured claim that the two paths write the same
    /// records — not independent branch coverage.
    #[test]
    fn an_ask_user_question_decline_is_found() {
        let path = temp_path("question-decline");
        write_transcript(&path, QUESTION_DECLINE);
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(scan.rejected);
    }

    /// The accepted over-match discussed in the module doc: the chat-exit record starts with the
    /// same prefix as a real rejection, so it is read as one too.
    #[test]
    fn the_chat_about_this_exit_also_matches_the_prefix() {
        // Pins the premise this test exists to check: if the fixture were ever re-captured with
        // an interrupt record too, it would stop testing the prefix branch and this must fail
        // loudly rather than silently pass for a different reason.
        assert!(!QUESTION_CHAT_EXIT.contains(INTERRUPTED_MARKER));

        let path = temp_path("question-chat-exit");
        write_transcript(&path, QUESTION_CHAT_EXIT);
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(scan.rejected);
    }

    /// The realistic false positive: an agent reading this very file, or the transcript itself —
    /// which contains both marker strings verbatim — produces a completed, non-error `tool_result`
    /// whose text happens to start with the rejection prefix. The substring pre-filter alone would
    /// match it; the structural check (`is_error: true`) must not.
    #[test]
    fn a_tool_result_that_only_starts_with_the_prefix_but_is_not_an_error_does_not_match() {
        let path = temp_path("quoted-prefix");
        let line = format!(
            "{{\"message\":{{\"content\":[{{\"type\":\"tool_result\",\
             \"content\":\"{REJECTED_TOOL_USE_PREFIX} but this one is not an error\",\
             \"is_error\":false}}]}}}}\n"
        );
        write_transcript(&path, &line);
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(!scan.rejected);
    }

    /// The substring pre-filter matches on raw bytes regardless of where they sit; the structural
    /// check that follows it must reject a marker sitting outside `/message/content` entirely.
    #[test]
    fn a_marker_string_outside_the_expected_shape_does_not_match() {
        let path = temp_path("marker-in-wrong-place");
        let line = format!("{{\"type\":\"summary\",\"note\":\"{REJECTED_TOOL_USE_PREFIX}\"}}\n");
        write_transcript(&path, &line);
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(!scan.rejected);
    }

    #[test]
    fn a_transcript_with_no_rejection_at_all_is_not_reported_as_one() {
        let path = temp_path("none");
        write_transcript(&path, "{\"type\":\"summary\"}\n");
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(!scan.rejected);
    }

    /// The transcript is rewritten on every Claude Code upgrade, so a line this does not recognise
    /// must not abort the scan — it is skipped, and whatever comes after it is still read.
    #[test]
    fn malformed_and_unknown_lines_are_skipped_without_error() {
        let path = temp_path("malformed");
        write_transcript(
            &path,
            &format!("not json at all\n{{\"no\":\"message field\"}}\n{REJECTION}"),
        );
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(scan.rejected);
    }

    /// A rejection that already sits before the start offset belongs to an earlier watch — a
    /// scan that starts after it must not report it.
    #[test]
    fn a_rejection_before_the_start_offset_is_not_reported() {
        let path = temp_path("before-offset");
        let filler = "{\"type\":\"summary\"}\n";
        write_transcript(&path, &format!("{REJECTION}{filler}"));
        let scan = scan_for_rejection(&path, REJECTION.len() as u64).expect("scan");
        assert!(!scan.rejected);
        assert_eq!(scan.offset, (REJECTION.len() + filler.len()) as u64);
    }

    /// A line with no trailing newline yet — the transcript still being written — must not be read
    /// as though it were complete: the offset returned must stop before it, so the next poll reads
    /// it whole.
    #[test]
    fn an_unterminated_trailing_line_is_left_for_the_next_scan() {
        let path = temp_path("partial");
        let partial = "{\"message\":{\"content\":[{\"type\":\"tool_result\"";
        write_transcript(&path, &format!("{REJECTION}{partial}"));
        let scan = scan_for_rejection(&path, 0).expect("scan");
        assert!(scan.rejected);
        assert_eq!(scan.offset, REJECTION.len() as u64);
    }

    fn app_state(name: &str) -> (Arc<AppState>, ScratchDir) {
        let dir = ScratchDir::new(&format!("state-{name}"));
        let store = Store::open(&dir.join("octoboard.db")).expect("store");
        store
            .insert_console(&Console {
                id: "console-1".to_string(),
                name: "Console".to_string(),
                workdir: dir.to_string_lossy().into_owned(),
                console_session_agent: Agent::Claude,
                default_agent: Agent::Claude,
                claude_config_dir: None,
                codex_config_dir: None,
                grok_config_dir: None,
                icon: None,
                created_at: 0,
            })
            .expect("console");
        (
            Arc::new(AppState::new(store, 1234, "/opt/octoboardd".to_string())),
            dir,
        )
    }

    fn waiting_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: None,
            host_id: crate::store::LOCAL_HOST_ID.to_string(),
            role: Role::Project,
            origin: Origin::User,
            title: "Session".to_string(),
            status: SessionStatus::WaitingUser,
            has_conversation: true,
            include_in_hub: false,
            config_dir: None,
            pinned: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// A stand-in live session with no real agent behind it — just something that stays alive on a
    /// PTY until dropped, which is all `watch_for_rejection` ever asks of `AppState::live_session`.
    fn fake_live(id: &str) -> Arc<LiveSession> {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("a PTY");
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", "sleep 30"]);
        let child = pty.slave.spawn_command(cmd).expect("the stand-in");
        let pid = child.process_id().expect("a pid");
        let fd = pty.master.as_raw_fd().expect("a descriptor");
        crate::ptyio::set_nonblocking(fd).expect("non-blocking");
        Arc::new(LiveSession::new(NewSession {
            id: id.to_string(),
            agent: Agent::Claude,
            pid,
            fd,
            master: pty.master,
            child,
            scratch_dir: None,
            resolves_approvals_itself: false,
        }))
    }

    /// Starting a second watch for the same session must retire the first: entering `WaitingUser`
    /// twice in a row must not leave two watchers racing to write the session's status.
    #[test]
    fn a_newer_watch_supersedes_the_one_before_it() {
        let (state, _dir) = app_state("supersede-generations");
        let first = state.begin_transcript_watch("s");
        let second = state.begin_transcript_watch("s");
        assert_ne!(first, second);
        assert!(!state.transcript_watch_current("s", first));
        assert!(state.transcript_watch_current("s", second));
    }

    /// Polls `done` until it holds or `timeout` passes.
    fn wait_for(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if done() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// How long a test waits for an outcome that is expected to happen, or for the watch loop to
    /// have had its chance to run when it is expected not to happen: enough headroom for the
    /// watcher to notice and write under load, without inflating the wait for the common case
    /// where nothing is expected to happen at all.
    const TEST_TIMEOUT: Duration = Duration::from_millis(POLL_INTERVAL.as_millis() as u64 * 3);

    fn status_of(state: &AppState, id: &str) -> Option<SessionStatus> {
        state.store.get_session(id).ok().flatten().map(|s| s.status)
    }

    /// Asserts that a watch was actually armed for `id`, as generation `0` — the first generation
    /// `watch_for_rejection` can hand out on a freshly built `AppState`, which is what each test
    /// below builds through `Fixture::new`.
    fn assert_watch_armed(state: &AppState, id: &str) {
        assert!(state.transcript_watch_current(id, 0));
    }

    /// A stand-in session is registered and watched the same way in every test below; this bundles
    /// the common setup and the common teardown so each test only has to state what is different
    /// about it.
    struct Fixture {
        state: Arc<AppState>,
        _dir: ScratchDir,
        path: PathBuf,
        live: Arc<LiveSession>,
    }

    impl Fixture {
        fn new(name: &str, session: Session) -> Self {
            let (state, dir) = app_state(name);
            state.store.insert_session(&session).expect("session");
            let live = fake_live(&session.id);
            state.register_live(live.clone());

            let path = dir.join("transcript.jsonl");
            std::fs::write(&path, "").expect("empty transcript");
            Self {
                state,
                _dir: dir,
                path,
                live,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // A failing assertion earlier in the test must still terminate the stand-in process.
            // The scratch directory goes with the `ScratchDir` this holds.
            self.live.terminate();
        }
    }

    /// The watcher must apply `Idle` only while the session is still the one it was started for:
    /// once a real hook event has moved the session on (here, straight to `Working`, standing in
    /// for a later prompt), a rejection record arriving afterwards must not overwrite it.
    ///
    /// This only exercises the pre-check at the top of each poll (`session.status ==
    /// WaitingUser`), which returns before the scan ever runs — the session is moved on before the
    /// rejection is even written, so the watcher never reaches the scan or the write to find out
    /// about it. Two further guards sit right before the write, for a session that moved on after a
    /// scan did find a rejection: the repeated generation check, and the conditional write that
    /// moves the session only while it is still waiting. Neither is reachable deterministically from
    /// outside the watch loop — the conditional write is covered directly by the store's own test.
    #[tokio::test]
    async fn a_session_that_moved_on_is_not_scanned_at_all() {
        let mut session = waiting_session("s");
        let fixture = Fixture::new("moved-on", session.clone());
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));
        assert_watch_armed(&fixture.state, "s");

        // A real hook event takes the session back to work before the rejection ever shows up.
        session.status = SessionStatus::Working;
        fixture
            .state
            .store
            .update_session(&session)
            .expect("update");

        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        // Nothing is expected to happen, so there is no event to poll for; give the watch loop a
        // few polls' worth of time to (wrongly) act, then check once. A false pass is bounded by
        // `TEST_TIMEOUT`, same as the other tests use to wait for a real event.
        let state = fixture.state.clone();
        let changed = tokio::task::spawn_blocking(move || {
            wait_for(TEST_TIMEOUT, || {
                status_of(&state, "s") != Some(SessionStatus::Working)
            })
        })
        .await
        .unwrap();
        assert!(!changed, "a session that moved on must not be scanned");
    }

    /// A rejection that arrives while the session is still genuinely waiting is applied — the
    /// positive case the test above is the negative of.
    #[tokio::test]
    async fn a_rejection_found_while_still_waiting_is_applied() {
        let fixture = Fixture::new("real-write", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));

        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        let state = fixture.state.clone();
        let applied = tokio::task::spawn_blocking(move || {
            wait_for(TEST_TIMEOUT, || {
                status_of(&state, "s") == Some(SessionStatus::Idle)
            })
        })
        .await
        .unwrap();
        assert!(applied, "the rejection must be applied");
    }

    /// A watch superseded by a newer one for the same session must not write either — the second
    /// guarantee `state.begin_transcript_watch` exists for, alongside retiring the stale watcher's
    /// polling loop.
    #[tokio::test]
    async fn a_superseded_watch_does_not_write() {
        let fixture = Fixture::new("superseded", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));
        assert_watch_armed(&fixture.state, "s");
        // Superseding directly, rather than through a second `watch_for_rejection`, isolates the
        // generation check from the rest of the watch loop.
        fixture.state.begin_transcript_watch("s");

        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        let state = fixture.state.clone();
        let changed = tokio::task::spawn_blocking(move || {
            wait_for(TEST_TIMEOUT, || {
                status_of(&state, "s") != Some(SessionStatus::WaitingUser)
            })
        })
        .await
        .unwrap();
        assert!(!changed, "a superseded watch must not write");
    }

    /// A transcript that shrinks mid-watch — the offset left pointing past the new end-of-file —
    /// must recover rather than stall: a rejection appended after the shrink is still found.
    #[tokio::test]
    async fn a_shrunk_transcript_recovers_and_a_later_append_is_still_seen() {
        let fixture = Fixture::new("shrink-recovers", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));

        // Grow the transcript past the watch's starting offset, then shrink it back to empty —
        // standing in for a rotated or truncated file — before the rejection is appended. The
        // filler is made longer than `REJECTION` itself, so that without the reset branch the
        // stale offset would still sit past the end of the file the rejection is written into —
        // `len < offset` stays true forever and the scan never runs again — rather than landing
        // inside `REJECTION`'s own bytes, where it could accidentally still find the marker.
        let filler_line = "{\"type\":\"summary\"}\n";
        let filler = filler_line.repeat(1 + REJECTION.len() / filler_line.len());
        std::fs::write(&fixture.path, &filler).expect("write filler");
        tokio::time::sleep(POLL_INTERVAL * 2).await;
        std::fs::write(&fixture.path, "").expect("shrink transcript");
        tokio::time::sleep(POLL_INTERVAL * 2).await;
        std::fs::write(&fixture.path, REJECTION).expect("write rejection");

        let state = fixture.state.clone();
        let applied = tokio::task::spawn_blocking(move || {
            wait_for(TEST_TIMEOUT * 2, || {
                status_of(&state, "s") == Some(SessionStatus::Idle)
            })
        })
        .await
        .unwrap();
        assert!(
            applied,
            "a rejection appended after a shrink must still be found"
        );
    }

    /// A poll that finds the transcript exactly as long as it was last scanned to — nothing
    /// appended since — leaves the session's status unchanged, and growth after it is still
    /// detected normally.
    #[tokio::test]
    async fn an_unchanged_transcript_length_leaves_status_unchanged_and_later_growth_is_still_seen()
    {
        let fixture = Fixture::new("equal-length", waiting_session("s"));
        watch_for_rejection(&fixture.state, "s", Some(&fixture.path.to_string_lossy()));

        std::fs::write(&fixture.path, "{\"type\":\"summary\"}\n").expect("write filler");
        tokio::time::sleep(POLL_INTERVAL * 2).await;
        // Rewriting the same content leaves the length — and so the offset check — unchanged
        // across a poll.
        std::fs::write(&fixture.path, "{\"type\":\"summary\"}\n").expect("rewrite filler");
        tokio::time::sleep(POLL_INTERVAL * 2).await;
        assert_eq!(
            status_of(&fixture.state, "s"),
            Some(SessionStatus::WaitingUser)
        );

        std::fs::write(
            &fixture.path,
            format!("{{\"type\":\"summary\"}}\n{REJECTION}"),
        )
        .expect("append rejection");
        let state = fixture.state.clone();
        let applied = tokio::task::spawn_blocking(move || {
            wait_for(TEST_TIMEOUT * 2, || {
                status_of(&state, "s") == Some(SessionStatus::Idle)
            })
        })
        .await
        .unwrap();
        assert!(
            applied,
            "growth after an unchanged poll must still be detected"
        );
    }
}
