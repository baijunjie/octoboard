//! What the daemon remembers of the user's shell environment, and when it may be served again.
//!
//! One capture is kept — the environment, when it was taken and which of its `PATH` directories
//! existed then — and every snapshot the daemon takes fills it, the start-up one included. Two
//! kinds of caller read it, each under its own policy, because taking a snapshot is slow (the
//! login shell runs a serial chain of execs, which under an endpoint-security agent that stalls
//! every exec is seconds) and the callers differ in what that cost buys them:
//!
//! - An agent launch ([`Cache::launch`]) wants the user's latest configuration: a node-version
//!   manager's `PATH` entry can point at a per-shell-instance directory, and a long-running daemon
//!   would otherwise never pick up an edit to the shell's rc files. It is served a capture younger
//!   than [`LAUNCH_MAX_AGE`] whose `PATH` directories all still exist, and retakes one in the
//!   background, stale-while-revalidate, so the next launch sees the latest configuration.
//! - A caller driven on someone else's schedule ([`Cache::scheduled`]) — the git-status sweep
//!   `refresh_git_status` runs every five minutes for as long as a console stays shown — is served
//!   a capture younger than [`SCHEDULED_MAX_AGE`], and a *failed* attempt is remembered for as long
//!   too: ten projects in one sweep hitting a wedged rc file would otherwise serialize behind this
//!   module's one-shell-at-a-time nature for ten attempts' worth of the snapshot timeout instead of
//!   one.
//!
//! A snapshot taken while none is servable is taken by one caller at a time; the others wait for it
//! and are served what it produced.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;

type Env = HashMap<String, String>;

/// How long [`Cache::launch`] serves a capture without insisting on a fresh one. Every launch that
/// reuses one also starts a background retake, so in steady use the cache is only ever one launch
/// behind; this bound matters after an idle spell, where the first launch would otherwise run on a
/// configuration from before the user's last edit to it. Ten minutes is long enough that a burst
/// of launches (opening several sessions in a row) never pays for a login shell, and short enough
/// that an rc-file edit made before a coffee break is picked up by the first launch after it
/// rather than the second. A directory that vanishes sooner is caught by the `PATH` check instead,
/// not by this age.
const LAUNCH_MAX_AGE: Duration = Duration::from_secs(10 * 60);

/// How long [`Cache::scheduled`] serves a capture, or a failure. An hour picks up a `PATH` edit
/// within the hour instead of never, while still amortising the fork across many sweeps — the
/// daemon itself owns no timer that would call for a shorter or longer number here.
const SCHEDULED_MAX_AGE: Duration = Duration::from_secs(60 * 60);

/// A capture younger than this is not retaken in the background by a launch that serves it: a
/// burst of launches, or one right after the start-up snapshot, has nothing newer to find.
const BACKGROUND_RETAKE_FLOOR: Duration = Duration::from_secs(60);

struct Capture {
    env: Env,
    taken_at: Instant,
    /// The `PATH` directories that existed when it was taken. A version manager's per-shell
    /// directory (fnm's multishell one) that has since been removed leaves a `PATH` entry the
    /// agent cannot resolve a binary through, and the age bound alone would not notice.
    path_dirs: Vec<PathBuf>,
}

impl Capture {
    fn new(env: Env) -> Self {
        let path_dirs = env
            .get("PATH")
            .map(|path| {
                std::env::split_paths(path)
                    .filter(|dir| dir.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            env,
            taken_at: Instant::now(),
            path_dirs,
        }
    }

    fn age(&self) -> Duration {
        self.taken_at.elapsed()
    }

    fn is_servable_to_launch(&self) -> bool {
        self.age() < LAUNCH_MAX_AGE && self.path_dirs.iter().all(|dir| dir.is_dir())
    }
}

/// A failed attempt by a scheduled caller, and when it was made.
struct Failure {
    message: String,
    at: Instant,
}

#[derive(Default)]
struct Stored {
    capture: Option<Capture>,
    failure: Option<Failure>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    Launch,
    Scheduled,
}

/// What a read finds servable: the outcome to answer with, and how old the capture behind it is.
struct Servable {
    outcome: Result<Env, String>,
    age: Duration,
}

pub struct Cache {
    stored: Mutex<Stored>,
    /// Held for the length of a snapshot, so callers that find nothing servable take one between
    /// them rather than one each.
    taking: Mutex<()>,
    /// Whether a background retake is running, so a burst of launches starts one login shell
    /// rather than one each.
    retaking: AtomicBool,
}

/// Marks a background retake as running for as long as it lives, so that it is cleared on every
/// way out, a panicking snapshot included.
struct Retaking<'a>(&'a AtomicBool);

impl Drop for Retaking<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Cache {
    pub const fn new() -> Self {
        Self {
            stored: Mutex::new(Stored {
                capture: None,
                failure: None,
            }),
            taking: Mutex::new(()),
            retaking: AtomicBool::new(false),
        }
    }

    /// Records a snapshot just taken, whoever took it, and forgets any failure before it.
    pub fn store(&self, env: &Env) {
        let mut stored = self.stored.lock().expect("shell env cache lock poisoned");
        stored.capture = Some(Capture::new(env.clone()));
        stored.failure = None;
    }

    /// A fresh snapshot, waited for whatever the cache holds, and stored. Takes its turn on the
    /// same lock as every other snapshot, so it never runs alongside one a launch or a sweep
    /// started.
    pub fn refresh(&self, take: fn() -> Result<Env>) -> Result<Env> {
        let _taking = self.taking.lock().expect("shell env taking lock poisoned");
        let env = take()?;
        self.store(&env);
        Ok(env)
    }

    /// The environment an agent launch runs with: a capture [`LAUNCH_MAX_AGE`] allows with every
    /// `PATH` directory it found still present, retaken in the background for the next launch; a
    /// fresh snapshot, waited for, when there is none. A configuration edit reaches the launch
    /// after the one that noticed it, and a vanished per-shell `PATH` directory is never launched
    /// with.
    pub fn launch(&'static self, take: fn() -> Result<Env>) -> Result<Env> {
        if let Some(servable) = self.servable(Policy::Launch) {
            if servable.age >= BACKGROUND_RETAKE_FLOOR {
                self.retake_in_background(take);
            }
            return servable.outcome.map_err(|err| anyhow::anyhow!(err));
        }
        self.take_once(Policy::Launch, take)
    }

    /// The environment for a caller on someone else's schedule: the capture or the failure
    /// [`SCHEDULED_MAX_AGE`] allows, else a fresh snapshot.
    pub fn scheduled(&self, take: fn() -> Result<Env>) -> Result<Env> {
        if let Some(servable) = self.servable(Policy::Scheduled) {
            return servable.outcome.map_err(|err| anyhow::anyhow!(err));
        }
        self.take_once(Policy::Scheduled, take)
    }

    /// What `policy` allows serving right now. The lock is held for the copy only, never across a
    /// snapshot.
    fn servable(&self, policy: Policy) -> Option<Servable> {
        let stored = self.stored.lock().expect("shell env cache lock poisoned");
        let capture = stored.capture.as_ref();
        match policy {
            Policy::Launch => capture
                .filter(|capture| capture.is_servable_to_launch())
                .map(|capture| Servable {
                    outcome: Ok(capture.env.clone()),
                    age: capture.age(),
                }),
            Policy::Scheduled => {
                if let Some(capture) = capture.filter(|capture| capture.age() < SCHEDULED_MAX_AGE) {
                    return Some(Servable {
                        outcome: Ok(capture.env.clone()),
                        age: capture.age(),
                    });
                }
                stored
                    .failure
                    .as_ref()
                    .filter(|failure| failure.at.elapsed() < SCHEDULED_MAX_AGE)
                    .map(|failure| Servable {
                        outcome: Err(failure.message.clone()),
                        age: Duration::MAX,
                    })
            }
        }
    }

    /// Takes a snapshot unless another caller took one while this one waited its turn. Only a
    /// scheduled caller's failure is remembered: a launch that failed is retried by the next one,
    /// which the user triggers.
    fn take_once(&self, policy: Policy, take: fn() -> Result<Env>) -> Result<Env> {
        let _taking = self.taking.lock().expect("shell env taking lock poisoned");
        if let Some(servable) = self.servable(policy) {
            return servable.outcome.map_err(|err| anyhow::anyhow!(err));
        }
        match take() {
            Ok(env) => {
                self.store(&env);
                Ok(env)
            }
            Err(err) => {
                if policy == Policy::Scheduled {
                    self.stored
                        .lock()
                        .expect("shell env cache lock poisoned")
                        .failure = Some(Failure {
                        message: format!("{err:#}"),
                        at: Instant::now(),
                    });
                }
                Err(err)
            }
        }
    }

    /// Retakes the snapshot on a thread of its own unless one is already being taken. A failed
    /// retake leaves the previous capture in place: it stays servable until it ages out or loses a
    /// `PATH` directory, and the launch after that takes its own and reports the failure.
    fn retake_in_background(&'static self, take: fn() -> Result<Env>) {
        if self.retaking.swap(true, Ordering::AcqRel) {
            return;
        }
        thread::spawn(move || {
            let _retaking = Retaking(&self.retaking);
            // A snapshot already under way is as good as this one would be.
            let Ok(_taking) = self.taking.try_lock() else {
                return;
            };
            if let Ok(env) = take() {
                self.store(&env);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use anyhow::bail;

    use super::*;
    use crate::test_support::{ScratchDir, PATIENCE};

    fn env_with_path(path: &str) -> Env {
        HashMap::from([("PATH".to_string(), path.to_string())])
    }

    fn aged(env: Env, age: Duration) -> Capture {
        let mut capture = Capture::new(env);
        capture.taken_at = Instant::now() - age;
        capture
    }

    #[test]
    fn a_capture_is_not_servable_to_a_launch_once_a_path_directory_it_saw_is_gone() {
        let dir = ScratchDir::new("capture-path");
        let kept = dir.to_string_lossy().into_owned();
        // A directory that never existed is not held against the capture.
        let path = format!("{kept}:/octoboard-test-no-such-dir");
        let capture = Capture::new(env_with_path(&path));
        assert!(capture.is_servable_to_launch());
        drop(dir);
        assert!(!capture.is_servable_to_launch());
    }

    #[test]
    fn a_capture_is_not_servable_to_a_launch_past_its_max_age() {
        assert!(aged(env_with_path("/"), Duration::ZERO).is_servable_to_launch());
        assert!(!aged(env_with_path("/"), LAUNCH_MAX_AGE).is_servable_to_launch());
    }

    #[test]
    fn a_launch_serves_the_capture_and_retakes_once_in_the_background() {
        static CACHE: Cache = Cache::new();
        static TAKEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn take() -> Result<Env> {
            let n = TAKEN.fetch_add(1, Ordering::SeqCst);
            Ok(HashMap::from([("N".to_string(), n.to_string())]))
        }

        // Cold: taken synchronously.
        assert_eq!(CACHE.launch(take).unwrap()["N"], "0");
        // Young: served with no retake, there being nothing newer to find.
        assert_eq!(CACHE.launch(take).unwrap()["N"], "0");
        assert_eq!(TAKEN.load(Ordering::SeqCst), 1);

        // Old enough to retake: served at once while one retake runs; the calls after start no
        // second.
        CACHE.stored.lock().unwrap().capture =
            Some(aged(env_with_path("/"), BACKGROUND_RETAKE_FLOOR));
        assert!(!CACHE.launch(take).unwrap().contains_key("N"));
        CACHE.launch(take).unwrap();
        let deadline = Instant::now() + PATIENCE;
        while CACHE.retaking.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(TAKEN.load(Ordering::SeqCst), 2);
        // The retake landed, so the next launch gets it.
        assert_eq!(CACHE.launch(take).unwrap()["N"], "1");
    }

    #[test]
    fn a_launch_takes_a_fresh_snapshot_when_none_is_servable_and_reports_its_failure() {
        static CACHE: Cache = Cache::new();
        fn fails() -> Result<Env> {
            bail!("no shell")
        }
        fn works() -> Result<Env> {
            Ok(env_with_path("/"))
        }
        assert!(CACHE.launch(fails).is_err());
        // A stale capture is not served, and a failed retake on top of it does not hide the error.
        CACHE.stored.lock().unwrap().capture = Some(aged(env_with_path("/"), LAUNCH_MAX_AGE));
        assert!(CACHE.launch(fails).is_err());
        assert!(CACHE.launch(works).is_ok());
        assert!(CACHE.servable(Policy::Launch).is_some());
    }

    #[test]
    fn a_scheduled_caller_is_served_a_capture_or_a_failure_for_an_hour() {
        let cache = Cache::new();
        fn fails() -> Result<Env> {
            bail!("no shell")
        }
        fn works() -> Result<Env> {
            Ok(env_with_path("/"))
        }
        assert!(cache.scheduled(fails).is_err());
        // The failure is remembered: a snapshot that would now work is not taken.
        assert!(cache.scheduled(works).is_err());
        cache.stored.lock().unwrap().failure.as_mut().unwrap().at =
            Instant::now() - SCHEDULED_MAX_AGE;
        assert!(cache.scheduled(works).is_ok());
        // Served from the capture, well past what a launch would accept.
        cache.stored.lock().unwrap().capture =
            Some(aged(env_with_path("/"), SCHEDULED_MAX_AGE / 2));
        assert!(cache.scheduled(fails).is_ok());
    }

    #[test]
    fn a_snapshot_taken_by_anyone_fills_the_cache_for_both_policies() {
        let cache = Cache::new();
        cache.store(&env_with_path("/"));
        assert!(cache.servable(Policy::Launch).is_some());
        assert!(cache.servable(Policy::Scheduled).is_some());
    }

    #[test]
    fn a_refresh_takes_its_turn_and_fills_the_cache() {
        static CACHE: Cache = Cache::new();
        static TAKING: AtomicBool = AtomicBool::new(false);
        fn take() -> Result<Env> {
            assert!(
                !TAKING.swap(true, Ordering::SeqCst),
                "two snapshots at once"
            );
            thread::sleep(Duration::from_millis(100));
            TAKING.store(false, Ordering::SeqCst);
            Ok(env_with_path("/"))
        }
        let callers: Vec<_> = (0..3)
            .map(|_| thread::spawn(|| CACHE.refresh(take).is_ok()))
            .collect();
        for caller in callers {
            assert!(caller.join().unwrap());
        }
        assert!(CACHE.servable(Policy::Launch).is_some());
    }

    #[test]
    fn callers_that_find_nothing_servable_take_one_snapshot_between_them() {
        static CACHE: Cache = Cache::new();
        static TAKEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn take() -> Result<Env> {
            TAKEN.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(200));
            Ok(env_with_path("/"))
        }
        let callers: Vec<_> = (0..4)
            .map(|_| thread::spawn(|| CACHE.scheduled(take).is_ok()))
            .collect();
        for caller in callers {
            assert!(caller.join().unwrap());
        }
        assert_eq!(TAKEN.load(Ordering::SeqCst), 1);
    }
}
