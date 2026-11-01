//! The bounded line reader every line-by-line pass over an agent's record goes through. A file
//! read again and again as it grows, such as an append-only record: [`LineFile`] keeps which file
//! it is, how far it has been read and what it looked like then, so a later pass reads only what
//! was added, and starts over when the file was replaced, truncated or rewritten. A one-off pass,
//! with no cursor ([`read_to`], [`read_from`], [`read_whole_until`]), reads the same way. Either way
//! a line past [`LINE_LIMIT`] is skipped, never held in memory. A file compressed with zstd
//! (Codex's `.jsonl.zst` rollouts, which it never appends to again) is read through its
//! decompression, with offsets counted in decompressed bytes. [`open`] is the one way to open an
//! agent's record for reading, since Codex may have compressed it.

use std::io::{BufRead, Read, Seek, SeekFrom};
use std::ops::ControlFlow;
use std::path::Path;
use std::time::SystemTime;

use crate::hostfs::identity::{self, Identity};

/// A file's length and modification time, which together say whether it changed.
pub type Signature = (u64, Option<SystemTime>);

/// The largest JSON file the daemon reads whole: an agent's small state files beside or above
/// its records (Grok's summary and usage ledger, Codex's catalogue cache), all far below it.
pub const JSON_FILE_LIMIT: u64 = 8 * 1024 * 1024;

/// The longest line, newline included, handed to a reader of these files. A record line longer
/// than this is not a supported record: it is skipped, never held whole in memory.
pub const LINE_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub struct LineFile {
    identity: Option<Identity>,
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
        let identity = identity::raw(&metadata);
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
    pub fn read(&mut self, path: &Path, mut visit: impl FnMut(&str)) -> std::io::Result<()> {
        self.read_with_position(path, |_, line| visit(line))
    }

    /// Reads complete lines with their starting byte offset (decompressed for a zstd file).
    pub fn read_with_position(
        &mut self,
        path: &Path,
        visit: impl FnMut(u64, &str),
    ) -> std::io::Result<()> {
        self.offset = scan_lines(path, self.offset, None, visit)?;
        self.settle();
        Ok(())
    }

    /// Takes the file as checked as read, for a caller that read it whole.
    pub fn settle(&mut self) {
        self.signature = self.pending;
    }
}

/// Reads the complete lines of the first `end` bytes of the file at `path` (decompressed ones for
/// a compressed file), once and without any cursor.
pub fn read_to(path: &Path, end: u64, mut visit: impl FnMut(&str)) -> std::io::Result<()> {
    scan_lines(path, 0, Some(end), |_, line| visit(line)).map(drop)
}

/// Reads the complete lines after byte `offset` of the file at `path`, once and without any cursor,
/// and answers the offset just past the last of them, where the next read starts: a line with no
/// newline yet is left for it. Each line reaches `visit` with its newline.
pub fn read_from(path: &Path, offset: u64, mut visit: impl FnMut(&str)) -> std::io::Result<u64> {
    scan_lines(path, offset, None, |_, line| visit(line))
}

/// Reads every line of the file at `path` from its start, the unfinished last one too, or only the
/// first `end` bytes when there is one, until `visit` breaks. For a one-off pass over a record
/// that is not read again from where it stopped. A read that fails part-way returns its error
/// after the lines before it were visited, saying whether it had read a line by then.
pub fn read_whole_until(
    path: &Path,
    end: Option<u64>,
    mut visit: impl FnMut(&str) -> ControlFlow<()>,
) -> Result<(), ReadFailure> {
    let mut read_a_line = false;
    scan(path, 0, end, true, &mut read_a_line, |_, line| visit(line))
        .map(drop)
        .map_err(|error| ReadFailure { error, read_a_line })
}

/// A one-off pass that failed.
#[derive(Debug)]
pub struct ReadFailure {
    pub error: std::io::Error,
    /// Whether a complete line of the record was read before the failure, whether or not it was
    /// visited: a line past [`LINE_LIMIT`] or not UTF-8 is read and skipped.
    pub read_a_line: bool,
}

/// Opens the file at `path` for reading from byte `offset`, decompressing it first when it is
/// zstd-compressed (a `.zst` name), in which case the offset counts decompressed bytes. For
/// anything that reads an agent's record directly, as Codex may have compressed it.
pub fn open(path: &Path, offset: u64) -> std::io::Result<Box<dyn BufRead>> {
    let mut file = std::fs::File::open(path)?;
    if path.extension().is_some_and(|ext| ext == "zst") {
        let mut decoded = std::io::BufReader::new(zstd::stream::read::Decoder::new(file)?);
        std::io::copy(&mut (&mut decoded).take(offset), &mut std::io::sink())?;
        Ok(Box::new(decoded))
    } else {
        file.seek(SeekFrom::Start(offset))?;
        Ok(Box::new(std::io::BufReader::new(file)))
    }
}

/// Reads complete lines with bounded memory even when a tool writes a very large body: a line past
/// [`LINE_LIMIT`] is skipped, never deserialized, and a line that is not UTF-8 too. The offset
/// still stops before an unfinished line.
fn scan_lines(
    path: &Path,
    offset: u64,
    end: Option<u64>,
    mut visit: impl FnMut(u64, &str),
) -> std::io::Result<u64> {
    scan(path, offset, end, false, &mut false, |position, line| {
        visit(position, line);
        ControlFlow::Continue(())
    })
}

/// [`scan_lines`] that also visits an unfinished last line when `unfinished`, and stops when
/// `visit` breaks, answering the offset past the last complete line visited. `read_a_line` is set
/// once a complete line has been read, visited or skipped, so a caller sees it after a failure too.
fn scan(
    path: &Path,
    offset: u64,
    end: Option<u64>,
    unfinished: bool,
    read_a_line: &mut bool,
    visit: impl FnMut(u64, &str) -> ControlFlow<()>,
) -> std::io::Result<u64> {
    let mut reader = open(path, offset)?;
    if let Some(end) = end {
        reader = Box::new(reader.take(end.saturating_sub(offset)));
    }
    scan_reader(reader, offset, unfinished, read_a_line, visit)
}

/// [`scan`] over `reader`, which is positioned at `offset`. A read the OS interrupted is retried,
/// as `BufRead::read_until` does: it is no failure of the file.
fn scan_reader(
    mut reader: impl BufRead,
    offset: u64,
    unfinished: bool,
    read_a_line: &mut bool,
    mut visit: impl FnMut(u64, &str) -> ControlFlow<()>,
) -> std::io::Result<u64> {
    let mut complete = offset;
    let mut position = offset;
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let buffer = match reader.fill_buf() {
            Ok(buffer) => buffer,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        if buffer.is_empty() {
            if unfinished && !oversized {
                if let Ok(line) = std::str::from_utf8(&line) {
                    if !line.is_empty() {
                        let _ = visit(complete, line);
                    }
                }
            }
            return Ok(complete);
        }
        let end = buffer.iter().position(|byte| *byte == b'\n');
        let length = end.map_or(buffer.len(), |end| end + 1);
        if !oversized {
            if line.len() + length <= LINE_LIMIT {
                line.extend_from_slice(&buffer[..length]);
            } else {
                line.clear();
                oversized = true;
            }
        }
        position += length as u64;
        reader.consume(length);
        if end.is_some() {
            *read_a_line = true;
            if !oversized {
                if let Ok(line) = std::str::from_utf8(&line) {
                    if visit(complete, line).is_break() {
                        return Ok(position);
                    }
                }
            }
            complete = position;
            line.clear();
            oversized = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchFile;

    fn lines_from(path: &Path, offset: u64) -> (Vec<String>, u64) {
        let mut lines = Vec::new();
        let read = scan_lines(path, offset, None, |_, line| lines.push(line.to_string())).unwrap();
        (lines, read)
    }

    /// A line past the limit is skipped whole, never held in memory beyond it, and the lines after
    /// it are still read; the offset moves past it like any complete line.
    #[test]
    fn a_line_past_the_limit_is_skipped_and_the_lines_after_it_are_read() {
        let path = ScratchFile::new("line-file-long", "record.jsonl");
        let long = "x".repeat(LINE_LIMIT);
        let content = format!("first\n{long}\nlast\n");
        std::fs::write(&path, &content).unwrap();
        let (lines, read) = lines_from(&path, 0);
        assert_eq!(lines, ["first\n", "last\n"]);
        assert_eq!(read, content.len() as u64);
    }

    /// A line within the limit, newline included, is read whole.
    #[test]
    fn a_line_at_the_limit_is_read() {
        let path = ScratchFile::new("line-file-at-limit", "record.jsonl");
        let line = format!("{}\n", "x".repeat(LINE_LIMIT - 1));
        std::fs::write(&path, &line).unwrap();
        let (lines, read) = lines_from(&path, 0);
        assert_eq!(lines, [line.as_str()]);
        assert_eq!(read, line.len() as u64);
    }

    /// A line past the limit that has no newline yet leaves the offset before it, so the next read
    /// starts at it again.
    #[test]
    fn an_unfinished_line_past_the_limit_leaves_the_offset_before_it() {
        let path = ScratchFile::new("line-file-long-unfinished", "record.jsonl");
        std::fs::write(&path, format!("first\n{}", "x".repeat(LINE_LIMIT + 1))).unwrap();
        let (lines, read) = lines_from(&path, 0);
        assert_eq!(lines, ["first\n"]);
        assert_eq!(read, "first\n".len() as u64);
    }

    /// The unfinished last line is visited by a one-off pass, and the pass stops when asked.
    #[test]
    fn a_pass_reads_the_unfinished_line_and_stops_when_asked() {
        let path = ScratchFile::new("line-file-until", "record.jsonl");
        std::fs::write(&path, "a\nb\nc").unwrap();
        let mut lines = Vec::new();
        read_whole_until(&path, None, |line| {
            lines.push(line.to_string());
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(lines, ["a\n", "b\n", "c"]);
        let mut lines = Vec::new();
        read_whole_until(&path, None, |line| {
            lines.push(line.to_string());
            ControlFlow::Break(())
        })
        .unwrap();
        assert_eq!(lines, ["a\n"]);
        let mut lines = Vec::new();
        read_whole_until(&path, Some(3), |line| {
            lines.push(line.to_string());
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(lines, ["a\n", "b"]);
    }

    /// A reader that hands out `chunks` one per `fill_buf`, `None` standing for a read the OS
    /// interrupted.
    struct Interrupting {
        chunks: std::collections::VecDeque<Option<&'static [u8]>>,
        consumed: usize,
    }

    impl Read for Interrupting {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            unreachable!("the scan reads through `fill_buf`")
        }
    }

    impl BufRead for Interrupting {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            match self.chunks.front() {
                None => Ok(&[]),
                Some(None) => {
                    self.chunks.pop_front();
                    Err(std::io::ErrorKind::Interrupted.into())
                }
                Some(Some(chunk)) => Ok(&chunk[self.consumed..]),
            }
        }

        fn consume(&mut self, amount: usize) {
            self.consumed += amount;
            if let Some(Some(chunk)) = self.chunks.front() {
                if self.consumed == chunk.len() {
                    self.chunks.pop_front();
                    self.consumed = 0;
                }
            }
        }
    }

    /// A read the OS interrupts is retried, mid-line too: the lines are read whole and the offset
    /// is past them, not a failure.
    #[test]
    fn an_interrupted_read_is_retried() {
        let reader = Interrupting {
            chunks: [
                None,
                Some(&b"first\nsec"[..]),
                None,
                Some(&b"ond\n"[..]),
                None,
            ]
            .into(),
            consumed: 0,
        };
        let mut lines = Vec::new();
        let mut read_a_line = false;
        let read = scan_reader(reader, 10, false, &mut read_a_line, |position, line| {
            lines.push((position, line.to_string()));
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(
            lines,
            [(10, "first\n".to_string()), (16, "second\n".to_string())]
        );
        assert_eq!(read, 10 + "first\nsecond\n".len() as u64);
        assert!(read_a_line);
    }
}
