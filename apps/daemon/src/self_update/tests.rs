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

/// A vetting that takes every new file.
fn vet_ok(_: &Path) -> Result<(), String> {
    Ok(())
}

/// A probe that takes every build for one that starts: the archive's stand-in is no daemon.
fn accept(_: &Path, _: &str) -> Result<(), String> {
    Ok(())
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
            probe: accept,
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
    fn a_reader_is_checked_the_way_a_path_is() {
        let verify = |bytes: &[u8], sha256: &str| {
            super::super::verify::verify_reader(
                &mut std::io::Cursor::new(bytes.to_vec()),
                sha256,
                SIGNATURE,
                &name(),
                &test_key(),
            )
        };
        verify(ARCHIVE, &sha256_of(ARCHIVE)).unwrap();
        assert!(verify(ARCHIVE, &sha256_of(b"other"))
            .unwrap_err()
            .contains("SHA-256"));
        // Bytes that are not the signed ones fail the signature even under their own SHA-256.
        let mut altered = ARCHIVE.to_vec();
        *altered.last_mut().unwrap() ^= 0xff;
        assert!(verify(&altered, &sha256_of(&altered)).is_err());
    }

    /// Without a SHA-256 to compare, the signature and the name it signs still decide.
    #[test]
    fn a_signature_alone_decides_where_no_digest_is_given() {
        let verify = |bytes: &[u8], expected: &str| {
            super::super::verify::verify_signature(
                &mut std::io::Cursor::new(bytes.to_vec()),
                SIGNATURE,
                expected,
                &test_key(),
            )
        };
        verify(ARCHIVE, &name()).unwrap();
        let mut altered = ARCHIVE.to_vec();
        *altered.last_mut().unwrap() ^= 0xff;
        assert!(verify(&altered, &name())
            .unwrap_err()
            .contains("not the release key's"));
        assert!(verify(ARCHIVE, "octoboardd_1.0.0_linux_x86_64.tar.gz")
            .unwrap_err()
            .contains("not of"));
    }

    /// The installation verifies and then unpacks the one open file, so that what is unpacked is
    /// what was verified: a rewind between the two is all it takes.
    #[test]
    fn one_open_file_is_verified_and_then_unpacked_after_a_rewind() {
        let dir = ScratchDir::new("verify-one-handle");
        let mut file = std::fs::File::open(archive_in(&dir, ARCHIVE)).unwrap();
        super::super::verify::verify_reader(
            &mut file,
            &sha256_of(ARCHIVE),
            SIGNATURE,
            &name(),
            &test_key(),
        )
        .unwrap();
        std::io::Seek::rewind(&mut file).unwrap();
        let mut out = Vec::new();
        super::super::unpack::unpack_reader(
            &mut file,
            &daemon_entry(VERSION, ARCH),
            &mut out,
            1024,
        )
        .unwrap();
        assert_eq!(out, NEW_DAEMON);
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
    fn the_manual_install_command_names_the_executables_directory_and_the_beta_channel() {
        let exe = Path::new("/opt/octoboard/bin/octoboardd");
        assert_eq!(
            manifest::install_command_for(exe, DaemonUpdateChannel::Stable),
            format!(
                "{} -s -- --bin-dir /opt/octoboard/bin",
                manifest::install_command()
            )
        );
        assert_eq!(
            manifest::install_command_for(exe, DaemonUpdateChannel::Beta),
            format!(
                "{} -s -- --bin-dir /opt/octoboard/bin --beta",
                manifest::install_command()
            )
        );
    }

    /// The directory is one word of the command, whatever characters its name has.
    #[test]
    fn a_directory_the_shell_would_split_or_expand_is_quoted_in_the_manual_command() {
        let command = |dir: &str| {
            manifest::install_command_for(
                &Path::new(dir).join("octoboardd"),
                DaemonUpdateChannel::Stable,
            )
        };
        let prefix = format!("{} -s -- --bin-dir ", manifest::install_command());
        assert_eq!(
            command("/home/me/my apps/bin"),
            format!("{prefix}'/home/me/my apps/bin'")
        );
        assert_eq!(
            command("/home/o'brien/bin"),
            format!("{prefix}'/home/o'\\''brien/bin'")
        );
        for dir in [
            "/home/$USER/bin",
            "/srv/a;b/bin",
            "/srv/a&b/bin",
            "/srv/a|b/bin",
            "/srv/`id`/bin",
            "/srv/\"quoted\"/bin",
            "/srv/star*/bin",
            "/srv/new\nline/bin",
            "/srv/back\\slash/bin",
            "/srv/(group)/bin",
        ] {
            assert_eq!(command(dir), format!("{prefix}'{dir}'"), "{dir}");
        }
        // A name of plain characters is left as it is.
        assert_eq!(
            command("/opt/my-app_1.2/bin"),
            format!("{prefix}/opt/my-app_1.2/bin")
        );
    }

    #[test]
    fn a_manual_update_carries_the_path_and_the_command_for_the_channel() {
        let exe = Path::new("/opt/octoboard/bin/octoboardd");
        for channel in [DaemonUpdateChannel::Stable, DaemonUpdateChannel::Beta] {
            let manual = super::super::not_writable(exe, channel);
            assert_eq!(manual.reason, DaemonUpdateManualReason::NotWritable);
            assert_eq!(manual.path, "/opt/octoboard/bin/octoboardd");
            assert_eq!(manual.command, manifest::install_command_for(exe, channel));
        }
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
    fn the_daemon_is_taken_out_of_any_reader_not_only_a_path() {
        let mut out = Vec::new();
        let copied = super::super::unpack::unpack_reader(
            std::io::Cursor::new(ARCHIVE),
            &daemon_entry(VERSION, ARCH),
            &mut out,
            1024,
        )
        .unwrap();
        assert_eq!(out, NEW_DAEMON);
        assert_eq!(copied, NEW_DAEMON.len() as u64);
        let err = super::super::unpack::unpack_reader(
            std::io::Cursor::new(b"not a tarball at all, not even gzip".to_vec()),
            "x",
            &mut Vec::new(),
            1024,
        )
        .unwrap_err();
        assert!(matches!(err, UnpackError::Archive(_)));
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
        replace(
            &installation.exe,
            |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
            vet_ok,
        )
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
        replace(
            &installation.exe,
            |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
            vet_ok,
        )
        .unwrap();
        let mut content = Vec::new();
        held.read_to_end(&mut content).unwrap();
        assert_eq!(content, OLD_DAEMON);
    }

    #[test]
    fn a_file_that_cannot_be_filled_leaves_the_installation_as_it_was() {
        let installation = Installation::new("replace-fill");
        let err = replace(&installation.exe, |_| Err("cut short".to_owned()), vet_ok).unwrap_err();
        assert_eq!(err, ReplaceError::Filling("cut short".to_owned()));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert_eq!(names_in(installation.exe.parent().unwrap()), ["octoboardd"]);
    }

    /// The new file is looked at where it lies, written whole and beside the target, before any
    /// rename: the old file is still at the path and nothing is kept as the previous one yet.
    #[test]
    fn the_new_file_is_vetted_whole_and_beside_the_target_before_anything_moves() {
        let installation = Installation::new("replace-vet-sees");
        let target = installation.exe.clone();
        let dir = target.parent().unwrap().to_path_buf();
        let mut vetted = None;
        replace(
            &target,
            |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
            |staged| {
                vetted = Some((
                    staged.to_path_buf(),
                    std::fs::read(staged).unwrap(),
                    std::fs::read(&target).unwrap(),
                    names_in(&dir),
                ));
                Ok(())
            },
        )
        .unwrap();
        let (staged, staged_content, target_content, names) =
            vetted.expect("the new file was vetted");
        assert_ne!(staged, target);
        assert_eq!(staged.parent(), Some(dir.as_path()));
        assert_eq!(staged_content, b"new");
        assert_eq!(
            target_content, OLD_DAEMON,
            "the path still held the old file"
        );
        assert_eq!(
            names.len(),
            2,
            "only the target and the new file: {names:?}"
        );
        assert!(names.contains(&"octoboardd".to_owned()));
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn a_new_file_the_vetting_refuses_is_removed_and_the_installation_is_as_it_was() {
        let installation = Installation::new("replace-vet-refuses");
        let dir = installation.exe.parent().unwrap().to_path_buf();
        let err = replace(
            &installation.exe,
            |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
            |_| Err("it does not start".to_owned()),
        )
        .unwrap_err();
        assert_eq!(err, ReplaceError::Refused("it does not start".to_owned()));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert_eq!(names_in(&dir), ["octoboardd"]);
    }

    #[test]
    fn a_refused_replacement_leaves_the_previous_file_kept_by_an_earlier_one_alone() {
        let installation = Installation::new("replace-vet-previous");
        replace(
            &installation.exe,
            |file| std::io::Write::write_all(file, b"first").map_err(|err| err.to_string()),
            vet_ok,
        )
        .unwrap();
        let previous = installation.exe.with_file_name("octoboardd.previous");
        assert_eq!(std::fs::read(&previous).unwrap(), OLD_DAEMON);
        let err = replace(
            &installation.exe,
            |file| std::io::Write::write_all(file, b"second").map_err(|err| err.to_string()),
            |_| Err("refused".to_owned()),
        )
        .unwrap_err();
        assert!(matches!(err, ReplaceError::Refused(_)));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), b"first");
        assert_eq!(std::fs::read(&previous).unwrap(), OLD_DAEMON);
        assert_eq!(
            names_in(installation.exe.parent().unwrap()),
            ["octoboardd", "octoboardd.previous"]
        );
    }

    #[test]
    fn a_file_that_cannot_be_filled_is_not_vetted() {
        let installation = Installation::new("replace-vet-skipped");
        let err = replace(
            &installation.exe,
            |_| Err("cut short".to_owned()),
            |_| panic!("nothing was written to vet"),
        )
        .unwrap_err();
        assert_eq!(err, ReplaceError::Filling("cut short".to_owned()));
    }

    #[test]
    fn a_directory_that_takes_no_new_file_is_refused_before_anything_is_written() {
        if ignores_modes() {
            return;
        }
        let installation = Installation::new("replace-readonly");
        let dir = installation.exe.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let err = replace(&installation.exe, |_| panic!("nothing to write"), vet_ok).unwrap_err();
        let replaceable = replaceable(&installation.exe);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(err, ReplaceError::NotWritable(path) if path == installation.exe));
        assert!(matches!(replaceable, Err(InstallError::NotWritable(_))));
    }

    #[test]
    fn a_rollback_puts_the_previous_file_back_and_keeps_what_was_there() {
        let installation = Installation::new("replace-rollback");
        replace(
            &installation.exe,
            |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
            vet_ok,
        )
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

    /// What a rollback to the replaced build checks against the database: the schema version
    /// this build reads, written with the replacement.
    #[test]
    fn the_record_names_the_schema_version_of_the_build_that_was_replaced() {
        let installation = Installation::new("install-schema");
        let host = Host::serving("");
        let key = test_key();
        install(
            &Installation::sources(&host, &key, is_running),
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap();
        let record = installation.files.read_installed().unwrap();
        assert_eq!(
            record.from_schema,
            Some(crate::store::Store::SCHEMA_VERSION)
        );
    }

    /// A probe that refuses, saying what it was given: whether the file is the unpacked daemon,
    /// the version it was told to expect, and whether the file is the installed path itself.
    fn refusing_probe(path: &Path, version: &str) -> Result<(), String> {
        let unpacked = std::fs::read(path).ok().as_deref() == Some(NEW_DAEMON);
        let installed = path.file_name().is_some_and(|name| name == "octoboardd");
        Err(format!("{unpacked} {version} {installed}"))
    }

    #[test]
    fn a_build_that_does_not_start_is_not_installed() {
        let installation = Installation::new("install-probe-refuses");
        let host = Host::serving("");
        let key = test_key();
        let sources = Sources {
            probe: refusing_probe,
            ..Installation::sources(&host, &key, is_running)
        };
        let err = install(
            &sources,
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::Replace(_)), "{err:?}");
        assert_eq!(err.code(), daemon_update_code::REPLACE_FAILED);
        let detail = err.detail();
        assert!(
            detail.starts_with("the new build did not start"),
            "{detail}"
        );
        // It was run where it was staged, as unpacked, and was told the release's version.
        assert!(detail.ends_with("true 9.9.9 false"), "{detail}");
        // Nothing was replaced, kept or recorded, and nothing of the staging is left.
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert_eq!(
            std::fs::read_dir(installation.exe.parent().unwrap())
                .unwrap()
                .count(),
            1
        );
        assert!(installation.files.read_installed().is_none());
        assert_eq!(
            std::fs::read_dir(installation.files.updates_dir())
                .unwrap()
                .count(),
            0
        );
    }

    /// The probe takes time, and the rename cannot be undone: a stop that began meanwhile keeps the
    /// build from being put in place.
    #[test]
    fn a_stop_that_begins_while_the_new_build_is_probed_keeps_it_from_being_installed() {
        static PROBED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        fn probe_then_stop(_: &Path, _: &str) -> Result<(), String> {
            PROBED.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        let installation = Installation::new("install-stop-after-probe");
        let host = Host::serving("");
        let key = test_key();
        let stopping = || PROBED.load(std::sync::atomic::Ordering::SeqCst);
        let sources = Sources {
            probe: probe_then_stop,
            stopping: &stopping,
            ..Installation::sources(&host, &key, is_running)
        };
        let err = install(
            &sources,
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(&err, InstallError::Replace(_)), "{err:?}");
        assert!(err.detail().contains("began to stop"), "{}", err.detail());
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert_eq!(
            std::fs::read_dir(installation.exe.parent().unwrap())
                .unwrap()
                .count(),
            1,
            "no previous file was kept and no staged file is left"
        );
        assert!(installation.files.read_installed().is_none());
    }

    /// The real probe against the archive's stand-in, which prints no version: the build is
    /// refused for naming none, and the installation is as it was.
    #[cfg(unix)]
    #[test]
    fn the_real_probe_refuses_a_build_that_names_no_version() {
        let installation = Installation::new("install-real-probe");
        let host = Host::serving("");
        let key = test_key();
        let sources = Sources {
            probe: super::super::probe::expect_version,
            ..Installation::sources(&host, &key, is_running)
        };
        let err = install(
            &sources,
            &installation.release(),
            &installation.exe,
            &installation.files,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, InstallError::Replace(_)), "{err:?}");
        let detail = err.detail();
        assert!(
            detail.starts_with("the new build did not start") && detail.contains("no version"),
            "{detail}"
        );
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert!(!installation
            .exe
            .with_file_name("octoboardd.previous")
            .exists());
        assert!(installation.files.read_installed().is_none());
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
            from_schema: None,
            path: "/opt/octoboardd".into(),
        };
        installation.files.write_installed(&record).unwrap();
        assert_eq!(installation.files.read_installed(), Some(record));
        installation.files.forget_installed().unwrap();
        assert!(installation.files.read_installed().is_none());
        std::fs::write(installation.data.join("update-installed.json"), "{").unwrap();
        assert!(installation.files.read_installed().is_none());
    }

    #[test]
    fn the_schema_version_a_build_read_is_recorded_and_read_back() {
        let installation = Installation::new("record-schema");
        let record = super::super::record::Installed {
            version: "0.2.0".into(),
            holder_floor: None,
            from_version: "0.1.0".into(),
            from_schema: Some(3),
            path: "/opt/octoboardd".into(),
        };
        installation.files.write_installed(&record).unwrap();
        assert_eq!(installation.files.read_installed(), Some(record));
    }

    /// A record an earlier build wrote names no schema version, and is still read.
    #[test]
    fn a_record_written_before_the_schema_version_was_recorded_still_reads() {
        let installation = Installation::new("record-old");
        std::fs::write(
            installation.data.join("update-installed.json"),
            r#"{
                "version": "0.2.0",
                "holder_floor": "0.1.0",
                "from_version": "0.1.0",
                "path": "/opt/octoboardd"
            }"#,
        )
        .unwrap();
        let record = installation
            .files
            .read_installed()
            .expect("the old record reads");
        assert_eq!(record.version, "0.2.0");
        assert_eq!(record.from_version, "0.1.0");
        assert_eq!(record.from_schema, None);
        assert_eq!(record.path, "/opt/octoboardd");
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
        probe: accept,
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
            code_of(&super::super::check_requested(&state).await.unwrap_err()),
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
        super::super::check_requested(&state).await.unwrap();
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
        super::super::check_requested(&state).await.unwrap();
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
        let first = super::super::check_requested(&state).await.unwrap();
        assert!(first.started);
        assert_eq!(
            first.last_check_at, None,
            "no check had ended before the first"
        );
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        let second = super::super::check_requested(&state).await.unwrap();
        assert!(!second.started, "the reply says that nothing started");
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Idle);
        assert_eq!(host.asked().len(), 1);
        // A forced one (the channel changed) is not held back by it.
        assert!(state.daemon_update.begin_check(&state, true).is_some());
    }

    #[tokio::test]
    async fn a_check_started_after_an_earlier_one_ended_names_the_time_that_one_ended() {
        let (state, _installation, _dir) = enabled("update-check-after", newer_host());
        let first = super::super::check_requested(&state).await.unwrap();
        assert_eq!(first.last_check_at, None);
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
                && phase(updates) == DaemonUpdatePhase::Idle
        })
        .await;
        let ended_at = state.daemon_update.last_check_at().unwrap();
        // The floor has passed without waiting for it.
        state.daemon_update.lock().last_ended = std::time::Instant::now()
            .checked_sub(super::super::CHECK_FLOOR + std::time::Duration::from_secs(1));
        let second = super::super::check_requested(&state).await.unwrap();
        assert!(second.started);
        assert_eq!(
            second.last_check_at,
            Some(ended_at),
            "the end of this check is the first status whose last check is another"
        );
        until(&state, |updates| phase(updates) == DaemonUpdatePhase::Idle).await;
    }

    #[tokio::test]
    async fn the_coordinator_answers_a_check_with_whether_it_started_and_an_install_with_when_it_began(
    ) {
        use crate::protocol::{Event, RequestBody};
        let (state, _installation, _dir) = enabled("update-replies", newer_host());
        let reply = crate::coordinator::handle(
            &state,
            Some("check-1".to_owned()),
            RequestBody::CheckDaemonUpdate,
        )
        .await
        .unwrap();
        assert!(
            matches!(
                &reply,
                Some(Event::DaemonUpdateCheck { id: Some(id), started: true, last_check_at: None }) if id == "check-1"
            ),
            "answered with daemon_update_check: {reply:?}"
        );
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
                && phase(updates) == DaemonUpdatePhase::Idle
        })
        .await;
        // Asked again within the floor: answered, with nothing started.
        let held = crate::coordinator::handle(
            &state,
            Some("check-2".to_owned()),
            RequestBody::CheckDaemonUpdate,
        )
        .await
        .unwrap();
        assert!(
            matches!(
                &held,
                Some(Event::DaemonUpdateCheck { id: Some(id), started: false, last_check_at: None }) if id == "check-2"
            ),
            "answered with daemon_update_check: {held:?}"
        );

        let reply = crate::coordinator::handle(
            &state,
            Some("install-1".to_owned()),
            RequestBody::UpdateDaemon,
        )
        .await
        .unwrap();
        let started_at = match &reply {
            Some(Event::DaemonUpdateStarted { id, started_at }) => {
                assert_eq!(id.as_deref(), Some("install-1"));
                *started_at
            }
            other => panic!("answered with daemon_update_started: {other:?}"),
        };
        until(&state, |updates| {
            phase(updates) == DaemonUpdatePhase::Installed
        })
        .await;
        assert_eq!(
            state
                .daemon_update
                .status()
                .unwrap()
                .install
                .map(|install| install.started_at),
            Some(started_at)
        );
    }

    #[tokio::test]
    async fn the_update_is_downloaded_verified_and_installed_and_restart_is_then_the_next_step() {
        let (state, installation, _dir) = enabled("update-install", newer_host());
        let mut events = state.subscribe();
        super::super::check_requested(&state).await.unwrap();
        until(&state, |updates| {
            updates.status().unwrap().available.is_some()
        })
        .await;
        let started_at = super::super::update_requested(&state).await.unwrap();
        until(&state, |updates| {
            phase(updates) == DaemonUpdatePhase::Installed
        })
        .await;

        let status = state.daemon_update.status().unwrap();
        assert_eq!(
            status.install.as_ref().map(|install| install.started_at),
            Some(started_at),
            "the reply names the install the status then tells of"
        );
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
        super::super::check_requested(&state).await.unwrap();
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

        super::super::check_requested(&state).await.unwrap();
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
        super::super::check_requested(&state).await.unwrap();
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
        super::super::check_requested(&state).await.unwrap();
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
        assert_eq!(
            manual.command,
            manifest::install_command_for(
                &crate::hostfs::canonical(&installation.exe).unwrap(),
                DaemonUpdateChannel::Stable
            )
        );
        assert!(!manual.command.contains("--beta"));
        assert!(status.available.is_some());

        let err = super::super::update_requested(&state).await.unwrap_err();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_NOT_WRITABLE);
        assert_eq!(param_of(&err, "path"), manual.path);
        assert_eq!(host.asked().len(), 1, "no download was made");
    }

    /// The command to run by hand follows the channel the daemon is on, at the check and at the
    /// refusal of an update (which reads the channel again).
    #[cfg(unix)]
    #[tokio::test]
    async fn the_command_to_upgrade_by_hand_follows_the_channel_of_the_daemon() {
        use std::os::unix::fs::PermissionsExt;
        if ignores_modes() {
            return;
        }
        let (state, installation, _dir) = enabled("update-manual-beta", newer_host());
        let real = crate::hostfs::canonical(&installation.exe).unwrap();
        let dir = installation.exe.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        state
            .store
            .set_daemon_update_channel(DaemonUpdateChannel::Beta)
            .unwrap();
        super::super::check_requested(&state).await.unwrap();
        until(&state, |updates| {
            updates.status().unwrap().last_check.is_some()
        })
        .await;
        let checked = state.daemon_update.status().unwrap().manual.unwrap();
        assert_eq!(
            checked.command,
            manifest::install_command_for(&real, DaemonUpdateChannel::Beta)
        );
        assert!(checked.command.ends_with(" --beta"), "{}", checked.command);

        // The channel the store names when the update is asked for is the one the command is for.
        state
            .store
            .set_daemon_update_channel(DaemonUpdateChannel::Stable)
            .unwrap();
        let err = super::super::update_requested(&state).await.unwrap_err();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(code_of(&err), error_code::DAEMON_UPDATE_NOT_WRITABLE);
        let refused = state.daemon_update.status().unwrap().manual.unwrap();
        assert_eq!(
            refused.command,
            manifest::install_command_for(&real, DaemonUpdateChannel::Stable)
        );
        assert!(!refused.command.contains("--beta"));
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
                from_schema: None,
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
                    from_schema: None,
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
            probe: accept,
        });
        super::super::check_requested(&state).await.unwrap();
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
                from_schema: None,
                path: crate::hostfs::canonical(&installation.exe)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            })
            .unwrap();
        super::super::check_requested(&state).await.unwrap();
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
        super::super::check_requested(&state).await.unwrap();
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
            super::super::check_requested(&state).await.unwrap();
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
            probe: accept,
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
            probe: accept,
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
            probe: accept,
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
        super::super::check_requested(&state).await.unwrap();
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
        super::super::check_requested(&state).await.unwrap();
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
        super::super::check_requested(&state).await.unwrap();
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
                from_schema: None,
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
                from_schema: None,
                path: crate::hostfs::canonical(&installation.exe)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            })
            .unwrap();
        let _ = super::super::update_requested(&state).await;
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Installed);
        super::super::check_requested(&state).await.unwrap();
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Installed);

        // `octoboardd update --rollback` put the running build back and forgot its record.
        ROLLED_BACK.store(true, Ordering::SeqCst);
        installation.files.forget_installed().unwrap();
        super::super::check_requested(&state).await.unwrap();
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
        replace(
            &installation.exe,
            |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
            vet_ok,
        )
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
            super::super::check_requested(&state).await.unwrap();
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
        super::super::check_requested(&state).await.unwrap();
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
                replace(
                    &target,
                    |file| {
                        std::io::Write::write_all(file, &round.to_le_bytes())
                            .map_err(|err| err.to_string())
                    },
                    vet_ok,
                )
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
            replace(
                &installation.exe,
                |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
                vet_ok,
            )
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
            let err = replace(
                &installation.exe,
                |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
                vet_ok,
            )
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

mod reading_answers {
    use super::super::probe::parse_answer;

    #[test]
    fn the_version_is_the_last_word_of_the_first_line() {
        for (answer, version) in [
            ("octoboardd 9.9.9\n", "9.9.9"),
            ("octoboardd 9.9.9", "9.9.9"),
            ("octoboardd 9.9.9\r\n", "9.9.9"),
            ("octoboardd 0.1.0-beta.2\n", "0.1.0-beta.2"),
            ("9.9.9\n", "9.9.9"),
            ("octoboardd 1.2.3\nsomething 4.5.6\n", "1.2.3"),
        ] {
            assert_eq!(parse_answer(answer).as_deref(), Some(version), "{answer:?}");
        }
    }

    #[test]
    fn an_answer_that_names_no_version_gives_none() {
        for answer in [
            "",
            "\n",
            "\n9.9.9\n",
            "octoboardd\n",
            "octoboardd nine\n",
            "usage: octoboardd [OPTIONS]\n",
            "error: unexpected argument '--version' found\n",
            "octoboardd 9.9\n",
        ] {
            assert_eq!(parse_answer(answer), None, "{answer:?}");
        }
    }
}

/// The real probe, against stand-in builds: shell scripts that behave as a build does, or does
/// not, when run with `--version`.
#[cfg(unix)]
mod probing {
    use super::super::probe::{expect_version, reported_version};
    use super::*;

    /// A build at `dir/<name>` that runs `body` (see `fake_agent`: one dispatcher, a body of its
    /// own per build).
    fn build(dir: &ScratchDir, name: &str, body: &str) -> PathBuf {
        crate::test_support::fake_agent(dir, name, body);
        dir.join(name)
    }

    /// Reads the version of `exe`, given all the time a slow machine may need.
    fn version_of(exe: &Path) -> Result<String, String> {
        reported_version(exe, PATIENCE)
    }

    /// Exits 3, 4 or 5 unless it was run as the update runs it: `--version` and nothing else, from
    /// the root directory, with nothing on stdin.
    const AS_RUN_BY_THE_UPDATE: &str = r#"
[ "$#" = 1 ] && [ "$1" = "--version" ] || exit 3
[ "$(pwd -P)" = / ] || exit 4
if read -r _line; then exit 5; fi
echo "noise on stderr" >&2
echo "octoboardd 9.9.9"
"#;

    #[test]
    fn a_build_run_with_dash_dash_version_reports_the_version_it_prints() {
        let dir = ScratchDir::new("probe-ok");
        let exe = build(&dir, "octoboardd", AS_RUN_BY_THE_UPDATE);
        assert_eq!(version_of(&exe), Ok("9.9.9".to_owned()));
    }

    #[test]
    fn only_the_first_line_of_the_answer_counts() {
        let dir = ScratchDir::new("probe-first-line");
        let exe = build(
            &dir,
            "octoboardd",
            "echo 'octoboardd 1.2.3'\necho 'octoboardd 4.5.6'\n",
        );
        assert_eq!(version_of(&exe), Ok("1.2.3".to_owned()));
    }

    #[test]
    fn a_build_that_fails_is_not_taken_at_the_version_it_printed() {
        let dir = ScratchDir::new("probe-fails");
        let exe = build(&dir, "octoboardd", "echo 'octoboardd 9.9.9'\nexit 1\n");
        let err = version_of(&exe).unwrap_err();
        assert!(err.contains("failed"), "{err}");
    }

    /// A build older than the flag rejects it as an unknown argument.
    #[test]
    fn a_build_that_rejects_the_flag_does_not_start_as_far_as_the_update_can_tell() {
        let dir = ScratchDir::new("probe-rejects");
        let exe = build(
            &dir,
            "octoboardd",
            "echo \"error: unexpected argument '--version' found\" >&2\nexit 2\n",
        );
        assert!(version_of(&exe).unwrap_err().contains("failed"));
    }

    /// The answer is read to 4096 bytes: a build that says more is not one that answers `--version`.
    #[test]
    fn a_build_that_says_far_more_than_a_version_is_refused() {
        let dir = ScratchDir::new("probe-flood");
        let exe = build(
            &dir,
            "octoboardd",
            "echo 'octoboardd 9.9.9'\ni=0\nwhile [ $i -lt 600 ]; do echo 0123456789; i=$((i+1)); done\n",
        );
        let err = version_of(&exe).unwrap_err();
        assert!(err.contains("failed"), "{err}");
    }

    #[test]
    fn a_build_that_names_no_version_is_refused() {
        let dir = ScratchDir::new("probe-silent");
        let silent = build(&dir, "silent", "exit 0\n");
        let err = version_of(&silent).unwrap_err();
        assert!(err.contains("no version"), "{err}");
        let nonsense = build(&dir, "nonsense", "echo 'octoboardd nine'\n");
        assert!(version_of(&nonsense).unwrap_err().contains("no version"));
    }

    #[test]
    fn a_build_that_does_not_answer_in_time_is_stopped_and_refused() {
        let dir = ScratchDir::new("probe-timeout");
        // `exec`, so that the sleep is the process the probe stops.
        let exe = build(&dir, "octoboardd", "exec sleep 600\n");
        let err = reported_version(&exe, Duration::from_millis(300)).unwrap_err();
        assert!(err.contains("did not answer"), "{err}");
    }

    #[test]
    fn a_file_that_cannot_be_run_is_refused() {
        let dir = ScratchDir::new("probe-missing");
        let err = version_of(&dir.join("not-there")).unwrap_err();
        assert!(err.contains("could not be run"), "{err}");
        let plain = dir.join("plain");
        std::fs::write(&plain, "octoboardd 9.9.9\n").unwrap();
        let err = version_of(&plain).unwrap_err();
        assert!(err.contains("could not be run"), "{err}");
    }

    #[test]
    fn the_version_a_build_reports_has_to_be_the_one_the_release_names() {
        let dir = ScratchDir::new("probe-expect");
        let exe = build(&dir, "octoboardd", "echo 'octoboardd 9.9.9'\n");
        assert_eq!(expect_version(&exe, "9.9.9"), Ok(()));
        let err = expect_version(&exe, "1.0.0").unwrap_err();
        assert!(err.contains("9.9.9") && err.contains("1.0.0"), "{err}");
        let silent = build(&dir, "silent", "exit 0\n");
        assert!(expect_version(&silent, "9.9.9").is_err());
        assert!(expect_version(&dir.join("not-there"), "9.9.9").is_err());
    }
}

/// A database file that says it is at schema version `version`.
fn database_at(path: &Path, version: i64) {
    let conn = rusqlite::Connection::open(path).expect("database");
    conn.pragma_update(None, "user_version", version)
        .expect("version");
}

/// A copy of the database at `database` from before a migration from schema version `from`,
/// last changed `seconds` after the epoch; `suffix` follows `.pre-v<from>` in its name.
fn migration_backup(database: &Path, from: i64, suffix: &str, seconds: u64) -> PathBuf {
    let path = database.with_file_name(format!("octoboard.db.pre-v{from}{suffix}"));
    std::fs::write(&path, b"a copy").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(seconds))
        .unwrap();
    path
}

/// What an update that replaced the file at `installation.exe` recorded, naming the schema
/// version of the replaced build as `from_schema`.
fn record_update(installation: &Installation, from_schema: Option<i64>) {
    installation
        .files
        .write_installed(&super::record::Installed {
            version: VERSION.into(),
            holder_floor: None,
            from_version: "0.1.0".into(),
            from_schema,
            path: crate::hostfs::canonical(&installation.exe)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        })
        .unwrap();
}

mod rolling_back {
    use super::super::rollback::{previous_fits_database, previous_version, DatabaseFit};
    use super::*;

    fn database_of(installation: &Installation) -> PathBuf {
        installation.data.join("octoboard.db")
    }

    fn real_of(installation: &Installation) -> PathBuf {
        crate::hostfs::canonical(&installation.exe).unwrap()
    }

    fn fit(installation: &Installation) -> DatabaseFit {
        previous_fits_database(
            &installation.files,
            &real_of(installation),
            &database_of(installation),
        )
    }

    #[test]
    fn with_no_database_yet_any_build_reads_it() {
        let installation = Installation::new("rollback-no-database");
        assert_eq!(fit(&installation), DatabaseFit::Reads);
        record_update(&installation, Some(1));
        assert_eq!(fit(&installation), DatabaseFit::Reads);
    }

    #[test]
    fn nothing_says_what_the_previous_build_reads_without_a_record_naming_that_file_and_a_version()
    {
        let installation = Installation::new("rollback-unknown");
        database_at(&database_of(&installation), 4);
        // No record at all.
        assert_eq!(fit(&installation), DatabaseFit::Unknown);
        // A record from before the schema version was recorded.
        record_update(&installation, None);
        assert_eq!(fit(&installation), DatabaseFit::Unknown);
        // A record of the replacement of another file.
        installation
            .files
            .write_installed(&super::super::record::Installed {
                version: VERSION.into(),
                holder_floor: None,
                from_version: "0.1.0".into(),
                from_schema: Some(4),
                path: "/somewhere/else/octoboardd".into(),
            })
            .unwrap();
        assert_eq!(fit(&installation), DatabaseFit::Unknown);
    }

    /// A file is there, but the version it is at cannot be read: nothing can be said of it.
    #[test]
    fn a_database_that_cannot_be_read_is_not_known_to_be_read() {
        let installation = Installation::new("rollback-unreadable");
        std::fs::write(
            database_of(&installation),
            b"this is not a database, and is long enough to be read as one",
        )
        .unwrap();
        record_update(&installation, Some(3));
        assert_eq!(fit(&installation), DatabaseFit::Unknown);
    }

    #[test]
    fn a_database_at_or_below_what_the_previous_build_reads_is_read() {
        let installation = Installation::new("rollback-reads");
        record_update(&installation, Some(3));
        for version in [1, 3] {
            database_at(&database_of(&installation), version);
            assert_eq!(fit(&installation), DatabaseFit::Reads, "version {version}");
        }
    }

    #[test]
    fn a_database_above_what_the_previous_build_reads_is_too_new_and_the_copies_are_listed() {
        let installation = Installation::new("rollback-too-new");
        let database = database_of(&installation);
        database_at(&database, 4);
        record_update(&installation, Some(3));
        assert_eq!(
            fit(&installation),
            DatabaseFit::TooNew {
                database: 4,
                reads: 3,
                backups: Vec::new()
            }
        );

        let older = migration_backup(&database, 3, "", 1_000);
        let newer = migration_backup(&database, 3, "-1700000000", 2_000);
        // Not copies of this migration: another version, a copy never finished, a log.
        migration_backup(&database, 2, "", 3_000);
        migration_backup(&database, 3, ".partial", 3_000);
        migration_backup(&database, 3, "-wal", 3_000);
        assert_eq!(
            fit(&installation),
            DatabaseFit::TooNew {
                database: 4,
                reads: 3,
                backups: vec![newer, older]
            }
        );
    }

    #[test]
    fn the_version_of_the_previous_build_is_the_one_the_record_names() {
        let installation = Installation::new("rollback-version-record");
        // No `.previous` file is run, or even there.
        record_update(&installation, Some(3));
        assert_eq!(
            previous_version(&installation.files, &real_of(&installation)),
            Some("0.1.0".to_owned())
        );
    }

    #[test]
    fn without_a_record_the_version_is_what_the_previous_build_says() {
        let installation = Installation::new("rollback-version-asked");
        let real = real_of(&installation);
        assert_eq!(
            previous_version(&installation.files, &real),
            None,
            "no previous file"
        );

        // A record of another file names nothing of this one.
        installation
            .files
            .write_installed(&super::super::record::Installed {
                version: VERSION.into(),
                holder_floor: None,
                from_version: "7.7.7".into(),
                from_schema: Some(3),
                path: "/somewhere/else/octoboardd".into(),
            })
            .unwrap();
        // A previous file that cannot be run names no version.
        let previous = installation.exe.with_file_name("octoboardd.previous");
        std::fs::write(&previous, OLD_DAEMON).unwrap();
        assert_eq!(previous_version(&installation.files, &real), None);

        #[cfg(unix)]
        {
            std::fs::remove_file(&previous).unwrap();
            crate::test_support::fake_agent(
                installation.exe.parent().unwrap(),
                "octoboardd.previous",
                "echo 'octoboardd 8.8.8'\n",
            );
            assert_eq!(
                previous_version(&installation.files, &real),
                Some("8.8.8".to_owned())
            );
        }
    }
}

/// The next daemon that does not start after an update: whether the build that ran is put back.
mod falling_back {
    use super::super::successor::fall_back;
    use super::*;

    /// An installation whose file at the path is not the build that runs: it holds what an update
    /// put there ([`replace`] kept the build that runs as the previous file, [`OLD_DAEMON`]), and
    /// the update was recorded.
    fn updated(label: &str) -> Installation {
        let installation = Installation::new(label);
        replace(
            &installation.exe,
            |file| std::io::Write::write_all(file, b"new").map_err(|err| err.to_string()),
            vet_ok,
        )
        .unwrap();
        record_update(&installation, Some(crate::store::Store::SCHEMA_VERSION));
        installation
    }

    fn database_of(installation: &Installation) -> PathBuf {
        installation.data.join("octoboard.db")
    }

    fn previous_of(installation: &Installation) -> PathBuf {
        installation.exe.with_file_name("octoboardd.previous")
    }

    fn fall_back_over(installation: &Installation, is_running_image: fn(&Path) -> bool) -> bool {
        fall_back(
            &installation.exe,
            &installation.files,
            &database_of(installation),
            is_running_image,
        )
    }

    /// The installation as it stands: the file at the path, the previous one and the record.
    fn stands(installation: &Installation) -> (Vec<u8>, Option<Vec<u8>>, bool) {
        (
            std::fs::read(&installation.exe).unwrap(),
            std::fs::read(previous_of(installation)).ok(),
            installation.files.read_installed().is_some(),
        )
    }

    #[test]
    fn the_build_that_ran_is_put_back_and_the_update_kept_as_the_previous_one() {
        let installation = updated("fall-back-ok");
        assert!(fall_back_over(&installation, is_running_old));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
        assert_eq!(std::fs::read(previous_of(&installation)).unwrap(), b"new");
        assert!(
            installation.files.read_installed().is_none(),
            "the record no longer describes the file"
        );
        // The update lock was let go again.
        assert!(crate::platform::fs::try_lock_file(&installation.files.lock()).is_ok());
    }

    #[test]
    fn a_database_the_previous_build_reads_does_not_stop_the_fall_back() {
        let installation = updated("fall-back-reads");
        database_at(
            &database_of(&installation),
            crate::store::Store::SCHEMA_VERSION,
        );
        assert!(fall_back_over(&installation, is_running_old));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
    }

    /// The build to go back to is the one that runs, whose schema version is its own: no record of
    /// the update is needed, and none is consulted.
    #[test]
    fn what_the_update_recorded_does_not_matter_to_the_fall_back() {
        let installation = updated("fall-back-no-record");
        installation.files.forget_installed().unwrap();
        database_at(
            &database_of(&installation),
            crate::store::Store::SCHEMA_VERSION,
        );
        assert!(fall_back_over(&installation, is_running_old));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);

        // Even a record that says the previous build read a newer schema than the database is at.
        let installation = updated("fall-back-odd-record");
        record_update(&installation, Some(crate::store::Store::SCHEMA_VERSION + 5));
        assert!(fall_back_over(&installation, is_running_old));
    }

    #[test]
    fn a_database_that_cannot_be_read_does_not_stop_the_fall_back() {
        let installation = updated("fall-back-unreadable");
        std::fs::write(
            database_of(&installation),
            b"this is not a database, and is long enough to be read as one",
        )
        .unwrap();
        assert!(fall_back_over(&installation, is_running_old));
        assert_eq!(std::fs::read(&installation.exe).unwrap(), OLD_DAEMON);
    }

    /// Without a record to say so, a database above what this build reads still stops the fall
    /// back.
    #[test]
    fn a_database_above_what_the_build_that_ran_reads_stops_it_with_or_without_a_record() {
        let installation = updated("fall-back-too-new-no-record");
        installation.files.forget_installed().unwrap();
        database_at(
            &database_of(&installation),
            crate::store::Store::SCHEMA_VERSION + 1,
        );
        let before = stands(&installation);
        assert!(!fall_back_over(&installation, is_running_old));
        assert_eq!(stands(&installation), before);
        assert_eq!(before.0, b"new");
    }

    #[test]
    fn a_database_migrated_beyond_what_the_previous_build_reads_is_not_gone_back_to() {
        let installation = updated("fall-back-too-new");
        database_at(
            &database_of(&installation),
            crate::store::Store::SCHEMA_VERSION + 1,
        );
        let before = stands(&installation);
        assert!(!fall_back_over(&installation, is_running_old));
        assert_eq!(stands(&installation), before);
        assert_eq!(before.0, b"new");
    }

    #[test]
    fn the_file_at_the_path_being_the_build_that_ran_leaves_nothing_to_go_back_to() {
        let installation = updated("fall-back-is-running");
        // Whatever is kept beside it, the build that ran is the one that did not start.
        std::fs::write(previous_of(&installation), OLD_DAEMON).unwrap();
        std::fs::write(&installation.exe, OLD_DAEMON).unwrap();
        let before = stands(&installation);
        assert!(!fall_back_over(&installation, is_running_old));
        assert_eq!(stands(&installation), before);
    }

    #[test]
    fn a_previous_file_that_is_not_the_build_that_ran_is_not_put_back() {
        let installation = updated("fall-back-other-previous");
        std::fs::write(previous_of(&installation), b"some other build").unwrap();
        let before = stands(&installation);
        assert!(!fall_back_over(&installation, is_running_old));
        assert_eq!(stands(&installation), before);
    }

    #[test]
    fn with_no_previous_file_there_is_nothing_to_put_back() {
        let installation = updated("fall-back-no-previous");
        std::fs::remove_file(previous_of(&installation)).unwrap();
        let before = stands(&installation);
        assert!(!fall_back_over(&installation, is_running_old));
        assert_eq!(stands(&installation), before);
    }

    #[test]
    fn a_launch_path_that_leads_nowhere_is_not_gone_back_over() {
        let installation = updated("fall-back-no-launch");
        std::fs::remove_file(&installation.exe).unwrap();
        assert!(!fall_back_over(&installation, is_running_old));
        assert_eq!(
            std::fs::read(previous_of(&installation)).unwrap(),
            OLD_DAEMON
        );
    }

    #[test]
    fn nothing_is_put_back_while_another_process_holds_the_update_lock() {
        let installation = updated("fall-back-locked");
        let held = crate::platform::fs::try_lock_file(&installation.files.lock()).unwrap();
        let before = stands(&installation);
        assert!(!fall_back_over(&installation, is_running_old));
        assert_eq!(stands(&installation), before);
        drop(held);
        // The lock is an `flock`, held by the open file description: a child another test forks
        // in the meantime shares it until that child execs, so it can outlive `held` for a moment
        // under the suite's load. A fall back refused meanwhile changes nothing, so it is asked
        // again until the lock is free.
        let deadline = std::time::Instant::now() + PATIENCE;
        while !fall_back_over(&installation, is_running_old) {
            assert!(
                std::time::Instant::now() < deadline,
                "nothing was put back once the lock was let go"
            );
            assert_eq!(stands(&installation), before);
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_swap_that_fails_leaves_the_record_and_the_files_and_reports_it() {
        use std::os::unix::fs::PermissionsExt;
        if ignores_modes() {
            return;
        }
        let installation = updated("fall-back-readonly");
        let dir = installation.exe.parent().unwrap().to_path_buf();
        let before = stands(&installation);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let fell_back = fall_back_over(&installation, is_running_old);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!fell_back);
        assert_eq!(stands(&installation), before);
    }
}

/// The executable the helpers of a launch run.
mod helper_executable {
    use super::super::helpers::local_helper_exe;
    use super::*;
    use crate::holder::build::Builds;

    #[test]
    fn a_daemon_that_does_not_update_itself_is_given_its_own_path_while_it_is_the_running_build() {
        let installation = Installation::new("helper-running");
        let builds_dir = installation.data.join("builds");
        let exe = installation.exe.to_string_lossy().into_owned();
        let helper = local_helper_exe(&exe, false, is_running, &Builds::new(builds_dir.clone()));
        assert_eq!(helper, exe);
        assert!(!builds_dir.exists(), "no copy was made");
    }

    #[test]
    fn a_daemon_that_does_not_update_itself_is_given_a_copy_once_the_path_leads_to_another_build() {
        let installation = Installation::new("helper-copy");
        let builds_dir = installation.data.join("builds");
        let exe = installation.exe.to_string_lossy().into_owned();
        let helper = PathBuf::from(local_helper_exe(
            &exe,
            false,
            is_not_running,
            &Builds::new(builds_dir.clone()),
        ));
        assert_ne!(helper, installation.exe);
        assert!(helper.starts_with(&builds_dir), "{}", helper.display());
        assert_eq!(std::fs::read(&helper).unwrap(), OLD_DAEMON);
        assert_eq!(helper.file_name(), installation.exe.file_name());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&helper).unwrap().permissions().mode();
            assert_ne!(mode & 0o111, 0, "the copy can be run");
        }
    }

    /// A daemon that updates itself is given a copy from the first launch on, since the file at its
    /// path may be replaced at any time, even before the path is looked at.
    #[test]
    fn a_daemon_that_updates_itself_is_always_given_a_copy() {
        let installation = Installation::new("helper-updates");
        let builds_dir = installation.data.join("builds");
        let exe = installation.exe.to_string_lossy().into_owned();
        let helper = PathBuf::from(local_helper_exe(
            &exe,
            true,
            is_running,
            &Builds::new(builds_dir.clone()),
        ));
        assert_ne!(helper, installation.exe);
        assert!(helper.starts_with(&builds_dir), "{}", helper.display());
        assert_eq!(std::fs::read(&helper).unwrap(), OLD_DAEMON);
    }

    /// An update then replaces the file at the path: the helpers go on with the copy.
    #[test]
    fn the_copy_is_reused_and_does_not_follow_the_file_at_the_path() {
        let installation = Installation::new("helper-reuse");
        let builds = Builds::new(installation.data.join("builds"));
        let exe = installation.exe.to_string_lossy().into_owned();
        let first = local_helper_exe(&exe, true, is_running, &builds);
        std::fs::write(&installation.exe, NEW_DAEMON).unwrap();
        let second = local_helper_exe(&exe, true, is_running, &builds);
        assert_eq!(first, second);
        assert_eq!(std::fs::read(&second).unwrap(), OLD_DAEMON);
    }

    #[test]
    fn where_no_copy_can_be_made_the_path_stays() {
        let installation = Installation::new("helper-no-copy");
        // A file where the directory of copies should be.
        let builds_dir = installation.data.join("builds");
        std::fs::write(&builds_dir, b"in the way").unwrap();
        let exe = installation.exe.to_string_lossy().into_owned();
        for updates in [false, true] {
            assert_eq!(
                local_helper_exe(
                    &exe,
                    updates,
                    is_not_running,
                    &Builds::new(builds_dir.clone())
                ),
                exe
            );
        }
    }
}

/// The real fetcher, as far as it can be driven without a TLS server (none is made here):
/// that it is held to HTTPS and tells an unreachable host. What the stand-in `Host` of the other
/// tests replaces, and these do not reach: the status of an answer, the limit on its size, a
/// redirect to plain HTTP and the stall of a body over a real connection.
mod real_fetcher {
    use super::*;
    use std::net::TcpListener;

    fn get(url: &str) -> (Result<(), FetchError>, Vec<u8>) {
        let mut body = Vec::new();
        let result = super::super::fetch::UreqFetcher::command_line().get(
            url,
            1024,
            Duration::from_secs(10),
            &mut |chunk, _| {
                body.extend_from_slice(chunk);
                Ok(())
            },
        );
        (result, body)
    }

    #[test]
    fn a_plain_http_address_is_refused_before_any_connection_is_made() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/manifest.json", listener.local_addr().unwrap());
        let (result, body) = get(&url);
        let err = result.unwrap_err();
        assert!(
            matches!(&err, FetchError::Failed(text) if text.contains("https")),
            "{err:?}"
        );
        assert!(body.is_empty());
        let accepted = listener.accept();
        assert!(
            matches!(&accepted, Err(err) if err.kind() == std::io::ErrorKind::WouldBlock),
            "something connected: {accepted:?}"
        );
    }

    #[test]
    fn a_host_that_cannot_be_reached_is_a_failure_to_reach_it_not_a_status() {
        let closed = crate::test_support::ClosedPort::new();
        let (result, body) = get(&format!("https://127.0.0.1:{}/manifest.json", closed.port));
        assert!(matches!(result, Err(FetchError::Failed(_))), "{result:?}");
        assert!(body.is_empty());
    }
}

/// The claim to work on the update, which a thread that panics lets go of.
mod claims {
    use super::*;

    /// A state whose last check found a release, with no claim held.
    fn found(label: &str) -> (Arc<AppState>, Installation, ScratchDir) {
        let (state, installation, dir) = enabled(label, newer_host());
        let claim = state.daemon_update.begin_check(&state, true).unwrap();
        assert!(state
            .daemon_update
            .record_check(&claim, Ok(Some(installation.release())), None));
        drop(claim);
        (state, installation, dir)
    }

    fn assert_failed_as_an_internal_error(state: &AppState) {
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.phase, DaemonUpdatePhase::Idle);
        assert_eq!(status.progress, None);
        let install = status.install.unwrap();
        assert_eq!(install.state, DaemonUpdateInstallState::Failed);
        assert_eq!(install.error, Some(daemon_update_code::REPLACE_FAILED));
        assert_eq!(install.detail.as_deref(), Some("internal error"));
        assert!(install.finished_at.is_some());
    }

    #[tokio::test]
    async fn a_claim_dropped_while_downloading_ends_the_install_as_failed_and_frees_the_update() {
        let (state, _installation, _dir) = found("claim-drop-download");
        let (claim, _) = state.daemon_update.begin_install(&state).unwrap();
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Downloading);
        drop(claim);
        assert_failed_as_an_internal_error(&state);
        // Nothing is held: the user may check, and install again.
        assert!(state.daemon_update.begin_install(&state).is_ok());
    }

    /// The download runs on a thread that can panic: its claim goes with it.
    #[tokio::test]
    async fn a_download_that_panics_ends_the_install_as_failed() {
        let (state, _installation, _dir) = found("claim-panic-download");
        let (claim, _) = state.daemon_update.begin_install(&state).unwrap();
        let ended = std::thread::spawn(move || {
            let _claim = claim;
            panic!("the download went wrong");
        })
        .join();
        assert!(ended.is_err());
        assert_failed_as_an_internal_error(&state);
        assert!(state.daemon_update.begin_check(&state, true).is_some());
    }

    #[tokio::test]
    async fn a_claim_dropped_while_checking_goes_back_to_idle() {
        let (state, _installation, _dir) = enabled("claim-drop-check", newer_host());
        let claim = state.daemon_update.begin_check(&state, true).unwrap();
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Checking);
        drop(claim);
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.phase, DaemonUpdatePhase::Idle);
        assert_eq!(status.install, None, "a check leaves no install behind");
    }

    #[tokio::test]
    async fn a_claim_dropped_after_the_install_ended_does_not_undo_it() {
        let (state, installation, _dir) = found("claim-drop-after-end");
        let (claim, release) = state.daemon_update.begin_install(&state).unwrap();
        assert_eq!(release.version, installation.release().version);
        state
            .daemon_update
            .record_installed(&claim, &release, Ok(true));
        drop(claim);
        let status = state.daemon_update.status().unwrap();
        assert_eq!(status.phase, DaemonUpdatePhase::Installed);
        assert_eq!(
            status.install.unwrap().state,
            DaemonUpdateInstallState::Succeeded
        );
    }
}

/// A fetcher that waits to be released, telling when a request has reached it.
struct Stuck {
    entered: Mutex<std::sync::mpsc::Sender<()>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Fetcher for Stuck {
    fn get(&self, _: &str, _: u64, _: Duration, _: &mut Sink<'_>) -> Result<(), FetchError> {
        self.entered.lock().unwrap().send(()).ok();
        // Released by a message or by the sender's end.
        let _ = self.release.lock().unwrap().recv();
        Err(FetchError::Failed("released".to_owned()))
    }
}

/// A fetcher that panics, as a check does that finds the world gone wrong, telling that a request
/// has reached it.
struct Panicking(Mutex<std::sync::mpsc::Sender<()>>);

impl Panicking {
    fn new() -> (Arc<Self>, std::sync::mpsc::Receiver<()>) {
        let (entered, reached) = std::sync::mpsc::channel();
        (Arc::new(Self(Mutex::new(entered))), reached)
    }
}

impl Fetcher for Panicking {
    fn get(&self, _: &str, _: u64, _: Duration, _: &mut Sink<'_>) -> Result<(), FetchError> {
        self.0.lock().unwrap().send(()).ok();
        panic!("the check went wrong");
    }
}

/// The check runs on a thread of its own, not on the runtime's blocking pool.
mod check_thread {
    use super::*;

    /// Lets the stuck fetcher go when it is dropped, so that a test that fails leaves nothing
    /// waiting.
    struct Release(std::sync::mpsc::Sender<()>);

    impl Drop for Release {
        fn drop(&mut self) {
            self.0.send(()).ok();
        }
    }

    /// A runtime whose blocking pool is one thread, and what the tests below put in it.
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap()
    }

    /// A daemon at the end of its life must not wait for a check that waits on the network.
    #[test]
    fn a_check_stuck_on_the_network_does_not_hold_the_runtime_at_its_end() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let _release = Release(release_tx);
        let stuck = Arc::new(Stuck {
            entered: Mutex::new(entered_tx),
            release: Mutex::new(release_rx),
        });
        let rt = runtime();
        let _state_and_dir = rt.block_on(async {
            let (state, _installation, dir) =
                enabled_running("check-thread-stuck", stuck, is_running_old);
            super::super::check_requested(&state).await.unwrap();
            entered_rx
                .recv_timeout(PATIENCE)
                .expect("the check reached the fetcher");
            (state, dir)
        });
        let (ended_tx, ended_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(rt);
            ended_tx.send(()).ok();
        });
        ended_rx
            .recv_timeout(PATIENCE)
            .expect("the runtime ended without waiting for the check");
    }

    /// Where the blocking pool has no thread to spare, the checks still run.
    #[test]
    fn checks_run_while_the_runtime_has_no_blocking_thread_to_spare() {
        let rt = runtime();
        rt.block_on(async {
            let host = newer_host();
            let (state, _installation, _dir) = enabled("check-thread-pool", host.clone());
            let (hold_tx, hold_rx) = std::sync::mpsc::channel::<()>();
            let held = tokio::task::spawn_blocking(move || hold_rx.recv().ok());

            super::super::check_requested(&state).await.unwrap();
            until(&state, |updates| {
                updates.status().unwrap().last_check.is_some()
            })
            .await;
            assert_eq!(host.asked().len(), 1);

            // The schedule's look returns once its check has run.
            scheduled_check(&state, true).await;
            assert_eq!(host.asked().len(), 2);

            hold_tx.send(()).unwrap();
            held.await.unwrap();
        });
    }

    #[tokio::test]
    async fn a_scheduled_check_that_panics_still_ends_and_lets_go_of_its_claim() {
        let (fetcher, _reached) = Panicking::new();
        let (state, _installation, _dir) =
            enabled_running("check-thread-panic", fetcher, is_running_old);
        scheduled_check(&state, true).await;
        assert_eq!(phase(&state.daemon_update), DaemonUpdatePhase::Idle);
        assert_eq!(state.daemon_update.status().unwrap().last_check, None);
        assert!(state.daemon_update.begin_check(&state, true).is_some());
    }

    #[tokio::test]
    async fn a_requested_check_that_panics_lets_go_of_its_claim() {
        let (fetcher, reached) = Panicking::new();
        let (state, _installation, _dir) =
            enabled_running("check-thread-requested-panic", fetcher, is_running_old);
        super::super::check_requested(&state).await.unwrap();
        reached
            .recv_timeout(PATIENCE)
            .expect("the check reached the fetcher");
        // Another check can be claimed once the thread has unwound.
        let deadline = std::time::Instant::now() + PATIENCE;
        while state.daemon_update.begin_check(&state, true).is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "the claim was never let go of"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(state.daemon_update.status().unwrap().last_check, None);
    }
}
