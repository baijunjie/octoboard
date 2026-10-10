//! The mechanism behind watching an agent's own session record for something its hooks never
//! report: `crate::transcript` (Claude Code's transcript, a declined prompt) and `crate::rollout`
//! (Codex's rollout file, a turn that ended in an error). Each of those owns what a line means,
//! when a watch is armed and what a finding does; this owns reading the file and the poll loop
//! around it.
//!
//! A record is append-only JSONL that the agent flushes per line, and belongs to the agent, which
//! rewrites its format on upgrades. So every line is judged on its own, and one that is not UTF-8,
//! does not parse or has a shape the caller does not recognise is simply not a finding; nothing
//! here may abort a watch over a file it does not understand. A line can run to hundreds of KB (a
//! tool's output), so a caller's line check should rule most lines out with a plain
//! `str::contains` before it pays for a JSON parse.
//!
//! [`scan_lines`] is the pure half: a path, an offset and a line check in, what was found among the
//! lines added since and where to read from next out. [`spawn`] is what calls it on a timer.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::state::AppState;

/// What one scan of a record found.
pub struct Scan<T> {
    /// What the line check made of the last line it recognised, if any.
    pub found: Option<T>,
    /// Where the next scan should start: the offset just past the last complete line read. A line
    /// still being written, with no trailing newline yet, is left for the next scan, so an offset
    /// never splits a line in two.
    pub offset: u64,
}

/// Reads whatever full lines were appended to the record at `path` since `offset`, and runs
/// `line_found` over each. An error is an I/O error opening or reading the file, `NotFound`
/// included, not a line that fails to parse.
pub fn scan_lines<T>(
    path: &Path,
    offset: u64,
    mut line_found: impl FnMut(&str) -> Option<T>,
) -> io::Result<Scan<T>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut added = Vec::new();
    file.read_to_end(&mut added)?;

    let mut consumed = 0usize;
    let mut found = None;
    while let Some(end) = added[consumed..].iter().position(|&byte| byte == b'\n') {
        let line = &added[consumed..consumed + end];
        consumed += end + 1;
        if let Some(hit) = std::str::from_utf8(line).ok().and_then(&mut line_found) {
            found = Some(hit);
        }
    }
    Ok(Scan {
        found,
        offset: offset + consumed as u64,
    })
}

/// Where a watch starts reading, which also decides how it recovers from a file that shrank and
/// what it does about a file that is not there. The three go together: each is what makes its
/// start safe.
///
/// Neither start can tell a shrink from a file replaced outright by a longer, unrelated one, since
/// length is all that is compared; that would resume the scan mid-record. Callers use both only
/// for per-session paths named for their own thread, where it does not happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// At the file's end as it stands when the watch is armed, so a record left over from before
    /// is never mistaken for a new one. A file that is not there at that moment means the watch is
    /// never armed. A file that shrinks resumes at its new end, never from zero, which could
    /// re-read an old record still in the file as a fresh one.
    AtEnd,
    /// Where the session's previous watch of the same file got to, and the file's start for the
    /// first. For a record that can already hold the finding by the time the watch is armed, and
    /// whose lines say for themselves which watch they belong to. A file that is not there yet is
    /// waited for. A file that shrinks is read again from the start: skipping to its new end could
    /// skip the very record the watch is after.
    Carried,
}

/// What a watch's owner says about whether anything is still owed to it, asked once per poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owed {
    /// The watch is still wanted.
    Yes,
    /// It is not: the session has moved on, or is gone.
    No,
    /// That could not be told this time; ask again at the next poll.
    Unknown,
}

/// How a watch runs.
pub struct Watch {
    /// How often the file is checked for new lines.
    pub interval: Duration,
    pub start: Start,
}

/// Starts watching the record at `path` for `session_id`. Spawned, never awaited: a hook response
/// has to return inside the adapters' few-second timeout.
///
/// Each poll, the watch stops if a newer watch for the session has superseded it
/// (`AppState::begin_record_watch`) or a hook retired it (`AppState::end_record_watch`), if the
/// process is gone, or if `owed` says `No`. Otherwise it scans the lines added since it last read,
/// and when `line_found` recognises one it stops and, if it is still the session's current watch,
/// calls `on_found` with what that returned. Whether a file that is not there yet is waited for
/// is [`Start`]'s to say.
///
/// The scan runs on a blocking worker: on a long session the added bytes run to megabytes, which
/// must not sit on the runtime's threads. A read that fails for any other reason ends the watch.
pub fn spawn<T, P, A>(
    state: &Arc<AppState>,
    session_id: &str,
    path: PathBuf,
    watch: Watch,
    owed: impl Fn(&AppState, &str) -> Owed + Send + 'static,
    line_found: P,
    on_found: A,
) where
    T: Send + 'static,
    P: Fn(&str) -> Option<T> + Send + Sync + 'static,
    A: FnOnce(&Arc<AppState>, &str, T) + Send + 'static,
{
    let mut offset = match watch.start {
        Start::AtEnd => match std::fs::metadata(&path) {
            Ok(meta) => meta.len(),
            Err(err) => {
                tracing::debug!(
                    session = %session_id, %err,
                    "the record could not be read; it cannot be watched"
                );
                return;
            }
        },
        Start::Carried => state.record_offset(session_id, &path),
    };

    let generation = state.begin_record_watch(session_id);
    let state = state.clone();
    let session_id = session_id.to_string();
    let line_found = Arc::new(line_found);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(watch.interval).await;

            if !state.record_watch_current(&session_id, generation) {
                return; // A newer watch, or a hook ending the turn, has taken over.
            }
            if state.live_session(&session_id).is_none() {
                return; // The process is gone.
            }
            match owed(&state, &session_id) {
                Owed::Yes => {}
                Owed::No => return,
                Owed::Unknown => continue,
            }

            let len = match tokio::fs::metadata(&path).await {
                Ok(meta) => meta.len(),
                Err(_) => continue, // Not created yet, or transient between writes.
            };
            if len < offset {
                // An offset left past the end would never again satisfy the `len == offset`
                // check below, so the watch would poll forever without scanning again.
                tracing::debug!(
                    session = %session_id, old_offset = offset, new_len = len,
                    "the record shrank; resetting the watch offset"
                );
                match watch.start {
                    Start::AtEnd => {
                        offset = len;
                        continue; // Recovers on the next append.
                    }
                    Start::Carried => offset = 0,
                }
            }
            if len == offset {
                // A cheap short-circuit, not a separately observable branch: a scan over zero
                // added bytes behaves the same from outside, so this only saves the file open.
                continue;
            }

            let scan = {
                let (path, line_found) = (path.clone(), line_found.clone());
                tokio::task::spawn_blocking(move || {
                    scan_lines(&path, offset, |line| line_found(line))
                })
                .await
            };
            let scan = match scan {
                Ok(Ok(scan)) => scan,
                Ok(Err(err)) if err.kind() == io::ErrorKind::NotFound => continue,
                Ok(Err(err)) => {
                    tracing::warn!(
                        session = %session_id, %err,
                        "reading the record failed while watching it"
                    );
                    return;
                }
                Err(err) => {
                    tracing::warn!(
                        session = %session_id, %err,
                        "the scan of the record panicked; ending the watch"
                    );
                    return;
                }
            };
            offset = scan.offset;
            if watch.start == Start::Carried {
                state.note_record_offset(&session_id, &path, offset);
            }
            let Some(found) = scan.found else {
                continue;
            };
            if state.record_watch_current(&session_id, generation) {
                on_found(&state, &session_id, found);
            }
            return;
        }
    });
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::test_support::ScratchFile;

    const LINE: &str = "{\"type\":\"summary\"}\n";
    const HIT: &str = "{\"type\":\"hit\"}\n";

    fn is_hit(line: &str) -> Option<()> {
        line.contains("\"hit\"").then_some(())
    }

    fn record(name: &str, content: &str) -> ScratchFile {
        let path = ScratchFile::new(&format!("record-watch-{name}"), "record.jsonl");
        let mut file = File::create(&path).expect("record file");
        file.write_all(content.as_bytes()).expect("write record");
        path
    }

    #[test]
    fn a_line_the_check_recognises_is_found_and_the_offset_moves_past_it() {
        let path = record("found", &format!("{LINE}{HIT}"));
        let scan = scan_lines(&path, 0, is_hit).expect("scan");
        assert_eq!(scan.found, Some(()));
        assert_eq!(scan.offset, (LINE.len() + HIT.len()) as u64);
    }

    #[test]
    fn a_record_with_nothing_recognised_finds_nothing() {
        let path = record("none", LINE);
        let scan = scan_lines(&path, 0, is_hit).expect("scan");
        assert_eq!(scan.found, None);
        assert_eq!(scan.offset, LINE.len() as u64);
    }

    /// The last recognised line is what is reported.
    #[test]
    fn the_last_recognised_line_wins() {
        let path = record("last", "first\nsecond\n");
        let scan = scan_lines(&path, 0, |line| Some(line.to_string())).expect("scan");
        assert_eq!(scan.found.as_deref(), Some("second"));
    }

    /// A line that is not UTF-8 never reaches the check, and the lines after it are still read.
    #[test]
    fn a_line_that_is_not_utf8_is_skipped() {
        let path = ScratchFile::new("record-watch-binary", "record.jsonl");
        std::fs::write(&path, [&[0xff, 0xfe, b'\n'][..], HIT.as_bytes()].concat()).expect("write");
        assert_eq!(scan_lines(&path, 0, is_hit).expect("scan").found, Some(()));
    }

    /// What a finding before the start offset belongs to is an earlier watch: a scan that starts
    /// after it must not report it.
    #[test]
    fn a_line_before_the_start_offset_is_not_read() {
        let path = record("before-offset", &format!("{HIT}{LINE}"));
        let scan = scan_lines(&path, HIT.len() as u64, is_hit).expect("scan");
        assert_eq!(scan.found, None);
        assert_eq!(scan.offset, (HIT.len() + LINE.len()) as u64);
    }

    /// A line with no trailing newline yet, the record still being written, is not read as though
    /// it were complete: the offset stops before it, and the next scan reads it whole.
    #[test]
    fn a_line_cut_mid_write_is_left_for_the_next_scan_and_read_once_complete() {
        let cut = &HIT[..HIT.len() / 2];
        let path = record("partial", &format!("{LINE}{cut}"));
        let first = scan_lines(&path, 0, is_hit).expect("scan");
        assert_eq!(first.found, None);
        assert_eq!(first.offset, LINE.len() as u64);

        std::fs::write(&path, format!("{LINE}{HIT}")).expect("complete the line");
        let second = scan_lines(&path, first.offset, is_hit).expect("scan");
        assert_eq!(second.found, Some(()));
        assert_eq!(second.offset, (LINE.len() + HIT.len()) as u64);
    }

    /// The entry condition of a watch armed before the agent has created the file.
    #[test]
    fn a_file_that_does_not_exist_yet_reads_as_not_found() {
        let path = record("missing", "");
        std::fs::remove_file(&path).ok();
        let err = scan_lines(&path, 0, is_hit).err().expect("an error");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
