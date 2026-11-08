//! The host operations that requests reach the machine through and that a test stands in for: the
//! user's own `git`, which each family of `git` call sites runs under its own policy and limits
//! (`crate::git::run::Runner`), moving something to the Trash, as every removal that offers it
//! does, and the shell a shell terminal runs with the environment it is started in. A test puts
//! stand-ins in their place, so no test runs the user's login shell or fills their Trash.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use anyhow::Result;

use crate::git::isolated::IsolatedGit;
use crate::git::run::UserGit;
use crate::hostfs::trash::{move_to_trash, TrashFailure};
use crate::protocol::{error_code, CodedError};

#[derive(Clone, Copy)]
pub struct Tools {
    /// The user's `git` as the scheduled snapshot resolves it, which is served from the cache for
    /// up to an hour, a failure included.
    pub git: fn() -> Result<UserGit>,
    /// The user's `git` as a snapshot of the login shell taken now resolves it, which then becomes
    /// what `git` is served from; only for [`Tools::or_fresh_git`], which [`Tools::git_for`] a
    /// person and a clone use.
    pub fresh_git: fn() -> Result<UserGit>,
    pub trash: fn(&Path) -> Result<(), TrashFailure>,
    /// The shell a shell terminal runs.
    pub shell: fn() -> crate::platform::shell::ShellCommand,
    /// The environment a shell terminal is started in, as an agent launch takes it (and a launch
    /// resolves Claude Code's configuration in).
    pub shell_env: fn() -> Result<HashMap<String, String>>,
    /// The environment for a request a person or a tool makes between launches, such as the
    /// Claude Code model listing: the scheduled snapshot, which is served from the cache for up
    /// to an hour, so that no picker opened or tool called forks a login shell.
    pub scheduled_shell_env: fn() -> Result<HashMap<String, String>>,
}

/// Who a `git` is resolved for, which decides whether one that cannot be found is looked for
/// again ([`Tools::git_for`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// A request a person just made, such as a push or adding a project.
    Person,
    /// Anything repeating or not asked for by a person: a sweep, a listing refreshed while it is on
    /// screen, a tool an agent calls, work that follows on from another.
    Scheduled,
}

impl Tools {
    /// `found`, the user's `git` as some snapshot resolved it, or where it is a `git` that was not
    /// found, [`Tools::fresh_git`]: what [`Tools::git_for`] a person does, for a caller that
    /// resolves `git` from another snapshot than [`Tools::git`]'s. A fresh snapshot that cannot be
    /// taken leaves the first refusal standing.
    pub fn or_fresh_git(&self, found: Result<UserGit>) -> Result<UserGit> {
        match found {
            Err(err) if is_git_not_found(&err) => match (self.fresh_git)() {
                Err(fresh) if !is_git_not_found(&fresh) => Err(err),
                fresh => fresh,
            },
            found => found,
        }
    }

    /// The user's `git` for `pace`. [`Tools::git`] serves a snapshot up to an hour old, so a person
    /// who has just installed `git`, or changed their `PATH` to reach it, and tries again would
    /// keep being refused; for a person, where it finds no `git`, [`Tools::fresh_git`] looks once
    /// more. Anything else, which may repeat, takes [`Tools::git`] alone, so that it does not take
    /// a login shell on every call for as long as `git` is missing.
    pub fn git_for(&self, pace: Pace) -> Result<UserGit> {
        match pace {
            Pace::Person => self.or_fresh_git((self.git)()),
            Pace::Scheduled => (self.git)(),
        }
    }
}

/// Whether `err` is the refusal of a `git` that does not resolve on the snapshot's `PATH`
/// (`binary_not_found` naming `git`), which a client words as Git not being installed.
pub fn is_git_not_found(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>().is_some_and(|coded| {
        coded.code == error_code::BINARY_NOT_FOUND
            && coded.params.get("binary").map(String::as_str) == Some("git")
    })
}

/// Held on `AppState`.
pub struct HostTools {
    tools: Mutex<Tools>,
}

impl Default for HostTools {
    fn default() -> Self {
        Self {
            tools: Mutex::new(Tools {
                git: UserGit::cached_shell,
                fresh_git: UserGit::fresh_shell,
                trash: move_to_trash,
                shell: crate::platform::shell::login_shell,
                shell_env: crate::env_shell::launch_snapshot,
                scheduled_shell_env: crate::env_shell::cached_snapshot,
            }),
        }
    }
}

impl HostTools {
    pub fn get(&self) -> Tools {
        *self
            .tools
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The user's `git` as browse reads run it: isolated from the user's repository settings.
    pub fn isolated_git(&self) -> Result<IsolatedGit> {
        self.isolated_git_for(Pace::Scheduled)
    }

    /// [`HostTools::isolated_git`] for `pace` ([`Tools::git_for`]).
    fn isolated_git_for(&self, pace: Pace) -> Result<IsolatedGit> {
        self.get().git_for(pace).map(IsolatedGit::of)
    }

    /// The `git` resolver of one browse request: a request a person asked for looks for a `git`
    /// that is missing once, at the first resolution, and the resolutions after it take
    /// [`Pace::Scheduled`], which see the fresh snapshot the first one stored. The request would
    /// otherwise take a login shell on every resolution for as long as `git` is missing.
    pub fn isolated_git_per_request(
        &self,
        asked_by_person: bool,
    ) -> impl Fn() -> Result<IsolatedGit> + '_ {
        let person_turn = AtomicBool::new(asked_by_person);
        move || {
            let pace = if person_turn.swap(false, Ordering::Relaxed) {
                Pace::Person
            } else {
                Pace::Scheduled
            };
            self.isolated_git_for(pace)
        }
    }

    #[cfg(test)]
    pub fn replace(&self, tools: Tools) {
        *self.tools.lock().unwrap() = tools;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::test_support::{git_not_found, host_tools};

    fn shell_timed_out() -> Result<UserGit> {
        anyhow::bail!("the login shell timed out")
    }

    #[test]
    fn a_person_is_given_the_git_a_fresh_snapshot_finds_where_the_cached_one_has_none() {
        let tools = Tools {
            git: git_not_found,
            ..host_tools()
        };
        assert!(tools.git_for(Pace::Person).is_ok());
        assert!(is_git_not_found(
            &tools.git_for(Pace::Scheduled).unwrap_err()
        ));
    }

    #[test]
    fn a_browse_read_a_person_asked_for_looks_again_as_well() {
        let tools = HostTools::default();
        tools.replace(Tools {
            git: git_not_found,
            ..host_tools()
        });
        assert!(tools.isolated_git_for(Pace::Person).is_ok());
        assert!(is_git_not_found(&tools.isolated_git().unwrap_err()));
    }

    #[test]
    fn a_browse_request_a_person_asked_for_takes_at_most_one_fresh_snapshot() {
        static FRESH: AtomicUsize = AtomicUsize::new(0);
        fn counted_not_found() -> Result<UserGit> {
            FRESH.fetch_add(1, Ordering::SeqCst);
            git_not_found()
        }
        let tools = HostTools::default();
        tools.replace(Tools {
            git: git_not_found,
            fresh_git: counted_not_found,
            ..host_tools()
        });
        let git = tools.isolated_git_per_request(true);
        for _ in 0..3 {
            assert!(is_git_not_found(&git().unwrap_err()));
        }
        assert_eq!(FRESH.load(Ordering::SeqCst), 1);
        FRESH.store(0, Ordering::SeqCst);
        let scheduled = tools.isolated_git_per_request(false);
        assert!(is_git_not_found(&scheduled().unwrap_err()));
        assert_eq!(FRESH.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_git_neither_snapshot_finds_is_refused_as_not_found() {
        let tools = Tools {
            git: git_not_found,
            fresh_git: git_not_found,
            ..host_tools()
        };
        assert!(is_git_not_found(&tools.git_for(Pace::Person).unwrap_err()));
    }

    #[test]
    fn a_fresh_snapshot_that_cannot_be_taken_leaves_the_first_refusal() {
        let tools = Tools {
            git: git_not_found,
            fresh_git: shell_timed_out,
            ..host_tools()
        };
        assert!(is_git_not_found(&tools.git_for(Pace::Person).unwrap_err()));
    }

    #[test]
    fn only_a_git_that_was_not_found_is_looked_for_again() {
        let tools = Tools {
            git: shell_timed_out,
            fresh_git: || unreachable!("no fresh snapshot for another failure"),
            ..host_tools()
        };
        let err = tools.git_for(Pace::Person).unwrap_err();
        assert!(format!("{err:#}").contains("timed out"));
        let other_binary = CodedError::raised(
            error_code::BINARY_NOT_FOUND,
            "`claude` was not found",
            &[("binary", "claude")],
        );
        assert!(!is_git_not_found(&other_binary));
    }
}
