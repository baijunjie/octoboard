//! The daemon's log file, `daemon.log` under `paths::logs_dir()`: a writer for the tracing
//! subscriber that rotates by size, so the log of a daemon that runs for months stays bounded.
//!
//! Nothing here ever reports a failure. The log is also how a failure is told, so a log that cannot
//! be written (a full disk, a directory that cannot be created, a file removed underneath it) is
//! dropped at once, and never panics. Writes are synchronous, under a mutex, as writes to stderr
//! are.
//!
//! The file belongs to the one daemon that holds the instance lock, so [`LogFile::open`] writes
//! nothing until [`LogFile::enable`] is called: a second daemon that loses the lock must not
//! append to, or rotate, the running daemon's log.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use tracing_subscriber::fmt::MakeWriter;

const FILE_NAME: &str = "daemon.log";

/// How large `daemon.log` may grow before it is rotated.
const MAX_BYTES: u64 = 5 * 1024 * 1024;

/// How many rotated files are kept next to the live one: `daemon.log.1` (the newest) to
/// `daemon.log.3`.
const ROTATED_FILES: u32 = 3;

/// A size-rotated log file, cloned for every event the subscriber writes.
#[derive(Clone)]
pub struct LogFile {
    state: Arc<Mutex<State>>,
}

struct State {
    /// Whether writes reach the file. They are dropped while it is `false`.
    enabled: bool,
    dir: PathBuf,
    max_bytes: u64,
    rotated_files: u32,
    /// The open live file and its size, or `None` before the first write and after a failure.
    file: Option<(File, u64)>,
}

impl LogFile {
    /// The daemon's log, in `paths::logs_dir()`, writing nothing until [`LogFile::enable`].
    pub fn open() -> Self {
        Self::new(crate::paths::logs_dir(), MAX_BYTES, ROTATED_FILES).disabled()
    }

    /// Starts writing to the file; the writes before it are not kept.
    pub fn enable(&self) {
        self.state().enabled = true;
    }

    fn disabled(self) -> Self {
        self.state().enabled = false;
        self
    }

    fn new(dir: PathBuf, max_bytes: u64, rotated_files: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                enabled: true,
                dir,
                max_bytes,
                rotated_files,
                file: None,
            })),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl State {
    fn path(&self, rotation: u32) -> PathBuf {
        match rotation {
            0 => self.dir.join(FILE_NAME),
            n => self.dir.join(format!("{FILE_NAME}.{n}")),
        }
    }

    fn open_live(&self) -> io::Result<(File, u64)> {
        fs::create_dir_all(&self.dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(0))?;
        let size = file.metadata()?.len();
        Ok((file, size))
    }

    /// Shifts every rotated file one place up, the oldest one dropped, and moves the live file to
    /// `.1`. Only the open handle is given up here; the next write opens a fresh live file.
    fn rotate(&mut self) {
        self.file = None;
        for rotation in (1..self.rotated_files).rev() {
            let _ = fs::rename(self.path(rotation), self.path(rotation + 1));
        }
        if self.rotated_files == 0 || fs::rename(self.path(0), self.path(1)).is_err() {
            // A live file that cannot be moved aside is removed, or it would grow without bound.
            let _ = fs::remove_file(self.path(0));
        }
    }

    fn append(&mut self, buf: &[u8]) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if let Some((_, size)) = &self.file {
            // An event is never split across two files, and an empty file takes whatever it is
            // given.
            if *size > 0 && *size + buf.len() as u64 > self.max_bytes {
                self.rotate();
            }
        }
        if self.file.is_none() {
            self.file = Some(self.open_live()?);
        }
        let (file, size) = self.file.as_mut().expect("opened above");
        match file.write_all(buf) {
            Ok(()) => {
                *size += buf.len() as u64;
                Ok(())
            }
            Err(err) => {
                // Opened again on the next write.
                self.file = None;
                Err(err)
            }
        }
    }
}

impl Write for LogFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = self.state().append(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogFile {
    type Writer = LogFile;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    /// A file of the `logs` directory under `dir`.
    fn read(dir: &ScratchDir, name: &str) -> Option<String> {
        fs::read_to_string(dir.join("logs").join(name)).ok()
    }

    #[test]
    fn a_file_over_the_threshold_is_rotated_and_the_oldest_dropped() {
        let dir = ScratchDir::new("log-rotation");
        let mut log = LogFile::new(dir.join("logs"), 10, 3);
        // Each line is 6 bytes, so a second one in the same file would pass 10.
        for line in ["aaaaa\n", "bbbbb\n", "ccccc\n", "ddddd\n", "eeeee\n"] {
            log.write_all(line.as_bytes()).unwrap();
        }
        assert_eq!(read(&dir, "daemon.log").as_deref(), Some("eeeee\n"));
        assert_eq!(read(&dir, "daemon.log.1").as_deref(), Some("ddddd\n"));
        assert_eq!(read(&dir, "daemon.log.2").as_deref(), Some("ccccc\n"));
        assert_eq!(read(&dir, "daemon.log.3").as_deref(), Some("bbbbb\n"));
        // "aaaaa" was the oldest, and there is no fourth file.
        assert_eq!(read(&dir, "daemon.log.4"), None);
    }

    #[test]
    fn a_file_up_to_the_threshold_is_not_rotated() {
        let dir = ScratchDir::new("log-threshold");
        let mut log = LogFile::new(dir.join("logs"), 10, 3);
        log.write_all(b"12345").unwrap();
        log.write_all(b"67890").unwrap();
        assert_eq!(read(&dir, "daemon.log").as_deref(), Some("1234567890"));
        assert_eq!(read(&dir, "daemon.log.1"), None);
    }

    #[test]
    fn nothing_is_written_until_the_log_is_enabled() {
        let dir = ScratchDir::new("log-enable");
        let mut log = LogFile::new(dir.join("logs"), 10, 3).disabled();
        log.write_all(b"before\n").unwrap();
        assert_eq!(read(&dir, "daemon.log"), None);
        log.enable();
        log.write_all(b"after\n").unwrap();
        assert_eq!(read(&dir, "daemon.log").as_deref(), Some("after\n"));
    }

    #[test]
    fn a_log_that_cannot_be_written_is_dropped_without_failing() {
        let dir = ScratchDir::new("log-failure");
        // A file where the directory should be.
        fs::write(dir.join("logs"), "").unwrap();
        let mut log = LogFile::new(dir.join("logs"), 10, 3);
        assert_eq!(log.write(b"lost\n").unwrap(), 5);
    }
}
