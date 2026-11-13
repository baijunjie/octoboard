//! The self-update, against a stand-in for the host that serves the release: a signed archive made
//! with a test key (`testdata/self_update/`), never the network and never the release key.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use minisign_verify::PublicKey;
use sha2::{Digest, Sha256};

use super::check::{check, Checked};
use super::fetch::{FetchError, Fetcher, Sink};
use super::install::{install, replaceable, InstallError, Sources};
use super::manifest::{self, archive_name, manifest_url, parse, platform_key, Release};
use super::record::Files;
use super::replace::{replace, swap_with_previous, ReplaceError};
use super::task::{due, scheduled_check, Throttle};
use super::unpack::{daemon_entry, unpack_entry, UnpackError};
use super::verify::{decode_signature, verify_archive, PUBLIC_KEY};
use super::{DaemonUpdates, Setup};
use crate::protocol::{
    daemon_update_code, error_code, DaemonUpdateChannel, DaemonUpdateCheckOutcome,
    DaemonUpdateInstallState, DaemonUpdateManualReason, DaemonUpdatePhase,
};
use crate::state::AppState;
use crate::test_support::{app_state, code_of, param_of, ScratchDir, PATIENCE};

const ARCH: &str = "x86_64";
const VERSION: &str = "9.9.9";
const ARCHIVE: &[u8] =
    include_bytes!("../../testdata/self_update/octoboardd_9.9.9_linux_x86_64.tar.gz");
const SIGNATURE: &str =
    include_str!("../../testdata/self_update/octoboardd_9.9.9_linux_x86_64.tar.gz.sig");
const TEST_KEY: &str = include_str!("../../testdata/self_update/test-release-key.txt");
/// What `testdata/self_update/octoboardd_9.9.9_linux_x86_64.tar.gz` holds as its daemon.
const NEW_DAEMON: &[u8] = b"#!/bin/sh\necho new daemon\n";
const OLD_DAEMON: &[u8] = b"the daemon that runs";
const ARCHIVE_URL: &str = "https://downloads.example/octoboardd_9.9.9_linux_x86_64.tar.gz";

fn test_key() -> PublicKey {
    PublicKey::from_base64(TEST_KEY.trim()).expect("the test key")
}

fn sha256_of(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A manifest for the platform this build runs on, naming the fixture archive.
fn manifest_text(version: &str, floor: &str, sha256: &str) -> String {
    serde_json::json!({
        "version": version,
        "notes": "What is new",
        "pub_date": "2026-11-01T00:00:00Z",
        "holder_floor": floor,
        "platforms": {
            platform_key(): {
                "url": ARCHIVE_URL,
                "signature": SIGNATURE.trim(),
                "sha256": sha256,
            },
        },
    })
    .to_string()
}

/// What a request to one address gets.
#[derive(Clone)]
enum Answer {
    Body(Vec<u8>),
    Error(FetchError),
}

/// The host that serves the release: answers by address, in pieces of a few hundred bytes, and
/// keeps count of what was asked for.
#[derive(Default)]
struct Host {
    answers: Mutex<Vec<(String, Answer)>>,
    asked: Mutex<Vec<String>>,
}

impl Host {
    fn serving(manifest: &str) -> Arc<Self> {
        let host = Arc::new(Self::default());
        host.answer(
            &manifest_url(DaemonUpdateChannel::Stable),
            Answer::Body(manifest.into()),
        );
        host.answer(
            &manifest_url(DaemonUpdateChannel::Beta),
            Answer::Body(manifest.into()),
        );
        host.answer(ARCHIVE_URL, Answer::Body(ARCHIVE.to_vec()));
        host
    }

    fn answer(&self, url: &str, answer: Answer) {
        let mut answers = self.answers.lock().unwrap();
        answers.retain(|(known, _)| known != url);
        answers.push((url.to_owned(), answer));
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Fetcher for Host {
    fn get(
        &self,
        url: &str,
        max_bytes: u64,
        _timeout: Duration,
        sink: &mut Sink<'_>,
    ) -> Result<(), FetchError> {
        self.asked.lock().unwrap().push(url.to_owned());
        let answer = self
            .answers
            .lock()
            .unwrap()
            .iter()
            .find(|(known, _)| known == url)
            .map(|(_, answer)| answer.clone())
            .unwrap_or(Answer::Error(FetchError::Status(404)));
        match answer {
            Answer::Error(err) => Err(err),
            Answer::Body(body) => {
                if body.len() as u64 > max_bytes {
                    return Err(FetchError::Failed("too long".to_owned()));
                }
                for piece in body.chunks(100) {
                    sink(piece, Some(body.len() as u64))
                        .map_err(|err| FetchError::Failed(err.to_string()))?;
                }
                Ok(())
            }
        }
    }
}

fn is_running(_: &Path) -> bool {
    true
}

/// Whether a path is still the build that runs: it holds the old daemon, as a path does until an
/// update replaces it.
fn is_running_old(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| bytes == OLD_DAEMON)
}

fn is_not_running(_: &Path) -> bool {
    false
}

/// The data directory and the installed executable of a test, the executable holding
/// [`OLD_DAEMON`].
struct Installation {
    _dir: ScratchDir,
    data: PathBuf,
    exe: PathBuf,
    files: Files,
}

impl Installation {
    fn new(label: &str) -> Self {
        let dir = ScratchDir::new(label);
        let data = dir.join("data");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("octoboardd");
        std::fs::write(&exe, OLD_DAEMON).unwrap();
        #[cfg(unix)]
        crate::test_support::make_executable(&exe);
        Self {
            files: Files::new(&data),
            _dir: dir,
            data,
            exe,
        }
    }

    fn sources<'a>(host: &'a Host, key: &'a PublicKey, running: fn(&Path) -> bool) -> Sources<'a> {
        Sources {
            fetcher: host,
            key,
            arch: ARCH,
            is_running_image: running,
            stopping: &|| false,
        }
    }

    fn release(&self) -> Release {
        parse(
            &manifest_text(VERSION, "0.1.0", &sha256_of(ARCHIVE)),
            &platform_key(),
            DaemonUpdateChannel::Stable,
        )
        .unwrap()
    }
}

/// Whether the process may write where a mode refuses it: as root it may, and a test of that
/// refusal has nothing to show.
#[cfg(unix)]
fn ignores_modes() -> bool {
    // SAFETY: `geteuid` has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

mod key {
    use super::*;

    #[test]
    fn the_built_in_key_is_the_one_the_desktop_updater_trusts() {
        use base64::Engine;
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../../desktop/src-tauri/tauri.conf.json"))
                .unwrap();
        let pubkey = config["plugins"]["updater"]["pubkey"].as_str().unwrap();
        let file = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(pubkey)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(file.lines().nth(1), Some(PUBLIC_KEY));
        super::super::verify::release_key();
    }
}

mod verifying {
    use super::*;

    fn archive_in(dir: &Path, bytes: &[u8]) -> PathBuf {
        let path = dir.join("archive.tar.gz");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn name() -> String {
        archive_name(VERSION, ARCH)
    }

    #[test]
    fn a_signed_archive_of_this_release_is_accepted() {
        let dir = ScratchDir::new("verify-ok");
        let archive = archive_in(&dir, ARCHIVE);
        verify_archive(
            &archive,
            &sha256_of(ARCHIVE),
            SIGNATURE,
            &name(),
            &test_key(),
        )
        .unwrap();
    }

    #[test]
    fn the_signature_is_read_as_the_sig_file_the_signer_writes_and_as_the_minisign_file_in_it() {
        let wrapped = decode_signature(SIGNATURE).unwrap();
        use base64::Engine;
        let plain = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(SIGNATURE.trim())
                .unwrap(),
        )
        .unwrap();
        let unwrapped = decode_signature(&plain).unwrap();
        assert_eq!(wrapped.trusted_comment(), unwrapped.trusted_comment());
        assert!(decode_signature("not a signature").is_err());
        assert!(decode_signature("").is_err());
    }

    #[test]
    fn an_archive_with_another_sha256_than_the_manifests_is_refused() {
        let dir = ScratchDir::new("verify-sha");
        let archive = archive_in(&dir, ARCHIVE);
        let err = verify_archive(
            &archive,
            &sha256_of(b"other"),
            SIGNATURE,
            &name(),
            &test_key(),
        )
        .unwrap_err();
        assert!(err.contains("SHA-256"), "{err}");
    }

    #[test]
    fn an_archive_changed_after_it_was_signed_is_refused_even_with_its_own_sha256() {
        let dir = ScratchDir::new("verify-tampered");
        let mut changed = ARCHIVE.to_vec();
        *changed.last_mut().unwrap() ^= 1;
        let archive = archive_in(&dir, &changed);
        let err = verify_archive(
            &archive,
            &sha256_of(&changed),
            SIGNATURE,
            &name(),
            &test_key(),
        )
        .unwrap_err();
        assert!(err.contains("signature"), "{err}");
    }

    /// An older release, which the key signed too, must not pass for the one the manifest names.
    #[test]
    fn a_signature_of_another_archive_is_refused() {
        let dir = ScratchDir::new("verify-downgrade");
        let archive = archive_in(&dir, ARCHIVE);
        let err = verify_archive(
            &archive,
            &sha256_of(ARCHIVE),
            SIGNATURE,
            &archive_name("9.9.10", ARCH),
            &test_key(),
        )
        .unwrap_err();
        assert!(err.contains(&archive_name(VERSION, ARCH)), "{err}");
        let err = verify_archive(
            &archive,
            &sha256_of(ARCHIVE),
            SIGNATURE,
            &archive_name(VERSION, "aarch64"),
            &test_key(),
        )
        .unwrap_err();
        assert!(err.contains("not of"), "{err}");
    }

    #[test]
    fn a_signature_of_another_key_is_refused() {
        let dir = ScratchDir::new("verify-key");
        let archive = archive_in(&dir, ARCHIVE);
        let err = verify_archive(
            &archive,
            &sha256_of(ARCHIVE),
            SIGNATURE,
            &name(),
            &super::super::verify::release_key(),
        )
        .unwrap_err();
        assert!(err.contains("release key"), "{err}");
    }
}

mod manifests {
    use super::*;

    #[test]
    fn the_address_follows_the_channel_and_the_key_the_architecture() {
        assert!(manifest_url(DaemonUpdateChannel::Stable)
            .ends_with("/releases/download/updater/stable-linux.json"));
        assert!(manifest_url(DaemonUpdateChannel::Beta)
            .ends_with("/releases/download/updater/beta-linux.json"));
        assert!(manifest_url(DaemonUpdateChannel::Stable).starts_with("https://"));
        assert_eq!(platform_key(), format!("linux-{}", std::env::consts::ARCH));
        assert!(manifest::install_command().contains("/install.sh | sh"));
    }

    #[test]
    fn a_manifest_names_the_release_for_this_machine() {
        let release = parse(
            &manifest_text(VERSION, "0.1.0", &sha256_of(ARCHIVE)),
            &platform_key(),
            DaemonUpdateChannel::Beta,
        )
        .unwrap();
        assert_eq!(release.version, VERSION);
        assert_eq!(release.notes.as_deref(), Some("What is new"));
        assert_eq!(release.holder_floor.as_deref(), Some("0.1.0"));
        assert_eq!(release.url, ARCHIVE_URL);
        assert_eq!(release.sha256, sha256_of(ARCHIVE));
        assert_eq!(release.channel, DaemonUpdateChannel::Beta);
    }

    #[test]
    fn a_manifest_that_cannot_be_trusted_to_name_a_release_is_refused() {
        let good = manifest_text(VERSION, "0.1.0", &sha256_of(ARCHIVE));
        let parsed = |text: &str| parse(text, &platform_key(), DaemonUpdateChannel::Stable);
        assert!(parsed("{").unwrap_err().contains("not readable"));
        assert!(parse(&good, "linux-riscv", DaemonUpdateChannel::Stable)
            .unwrap_err()
            .contains("no release for linux-riscv"));
        assert!(parsed(&good.replace(VERSION, "nine"))
            .unwrap_err()
            .contains("not a version"));
        assert!(parsed(&good.replace("https://", "http://"))
            .unwrap_err()
            .contains("not https"));
        assert!(parsed(&manifest_text(VERSION, "0.1.0", "abc"))
            .unwrap_err()
            .contains("SHA-256"));
    }

    #[test]
    fn a_holder_floor_that_is_no_version_is_left_out_not_fatal() {
        let release = parse(
            &manifest_text(VERSION, "soon", &sha256_of(ARCHIVE)),
            &platform_key(),
            DaemonUpdateChannel::Stable,
        )
        .unwrap();
        assert_eq!(release.holder_floor, None);
    }

    #[test]
    fn only_a_strictly_greater_version_is_newer() {
        let release = Installation::new("manifest-newer").release();
        assert!(release.is_newer_than("9.9.8"));
        assert!(release.is_newer_than("9.9.9-beta.1"));
        assert!(!release.is_newer_than("9.9.9"));
        assert!(!release.is_newer_than("10.0.0"));
        assert!(!release.is_newer_than("unknown"));
    }
}

mod checking {
    use super::*;

    fn found(host: &Host, running: &str) -> Result<Checked, String> {
        check(host, DaemonUpdateChannel::Stable, running)
    }

    #[test]
    fn a_newer_release_is_available_and_an_equal_or_older_one_is_not() {
        let host = Host::serving(&manifest_text(VERSION, "0.1.0", &sha256_of(ARCHIVE)));
        assert!(
            matches!(found(&host, "0.1.0"), Ok(Checked::Available(release)) if release.version == VERSION)
        );
        assert_eq!(found(&host, VERSION), Ok(Checked::UpToDate));
        assert_eq!(found(&host, "10.0.0"), Ok(Checked::UpToDate));
        assert_eq!(host.asked().len(), 3);
        assert!(host
            .asked()
            .iter()
            .all(|url| url == &manifest_url(DaemonUpdateChannel::Stable)));
    }

    #[test]
    fn the_beta_channel_reads_its_own_manifest() {
        let host = Host::serving(&manifest_text(VERSION, "0.1.0", &sha256_of(ARCHIVE)));
        check(&*host, DaemonUpdateChannel::Beta, "0.1.0").unwrap();
        assert_eq!(host.asked(), [manifest_url(DaemonUpdateChannel::Beta)]);
    }

    /// Only "not found" is "nothing newer": a channel that has had no release has no manifest.
    #[test]
    fn only_a_missing_manifest_means_nothing_newer_and_every_other_failure_is_a_failed_check() {
        let host = Host::default();
        assert_eq!(found(&host, "0.1.0"), Ok(Checked::UpToDate));
        let url = manifest_url(DaemonUpdateChannel::Stable);
        host.answer(&url, Answer::Error(FetchError::Status(500)));
        assert!(found(&host, "0.1.0").unwrap_err().contains("500"));
        host.answer(&url, Answer::Error(FetchError::Status(403)));
        assert!(found(&host, "0.1.0").is_err());
        host.answer(
            &url,
            Answer::Error(FetchError::Failed("connection refused".into())),
        );
        assert!(found(&host, "0.1.0")
            .unwrap_err()
            .contains("connection refused"));
        host.answer(&url, Answer::Body(b"<html>".to_vec()));
        assert!(found(&host, "0.1.0").unwrap_err().contains("not readable"));
        host.answer(&url, Answer::Body(vec![0xff, 0xfe]));
        assert!(found(&host, "0.1.0").is_err());
    }
}

mod unpacking {
    use super::*;

    fn archive_in(dir: &Path) -> PathBuf {
        let path = dir.join("archive.tar.gz");
        std::fs::write(&path, ARCHIVE).unwrap();
        path
    }

    #[test]
    fn the_daemon_is_taken_out_and_nothing_else() {
        let dir = ScratchDir::new("unpack-ok");
        let mut out = Vec::new();
        let copied = unpack_entry(
            &archive_in(&dir),
            &daemon_entry(VERSION, ARCH),
            &mut out,
            1024,
        )
        .unwrap();
        assert_eq!(out, NEW_DAEMON);
        assert_eq!(copied, NEW_DAEMON.len() as u64);
    }

    #[test]
    fn an_entry_the_archive_does_not_hold_is_refused() {
        let dir = ScratchDir::new("unpack-missing");
        let err = unpack_entry(
            &archive_in(&dir),
            &daemon_entry("1.0.0", ARCH),
            &mut Vec::new(),
            1024,
        )
        .unwrap_err();
        assert!(matches!(err, UnpackError::Archive(text) if text.contains("no `octoboardd_1.0.0")));
        // A directory of that name, and a file at another place in the tree, are not it either.
        let err = unpack_entry(
            &archive_in(&dir),
            &format!("octoboardd_{VERSION}_linux_{ARCH}"),
            &mut Vec::new(),
            1024,
        )
        .unwrap_err();
        assert!(matches!(err, UnpackError::Archive(_)));
        let err = unpack_entry(&archive_in(&dir), "octoboardd", &mut Vec::new(), 1024).unwrap_err();
        assert!(matches!(err, UnpackError::Archive(_)));
    }

    #[test]
    fn a_daemon_larger_than_the_cap_is_refused_and_not_written_past_it() {
        let dir = ScratchDir::new("unpack-cap");
        let mut out = Vec::new();
        let err = unpack_entry(
            &archive_in(&dir),
            &daemon_entry(VERSION, ARCH),
            &mut out,
            NEW_DAEMON.len() as u64 - 1,
        )
        .unwrap_err();
        assert!(matches!(err, UnpackError::Archive(text) if text.contains("larger than")));
        assert!(out.len() < NEW_DAEMON.len());
        // Exactly at the cap is taken.
        unpack_entry(
            &archive_in(&dir),
            &daemon_entry(VERSION, ARCH),
            &mut Vec::new(),
            NEW_DAEMON.len() as u64,
        )
        .unwrap();
    }

    #[test]
    fn what_is_not_an_archive_is_refused() {
        let dir = ScratchDir::new("unpack-garbage");
        let path = dir.join("archive.tar.gz");
        std::fs::write(&path, b"not a tarball at all, not even gzip").unwrap();
        let err = unpack_entry(&path, "x", &mut Vec::new(), 1024).unwrap_err();
        assert!(matches!(err, UnpackError::Archive(_)));
    }

    #[test]
    fn a_failing_write_is_told_from_a_failing_archive() {
        struct Full;
        impl std::io::Write for Full {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("no space left"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let dir = ScratchDir::new("unpack-full");
        let err = unpack_entry(
            &archive_in(&dir),
            &daemon_entry(VERSION, ARCH),
            &mut Full,
            1024,
        )
        .unwrap_err();
        assert_eq!(err, UnpackError::Write("no space left".to_owned()));
    }
}

#[cfg(unix)]
mod replacing {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_new_file_takes_the_path_with_the_old_mode_and_the_old_one_is_kept() {
        let installation = Installation::new("replace-ok");
        std::fs::set_permissions(&installation.exe, std::fs::Permissions::from_mode(0o750))
            .unwrap();
        replace(&installation.exe, |file| {
            std::io::Write::write_all(file, b"new").map_err(|err| err.to_string())
        })
        .unwrap();
        assert_eq!(std::fs::read(&installation.exe).unwrap(), b"new");
        assert_eq!(mode_of(&installation.exe), 0o750);
        let previous = installation.exe.with_file_name("octoboardd.previous");
        assert_eq!(std::fs::read(&previous).unwrap(), OLD_DAEMON);
        // Nothing of the staging is left beside them.
        assert_eq!(
            names_in(installation.exe.parent().unwrap()),
            ["octoboardd", "octoboardd.previous"]
        );
    }

    #[test]
    fn a_process_running_the_old_file_keeps_it() {
        use std::io::Read;
        let installation = Installation::new("replace-open");
        let mut held = std::fs::File::open(&installation.exe).unwrap();
        replace(&installation.exe, |file| {
            std::io::Write::write_all(file, b"new").map_err(|err| err.to_string())
        })
        .unwrap();
        let mut content = Vec::new();
        held.read_to_end(&mut content).unwrap();
        assert_eq!(content, OLD_DAEMON);
    }

    #[test]
    fn a_file_that_cannot_be_filled_leaves_the_installation_as_it_was() {
        let installation = Installation::new("replace-fill");
        let err = replace(&installation.exe, |_| Err("cut short".to_owned())).unwrap_err();
        assert_eq!(err, ReplaceError::Filling("cut short".to_owned()));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert_eq!(names_in(installation.exe.parent().unwrap()), ["octoboardd"]);
    }

    #[test]
    fn a_directory_that_takes_no_new_file_is_refused_before_anything_is_written() {
        if ignores_modes() {
            return;
        }
        let installation = Installation::new("replace-readonly");
        let dir = installation.exe.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let err = replace(&installation.exe, |_| panic!("nothing to write")).unwrap_err();
        let replaceable = replaceable(&installation.exe);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(err, ReplaceError::NotWritable(path) if path == installation.exe));
        assert!(matches!(replaceable, Err(InstallError::NotWritable(_))));
    }

    #[test]
    fn a_rollback_puts_the_previous_file_back_and_keeps_what_was_there() {
        let installation = Installation::new("replace-rollback");
        replace(&installation.exe, |file| {
            std::io::Write::write_all(file, b"new").map_err(|err| err.to_string())
        })
        .unwrap();
        swap_with_previous(&installation.exe).unwrap();
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        let previous = installation.exe.with_file_name("octoboardd.previous");
        assert_eq!(std::fs::read(&previous).unwrap(), b"new");
        // And back again.
        swap_with_previous(&installation.exe).unwrap();
        assert_eq!(std::fs::read(&installation.exe).unwrap(), b"new");
        assert_eq!(std::fs::read(&previous).unwrap(), OLD_DAEMON);
        assert_eq!(
            names_in(installation.exe.parent().unwrap()),
            ["octoboardd", "octoboardd.previous"]
        );
    }

    #[test]
    fn there_is_nothing_to_roll_back_to_before_an_update() {
        let installation = Installation::new("replace-no-previous");
        let err = swap_with_previous(&installation.exe).unwrap_err();
        assert!(matches!(err, ReplaceError::Failed(text) if text.contains("octoboardd.previous")));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
    }

    #[test]
    fn the_executable_of_a_link_is_replaced_where_the_link_leads() {
        let installation = Installation::new("replace-link");
        let dir = installation
            .exe
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let link = dir.join("octoboardd-link");
        crate::test_support::symlink(&installation.exe, &link).unwrap();
        let real = replaceable(&link).unwrap();
        assert_eq!(real, crate::hostfs::canonical(&installation.exe).unwrap());
    }
}

mod installing {
    use super::*;

    #[test]
    fn a_verified_archive_replaces_the_executable_and_is_recorded() {
        let installation = Installation::new("install-ok");
        let host = Host::serving("");
        let key = test_key();
        let mut progress = Vec::new();
        let outcome = install(
            &Installation::sources(&host, &key, is_running),
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |reported| progress.push(reported),
        )
        .unwrap();
        assert!(outcome.replaced);
        assert_eq!(std::fs::read(&installation.exe).unwrap(), NEW_DAEMON);
        assert_eq!(
            std::fs::read(installation.exe.with_file_name("octoboardd.previous")).unwrap(),
            OLD_DAEMON
        );
        assert_eq!(host.asked(), [ARCHIVE_URL]);

        // The archive is not kept, and its progress was reported up to the whole of it.
        assert_eq!(
            std::fs::read_dir(installation.files.updates_dir())
                .unwrap()
                .count(),
            0
        );
        let last = progress.last().unwrap();
        assert_eq!(last.downloaded, ARCHIVE.len() as u64);
        assert_eq!(last.total, Some(ARCHIVE.len() as u64));
        assert!(progress
            .windows(2)
            .all(|pair| pair[0].downloaded < pair[1].downloaded));

        let record = installation.files.read_installed().unwrap();
        assert_eq!(record.version, VERSION);
        assert_eq!(record.holder_floor.as_deref(), Some("0.1.0"));
        assert_eq!(record.from_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            PathBuf::from(record.path),
            crate::hostfs::canonical(&installation.exe).unwrap()
        );
    }

    #[test]
    fn an_archive_that_does_not_verify_changes_nothing() {
        for (label, change) in [
            (
                "sha",
                Box::new(|release: &mut Release| release.sha256 = sha256_of(b"x"))
                    as Box<dyn Fn(&mut Release)>,
            ),
            (
                "signature",
                Box::new(|release: &mut Release| release.signature = "bm9wZQ==".into()),
            ),
        ] {
            let installation = Installation::new(&format!("install-unverified-{label}"));
            let host = Host::serving("");
            let key = test_key();
            let mut release = installation.release();
            change(&mut release);
            let err = install(
                &Installation::sources(&host, &key, is_running),
                &release,
                &installation.exe,
                &installation.files,
                &mut |_| {},
            )
            .unwrap_err();
            assert!(
                matches!(err, InstallError::Unverified(_)),
                "{label}: {err:?}"
            );
            assert_eq!(err.code(), daemon_update_code::UNVERIFIED);
            assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
            assert!(!installation
                .exe
                .with_file_name("octoboardd.previous")
                .exists());
            assert!(installation.files.read_installed().is_none());
            assert_eq!(
                std::fs::read_dir(installation.files.updates_dir())
                    .unwrap()
                    .count(),
                0
            );
        }
    }

    #[test]
    fn the_wrong_key_refuses_a_release_signed_by_another() {
        let installation = Installation::new("install-wrong-key");
        let host = Host::serving("");
        let key = super::super::verify::release_key();
        let err = install(
            &Installation::sources(&host, &key, is_running),
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::Unverified(_)));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
    }

    #[test]
    fn a_download_that_fails_is_a_download_failure_and_leaves_nothing_behind() {
        for answer in [
            Answer::Error(FetchError::Status(503)),
            Answer::Error(FetchError::Failed("connection reset".into())),
        ] {
            let installation = Installation::new("install-download");
            let host = Host::serving("");
            host.answer(ARCHIVE_URL, answer);
            let key = test_key();
            let err = install(
                &Installation::sources(&host, &key, is_running),
                &installation.release(),
                &installation.exe,
                &installation.files,
                &mut |_| {},
            )
            .unwrap_err();
            assert!(matches!(err, InstallError::Download(_)), "{err:?}");
            assert_eq!(err.code(), daemon_update_code::DOWNLOAD_FAILED);
            assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
            assert_eq!(
                std::fs::read_dir(installation.files.updates_dir())
                    .unwrap()
                    .count(),
                0
            );
        }
    }

    /// Something else replaced the file (the install script, say): it is not overwritten, and not
    /// downloaded for either.
    #[test]
    fn a_path_that_is_no_longer_the_running_build_is_left_alone() {
        let installation = Installation::new("install-replaced-already");
        let host = Host::default();
        let key = test_key();
        let outcome = install(
            &Installation::sources(&host, &key, is_not_running),
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap();
        assert!(!outcome.replaced);
        assert!(host.asked().is_empty());
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert!(installation.files.read_installed().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_the_user_may_not_write_is_refused_before_the_download() {
        use std::os::unix::fs::PermissionsExt;
        if ignores_modes() {
            return;
        }
        let installation = Installation::new("install-not-writable");
        let dir = installation.exe.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let host = Host::default();
        let key = test_key();
        let err = install(
            &Installation::sources(&host, &key, is_running),
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap_err();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(err, InstallError::NotWritable(path) if path == installation.exe));
        assert!(host.asked().is_empty());
    }
}

mod records {
    use super::*;

    #[test]
    fn a_daemon_that_starts_newer_than_the_one_before_it_says_what_it_replaced() {
        let installation = Installation::new("record-updated-from");
        let files = &installation.files;
        assert_eq!(files.started_as("0.1.0"), None);
        assert_eq!(files.read_daemon_version().as_deref(), Some("0.1.0"));
        assert_eq!(
            files.started_as("0.1.0"),
            None,
            "the same version replaced nothing"
        );
        assert_eq!(files.started_as("0.2.0").as_deref(), Some("0.1.0"));
        assert_eq!(
            files.started_as("0.1.5"),
            None,
            "a rollback replaced nothing"
        );
        assert_eq!(files.read_daemon_version().as_deref(), Some("0.1.5"));
    }

    #[test]
    fn a_version_file_that_is_no_version_is_not_one() {
        let installation = Installation::new("record-bad-version");
        std::fs::write(installation.data.join("daemon.version"), "banana\n").unwrap();
        assert_eq!(installation.files.read_daemon_version(), None);
        assert_eq!(installation.files.started_as("0.2.0"), None);
        assert_eq!(
            installation.files.read_daemon_version().as_deref(),
            Some("0.2.0")
        );
    }

    #[test]
    fn what_was_installed_is_read_back_and_forgotten() {
        let installation = Installation::new("record-installed");
        assert!(installation.files.read_installed().is_none());
        installation.files.forget_installed().unwrap();
        let record = super::super::record::Installed {
            version: "0.2.0".into(),
            holder_floor: None,
            from_version: "0.1.0".into(),
            path: "/opt/octoboardd".into(),
        };
        installation.files.write_installed(&record).unwrap();
        assert_eq!(installation.files.read_installed(), Some(record));
        installation.files.forget_installed().unwrap();
        assert!(installation.files.read_installed().is_none());
        std::fs::write(installation.data.join("update-installed.json"), "{").unwrap();
        assert!(installation.files.read_installed().is_none());
    }
}

mod schedule {
    use super::*;

    const HOUR: i64 = 60 * 60 * 1000;

    #[test]
    fn a_check_is_due_at_the_start_every_six_hours_and_when_the_clock_went_back() {
        assert!(due(1_000, None));
        assert!(!due(1_000 + 5 * HOUR, Some(1_000)));
        assert!(due(1_000 + 6 * HOUR, Some(1_000)));
        assert!(due(500, Some(1_000)));
    }

    #[test]
    fn the_progress_is_announced_every_256_kib_and_at_most_four_times_a_second() {
        let mut throttle = Throttle::default();
        let start = std::time::Instant::now();
        let step = 256 * 1024;
        assert!(!throttle.due(step - 1, start));
        assert!(throttle.due(step, start));
        // Far enough but too soon, then late enough but not far enough.
        assert!(!throttle.due(2 * step, start + Duration::from_millis(100)));
        assert!(!throttle.due(step + 10, start + Duration::from_millis(300)));
        assert!(throttle.due(2 * step, start + Duration::from_millis(300)));
    }
}

/// A daemon state whose update is enabled over an installation and the host `host`.
fn enabled(label: &str, host: Arc<Host>) -> (Arc<AppState>, Installation, ScratchDir) {
    enabled_running(label, host, is_running_old)
}

/// [`enabled`], with its own judgement of whether a path is still the running build.
fn enabled_running(
    label: &str,
    host: Arc<dyn Fetcher>,
    running: fn(&Path) -> bool,
) -> (Arc<AppState>, Installation, ScratchDir) {
    let (state, dir) = app_state(label);
    let installation = Installation::new(label);
    state.daemon_update.enable(Setup {
        files: installation.files.clone(),
        fetcher: host,
        key: test_key(),
        arch: ARCH.to_owned(),
        launch: installation.exe.clone(),
        is_running_image: running,
    });
    (state, installation, dir)
}

fn newer_host() -> Arc<Host> {
    Host::serving(&manifest_text(VERSION, "99.0.0", &sha256_of(ARCHIVE)))
}

async fn until(state: &AppState, done: impl Fn(&DaemonUpdates) -> bool) {
    let deadline = std::time::Instant::now() + PATIENCE;
    while !done(&state.daemon_update) {
        assert!(
            std::time::Instant::now() < deadline,
            "the update did not get there"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn phase(updates: &DaemonUpdates) -> DaemonUpdatePhase {
    updates.status().unwrap().phase
}

mod requests {
    use super::*;

    #[tokio::test]
    async fn a_daemon_that_does_not_update_itself_has_no_status_and_refuses_every_request() {
        let (state, _dir) = app_state("update-disabled");
        assert!(!state.daemon_update.enabled());
        assert_eq!(state.daemon_update.status(), None);
        let unavailable = error_code::DAEMON_UPDATE_UNAVAILABLE;
        assert_eq!(
            code_of(&super::super::check_requested(&state).unwrap_err()),
            unavailable
        );
        assert_eq!(
            code_of(&super::super::update_requested(&state).await.unwrap_err()),
            unavailable
        );
        assert_eq!(
            code_of(&super::super::restart_requested(&state).unwrap_err()),
            unavailable
        );
        assert!(!state.planned_stop_asked());
    }

    #[tokio::test]
    async fn the_daemon_of_the_desktop_application_is_not_enabled() {
        // `start` is what decides; a daemon following an application, or a build without the
        // feature, never reaches `enable`.
        let (state, dir) = app_state("update-follows");
        assert!(!super::super::start(&state, true, &dir));
        assert!(!state.daemon_update.enabled());
        assert_eq!(state.daemon_update.status(), None);
    }

    #[tokio::test]
    async fn a_check_finds_the_release_and_announces_it() {
        let (state, installation, _dir) = enabled("update-check", newer_host());
        installation.files.write_daemon_version("0.1.0").unwrap();
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.phase, DaemonUpdatePhase::Idle);
        let available = status.available.unwrap();
        assert_eq!(available.version, VERSION);
        assert_eq!(available.notes.as_deref(), Some("What is new"));
        assert_eq!(available.holder_floor.as_deref(), Some("99.0.0"));
        assert_eq!(available.channel, DaemonUpdateChannel::Stable);
        let check = status.last_check.unwrap();
        assert_eq!(check.outcome, DaemonUpdateCheckOutcome::Available);
        assert_eq!(check.error, None);
        assert_eq!(status.manual, None);
    }

    #[tokio::test]
    async fn a_failed_check_is_told_with_its_cause_and_keeps_what_an_earlier_one_found() {
        let host = newer_host();
        let (state, _installation, _dir) = enabled("update-check-fails", host.clone());
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        // Past the floor between two checks of a client's own.
        host.answer(
            &manifest_url(DaemonUpdateChannel::Stable),
            Answer::Error(FetchError::Failed("offline".into())),
        );
        let claim = state.daemon_update.begin_check(&state, true).unwrap();
        let found = check(&*host, DaemonUpdateChannel::Stable, "0.1.0");
        state
            .daemon_update
            .record_check(&claim, found.map(|_| None), None);
        drop(claim);
        let status = state.daemon_update.status().unwrap();
        let check = status.last_check.unwrap();
        assert_eq!(check.outcome, DaemonUpdateCheckOutcome::Failed);
        assert_eq!(check.error, Some(daemon_update_code::CHECK_FAILED));
        assert_eq!(check.detail.as_deref(), Some("offline"));
        assert!(
            status.available.is_some(),
            "a failed look says nothing about what was found"
        );
    }

    #[tokio::test]
    async fn a_second_check_within_the_floor_changes_nothing() {
        let host = newer_host();
        let (state, _installation, _dir) = enabled("update-check-floor", host.clone());
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        super::super::check_requested(&state).unwrap();
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Idle);
        assert_eq!(host.asked().len(), 1);
        // A forced one (the channel changed) is not held back by it.
        assert!(state.daemon_update.begin_check(&state, true).is_some());
    }

    #[tokio::test]
    async fn the_update_is_downloaded_verified_and_installed_and_restart_is_then_the_next_step() {
        let (state, installation, _dir) = enabled("update-install", newer_host());
        let mut events = state.subscribe();
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
        })
        .await;
        super::super::update_requested(&state).await.unwrap();
        until(&state, |updates| {
            phase(updates) == DaemonUpdatePhase::Installed
        })
        .await;

        let status = state.daemon_update.status().unwrap();
        let installed = status.installed.unwrap();
        assert_eq!(installed.version.as_deref(), Some(VERSION));
        assert_eq!(installed.holder_floor.as_deref(), Some("99.0.0"));
        assert_eq!(status.available, None);
        assert_eq!(status.progress, None);
        let install = status.install.unwrap();
        assert_eq!(install.state, DaemonUpdateInstallState::Succeeded);
        assert!(install.finished_at.is_some());
        assert_eq!(std::fs::read(&installation.exe).unwrap(), NEW_DAEMON);

        // Every step was announced as the whole status, ending with the installed one.
        let announced: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
            .filter_map(|event| match event {
                crate::protocol::Event::DaemonUpdateStatusUpdated { status } => Some(status),
                _ => None,
            })
            .collect();
        let phases: Vec<_> = announced.iter().map(|status| status.phase).collect();
        assert_eq!(phases.first(), Some(&DaemonUpdatePhase::Checking));
        assert!(phases.contains(&DaemonUpdatePhase::Downloading));
        assert_eq!(phases.last(), Some(&DaemonUpdatePhase::Installed));
        // The archive is smaller than the step the progress is announced by, so the one announcement
        // of it is the start of the download.
        assert!(announced.iter().any(|status| {
            status.phase == DaemonUpdatePhase::Downloading && status.progress.is_some()
        }));

        // Once installed, nothing is checked and nothing installed again.
        let err = super::super::update_requested(&state).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_INSTALLED);
        super::super::check_requested(&state).unwrap();
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Installed);
        assert!(state.daemon_update.begin_check(&state, true).is_none());

        super::super::restart_requested(&state).unwrap();
        assert_eq!(
            state.await_stop().await,
            crate::state::StopKind::Planned { successor: true }
        );
    }

    #[tokio::test]
    async fn an_update_is_refused_when_none_was_found_or_one_is_running() {
        let (state, installation, _dir) = enabled("update-refused", newer_host());
        let err = super::super::update_requested(&state).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_NOTHING_TO_INSTALL);

        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
        })
        .await;
        // Another update, from the command line, holds the lock.
        let lock = crate::platform::fs::try_lock_file(&installation.files.lock()).unwrap();
        let err = super::super::update_requested(&state).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_RUNNING);
        drop(lock);
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Idle);

        // One of the daemon's own, begun and not ended.
        let (claim, _release) = state.daemon_update.begin_install(&state).unwrap();
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Downloading);
        let err = super::super::update_requested(&state).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_RUNNING);
        assert!(state.daemon_update.begin_check(&state, true).is_none());
        drop(claim);
        // A claim let go with the work unfinished ends it as failed, so no client waits on it.
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.phase, DaemonUpdatePhase::Idle);
        assert_eq!(
            status.install.unwrap().state,
            DaemonUpdateInstallState::Failed
        );
        assert!(status.available.is_some());
    }

    #[tokio::test]
    async fn an_update_is_refused_once_the_daemon_is_stopping() {
        let (state, _installation, _dir) = enabled("update-stopping", newer_host());
        state.stop_all_sessions().await;
        let err = super::super::update_requested(&state).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
    }

    #[tokio::test]
    async fn a_failed_install_is_told_by_its_code_and_can_be_asked_for_again() {
        let host = newer_host();
        let (state, installation, _dir) = enabled("update-install-fails", host.clone());
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
        })
        .await;
        host.answer(ARCHIVE_URL, Answer::Error(FetchError::Status(502)));
        super::super::update_requested(&state).await.unwrap();
        until(&state, |updates| {
            updates
                .status()
                .unwrap()
                .install
                .is_some_and(|install| install.state != DaemonUpdateInstallState::Running)
        })
        .await;
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.phase, DaemonUpdatePhase::Idle);
        let install = status.install.unwrap();
        assert_eq!(install.state, DaemonUpdateInstallState::Failed);
        assert_eq!(install.error, Some(daemon_update_code::DOWNLOAD_FAILED));
        assert_eq!(install.detail.as_deref(), Some("the server answered 502"));
        assert!(status.available.is_some());
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);

        host.answer(ARCHIVE_URL, Answer::Body(ARCHIVE.to_vec()));
        super::super::update_requested(&state).await.unwrap();
        until(&state, |updates| {
            phase(updates) == DaemonUpdatePhase::Installed
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_executable_the_user_may_not_replace_is_told_as_one_to_upgrade_by_hand() {
        use std::os::unix::fs::PermissionsExt;
        if ignores_modes() {
            return;
        }
        let host = newer_host();
        let (state, installation, _dir) = enabled("update-manual", host.clone());
        let dir = installation.exe.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        let status = state.daemon_update.status().unwrap();
        let manual = status.manual.unwrap();
        assert_eq!(manual.reason, DaemonUpdateManualReason::NotWritable);
        assert_eq!(
            PathBuf::from(&manual.path),
            crate::hostfs::canonical(&installation.exe).unwrap()
        );
        assert!(status.available.is_some());

        let err = super::super::update_requested(&state).await.unwrap_err();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_NOT_WRITABLE);
        assert_eq!(param_of(&err, "path"), manual.path);
        assert_eq!(host.asked().len(), 1, "no download was made");
    }

    /// What `octoboardd update` left (its record names this daemon's file and a newer version) is
    /// learned of at the next check or update, and the daemon then says installed.
    #[tokio::test]
    async fn an_update_made_from_the_command_line_is_learned_of() {
        let (state, installation, _dir) = enabled("update-adopt", newer_host());
        installation
            .files
            .write_installed(&super::super::record::Installed {
                version: VERSION.into(),
                holder_floor: Some("0.1.0".into()),
                from_version: env!("CARGO_PKG_VERSION").into(),
                path: crate::hostfs::canonical(&installation.exe)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            })
            .unwrap();
        let err = super::super::update_requested(&state).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_INSTALLED);
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.phase, DaemonUpdatePhase::Installed);
        assert_eq!(status.installed.unwrap().version.as_deref(), Some(VERSION));
    }

    #[tokio::test]
    async fn a_record_of_another_file_or_an_older_version_is_not_this_daemons_update() {
        let (state, installation, _dir) = enabled("update-adopt-other", newer_host());
        for (version, path) in [
            (VERSION, "/somewhere/else/octoboardd".to_owned()),
            (
                "0.0.1",
                crate::hostfs::canonical(&installation.exe)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
        ] {
            installation
                .files
                .write_installed(&super::super::record::Installed {
                    version: version.into(),
                    holder_floor: None,
                    from_version: "0.0.0".into(),
                    path,
                })
                .unwrap();
            let err = super::super::update_requested(&state).await.unwrap_err();
            assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_NOTHING_TO_INSTALL);
            assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Idle);
        }
    }

    /// The install script, say, replaced the file after the daemon started: nothing is downloaded
    /// or overwritten, and the daemon says an update is installed without knowing which.
    #[tokio::test]
    async fn a_file_something_else_replaced_is_reported_installed_with_no_known_version() {
        let host = newer_host();
        let (state, dir) = app_state("update-replaced-already");
        let installation = Installation::new("update-replaced-already");
        state.daemon_update.enable(Setup {
            files: installation.files.clone(),
            fetcher: host.clone(),
            key: test_key(),
            arch: ARCH.to_owned(),
            launch: installation.exe.clone(),
            is_running_image: is_not_running,
        });
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
        })
        .await;
        super::super::update_requested(&state).await.unwrap();
        until(&state, |updates| {
            phase(updates) == DaemonUpdatePhase::Installed
        })
        .await;
        let installed = state.daemon_update.status().unwrap().installed.unwrap();
        assert_eq!((installed.version, installed.holder_floor), (None, None));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert_eq!(host.asked().len(), 1, "only the manifest was fetched");
        assert!(installation.files.read_installed().is_none());
        drop(dir);
    }

    /// A check made after `octoboardd update` replaced the file learns of that instead of looking
    /// for a release again.
    #[tokio::test]
    async fn a_check_learns_of_an_update_made_from_the_command_line() {
        let host = newer_host();
        let (state, installation, _dir) = enabled("update-check-adopts", host.clone());
        installation
            .files
            .write_installed(&super::super::record::Installed {
                version: VERSION.into(),
                holder_floor: Some("0.1.0".into()),
                from_version: env!("CARGO_PKG_VERSION").into(),
                path: crate::hostfs::canonical(&installation.exe)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            })
            .unwrap();
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            phase(updates) == DaemonUpdatePhase::Installed
        })
        .await;
        let installed = state.daemon_update.status().unwrap().installed.unwrap();
        assert_eq!(installed.version.as_deref(), Some(VERSION));
        assert_eq!(installed.holder_floor.as_deref(), Some("0.1.0"));
    }

    #[tokio::test]
    async fn a_restart_is_refused_while_a_stop_or_restart_is_under_way_and_asked_for_once() {
        let (state, _installation, _dir) = enabled("update-restart", newer_host());
        super::super::restart_requested(&state).unwrap();
        let err = super::super::restart_requested(&state).unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);

        let (stopping, _installation, _dir) = enabled("update-restart-stop", newer_host());
        stopping.request_planned_stop();
        let err = super::super::restart_requested(&stopping).unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
        assert_eq!(
            stopping.await_stop().await,
            crate::state::StopKind::Planned { successor: false },
            "the stop asked for first stays the one made"
        );

        let (ordinary, _installation, _dir) = enabled("update-restart-ordinary", newer_host());
        ordinary.request_shutdown();
        let err = super::super::restart_requested(&ordinary).unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
    }
}

mod scheduled {
    use super::*;

    #[tokio::test]
    async fn a_scheduled_check_runs_when_due_and_the_setting_is_on() {
        let host = newer_host();
        let (state, _installation, _dir) = enabled("schedule-due", host.clone());
        scheduled_check(&state, false).await;
        assert_eq!(host.asked().len(), 1);
        let status = state.daemon_update.status().unwrap();
        assert_eq!(
            status.last_check.unwrap().outcome,
            DaemonUpdateCheckOutcome::Available
        );
        // Not due again for six hours.
        scheduled_check(&state, false).await;
        assert_eq!(host.asked().len(), 1);
    }

    #[tokio::test]
    async fn the_setting_off_leaves_only_the_users_own_check() {
        let host = newer_host();
        let (state, _installation, _dir) = enabled("schedule-off", host.clone());
        state.store.set_daemon_update_check(false).unwrap();
        scheduled_check(&state, true).await;
        assert!(host.asked().is_empty());
        assert_eq!(state.daemon_update.status().unwrap().last_check, None);
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        assert_eq!(host.asked().len(), 1);
    }

    #[tokio::test]
    async fn a_forced_check_goes_ahead_of_the_schedule_and_follows_the_channel() {
        let host = newer_host();
        let (state, _installation, _dir) = enabled("schedule-forced", host.clone());
        scheduled_check(&state, false).await;
        state
            .store
            .set_daemon_update_channel(DaemonUpdateChannel::Beta)
            .unwrap();
        scheduled_check(&state, true).await;
        assert_eq!(
            host.asked(),
            [
                manifest_url(DaemonUpdateChannel::Stable),
                manifest_url(DaemonUpdateChannel::Beta)
            ]
        );
    }

    #[tokio::test]
    async fn nothing_is_checked_once_an_update_is_installed() {
        let host = newer_host();
        let (state, installation, _dir) = enabled("schedule-installed", host.clone());
        let release = installation.release();
        let (claim, _) = {
            super::super::check_requested(&state).unwrap();
            until(&state, |updates| {
                updates.status().unwrap().available.is_some()
            })
            .await;
            state.daemon_update.begin_install(&state).unwrap()
        };
        state
            .daemon_update
            .record_installed(&claim, &release, Ok(true));
        drop(claim);
        // What the update left at the path.
        std::fs::write(&installation.exe, NEW_DAEMON).unwrap();
        let before = host.asked().len();
        scheduled_check(&state, true).await;
        assert_eq!(host.asked().len(), before);
    }

    /// Whether the schedule was woken since the last look: the wake is a stored permit, so it is
    /// there as soon as the settings change returns, and nothing is waited for.
    fn woken(state: &AppState) -> bool {
        use futures_util::FutureExt;
        state
            .daemon_update
            .again
            .notified()
            .now_or_never()
            .is_some()
    }

    #[tokio::test]
    async fn turning_the_check_on_or_changing_the_channel_has_the_schedule_run_at_once() {
        use crate::coordinator::settings::{update_settings, SettingsChanges};
        let (state, _installation, _dir) = enabled("schedule-settings", newer_host());
        // Turning it off wakes nothing.
        let off = SettingsChanges {
            daemon_update_check: Some(false),
            ..Default::default()
        };
        update_settings(&state, off).unwrap();
        assert!(!woken(&state));
        let on = SettingsChanges {
            daemon_update_check: Some(true),
            ..Default::default()
        };
        update_settings(&state, on).unwrap();
        assert!(woken(&state), "turning it on wakes the schedule");
        // Turning it on when it is on already changes nothing.
        let on_again = SettingsChanges {
            daemon_update_check: Some(true),
            ..Default::default()
        };
        update_settings(&state, on_again).unwrap();
        assert!(!woken(&state));
        // The same channel again changes nothing, another one does.
        let same = SettingsChanges {
            daemon_update_channel: Some(DaemonUpdateChannel::Stable),
            ..Default::default()
        };
        update_settings(&state, same).unwrap();
        assert!(!woken(&state));
        let beta = SettingsChanges {
            daemon_update_channel: Some(DaemonUpdateChannel::Beta),
            ..Default::default()
        };
        update_settings(&state, beta).unwrap();
        assert!(woken(&state), "a new channel wakes the schedule");
    }
}

mod hardening {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    /// A host that runs `after` once it has served the archive.
    struct Then<F: Fn() + Send + Sync>(Arc<Host>, F);

    impl<F: Fn() + Send + Sync> Fetcher for Then<F> {
        fn get(
            &self,
            url: &str,
            max_bytes: u64,
            timeout: Duration,
            sink: &mut Sink<'_>,
        ) -> Result<(), FetchError> {
            let served = self.0.get(url, max_bytes, timeout, sink);
            if url == ARCHIVE_URL {
                (self.1)();
            }
            served
        }
    }

    #[test]
    fn a_path_that_stops_being_the_running_build_during_the_download_is_not_replaced() {
        static REPLACED_MEANWHILE: AtomicBool = AtomicBool::new(false);
        fn still_running(_: &Path) -> bool {
            !REPLACED_MEANWHILE.load(Ordering::SeqCst)
        }
        let installation = Installation::new("harden-flips");
        let host = Then(Host::serving(""), || {
            REPLACED_MEANWHILE.store(true, Ordering::SeqCst)
        });
        let key = test_key();
        let sources = Sources {
            fetcher: &host,
            key: &key,
            arch: ARCH,
            is_running_image: still_running,
            stopping: &|| false,
        };
        let outcome = install(
            &sources,
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap();
        assert!(!outcome.replaced);
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert!(!installation
            .exe
            .with_file_name("octoboardd.previous")
            .exists());
        assert!(installation.files.read_installed().is_none());
        assert_eq!(
            std::fs::read_dir(installation.files.updates_dir())
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn a_stop_ends_the_download_and_keeps_the_executable_from_being_replaced() {
        let key = test_key();
        // During the download.
        let installation = Installation::new("harden-stop-download");
        let host = Host::serving("");
        let stopped = AtomicBool::new(true);
        let stopping = || stopped.load(Ordering::SeqCst);
        let sources = Sources {
            fetcher: &*host,
            key: &key,
            arch: ARCH,
            is_running_image: is_running,
            stopping: &stopping,
        };
        let err = install(
            &sources,
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::Download(text) if text.contains("stopping")));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);

        // After it, just before the file is touched.
        let installation = Installation::new("harden-stop-replace");
        let began = AtomicBool::new(false);
        let host = Then(Host::serving(""), || began.store(true, Ordering::SeqCst));
        let stopping = || began.load(Ordering::SeqCst);
        let sources = Sources {
            fetcher: &host,
            key: &key,
            arch: ARCH,
            is_running_image: is_running,
            stopping: &stopping,
        };
        let err = install(
            &sources,
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::Replace(text) if text.contains("began to stop")));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert!(!installation
            .exe
            .with_file_name("octoboardd.previous")
            .exists());
        assert_eq!(
            std::fs::read_dir(installation.files.updates_dir())
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn a_restart_is_refused_while_the_download_runs() {
        let (state, _installation, _dir) = enabled("harden-restart-download", newer_host());
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
        })
        .await;
        let (claim, _) = state.daemon_update.begin_install(&state).unwrap();
        let err = super::super::restart_requested(&state).unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_RUNNING);
        assert!(!state.planned_stop_asked());
        drop(claim);
        super::super::restart_requested(&state).unwrap();
    }

    #[tokio::test]
    async fn a_forced_check_asked_for_during_work_runs_when_it_ends() {
        let (state, _installation, _dir) = enabled("harden-recheck", newer_host());
        let claim = state.daemon_update.begin_check(&state, false).unwrap();
        assert!(state.daemon_update.begin_check(&state, true).is_none());
        assert!(
            tokio::time::timeout(
                Duration::from_millis(50),
                state.daemon_update.again.notified()
            )
            .await
            .is_err(),
            "nothing is woken while the work runs"
        );
        drop(claim);
        tokio::time::timeout(PATIENCE, state.daemon_update.again.notified())
            .await
            .expect("the check asked for meanwhile is asked for again");
        // An unforced one is not remembered.
        let claim = state.daemon_update.begin_check(&state, true).unwrap();
        assert!(state.daemon_update.begin_check(&state, false).is_none());
        drop(claim);
        assert!(tokio::time::timeout(
            Duration::from_millis(50),
            state.daemon_update.again.notified()
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn a_new_channel_forgets_what_the_old_one_offered_even_with_the_check_off() {
        let (state, _installation, _dir) = enabled("harden-channel", newer_host());
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
        })
        .await;
        state.store.set_daemon_update_check(false).unwrap();
        let mut events = state.subscribe();
        crate::coordinator::settings::update_settings(
            &state,
            crate::coordinator::settings::SettingsChanges {
                daemon_update_channel: Some(DaemonUpdateChannel::Beta),
                ..Default::default()
            },
        )
        .unwrap();
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.available, None);
        assert_eq!(status.manual, None);
        let err = super::super::update_requested(&state).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_NOTHING_TO_INSTALL);
        let announced = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
            matches!(event, crate::protocol::Event::DaemonUpdateStatusUpdated { status } if status.available.is_none())
        });
        assert!(announced);
    }

    #[tokio::test]
    async fn the_lock_comes_before_an_adopted_record_and_an_update_made_meanwhile_is_not_noted_over_a_download(
    ) {
        let (state, installation, _dir) = enabled("harden-adopt-lock", newer_host());
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
        })
        .await;
        installation
            .files
            .write_installed(&super::super::record::Installed {
                version: VERSION.into(),
                holder_floor: None,
                from_version: env!("CARGO_PKG_VERSION").into(),
                path: crate::hostfs::canonical(&installation.exe)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            })
            .unwrap();
        // The command line holds the lock, so its record may be half made: not read.
        let lock = crate::platform::fs::try_lock_file(&installation.files.lock()).unwrap();
        let err = super::super::update_requested(&state).await.unwrap_err();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_RUNNING);
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Idle);
        drop(lock);

        // While this daemon downloads, nothing is noted over it.
        let release = installation.release();
        let (claim, _) = {
            let found = state.daemon_update.begin_check(&state, true).unwrap();
            state
                .daemon_update
                .record_check(&found, Ok(Some(release)), None);
            drop(found);
            state.daemon_update.begin_install(&state).unwrap()
        };
        assert!(!state
            .daemon_update
            .record_installed_elsewhere(&state, VERSION.into(), None));
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Downloading);
        drop(claim);
    }

    #[tokio::test]
    async fn an_installed_update_is_forgotten_once_the_launch_path_is_the_running_build_again() {
        static ROLLED_BACK: AtomicBool = AtomicBool::new(false);
        fn running(_: &Path) -> bool {
            ROLLED_BACK.load(Ordering::SeqCst)
        }
        let (state, installation, _dir) = enabled_running("harden-rollback", newer_host(), running);
        installation
            .files
            .write_installed(&super::super::record::Installed {
                version: VERSION.into(),
                holder_floor: None,
                from_version: env!("CARGO_PKG_VERSION").into(),
                path: crate::hostfs::canonical(&installation.exe)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            })
            .unwrap();
        let _ = super::super::update_requested(&state).await;
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Installed);
        super::super::check_requested(&state).unwrap();
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Installed);

        // `octoboardd update --rollback` put the running build back and forgot its record.
        ROLLED_BACK.store(true, Ordering::SeqCst);
        installation.files.forget_installed().unwrap();
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.installed, None);
        assert_eq!(status.install, None);
        assert!(status.available.is_some(), "checks resumed");
    }

    #[test]
    fn a_version_that_could_put_a_path_in_a_file_name_is_refused() {
        for version in ["../9.9.9", "9.9.9/x", "9.9.9+a/b", "9.9.9 ", "9.9.9\n"] {
            let text = manifest_text(version, "0.1.0", &sha256_of(ARCHIVE));
            assert!(
                parse(&text, &platform_key(), DaemonUpdateChannel::Stable).is_err(),
                "{version:?}"
            );
        }
        assert!(parse(
            &manifest_text("9.9.9-beta.1+build.5", "0.1.0", &sha256_of(ARCHIVE)),
            &platform_key(),
            DaemonUpdateChannel::Stable
        )
        .is_ok());
        assert!(manifest::is_newer("9.9.9", "9.9.8"));
        assert!(!manifest::is_newer("9.9.8", "9.9.9"));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_planted_at_the_staging_name_is_removed_not_written_through() {
        let installation = Installation::new("harden-staged-link");
        let victim = installation.exe.with_file_name("victim");
        std::fs::write(&victim, "kept").unwrap();
        let staged = installation
            .exe
            .with_file_name(format!(".octoboardd.update-{}", std::process::id()));
        crate::test_support::symlink(&victim, &staged).unwrap();
        replace(&installation.exe, |file| {
            std::io::Write::write_all(file, b"new").map_err(|err| err.to_string())
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "kept");
        assert_eq!(std::fs::read(&installation.exe).unwrap(), b"new");
        assert!(std::fs::symlink_metadata(&staged).is_err());
    }

    /// A host whose download gives one piece, and then waits until it is told to go on.
    struct Stalls {
        host: Arc<Host>,
        reached: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
    }

    impl Fetcher for Stalls {
        fn get(
            &self,
            url: &str,
            max_bytes: u64,
            timeout: Duration,
            sink: &mut Sink<'_>,
        ) -> Result<(), FetchError> {
            let mut first = true;
            self.host.get(url, max_bytes, timeout, &mut |piece, total| {
                let taken = sink(piece, total);
                if url == ARCHIVE_URL && std::mem::take(&mut first) {
                    self.reached.store(true, Ordering::SeqCst);
                    while !self.release.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                taken
            })
        }
    }

    /// A download that waits on a stalled connection must not hold the daemon's exit: it does not
    /// run on the runtime's blocking pool, whose end the runtime waits for. When it moves on again
    /// the stop that began keeps the executable as it was.
    #[test]
    fn a_stalled_download_does_not_hold_the_runtime_and_leaves_the_executable_alone_when_it_moves()
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (reached, release) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let stalls = Arc::new(Stalls {
            host: newer_host(),
            reached: reached.clone(),
            release: release.clone(),
        });
        let (state, installation, _dir) = enabled_running("harden-stall", stalls, is_running_old);
        runtime.block_on(async {
            super::super::check_requested(&state).unwrap();
            until(&state, |updates| {
                updates.status().unwrap().available.is_some()
            })
            .await;
            super::super::update_requested(&state).await.unwrap();
        });
        let deadline = std::time::Instant::now() + PATIENCE;
        while !reached.load(Ordering::SeqCst) {
            assert!(
                std::time::Instant::now() < deadline,
                "the download did not begin"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        state.request_shutdown();
        let began = std::time::Instant::now();
        drop(runtime);
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "the runtime waited for the download: {:?}",
            began.elapsed()
        );

        release.store(true, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + PATIENCE;
        let install = loop {
            let install = state.daemon_update.status().unwrap().install.unwrap();
            if install.state != DaemonUpdateInstallState::Running {
                break install;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the install did not end"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(install.state, DaemonUpdateInstallState::Failed);
        assert_eq!(install.error, Some(daemon_update_code::DOWNLOAD_FAILED));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert!(!installation
            .exe
            .with_file_name("octoboardd.previous")
            .exists());
    }

    /// A host that moves the daemon to the beta channel while the stable manifest is being read.
    struct Flips {
        host: Arc<Host>,
        state: Mutex<Option<std::sync::Weak<AppState>>>,
        flipped: AtomicBool,
    }

    impl Fetcher for Flips {
        fn get(
            &self,
            url: &str,
            max_bytes: u64,
            timeout: Duration,
            sink: &mut Sink<'_>,
        ) -> Result<(), FetchError> {
            if url == manifest_url(DaemonUpdateChannel::Stable)
                && !self.flipped.swap(true, Ordering::SeqCst)
            {
                let state = self
                    .state
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|state| state.upgrade());
                crate::coordinator::settings::update_settings(
                    &state.unwrap(),
                    crate::coordinator::settings::SettingsChanges {
                        daemon_update_channel: Some(DaemonUpdateChannel::Beta),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            self.host.get(url, max_bytes, timeout, sink)
        }
    }

    #[tokio::test]
    async fn a_channel_change_during_a_users_check_has_it_read_the_new_channel() {
        let host = newer_host();
        let flips = Arc::new(Flips {
            host: host.clone(),
            state: Mutex::new(None),
            flipped: AtomicBool::new(false),
        });
        let (state, _installation, _dir) =
            enabled_running("harden-channel-mid-check", flips.clone(), is_running_old);
        *flips.state.lock().unwrap() = Some(Arc::downgrade(&state));
        // The automatic check is off, so nothing follows the change but this check itself.
        state.store.set_daemon_update_check(false).unwrap();
        super::super::check_requested(&state).unwrap();
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        assert_eq!(
            host.asked(),
            [
                manifest_url(DaemonUpdateChannel::Stable),
                manifest_url(DaemonUpdateChannel::Beta)
            ]
        );
        let available = state.daemon_update.status().unwrap().available.unwrap();
        assert_eq!(available.channel, DaemonUpdateChannel::Beta);
    }

    /// A result read under the old channel is not recorded: the claim stands for the new one.
    #[tokio::test]
    async fn a_check_of_the_old_channel_is_not_recorded_once_the_channel_moved() {
        let (state, _installation, _dir) = enabled("harden-generation", newer_host());
        let claim = state.daemon_update.begin_check(&state, true).unwrap();
        state.daemon_update.channel_changed(&state);
        let release = _installation.release();
        assert!(!state
            .daemon_update
            .record_check(&claim, Ok(Some(release.clone())), None));
        assert_eq!(state.daemon_update.status().unwrap().last_check, None);
        assert!(state
            .daemon_update
            .record_check(&claim, Ok(Some(release)), None));
        assert!(state.daemon_update.status().unwrap().available.is_some());
    }

    #[cfg(unix)]
    mod unbroken_path {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn names_in(dir: &Path) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .unwrap()
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        /// The path is looked at from another thread throughout many replacements and rollbacks:
        /// there is always a file there, the old one or the new.
        #[test]
        fn the_path_is_never_without_a_file_while_it_is_replaced_or_rolled_back() {
            let installation = Installation::new("harden-never-empty");
            let target = installation.exe.clone();
            let done = Arc::new(AtomicBool::new(false));
            let watcher = {
                let (target, done) = (target.clone(), done.clone());
                std::thread::spawn(move || {
                    let mut missing = 0usize;
                    while !done.load(Ordering::SeqCst) {
                        if std::fs::metadata(&target).is_err() {
                            missing += 1;
                        }
                    }
                    missing
                })
            };
            for round in 0..200u32 {
                replace(&target, |file| {
                    std::io::Write::write_all(file, &round.to_le_bytes())
                        .map_err(|err| err.to_string())
                })
                .unwrap();
                swap_with_previous(&target).unwrap();
            }
            done.store(true, Ordering::SeqCst);
            assert_eq!(watcher.join().unwrap(), 0, "the path was without a file");
            assert_eq!(
                names_in(target.parent().unwrap()),
                ["octoboardd", "octoboardd.previous"]
            );
        }

        /// What earlier runs left beside the executable is cleared at the start of a replacement and of
        /// a rollback, links included, and nothing else is.
        #[test]
        fn leftovers_of_earlier_runs_are_cleared_and_nothing_else() {
            let installation = Installation::new("harden-sweep");
            let dir = installation.exe.parent().unwrap().to_path_buf();
            let victim = dir.join("victim");
            std::fs::write(&victim, "kept").unwrap();
            let plant = |dir: &Path| {
                for name in [
                    ".octoboardd.update-1",
                    ".octoboardd.prev-22",
                    ".octoboardd.rollback-333",
                ] {
                    std::fs::write(dir.join(name), "stale").unwrap();
                }
                crate::test_support::symlink(&victim, dir.join(".octoboardd.update-4444")).ok();
                std::fs::write(dir.join(".octoboardd.update-x"), "not a pid").unwrap();
                std::fs::write(dir.join(".other.update-1"), "of another file").unwrap();
                std::fs::create_dir(dir.join(".octoboardd.prev-5")).unwrap();
            };
            plant(&dir);
            replace(&installation.exe, |file| {
                std::io::Write::write_all(file, b"new").map_err(|err| err.to_string())
            })
            .unwrap();
            let expected = [
                ".octoboardd.prev-5",
                ".octoboardd.update-x",
                ".other.update-1",
                "octoboardd",
                "octoboardd.previous",
                "victim",
            ];
            assert_eq!(names_in(&dir), expected);
            assert_eq!(std::fs::read_to_string(&victim).unwrap(), "kept");

            std::fs::remove_dir(dir.join(".octoboardd.prev-5")).unwrap();
            plant_again(&dir, &victim);
            swap_with_previous(&installation.exe).unwrap();
            assert_eq!(names_in(&dir), expected);
            assert_eq!(std::fs::read_to_string(&victim).unwrap(), "kept");

            fn plant_again(dir: &Path, victim: &Path) {
                for name in [".octoboardd.rollback-9", ".octoboardd.prev-8"] {
                    std::fs::write(dir.join(name), "stale").unwrap();
                }
                crate::test_support::symlink(victim, dir.join(".octoboardd.update-7")).ok();
                std::fs::create_dir(dir.join(".octoboardd.prev-5")).unwrap();
            }
        }

        /// Where the file system makes no link the old file is kept by a copy made new, with the
        /// same content and permissions.
        #[test]
        fn the_copy_that_keeps_the_old_file_is_made_new_with_its_permissions() {
            let installation = Installation::new("harden-keep-copy");
            let kept = installation.exe.with_file_name("kept");
            super::super::super::replace::keep_copy_for_test(&installation.exe, &kept).unwrap();
            assert_eq!(std::fs::read(&kept).unwrap(), OLD_DAEMON);
            assert_eq!(
                std::fs::metadata(&kept).unwrap().permissions().mode() & 0o777,
                std::fs::metadata(&installation.exe)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777
            );
            // A file there is refused by the copy itself.
            assert!(
                super::super::super::replace::keep_copy_for_test(&installation.exe, &kept).is_err()
            );
        }

        /// A failure partway leaves the installation as it was, and nothing beside it.
        #[test]
        fn a_replacement_that_fails_after_the_file_is_written_changes_nothing() {
            let installation = Installation::new("harden-replace-fails");
            // A directory at the name of the previous file: it cannot be replaced by a file.
            let previous = installation.exe.with_file_name("octoboardd.previous");
            std::fs::create_dir(&previous).unwrap();
            std::fs::write(previous.join("keeps it a directory"), "x").unwrap();
            let err = replace(&installation.exe, |file| {
                std::io::Write::write_all(file, b"new").map_err(|err| err.to_string())
            })
            .unwrap_err();
            assert!(matches!(err, ReplaceError::Failed(_)), "{err:?}");
            assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
            assert_eq!(
                std::fs::metadata(&installation.exe)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o100,
                0o100
            );
            assert_eq!(
                names_in(installation.exe.parent().unwrap()),
                ["octoboardd", "octoboardd.previous"]
            );
        }
    }

    #[test]
    fn a_download_a_stop_left_is_swept_unless_another_process_holds_the_lock() {
        let installation = Installation::new("harden-sweep-updates");
        let dir = installation.files.updates_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("octoboardd_9.9.9_linux_x86_64.tar.gz.part");
        std::fs::write(&part, "half").unwrap();

        let lock = crate::platform::fs::try_lock_file(&installation.files.lock()).unwrap();
        assert!(!installation.files.sweep_updates());
        assert!(part.exists(), "another process's download is not touched");
        drop(lock);

        assert!(installation.files.sweep_updates());
        assert!(!part.exists());
        assert!(dir.is_dir());
        // Nothing to sweep, and no folder, is no failure.
        assert!(installation.files.sweep_updates());
        std::fs::remove_dir(&dir).unwrap();
        assert!(installation.files.sweep_updates());
    }
}
