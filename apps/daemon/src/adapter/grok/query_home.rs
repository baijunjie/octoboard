//! The directories Grok Build's background runs are made against in place of an account's own:
//! its quota reading, its model listing and its kick.
//!
//! Run against a `GROK_HOME` it has not run in before, Grok Build changes that home's
//! configuration behind the user's back: on 1.0.92 `grok agent --no-leader stdio`, `grok models`
//! and a headless turn (`grok -p …`, before any request is sent) each append `[marketplace]
//! default_skills_installs_purged = true` to its `config.toml`, writing through a link
//! (`docs/agent-cli-reference.md`, "What the background runs write into an account's
//! configuration"). So each of these runs is made against a directory of Octoboard's own that
//! holds a copy of the account's `config.toml`, which names the account's endpoints and any key,
//! and nothing else of the account's; the sign-in is reached through `GROK_AUTH_PATH`, which names
//! the account's own `auth.json`. Grok takes that file's lock beside it (`auth.json.lock`, measured
//! on 1.0.92), so a sign-in refreshed here and one refreshed by the user's own `grok` wait on each
//! other, and the refreshed sign-in is saved where the account keeps it. Measured on 1.0.92: a
//! reading made this way answers `initialize` and `_x.ai/billing` as the account and leaves the
//! account's directory byte for byte as it was. Whatever Grok writes as it runs (its
//! documentation, logs, caches, the session of a kick, the marker above) lands in that directory.
//!
//! A directory is kept between runs, one per account and kind of run, so what Grok lays down in a
//! home it first runs in is laid down once. Its `config.toml` is copied afresh before every run,
//! which is what picks up an edit of the account's; the copy of a file that lacks Grok's marker
//! has it appended again by that run, and Grok's first-run step with it. Measured on 1.0.92 in a
//! scratch user home holding skills, plugins and a `~/.grok` of the user's own: four such runs in
//! a row changed nothing outside the run's `GROK_HOME`, so the repeat is left as it is. What does
//! grow there is Grok's log, some ten kilobytes a run, so the directory's `logs` is removed before
//! every run. A lock file in the directory is held for the whole run, so two runs of one kind for
//! one account, which nothing above this module keeps apart, take turns rather than rewrite the
//! copy under each other; the wait for it ends at the run's own deadline, or when the run is
//! cancelled. The directories of an account go when the account is removed or pointed at another
//! directory ([`forget`]).
//!
//! A kick is a headless turn, so it leaves a session of its own behind, and one more with every
//! kick; nothing reads them. What a kick wrote is therefore removed as soon as the kick has ended,
//! the whole of the directory but its lock ([`clear`]), so Grok lays the home down again at the
//! next kick. The kick writes runtime data alongside the conversation. The directory is cleared
//! as a whole so cleanup does not depend on the set of files a particular Grok release writes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};

use super::home::{copy_private_file, login_store_env};
use crate::adapter::account_cli::{BackgroundRun, RunHome, RunWait};
use crate::platform::fs::{create_private_dir, try_lock_file, Busy};
use crate::providers::grok::GROK_CONFIG_FILE;
use crate::session::wait::POLL;

/// The lock file of a run's directory, held while the run is made.
const LOCK_FILE: &str = ".octoboard-run.lock";

/// The file beside an account's directories that names the account's directory as Octoboard was
/// given it, so that the directories are found again once that directory is gone or leads
/// elsewhere ([`forget`]).
const ACCOUNT_FILE: &str = ".octoboard-account";

/// Grok's log directory in a home, which only grows.
const LOGS_DIR: &str = "logs";

/// The name of the directory of one kind of run, under the account's.
fn run_name(run: BackgroundRun) -> &'static str {
    match run {
        BackgroundRun::QuotaReading => "quota",
        BackgroundRun::ModelListing => "models",
        BackgroundRun::Kick => "kick",
    }
}

/// Where the directories of the account at `account_dir` are kept under `run_homes`: named by a
/// digest of the directory as it really is, keyed as directories are compared
/// ([`crate::hostfs::dir_key`]), so that two spellings of one directory share them and no path has
/// to be made safe as a name.
fn account_homes(run_homes: &Path, account_dir: &Path) -> PathBuf {
    let resolved =
        crate::hostfs::canonical(account_dir).unwrap_or_else(|_| account_dir.to_path_buf());
    let key = crate::hostfs::dir_key(&resolved.to_string_lossy());
    let digest = Sha256::digest(key.as_bytes());
    let name: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    run_homes.join("grok").join(name)
}

/// The directory `run` is made against for the account at `account_dir`.
pub(super) fn dir_for(run_homes: &Path, run: BackgroundRun, account_dir: &Path) -> PathBuf {
    account_homes(run_homes, account_dir).join(run_name(run))
}

/// Makes the directory `run` is made against ready for one run and takes its lock: the directory,
/// the owner's alone, and in it the account's `config.toml` as it is now, or none where the
/// account has none, so that nothing of an earlier copy is read. A login store the user's shell
/// names already reaches Grok through the shell snapshot and is left to it.
pub(super) fn for_run(
    run_homes: &Path,
    run: BackgroundRun,
    account_dir: &Path,
    shell_env: &HashMap<String, String>,
    wait: &RunWait<'_>,
) -> Result<RunHome> {
    let dir = dir_for(run_homes, run, account_dir);
    create_private_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let lock = lock(&dir, wait)?;
    // Whose directories these are, as the account's directory is written: its real location,
    // which names them, cannot be asked once the directory has gone.
    let named = account_homes(run_homes, account_dir).join(ACCOUNT_FILE);
    std::fs::write(&named, account_dir.as_os_str().as_encoded_bytes())
        .with_context(|| format!("writing {}", named.display()))?;
    match std::fs::remove_dir_all(dir.join(LOGS_DIR)) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            return Err(err).with_context(|| format!("removing {}'s logs", dir.display()));
        }
        _ => {}
    }
    let copy = dir.join(GROK_CONFIG_FILE);
    // Removed first: Grok may have replaced the copy with a file of its own making, and the new
    // copy is created private rather than written into whatever is there.
    match std::fs::remove_file(&copy) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            return Err(err).with_context(|| format!("removing {}", copy.display()));
        }
        _ => {}
    }
    copy_private_file(&account_dir.join(GROK_CONFIG_FILE), &copy)?;
    let after_run: Option<Box<dyn FnOnce() + Send>> = match run {
        BackgroundRun::Kick => {
            let kicked = dir.clone();
            Some(Box::new(move || clear(&kicked)))
        }
        BackgroundRun::QuotaReading | BackgroundRun::ModelListing => None,
    };
    Ok(RunHome {
        dir,
        after_run,
        env: login_store_env(account_dir, shell_env)
            .into_iter()
            .collect(),
        _lock: Some(lock),
    })
}

/// Takes the directory's lock, waiting for a run that holds it until `wait` ends: at the run's own
/// deadline, or as soon as the run is cancelled. A lock that comes free only at the deadline is
/// refused like one that never did: the run would be started with no time left and its failure
/// read as a timeout of its own, when what held it up was the run before.
fn lock(dir: &Path, wait: &RunWait<'_>) -> Result<std::fs::File> {
    let path = dir.join(LOCK_FILE);
    loop {
        let taken = match try_lock_file(&path) {
            Ok(file) => Some(file),
            Err(Busy::Held) => None,
            Err(Busy::Open(err)) => {
                return Err(err).with_context(|| format!("opening {}", path.display()))
            }
            Err(Busy::Lock(err)) => {
                return Err(err).with_context(|| format!("locking {}", path.display()))
            }
        };
        if wait
            .cancel
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            return Err(anyhow!(
                "the run was cancelled while it waited for its directory"
            ));
        }
        if Instant::now() >= wait.until {
            return Err(anyhow!(
                "another run of the same kind for this account was still going when this one's \
                 time ran out"
            ));
        }
        if let Some(file) = taken {
            return Ok(file);
        }
        std::thread::sleep(POLL);
    }
}

/// Removes everything a run left in its directory but the lock file, which the run still holds.
/// Best effort: an entry that cannot be removed is logged and left for the next run to clear.
fn clear(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name() == LOCK_FILE {
            continue;
        }
        let path = entry.path();
        // A link is removed as the link it is, never followed.
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        let removed = if is_dir {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(err) = removed {
            tracing::warn!(path = %path.display(), %err, "what a kick left was not removed");
        }
    }
}

/// Removes every directory kept for the account at `account_dir`, with the copies of its
/// configuration they hold: the ones its directory names as it really is now, and any whose note
/// ([`ACCOUNT_FILE`]) names a directory written the same way, which finds them once that
/// directory has gone to the Trash or leads elsewhere. Directories with no note beside them are
/// found by the resolved digest alone. Best effort: one that cannot be removed is logged and left.
pub(super) fn forget(run_homes: &Path, account_dir: &Path) {
    let mut homes = vec![account_homes(run_homes, account_dir)];
    let written = account_dir.as_os_str().as_encoded_bytes();
    if let Ok(entries) = std::fs::read_dir(run_homes.join("grok")) {
        homes.extend(entries.flatten().map(|entry| entry.path()).filter(|homes| {
            std::fs::read(homes.join(ACCOUNT_FILE)).is_ok_and(|named| same_written(&named, written))
        }));
    }
    for homes in homes {
        match std::fs::remove_dir_all(&homes) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                tracing::warn!(
                    dir = %homes.display(),
                    %err,
                    "the account's run directories were not removed"
                );
            }
            _ => {}
        }
    }
}

/// Whether two paths, as their bytes, are one directory as it is written: the same bytes, or,
/// where both are text, the same under the key directories are compared by
/// ([`crate::hostfs::dir_key`]). Never by a lossy reading, under which two paths that differ only
/// in bytes that are not text would pass for one.
fn same_written(a: &[u8], b: &[u8]) -> bool {
    a == b
        || matches!(
            (std::str::from_utf8(a), std::str::from_utf8(b)),
            (Ok(a), Ok(b)) if crate::hostfs::dir_key(a) == crate::hostfs::dir_key(b)
        )
}

#[cfg(test)]
mod tests {
    use super::super::home::LOGIN_STORE_VAR;
    use super::*;
    use crate::providers::grok::GROK_SIGN_IN_FILE;
    use crate::test_support::ScratchDir;

    /// A wait long enough for a directory nothing else holds.
    fn soon() -> RunWait<'static> {
        RunWait {
            until: Instant::now() + crate::test_support::PATIENCE,
            cancel: None,
        }
    }

    fn account(scratch: &ScratchDir, config: Option<&str>) -> PathBuf {
        let account = scratch.join("account");
        std::fs::create_dir_all(&account).expect("the account's directory");
        if let Some(config) = config {
            std::fs::write(account.join(GROK_CONFIG_FILE), config).expect("config");
        }
        std::fs::write(account.join(GROK_SIGN_IN_FILE), "{}").expect("login");
        account
    }

    /// The directory holds a copy of the account's configuration and names the account's own
    /// login; what a run writes into the copy never reaches the account, and the next run starts
    /// from the account's configuration again. Each kind of run has a directory of its own.
    #[test]
    fn a_run_is_made_against_a_copy_of_the_accounts_configuration() {
        let scratch = ScratchDir::new("grok-run-home");
        let users = "# mine\n[ui]\nfoo = \"bar\"\n";
        let account = account(&scratch, Some(users));
        let root = scratch.join("homes");
        let quota = BackgroundRun::QuotaReading;

        let home = for_run(&root, quota, &account, &HashMap::new(), &soon()).expect("a home");
        assert!(home.dir.starts_with(&root) && !home.dir.starts_with(&account));
        assert_eq!(home.dir, dir_for(&root, quota, &account));
        let copy = home.dir.join(GROK_CONFIG_FILE);
        assert!(!copy.is_symlink());
        assert_eq!(std::fs::read_to_string(&copy).expect("the copy"), users);
        let login = crate::hostfs::canonical(account.join(GROK_SIGN_IN_FILE)).expect("login");
        assert_eq!(
            home.env,
            [(
                LOGIN_STORE_VAR.to_string(),
                login.to_string_lossy().into_owned()
            )]
        );

        // What Grok appends as it first runs against the copy.
        std::fs::write(
            &copy,
            format!("{users}\n[marketplace]\ndefault_skills_installs_purged = true\n"),
        )
        .expect("Grok's write");
        assert_eq!(
            std::fs::read_to_string(account.join(GROK_CONFIG_FILE)).expect("the account's"),
            users
        );
        let dir = home.dir.clone();
        drop(home);
        let again = for_run(&root, quota, &account, &HashMap::new(), &soon()).expect("a home");
        assert_eq!(again.dir, dir, "the directory is kept between runs");
        assert_eq!(std::fs::read_to_string(&copy).expect("the copy"), users);

        for other in [BackgroundRun::ModelListing, BackgroundRun::Kick] {
            let home = for_run(&root, other, &account, &HashMap::new(), &soon()).expect("a home");
            assert_ne!(home.dir, dir);
        }
    }

    /// An account whose `config.toml` is a link, as a sharing account's is, has the file the link
    /// leads to copied: the copy is a file of its own, and neither the link nor its target is
    /// touched by a write into the copy.
    #[cfg(unix)]
    #[test]
    fn a_linked_configuration_is_copied_as_a_file_and_the_link_is_left() {
        let scratch = ScratchDir::new("grok-run-home-link");
        let account = account(&scratch, None);
        let users = "theme = \"x\"\n";
        let target = scratch.join("shared-config.toml");
        std::fs::write(&target, users).expect("the shared file");
        std::os::unix::fs::symlink(&target, account.join(GROK_CONFIG_FILE)).expect("the link");

        let home = for_run(
            &scratch.join("homes"),
            BackgroundRun::ModelListing,
            &account,
            &HashMap::new(),
            &soon(),
        )
        .expect("a home");
        let copy = home.dir.join(GROK_CONFIG_FILE);
        assert!(copy.is_file() && !copy.is_symlink());
        assert_eq!(std::fs::read_to_string(&copy).expect("the copy"), users);
        std::fs::write(&copy, "changed = true\n").expect("a write into the copy");
        assert!(account.join(GROK_CONFIG_FILE).is_symlink());
        assert_eq!(std::fs::read_to_string(&target).expect("the target"), users);
    }

    #[cfg(unix)]
    #[test]
    fn the_copy_and_its_directory_are_the_owners_alone() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = ScratchDir::new("grok-run-home-mode");
        let account = account(&scratch, Some("api_key = \"k\"\n"));
        let users = account.join(GROK_CONFIG_FILE);
        std::fs::set_permissions(&users, std::fs::Permissions::from_mode(0o644)).expect("mode");
        let home = for_run(
            &scratch.join("homes"),
            BackgroundRun::QuotaReading,
            &account,
            &HashMap::new(),
            &soon(),
        )
        .expect("a home");
        let mode = |path: &Path| {
            std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode()
        };
        assert_eq!(mode(&home.dir.join(GROK_CONFIG_FILE)) & 0o777, 0o600);
        assert_eq!(mode(&home.dir) & 0o077, 0);
    }

    /// An account that loses its configuration leaves no earlier copy behind, and a login store
    /// the user's shell names is left to the shell's variable.
    #[test]
    fn a_missing_configuration_and_a_named_login_store_are_followed() {
        let scratch = ScratchDir::new("grok-run-home-none");
        let account = account(&scratch, Some("x = 1\n"));
        let root = scratch.join("homes");
        let quota = BackgroundRun::QuotaReading;
        let home = for_run(&root, quota, &account, &HashMap::new(), &soon()).expect("a home");
        let copy = home.dir.join(GROK_CONFIG_FILE);
        drop(home);
        std::fs::remove_file(account.join(GROK_CONFIG_FILE)).expect("removed");
        let shell = HashMap::from([(LOGIN_STORE_VAR.to_string(), "/elsewhere/auth.json".into())]);
        let again = for_run(&root, quota, &account, &shell, &soon()).expect("a home");
        assert!(!copy.exists());
        assert!(again.env.is_empty());
    }

    /// A directory that cannot be made is an error, never the account's own directory instead.
    #[test]
    fn a_directory_that_cannot_be_made_is_refused() {
        let scratch = ScratchDir::new("grok-run-home-refused");
        let account = account(&scratch, Some("x = 1\n"));
        let root = scratch.join("homes");
        std::fs::write(&root, "a file where the directories go").expect("the obstacle");
        assert!(for_run(
            &root,
            BackgroundRun::Kick,
            &account,
            &HashMap::new(),
            &soon()
        )
        .is_err());
    }

    /// The directory is one run's at a time: its lock is held by the home a run was given, and
    /// free again once that home is dropped.
    #[test]
    fn the_directory_is_locked_for_as_long_as_a_run_holds_it() {
        let scratch = ScratchDir::new("grok-run-home-lock");
        let account = account(&scratch, Some("x = 1\n"));
        let root = scratch.join("homes");
        let quota = BackgroundRun::QuotaReading;
        let home = for_run(&root, quota, &account, &HashMap::new(), &soon()).expect("a home");
        let lock_file = home.dir.join(LOCK_FILE);
        assert!(matches!(try_lock_file(&lock_file), Err(Busy::Held)));
        drop(home);
        assert!(try_lock_file(&lock_file).is_ok());
    }

    /// What a kick left in its directory goes once the kick has ended, the copy of the account's
    /// configuration and the session Grok wrote alike, so nothing piles up from kick to kick; a
    /// reading's directory keeps what Grok laid down in it.
    #[test]
    fn what_a_kick_wrote_is_removed_once_the_kick_has_ended() {
        let scratch = ScratchDir::new("grok-run-home-kick");
        let account = account(&scratch, Some("x = 1\n"));
        let root = scratch.join("homes");
        let left = |dir: &Path| {
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .expect("the directory")
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        // What Grok writes as it runs: a session, and a file beside the configuration.
        let run = |home: &RunHome| {
            std::fs::create_dir_all(home.dir.join("sessions/2026/s1")).expect("a session");
            std::fs::write(home.dir.join("sessions/2026/s1/chat_history.jsonl"), "{}")
                .expect("its record");
            std::fs::write(home.dir.join("prompt_history.jsonl"), "{}").expect("a history");
        };

        let kick = for_run(
            &root,
            BackgroundRun::Kick,
            &account,
            &HashMap::new(),
            &soon(),
        )
        .expect("a home");
        run(&kick);
        let kicked = kick.dir.clone();
        drop(kick);
        assert_eq!(left(&kicked), [LOCK_FILE]);

        let quota = BackgroundRun::QuotaReading;
        let reading = for_run(&root, quota, &account, &HashMap::new(), &soon()).expect("a home");
        run(&reading);
        let read = reading.dir.clone();
        drop(reading);
        assert_eq!(
            left(&read),
            [
                LOCK_FILE,
                GROK_CONFIG_FILE,
                "prompt_history.jsonl",
                "sessions"
            ]
        );
    }

    /// A run that finds its directory held waits for it no longer than its own deadline, and
    /// gives the wait up at once when it is cancelled; either way it is refused, and the account's
    /// own directory is not used instead.
    #[test]
    fn a_held_directory_is_waited_for_until_the_runs_deadline_or_its_cancellation() {
        use std::sync::atomic::AtomicBool;
        use std::time::Duration;

        let scratch = ScratchDir::new("grok-run-home-wait");
        let account = account(&scratch, Some("x = 1\n"));
        let root = scratch.join("homes");
        let quota = BackgroundRun::QuotaReading;
        let held = for_run(&root, quota, &account, &HashMap::new(), &soon()).expect("a home");

        let short = RunWait {
            until: Instant::now() + Duration::from_millis(100),
            cancel: None,
        };
        let began = Instant::now();
        let refused = for_run(&root, quota, &account, &HashMap::new(), &short)
            .err()
            .expect("the wait ran out");
        assert!(refused.to_string().contains("still going"), "{refused}");
        assert!(began.elapsed() < crate::test_support::PATIENCE);

        let cancelled = AtomicBool::new(true);
        let wait = RunWait {
            until: Instant::now() + crate::test_support::PATIENCE,
            cancel: Some(&cancelled),
        };
        let began = Instant::now();
        let refused = for_run(&root, quota, &account, &HashMap::new(), &wait)
            .err()
            .expect("cancelled");
        assert!(refused.to_string().contains("cancelled"), "{refused}");
        assert!(began.elapsed() < crate::test_support::PATIENCE);

        // Another kind of run has a directory of its own and does not wait.
        for_run(
            &root,
            BackgroundRun::ModelListing,
            &account,
            &HashMap::new(),
            &soon(),
        )
        .expect("a home");
        drop(held);
    }

    /// A run whose time has already run out is refused even where its directory is free, as one
    /// held up by the run before: it is not started with no time left.
    #[test]
    fn a_run_whose_deadline_has_passed_is_refused_though_its_directory_is_free() {
        let scratch = ScratchDir::new("grok-run-home-late");
        let account = account(&scratch, Some("x = 1\n"));
        let root = scratch.join("homes");
        let late = RunWait {
            until: Instant::now(),
            cancel: None,
        };
        let refused = for_run(
            &root,
            BackgroundRun::QuotaReading,
            &account,
            &HashMap::new(),
            &late,
        )
        .err()
        .expect("no time left");
        assert!(refused.to_string().contains("still going"), "{refused}");
        // The lock was let go again.
        for_run(
            &root,
            BackgroundRun::QuotaReading,
            &account,
            &HashMap::new(),
            &soon(),
        )
        .expect("a home");
    }

    /// The note beside an account's run directories is compared as the bytes of the path it
    /// names: two paths that differ only in bytes that are not text are two accounts, and a path
    /// in another letter case is the same one only where directories are compared that way.
    #[test]
    fn a_note_names_its_account_by_the_bytes_of_its_path() {
        assert!(same_written(b"/home/me/grok", b"/home/me/grok"));
        assert!(!same_written(b"/home/me/grok-\xff", b"/home/me/grok-\xfe"));
        assert!(!same_written(b"/home/me/grok-\xff", b"/home/me/grok-"));
        assert_eq!(
            same_written(b"/home/me/Grok", b"/home/me/grok"),
            crate::platform::names::FOLDS_CASE
        );
    }

    /// Grok's log in a kept directory does not grow from run to run: it is removed before each.
    #[test]
    fn the_log_of_the_run_before_is_removed() {
        let scratch = ScratchDir::new("grok-run-home-logs");
        let account = account(&scratch, Some("x = 1\n"));
        let root = scratch.join("homes");
        let quota = BackgroundRun::QuotaReading;
        let home = for_run(&root, quota, &account, &HashMap::new(), &soon()).expect("a home");
        let log = home.dir.join(LOGS_DIR).join("unified.jsonl");
        std::fs::create_dir_all(home.dir.join(LOGS_DIR)).expect("logs");
        std::fs::write(&log, "{}\n").expect("a log");
        std::fs::write(home.dir.join("models_cache.json"), "{}").expect("a cache");
        drop(home);

        let again = for_run(&root, quota, &account, &HashMap::new(), &soon()).expect("a home");
        assert!(!log.exists());
        assert!(
            again.dir.join("models_cache.json").exists(),
            "the rest is kept"
        );
    }

    /// An account whose directory was reached through a link is forgotten by the way its
    /// directory is written, once the link and what it led to are gone: where the directory really
    /// was, which named its run directories, can no longer be asked.
    #[cfg(unix)]
    #[test]
    fn an_account_whose_directory_is_gone_is_still_forgotten() {
        let scratch = ScratchDir::new("grok-run-home-gone");
        let real = scratch.join("volume/grok");
        std::fs::create_dir_all(&real).expect("the directory");
        std::fs::write(real.join(GROK_CONFIG_FILE), "api_key = \"k\"\n").expect("config");
        let written = scratch.join("grok-work");
        std::os::unix::fs::symlink(&real, &written).expect("the link");
        let root = scratch.join("homes");
        let quota = BackgroundRun::QuotaReading;
        let home = for_run(&root, quota, &written, &HashMap::new(), &soon()).expect("a home");
        let kept = home.dir.clone();
        drop(home);
        assert!(kept.join(GROK_CONFIG_FILE).exists());

        // As a move to the Trash leaves it: the directory gone, the account still written as it was.
        std::fs::remove_file(&written).expect("the link");
        std::fs::remove_dir_all(&real).expect("the directory");
        forget(&root, &written);
        assert!(!kept.exists());
    }

    /// Removing an account removes every directory kept for it, and nothing of another account's.
    #[test]
    fn an_accounts_directories_go_with_the_account() {
        let scratch = ScratchDir::new("grok-run-home-forget");
        let account = account(&scratch, Some("api_key = \"k\"\n"));
        let other = scratch.join("other");
        std::fs::create_dir_all(&other).expect("another account");
        let root = scratch.join("homes");
        for run in [BackgroundRun::QuotaReading, BackgroundRun::ModelListing] {
            drop(for_run(&root, run, &account, &HashMap::new(), &soon()).expect("a home"));
        }
        let kept =
            for_run(&root, BackgroundRun::Kick, &other, &HashMap::new(), &soon()).expect("a home");

        forget(&root, &account);
        assert!(!dir_for(&root, BackgroundRun::QuotaReading, &account).exists());
        assert!(!dir_for(&root, BackgroundRun::ModelListing, &account).exists());
        assert!(kept.dir.is_dir());
        // An account that has none is no error.
        forget(&root, &account);
    }
}
