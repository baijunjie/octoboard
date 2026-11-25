//! Reading one conversation's files for the feed, from their start and then as they grow: the
//! earlier stretches of a conversation the agent keeps across several files are read whole once,
//! the last file (the head) is read again as lines are appended to it, and so is the file beside
//! it that shows what the agent is still producing (`RecordStore::feed_companion`). Which files
//! these are comes from `records::history_branch`; this holds no knowledge of any agent's formats.
//!
//! Of a record that is a tree (`RecordStore::branching`) the reader is handed the current branch
//! alone: the first pass leaves out the lines the agent abandoned, and a line appended later that
//! does not carry on from the one before it restarts the conversation ([`Read::Restarted`]), so
//! that it is read again along the branch that line begins.
//!
//! A conversation kept as rows (`records::rows`) has no file to follow: its follower is built
//! from the rows as they stand ([`Follower::build_rows`]) and shown them again whenever they were
//! read anew ([`Follower::read_rows`]). Rows that only grew are new lines; anything else (a row
//! changed in place, removed, or forking off the branch read so far) restarts the conversation.
//!
//! A follower is blocking work (file reads, parsing): it runs on the blocking pool, never on the
//! async threads.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use super::{reader_for, FeedOut, FeedReader};
use crate::adapter::provider;
use crate::protocol::AgentId;
use crate::records::branch::{for_each_row_on_branch, on_branch, Branching, Leaf};
use crate::records::conversation::for_each_line;
use crate::records::line_file::{signature, Change, LineFile, Signature};
use crate::records::Stretch;

/// How often a followed conversation's files are looked at for what was added.
pub const POLL_EVERY: Duration = Duration::from_millis(250);
/// How often the files the conversation is read from are looked for again while none is found
/// (the session's record may not exist yet) or the head is missing.
pub const RELOCATE_EVERY: Duration = Duration::from_secs(2);
/// How often they are looked for again while the head reads cleanly. A change of the session's
/// agent id, account directory or recorded path is acted on at once, so what this waits for is
/// the agent starting another file on its own (the new segment of a Codex thread that was
/// reverted).
pub const RELOCATE_BACKOFF: Duration = Duration::from_secs(15);

/// A conversation's files being read, and the reader that turns their lines into entries.
pub struct Follower {
    stretches: Vec<Stretch>,
    head: Option<Tail>,
    companion: Option<Tail>,
    reader: Box<dyn FeedReader>,
    /// For a record that is a tree: how its lines link, and where the lines read so far stand in
    /// it.
    tree: Option<(&'static dyn Branching, Leaf)>,
    /// For a conversation kept as rows: the rows the reader has been handed, as last read.
    rows: Option<Vec<Value>>,
}

/// A file read again as it grows.
struct Tail {
    path: PathBuf,
    file: LineFile,
    /// Set when the first read of it failed part-way, after lines were handed on: what follows
    /// cannot tell where it stands, so the conversation is read again from its start, once the
    /// file has changed (the same file would fail the same way). Holds the file's signature at
    /// the failure, or `None` where that could not be read, which counts as changed.
    broken: Option<Option<Signature>>,
}

/// What a look at a [`Tail`] found.
enum Advance {
    Unchanged,
    Lines(Vec<Value>),
    Restarted,
    Missing,
    /// The file could not be read this time; it is read again at the next look, or, after a
    /// first read that failed part-way, once it has changed.
    Failed,
}

impl Tail {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: LineFile::default(),
            broken: None,
        }
    }

    /// Reads the file from its start, line by line, as [`Follower::build`] does.
    fn read_from_start(&mut self, mut visit: impl FnMut(Value)) {
        if self.file.check(&self.path).is_err() {
            return;
        }
        let mut handed = false;
        let read = self.file.read(&self.path, |line| {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                handed = true;
                visit(value);
            }
        });
        // A failure before any line was handed on is an ordinary retry: nothing was applied.
        self.broken = (read.is_err() && handed).then(|| signature(&self.path).ok());
    }

    /// Looks at the file once: what was added to it, or why that cannot be said. A look whose
    /// read fails hands nothing on, and the next one reads the same lines again.
    fn advance(&mut self) -> Advance {
        if let Some(broken) = self.broken {
            return if broken.is_some() && signature(&self.path).ok() == broken {
                Advance::Failed
            } else {
                Advance::Restarted
            };
        }
        match self.file.check(&self.path) {
            Ok(Change::Unchanged) => Advance::Unchanged,
            Ok(Change::Restarted) => Advance::Restarted,
            Ok(Change::Changed) => {
                let mut lines = Vec::new();
                let read = self.file.read(&self.path, |line| {
                    if let Ok(value) = serde_json::from_str::<Value>(line) {
                        lines.push(value);
                    }
                });
                match read {
                    Ok(()) if lines.is_empty() => Advance::Unchanged,
                    Ok(()) => Advance::Lines(lines),
                    Err(_) => Advance::Failed,
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Advance::Missing,
            Err(_) => Advance::Failed,
        }
    }
}

/// A follower and the entries its first pass over the conversation gave.
pub struct Built {
    pub follower: Follower,
    pub out: FeedOut,
}

/// What a file's lines added since the last read.
#[derive(Debug, Default, PartialEq)]
pub struct Lines {
    record: Vec<Value>,
    companion: Vec<Value>,
}

impl Lines {
    pub fn is_empty(&self) -> bool {
        self.record.is_empty() && self.companion.is_empty()
    }
}

/// What a reading of the conversation found: [`Follower::read_new`] in its files,
/// [`Follower::read_rows`] in its rows.
#[derive(Debug, PartialEq)]
pub enum Read {
    Unchanged,
    Lines(Lines),
    /// A file was replaced, truncated or rewritten, rows read before no longer stand, or a line
    /// of a record that is a tree forked off the branch read so far: what was read no longer
    /// holds, and the conversation is read again from its start ([`Follower::build`],
    /// [`Follower::build_rows`]).
    Restarted,
    /// The record's file is gone: the conversation is looked for again.
    Missing,
}

impl Follower {
    /// Reads the conversation of `agent` held in `stretches` (the last one the head, unless it has
    /// a `cut`) from its start, handing every line that parses to a fresh reader, the head's
    /// lines before the companion's. A file that cannot be read adds nothing: the next read of it
    /// says what is wrong.
    pub fn build(agent: AgentId, stretches: Vec<Stretch>) -> Built {
        let mut reader = reader_for(agent);
        let mut out = FeedOut::default();
        let (earlier, head) = match stretches.split_last() {
            Some((last, earlier)) if last.cut.is_none() => (earlier, Some(last.path.clone())),
            _ => (stretches.as_slice(), None),
        };
        let mut head = head.map(Tail::new);
        let mut tree = provider(agent)
            .records()
            .branching()
            .map(|branching| (branching, Leaf::default()));
        match &mut tree {
            None => {
                // A failure part-way yields what was read before it (`for_each_line`); one before
                // the first line leaves the stretch empty, and the head's read below goes on
                // regardless.
                let _ = for_each_line(earlier, |line| reader.line(line, &mut out));
                if let Some(tail) = &mut head {
                    tail.read_from_start(|line| reader.line(&line, &mut out));
                }
            }
            Some((branching, leaf)) => {
                // Read once for the links alone, then again for the lines of the current branch,
                // up to where the first read of the head stopped: what was appended meanwhile is
                // the next look's.
                let mut links = Vec::new();
                let _ = for_each_line(earlier, |line| links.push(branching.link(line)));
                if let Some(tail) = &mut head {
                    tail.read_from_start(|line| links.push(branching.link(&line)));
                }
                for link in links.iter().flatten() {
                    leaf.follow(Some(link.clone()));
                }
                let mut read: Vec<Stretch> = earlier.to_vec();
                read.extend(head.as_ref().map(|tail| Stretch {
                    path: tail.path.clone(),
                    cut: Some(tail.file.offset()),
                }));
                let kept = on_branch(&links);
                let mut index = 0;
                let _ = for_each_line(&read, |line| {
                    if kept.get(index).copied().unwrap_or(false) {
                        reader.line(line, &mut out);
                    }
                    index += 1;
                });
            }
        }
        let companion = head
            .as_ref()
            .and_then(|tail| provider(agent).records().feed_companion(&tail.path))
            .map(Tail::new);
        let mut follower = Self {
            stretches,
            head,
            companion,
            reader,
            tree,
            rows: None,
        };
        if let Some(tail) = &mut follower.companion {
            tail.read_from_start(|line| follower.reader.companion_line(&line, &mut out));
        }
        Built { follower, out }
    }

    /// Reads the conversation of `agent` whose rows are `rows` from its start, handing each row
    /// on its current branch to a fresh reader.
    pub fn build_rows(agent: AgentId, rows: Vec<Value>) -> Built {
        let mut reader = reader_for(agent);
        let mut out = FeedOut::default();
        let branching = provider(agent).records().branching();
        for_each_row_on_branch(branching, &rows, |_, row| reader.line(row, &mut out));
        let tree = branching.map(|branching| {
            let mut leaf = Leaf::default();
            for row in &rows {
                leaf.follow(branching.link(row));
            }
            (branching, leaf)
        });
        let follower = Self {
            stretches: Vec::new(),
            head: None,
            companion: None,
            reader,
            tree,
            rows: Some(rows),
        };
        Built { follower, out }
    }

    /// Takes `rows`, the conversation's rows as just read, for a follower built from rows: the
    /// rows added when those it was handed before still stand at their start and, for a record
    /// that is a tree, each added one carries on from the one before it; otherwise the
    /// conversation is to be read again from its start ([`Self::build_rows`]).
    pub fn read_rows(&mut self, rows: &[Value]) -> Read {
        let Some(known) = &self.rows else {
            return Read::Restarted;
        };
        if rows.len() < known.len() || rows[..known.len()] != known[..] {
            return Read::Restarted;
        }
        let added = rows[known.len()..].to_vec();
        if added.is_empty() {
            return Read::Unchanged;
        }
        if let Some((branching, leaf)) = &mut self.tree {
            let mut carries_on = true;
            for row in &added {
                carries_on &= leaf.follow(branching.link(row));
            }
            if !carries_on {
                return Read::Restarted;
            }
        }
        self.rows = Some(rows.to_vec());
        Read::Lines(Lines {
            record: added,
            companion: Vec::new(),
        })
    }

    /// Marks the head as read part-way and failed, with the file as it is now.
    #[cfg(test)]
    pub(super) fn break_head(&mut self) {
        let tail = self.head.as_mut().expect("a head");
        tail.broken = Some(signature(&tail.path).ok());
    }

    /// The files this reads, as [`Self::build`] was given them.
    pub fn stretches(&self) -> &[Stretch] {
        &self.stretches
    }

    /// The lines added to the head and the companion since they were last read.
    pub fn read_new(&mut self) -> Read {
        let mut lines = Lines::default();
        if let Some(tail) = &mut self.head {
            match tail.advance() {
                Advance::Lines(read) => lines.record = read,
                Advance::Restarted => return Read::Restarted,
                Advance::Missing => return Read::Missing,
                Advance::Unchanged | Advance::Failed => {}
            }
        }
        if let Some((branching, leaf)) = &mut self.tree {
            // Every line is followed, also past the first that forks off: the leaf is then where
            // the record stands, whatever the caller does with the restart.
            let mut carries_on = true;
            for line in &lines.record {
                carries_on &= leaf.follow(branching.link(line));
            }
            if !carries_on {
                return Read::Restarted;
            }
        }
        if let Some(tail) = &mut self.companion {
            match tail.advance() {
                Advance::Lines(read) => lines.companion = read,
                Advance::Restarted => return Read::Restarted,
                // The agent writes it when it has something to show: not there yet is no failure.
                Advance::Missing | Advance::Unchanged | Advance::Failed => {}
            }
        }
        if lines.is_empty() {
            Read::Unchanged
        } else {
            Read::Lines(lines)
        }
    }

    /// Hands what [`Self::read_new`] read to the reader.
    pub fn apply(&mut self, lines: Lines, out: &mut FeedOut) {
        for line in &lines.record {
            self.reader.line(line, out);
        }
        for line in &lines.companion {
            self.reader.companion_line(line, out);
        }
    }
}
