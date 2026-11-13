//! The download, the checks and the replacement of an update, in the order that keeps the
//! installation whole at every step. Shared by the daemon (`task`) and `octoboardd update`.
//!
//! 1. The executable's own path is resolved and its directory tried for writing, before a byte is
//!    downloaded.
//! 2. The archive is downloaded to a file of its own under the data directory's `updates/`.
//! 3. Its SHA-256 and signature are checked (`verify`), against the release key and the name of
//!    this release's archive.
//! 4. Only then, with the path looked at once more (it must still be the running build, and no
//!    stop may have begun), is the daemon unpacked, straight into the file that is renamed over
//!    the executable (`replace`), and the replacement recorded (`record`).
//!
//! A failure before step 4 touches nothing of the installation; one in step 4 puts back what the
//! step changed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use minisign_verify::PublicKey;

use super::fetch::{FetchError, Fetcher};
use super::manifest::{archive_name, Release};
use super::record::{Files, Installed};
use super::replace::{self, ReplaceError};
use super::unpack::{daemon_entry, unpack_entry, UnpackError, DAEMON_LIMIT};
use super::verify::verify_archive;
use crate::platform::fs;
use crate::protocol::{daemon_update_code, Code};

/// The longest an archive may be, compressed.
const ARCHIVE_LIMIT: u64 = 256 * 1024 * 1024;

/// The longest the whole download may take.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How much of the archive has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Why an update was not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// The user may not replace the executable at this path. Nothing was downloaded.
    NotWritable(PathBuf),
    /// The archive could not be fetched or stored.
    Download(String),
    /// The archive is not the release it claims to be.
    Unverified(String),
    /// The new executable could not be put in place.
    Replace(String),
}

impl InstallError {
    /// The code the failure goes out under.
    pub fn code(&self) -> Code {
        match self {
            Self::Replace(_) => daemon_update_code::REPLACE_FAILED,
            // The request is refused for a directory that is not writable when it is made; one
            // that stops being writable afterwards fails the replacement, and is carried as that.
            Self::NotWritable(_) => daemon_update_code::REPLACE_FAILED,
            Self::Download(_) => daemon_update_code::DOWNLOAD_FAILED,
            Self::Unverified(_) => daemon_update_code::UNVERIFIED,
        }
    }

    /// What went wrong, in English.
    pub fn detail(&self) -> String {
        match self {
            Self::NotWritable(path) => {
                format!("`{}` cannot be replaced by this user", path.display())
            }
            Self::Download(detail) | Self::Unverified(detail) | Self::Replace(detail) => {
                detail.clone()
            }
        }
    }
}

/// What an installation did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The file the update is in: the executable's path with its links resolved.
    pub path: PathBuf,
    /// Whether this installation replaced it. False when the file at the path was no longer the
    /// running build when the update looked (an install script replaced it meanwhile), in which
    /// case nothing was downloaded or changed.
    pub replaced: bool,
}

/// The executable `launch` names, with its links resolved, and whether the user may replace it.
/// The error is the path that cannot be replaced. Blocking.
pub fn replaceable(launch: &Path) -> Result<PathBuf, InstallError> {
    let real = crate::hostfs::canonical(launch).map_err(|err| {
        InstallError::Replace(format!(
            "the executable `{}` could not be found: {err}",
            launch.display()
        ))
    })?;
    match real.parent() {
        Some(dir) if fs::directory_writable(dir) => Ok(real),
        _ => Err(InstallError::NotWritable(real)),
    }
}

/// What an installation reaches beyond the disk it works on: where the archive comes from, the key
/// its signature is checked against, the architecture it is for, and how it tells that a path is
/// still the build that runs, and whether the process is stopping, which ends the download and
/// keeps the executable from being replaced. A seam, so that a test makes no request and signs
/// with a key of its own.
pub struct Sources<'a> {
    pub fetcher: &'a dyn Fetcher,
    pub key: &'a PublicKey,
    pub arch: &'a str,
    pub is_running_image: fn(&Path) -> bool,
    pub stopping: &'a dyn Fn() -> bool,
}

/// Installs `release` over the executable `launch` names, which must still be the running build.
/// `progress` is told after each piece of the archive that arrives. Blocking.
pub fn install(
    sources: &Sources<'_>,
    release: &Release,
    launch: &Path,
    files: &Files,
    progress: &mut dyn FnMut(Progress),
) -> Result<Outcome, InstallError> {
    let Sources {
        arch,
        is_running_image,
        ..
    } = *sources;
    let real = replaceable(launch)?;
    if !is_running_image(&real) {
        return Ok(Outcome {
            path: real,
            replaced: false,
        });
    }
    let name = archive_name(&release.version, arch);
    let archive = download(sources, release, files, &name, progress)?;
    let outcome = put_in_place(sources, &archive, release, &real, &name);
    std::fs::remove_file(&archive).ok();
    if !outcome? {
        return Ok(Outcome {
            path: real,
            replaced: false,
        });
    }
    if let Err(err) = files.write_installed(&Installed {
        version: release.version.clone(),
        holder_floor: release.holder_floor.clone(),
        from_version: env!("CARGO_PKG_VERSION").to_owned(),
        path: real.to_string_lossy().into_owned(),
    }) {
        tracing::warn!(%err, "recording the update failed");
    }
    Ok(Outcome {
        path: real,
        replaced: true,
    })
}

/// Downloads the archive of `release` to `updates/<name>.part`, after clearing what an earlier
/// update left there; the file's path.
fn download(
    sources: &Sources<'_>,
    release: &Release,
    files: &Files,
    name: &str,
    progress: &mut dyn FnMut(Progress),
) -> Result<PathBuf, InstallError> {
    let failed =
        |what: &str, err: &dyn std::fmt::Display| InstallError::Download(format!("{what}: {err}"));
    let dir = files.updates_dir();
    fs::create_private_dir(&dir)
        .map_err(|err| failed("the download folder could not be made", &err))?;
    files.clear_updates();
    let part = dir.join(format!("{name}.part"));
    let mut out = fs::create_private(
        &part,
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )
    .map_err(|err| failed("the archive could not be stored", &err))?;
    let mut downloaded = 0u64;
    let fetched = sources.fetcher.get(
        &release.url,
        ARCHIVE_LIMIT,
        DOWNLOAD_TIMEOUT,
        &mut |chunk, total| {
            if (sources.stopping)() {
                return Err(std::io::Error::other("the daemon is stopping"));
            }
            std::io::Write::write_all(&mut out, chunk)?;
            downloaded += chunk.len() as u64;
            progress(Progress { downloaded, total });
            Ok(())
        },
    );
    drop(out);
    if let Err(err) = fetched {
        std::fs::remove_file(&part).ok();
        return Err(match err {
            FetchError::Status(status) => {
                InstallError::Download(format!("the server answered {status}"))
            }
            FetchError::Failed(detail) => InstallError::Download(detail),
        });
    }
    Ok(part)
}

/// Checks the archive at `archive` and replaces `real` with the daemon in it. False, with nothing
/// changed, where `real` is no longer the running build by the time of the replacement (something
/// else replaced it while the archive came in); an error where the process began to stop.
fn put_in_place(
    sources: &Sources<'_>,
    archive: &Path,
    release: &Release,
    real: &Path,
    name: &str,
) -> Result<bool, InstallError> {
    verify_archive(
        archive,
        &release.sha256,
        &release.signature,
        name,
        sources.key,
    )
    .map_err(InstallError::Unverified)?;
    // The last look before the file is touched: the download took time.
    if (sources.stopping)() {
        return Err(InstallError::Replace(
            "the daemon began to stop, so the update was not installed".to_owned(),
        ));
    }
    if !(sources.is_running_image)(real) {
        return Ok(false);
    }
    let entry = daemon_entry(&release.version, sources.arch);
    let mut unpacking = None;
    let replaced = replace::replace(real, |file| {
        match unpack_entry(archive, &entry, file, DAEMON_LIMIT) {
            Ok(_) => Ok(()),
            Err(err) => {
                let text = match &err {
                    UnpackError::Archive(text) | UnpackError::Write(text) => text.clone(),
                };
                unpacking = Some(err);
                Err(text)
            }
        }
    });
    match replaced {
        Ok(()) => Ok(true),
        Err(ReplaceError::NotWritable(path)) => Err(InstallError::NotWritable(path)),
        Err(ReplaceError::Filling(text)) => Err(match unpacking {
            Some(UnpackError::Archive(_)) => InstallError::Unverified(text),
            _ => InstallError::Replace(text),
        }),
        Err(ReplaceError::Failed(text)) => Err(InstallError::Replace(text)),
    }
}
