//! Taking the daemon out of a release archive: one named entry of a gzip tarball, copied to a file
//! of the caller's with a cap on its size, and nothing else of the archive written anywhere.

use std::io::{self, Read, Write};
use std::path::Path;

use flate2::read::GzDecoder;

/// The most of the daemon an archive may hold, far above a real build's size.
pub const DAEMON_LIMIT: u64 = 256 * 1024 * 1024;

/// Why the daemon could not be taken out of an archive.
#[derive(Debug, PartialEq, Eq)]
pub enum UnpackError {
    /// The archive is not what it should be: not a tarball, no such file in it, or one too large.
    Archive(String),
    /// Writing the file failed.
    Write(String),
}

/// The path of the daemon inside the archive of `version` for `arch`.
pub fn daemon_entry(version: &str, arch: &str) -> String {
    format!("octoboardd_{version}_linux_{arch}/octoboardd")
}

/// Copies the regular file `entry` of the gzip tarball at `archive` into `out`, which takes at
/// most `limit` bytes; the number of bytes copied. An error says, in English, that the archive has
/// no such file, holds more than `limit`, or is not a tarball. Blocking.
pub fn unpack_entry(
    archive: &Path,
    entry: &str,
    out: &mut impl Write,
    limit: u64,
) -> Result<u64, UnpackError> {
    let file = std::fs::File::open(archive).map_err(|err| {
        UnpackError::Archive(format!("the archive could not be read back: {err}"))
    })?;
    let mut tarball = tar::Archive::new(GzDecoder::new(io::BufReader::new(file)));
    let unreadable =
        |err: io::Error| UnpackError::Archive(format!("the archive is not readable: {err}"));
    for member in tarball.entries().map_err(unreadable)? {
        let member = member.map_err(unreadable)?;
        let named = member.path().map_err(unreadable)?;
        if named.as_ref() != Path::new(entry) || !member.header().entry_type().is_file() {
            continue;
        }
        let mut member = member.take(limit + 1);
        let mut copied = 0u64;
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            let read = member.read(&mut chunk).map_err(unreadable)?;
            if read == 0 {
                break;
            }
            copied += read as u64;
            if copied > limit {
                return Err(UnpackError::Archive(format!(
                    "the daemon in the archive is larger than {limit} bytes"
                )));
            }
            out.write_all(&chunk[..read])
                .map_err(|err| UnpackError::Write(err.to_string()))?;
        }
        return Ok(copied);
    }
    Err(UnpackError::Archive(format!(
        "the archive has no `{entry}`"
    )))
}
