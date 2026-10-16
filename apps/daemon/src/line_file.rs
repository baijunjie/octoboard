//! A file read again and again as it grows, such as an agent's append-only record: which file it
//! is, how far it has been read and what it looked like then, so a later pass reads only what was
//! added, and starts over when the file was replaced, truncated or rewritten.

use std::io::{BufRead, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::SystemTime;

/// A file's length and modification time, which together say whether it changed.
pub type Signature = (u64, Option<SystemTime>);

#[derive(Default)]
pub struct LineFile {
    identity: Option<(u64, u64)>,
    offset: u64,
    signature: Option<Signature>,
    pending: Option<Signature>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Unchanged,
    /// Grown, or changed in a way that leaves what was read intact.
    Changed,
    /// Replaced, truncated or rewritten: what was read from it before no longer holds.
    Restarted,
}

impl LineFile {
    /// Compares the file with how it was left. A changed file stays unsettled, to be read again,
    /// until [`Self::read`] or [`Self::settle`] succeeds.
    pub fn check(&mut self, path: &Path) -> std::io::Result<Change> {
        let metadata = std::fs::metadata(path)?;
        let identity = (metadata.dev(), metadata.ino());
        let signature = (metadata.len(), metadata.modified().ok());
        let replaced = self.identity.is_some_and(|previous| previous != identity);
        if replaced {
            *self = Self::default();
        }
        self.identity = Some(identity);
        if self.signature == Some(signature) {
            return Ok(Change::Unchanged);
        }
        self.pending = Some(signature);
        // A file no longer than what was read, yet changed, was rewritten.
        let rewritten = metadata.len() <= self.offset;
        if rewritten {
            self.offset = 0;
        }
        Ok(if replaced || rewritten {
            Change::Restarted
        } else {
            Change::Changed
        })
    }

    /// Reads the complete lines after the last read one and settles on the file as checked.
    pub fn read(&mut self, path: &Path, visit: impl FnMut(&str)) -> std::io::Result<()> {
        self.offset = scan_lines(path, self.offset, visit)?;
        self.settle();
        Ok(())
    }

    /// Takes the file as checked as read, for a caller that read it whole.
    pub fn settle(&mut self) {
        self.signature = self.pending;
    }
}

/// Read complete lines with bounded memory even when a tool writes a very large body. A record
/// larger than 8 MiB is not a supported record and is skipped, never deserialized. The
/// offset still stops before an unfinished line.
fn scan_lines(path: &Path, offset: u64, mut visit: impl FnMut(&str)) -> std::io::Result<u64> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = std::io::BufReader::new(file);
    let mut complete = offset;
    let mut position = offset;
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(complete);
        }
        let end = buffer.iter().position(|byte| *byte == b'\n');
        let length = end.map_or(buffer.len(), |end| end + 1);
        if !oversized {
            if line.len() + length <= 8 * 1024 * 1024 {
                line.extend_from_slice(&buffer[..length]);
            } else {
                line.clear();
                oversized = true;
            }
        }
        position += length as u64;
        reader.consume(length);
        if end.is_some() {
            if !oversized {
                if let Ok(line) = std::str::from_utf8(&line) {
                    visit(line);
                }
            }
            complete = position;
            line.clear();
            oversized = false;
        }
    }
}
