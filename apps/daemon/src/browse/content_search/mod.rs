//! Searching the contents of a project's files, for `search_project_content` (see "Searching file
//! contents" in `apps/daemon/PROTOCOL.md`): no index, a fresh scan on every search, made fast as
//! ripgrep makes it, with its own libraries.
//!
//! The project, or the directory the search is limited to (refused when reached through a
//! symbolic link, and searched even when Git ignores it, since the user asked for it), is walked
//! in parallel by `ignore`, which applies Git's ignore rules (every `.gitignore` level,
//! `.git/info/exclude` and the user's global excludes) unless ignored files are included, and
//! never enters an ignored folder or follows a symbolic link: a link is never searched, so nothing
//! outside the project is, and a file inside it that a link points at is searched under its own
//! path. An ignore file that cannot be read or does not parse leaves its rules out without
//! making the search incomplete, and a `.gitignore` that cannot be read is not searched either. The names the file tree
//! leaves out (`live::is_left_out`) are left out with all beneath them. Each file is opened
//! through `live::open_searchable`, refusing a link in any component of its path, so a folder
//! swapped for a link while the walk lists it yields names whose files cannot be opened, never a
//! file outside the project. Each file is then searched line by line by `grep-searcher`, in CRLF
//! mode so that `$` matches before a `\r\n`, which abandons a file once it finds a NUL byte (a
//! binary file: what it matched before is dropped).
//!
//! The walk's threads hand each file's matches to the calling thread, which gathers them into
//! batches and sends each as it fills or once its first match has waited a moment, so the first
//! results show while the scan goes on. The bounds in `budget` are checked as it goes: reaching
//! one stops the walk and ends the search marked incomplete with that bound named. What the
//! search holds while it runs is bounded too: the walk threads stop collecting once they have
//! found more matches than the search reports, hand over at most a few files ahead of the batches,
//! grow their line buffers to at most `MAX_CONTENT_LINE_BYTES` and read no file past the file size
//! bound, even one that grows while it is read.

mod line;
mod mask;
pub mod refusal;
#[cfg(test)]
mod refusal_tests;
#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use grep_matcher::{LineTerminator, Matcher};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::{WalkBuilder, WalkState};

use crate::browse::{budget, live, Cancelled};
use crate::protocol::{ContentMatch, ContentSearchStop};
use crate::wire_path::RelPath;
use line::{clip, trim_line_ending};
use mask::Mask;

/// What a search looks for, as the request gives it.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub text: String,
    pub match_case: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub file_mask: Option<String>,
    pub include_ignored: bool,
}

/// The bounds one search is held to; [`Bounds::standard`] are those of `budget`.
#[derive(Debug, Clone)]
pub struct Bounds {
    pub timeout: Duration,
    pub max_matches: usize,
    pub max_file_bytes: u64,
    pub max_bytes_read: u64,
    pub max_line_bytes: usize,
    pub files_ahead: usize,
    pub max_line_chars: usize,
    pub line_chars_before: usize,
    pub batch_lines: usize,
    pub batch_text_bytes: usize,
    pub batch_wait: Duration,
    /// The walk's threads; 0 lets `ignore` choose by the machine's cores.
    pub walk_threads: usize,
    /// Run before each file is read, with its wire path.
    #[cfg(test)]
    pub before_reading: Option<fn(&str)>,
    /// Counts, beside the search's own, the readers waiting now, for a test that cannot reach
    /// the search's state.
    #[cfg(test)]
    pub waiters_seen: Option<&'static std::sync::atomic::AtomicUsize>,
}

impl Bounds {
    pub fn standard() -> Self {
        Self {
            timeout: budget::CONTENT_SEARCH_TIMEOUT,
            max_matches: budget::MAX_CONTENT_MATCHES,
            max_file_bytes: budget::MAX_CONTENT_FILE_BYTES,
            max_bytes_read: budget::MAX_CONTENT_BYTES_READ,
            max_line_bytes: budget::MAX_CONTENT_LINE_BYTES,
            files_ahead: budget::CONTENT_FILES_AHEAD,
            max_line_chars: budget::MAX_CONTENT_LINE_CHARS,
            line_chars_before: budget::CONTENT_LINE_CHARS_BEFORE,
            batch_lines: budget::CONTENT_BATCH_LINES,
            batch_text_bytes: budget::CONTENT_BATCH_TEXT_BYTES,
            batch_wait: budget::CONTENT_BATCH_WAIT,
            walk_threads: 0,
            #[cfg(test)]
            before_reading: None,
            #[cfg(test)]
            waiters_seen: None,
        }
    }
}

/// How a search ended: what it found, and whether it searched everything it was to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub matches: u64,
    pub files: u64,
    pub complete: bool,
    pub stopped_by: Option<ContentSearchStop>,
}

impl Summary {
    /// The end of a search that failed after it was acknowledged: incomplete, with no bound named
    /// and nothing counted, since what was sent before the failure cannot be told from the rest.
    pub fn failed() -> Self {
        Self {
            matches: 0,
            files: 0,
            complete: false,
            stopped_by: None,
        }
    }
}

/// What became of a batch handed to a search's `send`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    Sent,
    /// Not sent: it would take the search past the bytes it sends.
    Refused,
    /// Not sent: the search's time ran out while it waited for room to send it.
    OutOfTime,
}

/// A query made ready to search with: its matcher and its file mask. Building it is where a
/// pattern or a glob that does not compile is refused, before the search is acknowledged.
pub struct Compiled {
    matcher: RegexMatcher,
    mask: Mask,
    include_ignored: bool,
    /// Nothing can match: an empty query.
    empty: bool,
}

impl Compiled {
    pub fn new(query: &Query) -> Result<Self> {
        let matcher = RegexMatcherBuilder::new()
            .case_insensitive(!query.match_case)
            .word(query.whole_word)
            .fixed_strings(!query.regex)
            // Line anchors match at each line's end, before a `\r\n` as before a `\n`, as
            // ripgrep's `--crlf`; `crlf` also sets the line terminator the searcher splits by.
            .multi_line(true)
            .crlf(true)
            .build(&query.text)
            .map_err(|err| {
                refusal::invalid_pattern(&query.text, query.regex, !query.match_case, &err)
            })?;
        Ok(Self {
            matcher,
            mask: Mask::parse(query.file_mask.as_deref().unwrap_or(""))?,
            include_ignored: query.include_ignored,
            empty: query.text.is_empty(),
        })
    }
}

/// A file's matching lines, each with how many matches it holds (counting those clipped from its
/// text), as a walk thread hands them over.
type FileMatches = Vec<(ContentMatch, usize)>;

/// What the walk's threads and the calling thread share.
struct Shared<'a> {
    cancel: &'a AtomicBool,
    deadline: Instant,
    max_bytes_read: u64,
    bytes_read: AtomicU64,
    max_matches: u64,
    /// The matches the walk threads have collected, sent or not, counted as each line is and
    /// taken back off for a file that turns out binary. Past `max_matches` a file holding counted
    /// lines stops at its next matching line and hands them over, and a reader holding none waits
    /// until the batches reach the bound or a binary file's lines are taken back off: what is held
    /// stays within about the bound and a line per thread.
    found: AtomicU64,
    /// The bound that stopped the search, as `stop_code` writes it; 0 while none has.
    stop: AtomicU8,
    /// The search is ending for a reason that is no bound (its batches could not be sent).
    quit: AtomicBool,
    /// Some file or folder could not be read.
    unreadable: AtomicBool,
    /// The readers wait while this holds: the stream waits for room on the reply queue without
    /// its read slot.
    paused: Mutex<bool>,
    /// Wakes the waiting readers: on a pause ending, a bound reached, the search quitting, and
    /// matches taken back off.
    wake: Condvar,
    /// The readers waiting in [`Shared::may_read`] now.
    #[cfg(test)]
    waiters: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    waiters_seen: Option<&'static std::sync::atomic::AtomicUsize>,
    /// Run at the start of each read, with the matches its file holds.
    #[cfg(test)]
    before_read: Option<BeforeRead>,
}

/// A test's hook run at the start of each read, with the matches the read's file holds.
#[cfg(test)]
type BeforeRead = Box<dyn Fn(&Shared, usize) + Sync>;

/// How long a waiting reader sleeps before it looks again at what nothing wakes it for: the
/// cancel, which the lane sets on a flag of its own, and the deadline.
const READER_LOOKS_EVERY: Duration = Duration::from_millis(20);

impl<'a> Shared<'a> {
    fn new(cancel: &'a AtomicBool, bounds: &Bounds) -> Self {
        Self {
            cancel,
            deadline: Instant::now() + bounds.timeout,
            max_bytes_read: bounds.max_bytes_read,
            bytes_read: AtomicU64::new(0),
            max_matches: bounds.max_matches as u64,
            found: AtomicU64::new(0),
            stop: AtomicU8::new(0),
            quit: AtomicBool::new(false),
            unreadable: AtomicBool::new(false),
            paused: Mutex::new(false),
            wake: Condvar::new(),
            #[cfg(test)]
            waiters: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            waiters_seen: bounds.waiters_seen,
            #[cfg(test)]
            before_read: None,
        }
    }

    fn stop(&self, stop: ContentSearchStop) {
        let _ =
            self.stop
                .compare_exchange(0, stop_code(stop), Ordering::Relaxed, Ordering::Relaxed);
        self.wake_readers();
    }

    /// Ends the search for a reason that is no bound.
    fn quit(&self) {
        self.quit.store(true, Ordering::Relaxed);
        self.wake_readers();
    }

    /// Wakes the waiting readers without taking the pause lock, since [`Shared::halted`], which
    /// may stop the search at its time, is asked under it; a reader that misses the wake looks
    /// again within [`READER_LOOKS_EVERY`].
    fn wake_readers(&self) {
        self.wake.notify_all();
    }

    /// Pauses the readers, or lets them go on.
    fn pause(&self, paused: bool) {
        *self.paused.lock().expect("pause lock poisoned") = paused;
        self.wake.notify_all();
    }

    /// Takes `matches` collected from a file that turned out binary back off the count.
    fn take_back(&self, matches: usize) {
        if matches > 0 {
            self.found.fetch_sub(matches as u64, Ordering::Relaxed);
            self.wake_readers();
        }
    }

    fn over_bound(&self) -> bool {
        self.found.load(Ordering::Relaxed) > self.max_matches
    }

    /// Whether a reader may read on: it waits while the readers are paused, and while the matches
    /// collected are past the bound unless `holding` (its file holds counted matches, which only
    /// reading on to the file's end hands over), and answers false once the search is halted.
    fn may_read(&self, holding: bool) -> bool {
        let mut paused = self.paused.lock().expect("pause lock poisoned");
        loop {
            if self.halted() {
                return false;
            }
            if !*paused && (holding || !self.over_bound()) {
                return true;
            }
            let left = self.deadline.saturating_duration_since(Instant::now());
            #[cfg(test)]
            {
                self.waiters.fetch_add(1, Ordering::Relaxed);
                if let Some(seen) = self.waiters_seen {
                    seen.fetch_add(1, Ordering::Relaxed);
                }
            }
            paused = self
                .wake
                .wait_timeout(paused, left.min(READER_LOOKS_EVERY))
                .expect("pause lock poisoned")
                .0;
            #[cfg(test)]
            {
                self.waiters.fetch_sub(1, Ordering::Relaxed);
                if let Some(seen) = self.waiters_seen {
                    seen.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn stopped_by(&self) -> Option<ContentSearchStop> {
        match self.stop.load(Ordering::Relaxed) {
            1 => Some(ContentSearchStop::Time),
            2 => Some(ContentSearchStop::Matches),
            3 => Some(ContentSearchStop::BytesRead),
            4 => Some(ContentSearchStop::ResultBytes),
            _ => None,
        }
    }

    /// Whether the walk should stop now: cancelled, a bound reached, quitting, or out of time
    /// (which this records as the bound reached).
    fn halted(&self) -> bool {
        if self.cancel.load(Ordering::Relaxed)
            || self.stop.load(Ordering::Relaxed) != 0
            || self.quit.load(Ordering::Relaxed)
        {
            return true;
        }
        if Instant::now() >= self.deadline {
            self.stop(ContentSearchStop::Time);
            return true;
        }
        false
    }
}

fn stop_code(stop: ContentSearchStop) -> u8 {
    match stop {
        ContentSearchStop::Time => 1,
        ContentSearchStop::Matches => 2,
        ContentSearchStop::BytesRead => 3,
        ContentSearchStop::ResultBytes => 4,
    }
}

/// The directory a search walks: `scope` (canonical), or `directory` within it, refused as a read
/// of it would be when it leads outside, does not exist or is not a directory.
pub fn root(scope: &Path, directory: Option<&RelPath>) -> Result<PathBuf> {
    match directory {
        Some(directory) if !directory.is_root() => live::directory_within(scope, directory),
        _ => Ok(scope.to_path_buf()),
    }
}

/// What a search hands each batch to, with what goes with sending it.
pub type Send<'a> = dyn FnMut(Vec<ContentMatch>, &Sending) -> Result<Taken> + 'a;

/// What goes with sending one batch: when the search's time runs out, and the readers to pause
/// while the batch waits for room without the read slot.
pub struct Sending<'a> {
    pub deadline: Instant,
    pause: &'a (dyn Fn(bool) + Sync),
}

impl Sending<'_> {
    /// Pauses the search's readers (`true`), or lets them go on (`false`).
    pub fn pause_readers(&self, paused: bool) {
        (self.pause)(paused);
    }
}

/// Searches `root`, `scope` (canonical) or a directory within it, sending the matches found to
/// `send` in batches, in no particular order. A batch `send` refuses ends the search at the result
/// bound, and one it ran out of time for at the time bound. Blocking: it reads the files on
/// threads of its own and returns once they have all stopped. A cancel ends it with [`Cancelled`].
pub fn search(
    scope: &Path,
    root: &Path,
    query: &Compiled,
    bounds: &Bounds,
    cancel: &AtomicBool,
    send: &mut Send,
) -> Result<Summary> {
    #[cfg(test)]
    if tests::fails_after_the_ack(root) {
        return Err(anyhow::anyhow!("a failure a test asked for"));
    }
    let shared = Shared::new(cancel, bounds);
    let mut summary = Summary {
        matches: 0,
        files: 0,
        complete: true,
        stopped_by: None,
    };
    if query.empty {
        return Ok(summary);
    }
    let include_ignored = query.include_ignored;
    let walker = WalkBuilder::new(root)
        .standard_filters(false)
        .hidden(false)
        .ignore(false)
        .git_ignore(!include_ignored)
        .git_global(!include_ignored)
        .git_exclude(!include_ignored)
        .parents(!include_ignored)
        .require_git(true)
        .follow_links(false)
        .filter_entry(|entry| !live::is_left_out(entry.file_name().as_encoded_bytes()))
        .threads(bounds.walk_threads)
        .build_parallel();
    let (sender, receiver) = mpsc::sync_channel::<FileMatches>(bounds.files_ahead);
    let mut sent_failure = None;
    std::thread::scope(|threads| {
        let shared = &shared;
        threads.spawn(move || {
            walker.run(|| {
                let sender = sender.clone();
                let mut searcher = searcher(bounds);
                Box::new(move |entry| {
                    if !shared.may_read(false) {
                        return WalkState::Quit;
                    }
                    let entry = match entry {
                        Ok(entry) => entry,
                        // An ignore file above the walk's root that cannot be read or does not
                        // parse is not the search failing: its rules are left out, as those of
                        // one below it are.
                        Err(err) if is_ignore_file_error(&err) => return WalkState::Continue,
                        Err(_) => {
                            shared.unreadable.store(true, Ordering::Relaxed);
                            return WalkState::Continue;
                        }
                    };
                    if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                        return WalkState::Continue;
                    }
                    let Some(path) = RelPath::below(scope, entry.path()) else {
                        shared.unreadable.store(true, Ordering::Relaxed);
                        return WalkState::Continue;
                    };
                    if !query.mask.admits(&path) {
                        return WalkState::Continue;
                    }
                    let found = search_file(scope, &path, query, bounds, shared, &mut searcher);
                    if !found.is_empty() && sender.send(found).is_err() {
                        return WalkState::Quit;
                    }
                    WalkState::Continue
                })
            });
        });
        sent_failure = gather(receiver, bounds, shared, &mut summary, send).err();
        if sent_failure.is_some() {
            shared.quit();
        }
    });
    if let Some(err) = sent_failure {
        return Err(err);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(anyhow::Error::new(Cancelled));
    }
    summary.stopped_by = shared.stopped_by();
    summary.complete = summary.stopped_by.is_none() && !shared.unreadable.load(Ordering::Relaxed);
    Ok(summary)
}

/// The searcher of one walk thread.
fn searcher(bounds: &Bounds) -> Searcher {
    SearcherBuilder::new()
        .line_number(true)
        .line_terminator(LineTerminator::crlf())
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .heap_limit(Some(bounds.max_line_bytes))
        .build()
}

/// Whether `err` is only ignore files failing to be read or parsed, rather than a file or folder
/// of the search that could not be read: `ignore` hands the walk those of the ignore files above
/// its root, each a glob that does not parse, a line that cannot be read, or the file itself,
/// alone or several together.
fn is_ignore_file_error(err: &ignore::Error) -> bool {
    is_ignore_file_error_within(err, false)
}

/// [`is_ignore_file_error`], below a walk's own error (`in_walk`): what the walk could not read
/// carries the depth it was at, and an ignore file's failure never does, so a folder named like
/// an ignore file that cannot be listed is not taken for one.
fn is_ignore_file_error_within(err: &ignore::Error, in_walk: bool) -> bool {
    match err {
        ignore::Error::Partial(errs) => errs
            .iter()
            .all(|err| is_ignore_file_error_within(err, in_walk)),
        ignore::Error::Glob { .. } | ignore::Error::WithLineNumber { .. } => true,
        ignore::Error::WithPath { path, err } => {
            is_ignore_file_error_within(err, in_walk)
                || (!in_walk && matches!(**err, ignore::Error::Io(_)) && is_ignore_file(path))
        }
        ignore::Error::WithDepth { err, .. } => is_ignore_file_error_within(err, true),
        _ => false,
    }
}

/// Whether the project file `path` is a `.gitignore`, which a search that cannot read it skips as
/// Git does. (An `info/exclude` is only ever a repository's in `.git`, which the walk leaves out:
/// a project file of that name is an ordinary one.)
fn is_gitignore(path: &RelPath) -> bool {
    path.as_bytes().rsplit(|&byte| byte == b'/').next() == Some(b".gitignore".as_slice())
}

/// Whether `path`, named by an error `ignore` hands the walk, is an ignore file it reads: a
/// `.gitignore`, or a repository's `info/exclude`.
fn is_ignore_file(path: &Path) -> bool {
    path.file_name() == Some(std::ffi::OsStr::new(".gitignore"))
        || (path.file_name() == Some(std::ffi::OsStr::new("exclude"))
            && path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "info"))
}

/// Gathers the files' matches into batches and sends each as it fills, or once its first match
/// has waited `batch_wait`, until the walk's threads have all stopped. Counts what it sends into
/// `summary`, stopping the walk at the match bound and at a batch `send` refuses. Returns `send`'s
/// own failure.
fn gather(
    receiver: mpsc::Receiver<FileMatches>,
    bounds: &Bounds,
    shared: &Shared,
    summary: &mut Summary,
    send: &mut Send,
) -> Result<()> {
    let mut batch = Batch::default();
    // Once a bound is reached, or the search is cancelled, what still arrives is dropped while
    // the walk's threads stop.
    let mut taking = true;
    loop {
        let wait = match batch.since {
            Some(since) => bounds.batch_wait.saturating_sub(since.elapsed()),
            None => bounds.batch_wait,
        };
        let received = receiver.recv_timeout(wait.max(Duration::from_millis(1)));
        if shared.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        match received {
            Ok(file) if taking => {
                if !batch.take(file, summary.matches, bounds.max_matches as u64) {
                    shared.stop(ContentSearchStop::Matches);
                    taking = false;
                }
            }
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let due = batch.lines.len() >= bounds.batch_lines
            || batch.text_bytes >= bounds.batch_text_bytes
            || batch
                .since
                .is_some_and(|since| since.elapsed() >= bounds.batch_wait)
            || !taking;
        if due && !batch.lines.is_empty() && !batch.send(summary, shared, send)? {
            taking = false;
        }
    }
    if !batch.lines.is_empty() {
        batch.send(summary, shared, send)?;
    }
    Ok(())
}

/// The matches gathered and not yet sent, with what they add to the summary once they are.
#[derive(Default)]
struct Batch {
    lines: Vec<ContentMatch>,
    matches: u64,
    files: u64,
    text_bytes: usize,
    since: Option<Instant>,
}

impl Batch {
    /// Adds one file's lines while the search's matches, `sent` with those gathered, stay within
    /// `max`; false once a line would take them past it.
    fn take(&mut self, file: FileMatches, sent: u64, max: u64) -> bool {
        let mut counted_file = false;
        for (line, count) in file {
            if sent + self.matches + count as u64 > max {
                return false;
            }
            self.matches += count as u64;
            if !counted_file {
                self.files += 1;
                counted_file = true;
            }
            self.text_bytes += line.text.len() + line.path.len();
            self.lines.push(line);
            self.since.get_or_insert_with(Instant::now);
        }
        true
    }

    /// Hands the batch to `send`, counting it into `summary` once sent; false, with the bound that
    /// stopped it recorded, when it was not. Either way the batch is empty after.
    fn send(&mut self, summary: &mut Summary, shared: &Shared, send: &mut Send) -> Result<bool> {
        let batch = std::mem::take(self);
        let pause = |paused: bool| shared.pause(paused);
        let sending = Sending {
            deadline: shared.deadline,
            pause: &pause,
        };
        match send(batch.lines, &sending)? {
            Taken::Sent => {
                summary.matches += batch.matches;
                summary.files += batch.files;
                Ok(true)
            }
            Taken::Refused => {
                shared.stop(ContentSearchStop::ResultBytes);
                Ok(false)
            }
            Taken::OutOfTime => {
                shared.stop(ContentSearchStop::Time);
                Ok(false)
            }
        }
    }
}

/// Searches one file, returning its matching lines; nothing for a binary file, one that is not a
/// regular file or is larger than the bound, and for one that cannot be read, which marks the
/// search incomplete unless it is a `.gitignore`. A bound reached while it reads keeps the lines found up to there.
fn search_file(
    scope: &Path,
    path: &RelPath,
    query: &Compiled,
    bounds: &Bounds,
    shared: &Shared,
    searcher: &mut Searcher,
) -> FileMatches {
    // A `.gitignore` that cannot be read is no failure of the search, which Git would skip as
    // well: not searching it leaves the search complete.
    let gitignore = is_gitignore(path);
    let file = match live::open_searchable(scope, path, bounds.max_file_bytes) {
        Ok(Some(file)) => file,
        Ok(None) => return Vec::new(),
        Err(_) => {
            if !gitignore {
                shared.unreadable.store(true, Ordering::Relaxed);
            }
            return Vec::new();
        }
    };
    let wire = path.to_wire();
    #[cfg(test)]
    if let Some(before_reading) = bounds.before_reading {
        before_reading(&wire);
    }
    // The matches the file has collected, which its reader reads on past the bound while it holds
    // any: they count toward the bound, and only handing them over lets the batches reach it.
    let held = Cell::new(0);
    let mut sink = FileSink {
        matcher: &query.matcher,
        path: &wire,
        bounds,
        shared,
        found: Vec::new(),
        held: &held,
        at_bound: false,
        binary: false,
    };
    let mut reader = Counted {
        file,
        shared,
        read: 0,
        limit: bounds.max_file_bytes,
        held: &held,
    };
    let mut outcome = searcher
        .search_reader(&query.matcher, &mut reader, &mut sink)
        .map(|()| false);
    // A file stopped at the bound may still turn out binary: the rest of it is read for a NUL, on
    // past the bound since it holds the line that took the search there.
    if !sink.binary && sink.at_bound && outcome.is_ok() {
        outcome = has_nul(&mut reader);
    }
    if sink.binary || matches!(outcome, Ok(true)) {
        shared.take_back(held.get());
        return Vec::new();
    }
    if outcome.is_err() && !shared.halted() && !gitignore {
        shared.unreadable.store(true, Ordering::Relaxed);
    }
    sink.found
}

/// Whether what is left of `reader` holds a NUL byte.
fn has_nul(reader: &mut impl Read) -> io::Result<bool> {
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(false);
        }
        if buffer[..read].contains(&0) {
            return Ok(true);
        }
    }
}

/// A file's reader that counts the bytes read against the search's bound, waits while the
/// readers must ([`Shared::may_read`]; on the bound only while its file holds no counted
/// matches, `held`), stops the read once the search is halted, and ends the file at `limit`
/// bytes, however much it has grown since it was opened.
struct Counted<'a, 's> {
    file: std::fs::File,
    shared: &'a Shared<'s>,
    read: u64,
    limit: u64,
    held: &'a Cell<usize>,
}

impl Read for Counted<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        #[cfg(test)]
        if let Some(before_read) = &self.shared.before_read {
            before_read(self.shared, self.held.get());
        }
        if !self.shared.may_read(self.held.get() > 0) {
            return Err(io::Error::other("the search stopped"));
        }
        let left = usize::try_from(self.limit.saturating_sub(self.read)).unwrap_or(usize::MAX);
        let room = buffer.len().min(left);
        let read = self.file.read(&mut buffer[..room])?;
        self.read += read as u64;
        let total = self
            .shared
            .bytes_read
            .fetch_add(read as u64, Ordering::Relaxed)
            + read as u64;
        if total > self.shared.max_bytes_read {
            self.shared.stop(ContentSearchStop::BytesRead);
            return Err(io::Error::other("the search read its whole budget"));
        }
        Ok(read)
    }
}

/// Collects one file's matching lines.
struct FileSink<'a, 's> {
    matcher: &'a RegexMatcher,
    path: &'a str,
    bounds: &'a Bounds,
    shared: &'a Shared<'s>,
    found: FileMatches,
    /// The matches in `found`, counted into the search's as each line is collected: the matches
    /// the file holds.
    held: &'a Cell<usize>,
    /// The search's matches went past the bound with this file's last line, which stopped it.
    at_bound: bool,
    binary: bool,
}

impl Sink for FileSink<'_, '_> {
    type Error = io::Error;

    fn matched(&mut self, _: &Searcher, found: &SinkMatch<'_>) -> Result<bool, io::Error> {
        // The ranges are found in the same text the searcher decided the line matches by, line
        // ending included, so that a `$` before a `\r\n` finds them too; a range is then kept to
        // the line without its ending.
        let line = trim_line_ending(found.bytes());
        let mut ranges = Vec::new();
        self.matcher
            .find_iter(found.bytes(), |range| {
                let end = range.end().min(line.len());
                if range.start() < end {
                    ranges.push(range.start()..end);
                }
                true
            })
            .map_err(|err| io::Error::other(err.to_string()))?;
        if !ranges.is_empty() {
            let count = ranges.len();
            self.held.set(self.held.get() + count);
            self.shared.found.fetch_add(count as u64, Ordering::Relaxed);
            let line_number = found.line_number().unwrap_or(0);
            let clipped = clip(line, &ranges, self.bounds);
            self.found.push((
                ContentMatch {
                    path: self.path.to_string(),
                    line: line_number,
                    text: clipped.text,
                    text_start: clipped.text_start,
                    ranges: clipped.ranges,
                    clipped_end: clipped.clipped_end,
                },
                count,
            ));
        }
        // A file stops at the line that takes the search past its bound, which is enough for the
        // batches to reach it.
        if self.shared.over_bound() {
            self.at_bound = true;
            return Ok(false);
        }
        Ok(!self.shared.halted())
    }

    fn binary_data(&mut self, _: &Searcher, _: u64) -> Result<bool, io::Error> {
        self.binary = true;
        Ok(false)
    }
}

#[cfg(test)]
mod gather_tests {
    use super::*;
    use crate::protocol::MatchRange;
    use crate::test_support::PATIENCE;

    fn line(path: &str, number: u64) -> (ContentMatch, usize) {
        let found = ContentMatch {
            path: path.to_string(),
            line: number,
            text: "needle".to_string(),
            text_start: 0,
            ranges: vec![MatchRange { start: 0, end: 6 }],
            clipped_end: false,
        };
        (found, 1)
    }

    fn shared(cancel: &AtomicBool) -> Shared<'_> {
        Shared::new(
            cancel,
            &Bounds {
                timeout: PATIENCE,
                max_bytes_read: u64::MAX,
                max_matches: usize::MAX / 2,
                ..Bounds::standard()
            },
        )
    }

    /// A reader over a file holding `bytes`, held to `limit` bytes of it, its file holding the
    /// matches `held` says.
    fn reader<'a, 's>(
        shared: &'a Shared<'s>,
        dir: &crate::test_support::ScratchDir,
        bytes: &[u8],
        limit: u64,
        held: &'a Cell<usize>,
    ) -> Counted<'a, 's> {
        let path = dir.join("file.txt");
        std::fs::write(&path, bytes).unwrap();
        Counted {
            file: std::fs::File::open(path).unwrap(),
            shared,
            read: 0,
            limit,
            held,
        }
    }

    /// Waits until `count` readers wait in `shared`.
    fn waiting(shared: &Shared, count: usize) {
        let waits = crate::session::wait::wait_for(PATIENCE, || {
            shared.waiters.load(Ordering::Relaxed) == count
        });
        assert!(waits, "the reader never waited");
    }

    #[test]
    fn a_file_is_read_only_to_its_limit() {
        let cancel = AtomicBool::new(false);
        let shared = shared(&cancel);
        let dir = crate::test_support::ScratchDir::new("content-search-counted");
        let mut read = Vec::new();
        let held = Cell::new(0);
        reader(&shared, &dir, &[b'x'; 100], 10, &held)
            .read_to_end(&mut read)
            .unwrap();
        assert_eq!(read.len(), 10);
        assert_eq!(shared.bytes_read.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn a_paused_reader_reads_once_resumed() {
        let cancel = AtomicBool::new(false);
        let shared = shared(&cancel);
        let dir = crate::test_support::ScratchDir::new("content-search-paused");
        shared.pause(true);
        std::thread::scope(|threads| {
            let reading = threads.spawn(|| {
                let mut buffer = [0; 16];
                let held = Cell::new(0);
                reader(&shared, &dir, b"needle", 1024, &held).read(&mut buffer)
            });
            waiting(&shared, 1);
            assert_eq!(shared.bytes_read.load(Ordering::Relaxed), 0);
            shared.pause(false);
            assert_eq!(reading.join().unwrap().unwrap(), 6);
        });
    }

    #[test]
    fn a_paused_reader_reads_nothing_before_the_deadline_or_a_cancel_ends_it() {
        let dir = crate::test_support::ScratchDir::new("content-search-paused-end");
        // Out of time while paused: a short deadline, as a test of a timeout keeps it.
        let cancel = AtomicBool::new(false);
        let timed = Shared::new(
            &cancel,
            &Bounds {
                timeout: Duration::from_millis(50),
                ..Bounds::standard()
            },
        );
        timed.pause(true);
        let mut buffer = [0; 16];
        let held = Cell::new(0);
        assert!(reader(&timed, &dir, b"needle", 1024, &held)
            .read(&mut buffer)
            .is_err());
        assert_eq!(timed.bytes_read.load(Ordering::Relaxed), 0);
        assert_eq!(timed.stopped_by(), Some(ContentSearchStop::Time));
        // Cancelled while paused, its deadline well past the test's patience.
        let cancel = AtomicBool::new(false);
        let cancelled = Shared::new(
            &cancel,
            &Bounds {
                timeout: PATIENCE * 4,
                ..Bounds::standard()
            },
        );
        cancelled.pause(true);
        std::thread::scope(|threads| {
            let reading = threads.spawn(|| {
                let mut buffer = [0; 16];
                let held = Cell::new(0);
                reader(&cancelled, &dir, b"needle", 1024, &held).read(&mut buffer)
            });
            waiting(&cancelled, 1);
            cancel.store(true, Ordering::Relaxed);
            assert!(reading.join().unwrap().is_err());
        });
        assert_eq!(cancelled.bytes_read.load(Ordering::Relaxed), 0);
        assert_eq!(cancelled.stopped_by(), None);
    }

    #[test]
    fn a_reader_waits_past_the_bound_until_the_matches_are_taken_back() {
        let cancel = AtomicBool::new(false);
        let shared = Shared::new(
            &cancel,
            &Bounds {
                timeout: PATIENCE,
                max_matches: 1,
                ..Bounds::standard()
            },
        );
        shared.found.store(2, Ordering::Relaxed);
        let dir = crate::test_support::ScratchDir::new("content-search-bound-wait");
        std::thread::scope(|threads| {
            let reading = threads.spawn(|| {
                let mut buffer = [0; 16];
                let held = Cell::new(0);
                reader(&shared, &dir, b"needle", 1024, &held).read(&mut buffer)
            });
            waiting(&shared, 1);
            shared.take_back(2);
            assert_eq!(reading.join().unwrap().unwrap(), 6);
        });
    }

    /// The wedge this guards against: a file holding counted lines whose reader waited on the
    /// bound another file took the search past would hold them until the deadline, and the
    /// batches, which see only lines handed over, could never reach the bound.
    #[test]
    fn a_file_holding_counted_lines_reads_on_past_the_bound_and_hands_them_over() {
        let cancel = AtomicBool::new(false);
        let bounds = Bounds {
            timeout: PATIENCE,
            max_matches: 5,
            ..Bounds::standard()
        };
        let mut shared = Shared::new(&cancel, &bounds);
        // Once the file holds its first line, another file takes the search past the bound,
        // between two of this file's reads.
        let crossed = AtomicBool::new(false);
        shared.before_read = Some(Box::new(move |shared, held| {
            if held > 0 && !crossed.swap(true, Ordering::Relaxed) {
                shared.found.fetch_add(5, Ordering::Relaxed);
            }
        }));
        let dir = crate::test_support::ScratchDir::new("content-search-holding");
        let scope = crate::hostfs::canonical(&dir).unwrap();
        // The second match is past the searcher's first buffer (64 KiB), read after the crossing.
        let mut text = b"needle\n".to_vec();
        text.extend(std::iter::repeat_n(b"filler line\n", 8_000).flatten());
        text.extend(b"needle again\n");
        text.extend(std::iter::repeat_n(b"filler line\n", 100).flatten());
        std::fs::write(scope.join("text.txt"), &text).unwrap();
        let query = Compiled::new(&Query {
            text: "needle".to_string(),
            ..Query::default()
        })
        .unwrap();
        let path = RelPath::parse("text.txt").unwrap();
        let found = search_file(
            &scope,
            &path,
            &query,
            &bounds,
            &shared,
            &mut searcher(&bounds),
        );
        assert_eq!(found.len(), 2, "the file did not hand its lines over");
        assert_eq!(
            shared.stopped_by(),
            None,
            "the file waited out the deadline"
        );
        assert!(shared.over_bound(), "the lines handed over reach the bound");
    }

    #[test]
    fn a_binary_file_s_lines_are_taken_back_off_the_count() {
        let cancel = AtomicBool::new(false);
        let bounds = Bounds {
            timeout: PATIENCE,
            max_matches: 2,
            ..Bounds::standard()
        };
        let shared = Shared::new(&cancel, &bounds);
        let dir = crate::test_support::ScratchDir::new("content-search-binary-count");
        let scope = crate::hostfs::canonical(&dir).unwrap();
        // The NUL comes after the searcher's first buffer (64 KiB), once the lines before it took
        // the count past the bound.
        let mut binary = b"needle\nneedle\nneedle\nneedle\n".to_vec();
        binary.extend(std::iter::repeat_n(b"filler line\n", 8_000).flatten());
        binary.extend(b"\0needle\n");
        std::fs::write(scope.join("binary.bin"), &binary).unwrap();
        let query = Compiled::new(&Query {
            text: "needle".to_string(),
            ..Query::default()
        })
        .unwrap();
        let path = RelPath::parse("binary.bin").unwrap();
        let found = search_file(
            &scope,
            &path,
            &query,
            &bounds,
            &shared,
            &mut searcher(&bounds),
        );
        assert!(found.is_empty());
        assert_eq!(shared.found.load(Ordering::Relaxed), 0);
        assert!(!shared.halted());
    }

    fn empty_summary() -> Summary {
        Summary {
            matches: 0,
            files: 0,
            complete: true,
            stopped_by: None,
        }
    }

    /// A batch goes out once its first match has waited, while the walk is still running: here
    /// the sender is still open when the batch arrives.
    #[test]
    fn a_batch_is_sent_while_the_walk_still_runs() {
        let cancel = AtomicBool::new(false);
        let shared = shared(&cancel);
        let bounds = Bounds::standard();
        let (sender, receiver) = mpsc::sync_channel::<FileMatches>(4);
        let (sent_tx, sent_rx) = mpsc::channel::<Vec<ContentMatch>>();
        let mut summary = empty_summary();
        std::thread::scope(|threads| {
            let gathering = threads.spawn(|| {
                let mut send = |batch: Vec<ContentMatch>, _: &Sending| {
                    sent_tx.send(batch).unwrap();
                    Ok(Taken::Sent)
                };
                gather(receiver, &bounds, &shared, &mut summary, &mut send)
            });
            sender.send(vec![line("a.txt", 1)]).unwrap();
            let first = sent_rx
                .recv_timeout(PATIENCE)
                .expect("a batch before the end");
            assert_eq!(first.len(), 1);
            sender
                .send(vec![line("b.txt", 2), line("b.txt", 3)])
                .unwrap();
            drop(sender);
            gathering.join().unwrap().unwrap();
        });
        let second = sent_rx.recv_timeout(PATIENCE).unwrap();
        assert_eq!(second.len(), 2);
        assert_eq!((summary.matches, summary.files), (3, 2));
    }

    #[test]
    fn a_full_batch_is_sent_without_waiting() {
        let cancel = AtomicBool::new(false);
        let shared = shared(&cancel);
        let bounds = Bounds {
            batch_lines: 2,
            batch_wait: PATIENCE,
            ..Bounds::standard()
        };
        let (sender, receiver) = mpsc::sync_channel::<FileMatches>(4);
        let (sent_tx, sent_rx) = mpsc::channel::<usize>();
        let mut summary = empty_summary();
        std::thread::scope(|threads| {
            let gathering = threads.spawn(|| {
                let mut send = |batch: Vec<ContentMatch>, _: &Sending| {
                    sent_tx.send(batch.len()).unwrap();
                    Ok(Taken::Sent)
                };
                gather(receiver, &bounds, &shared, &mut summary, &mut send)
            });
            sender
                .send(vec![line("a.txt", 1), line("a.txt", 2)])
                .unwrap();
            assert_eq!(sent_rx.recv_timeout(PATIENCE / 2), Ok(2));
            drop(sender);
            gathering.join().unwrap().unwrap();
        });
    }
}
