//! A project's recorded git remote URL: what counts as the project's remote, reading it from a
//! repository, and keeping the stored value true to it.
//!
//! One read serves every path that records it. Association (`add_project`) takes it once,
//! `reconcile_remote_urls` re-reads every project at daemon start, and the git status check reads
//! it in the pass where it already inspects the repository's remotes (see `status.rs`); all of
//! them go through [`inspect`] and [`Inspection::url`], so none can disagree about which remote is
//! the project's.
//!
//! The recorded value is the repository's own configuration, not how this machine's `git` resolves
//! it: a `url.<base>.insteadOf` rewrite is a setting of the machine and stays out of the stored
//! value.

use std::cell::OnceCell;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::blocking_pass::each_blocking;
use crate::git::env::SshConfiguration;
use crate::git::layout::is_git_repo;
use crate::git::remote_branch::split_remote_branch;
use crate::git::run::{GitError, Limits, Runner};
use crate::host_tools::Pace;
use crate::protocol::Project;
use crate::state::AppState;

/// Upper bound on one `git` read here. All of them read the repository's own files, so this is a
/// guard against a wedged repository (a dead network volume, a held lock), not a network
/// allowance; it is the browse reads' bound (`GIT_READ_TIMEOUT` in `git/isolated.rs`) for the same
/// reason, and a read that outlasts it is a read that could not be made, which leaves the recorded
/// URL alone.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// What each `git` read here is held to: [`READ_TIMEOUT`], and no output ceiling, a repository's
/// remotes being few.
const READ_LIMITS: Limits = Limits::deadline(READ_TIMEOUT);

/// What runs `git <args>` in a repository and returns its stdout, or why it did not.
pub type RunGit<'a> = &'a dyn Fn(&[&str]) -> Result<String, GitError>;

/// The runner for the repository at `path`: `git` (a project's, `super::project_git`) held to
/// [`READ_LIMITS`], each command a read, which keeps it from rewriting the repository's index.
///
/// Of the `GIT_*` variables of the user's shell only those saying which of their own material to
/// use are kept (see `crate::git::env::user_configuration_only`), and more is at stake here than
/// elsewhere: what this reads is written to the database and broadcast, not shown for a moment.
pub fn runner<'a>(
    git: &Runner,
    path: &'a Path,
) -> impl Fn(&[&str]) -> Result<String, GitError> + 'a {
    let git = git.with_limits(READ_LIMITS);
    move |args| {
        git.read(path, args)
            .map(|stdout| String::from_utf8_lossy(&stdout).into_owned())
    }
}

/// A configured remote of a repository: its name and its URL as written in the configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

/// What the branch's upstream read came to.
enum Upstream {
    /// The upstream remote's name.
    Remote(String),
    /// The branch has no upstream. `git` says so with exit status 128, which it uses for other
    /// failures too, so any exit status counts as this.
    None,
    /// `git` did not run to an exit — it could not be started, timed out or was killed — so
    /// whether there is an upstream is not known.
    Unknown,
}

/// What a repository says about its remotes. The upstream remote is read the first time something
/// asks for it, because the recorded URL needs it only when there is no `origin`.
pub struct Inspection<'a> {
    /// In the order `git config` lists them, which is by configuration scope — system, then
    /// global, then local, then worktree; empty when the repository has no remote.
    pub remotes: Vec<Remote>,
    /// Its `core.sshCommand`, taken in the same read, for a fetch or a push from it to reach a
    /// remote with (`UserGit::remote_command`) at no `git` of its own.
    pub ssh: SshConfiguration,
    run: RunGit<'a>,
    upstream: OnceCell<Upstream>,
}

impl Inspection<'_> {
    fn upstream_read(&self) -> &Upstream {
        self.upstream.get_or_init(|| {
            if self.remotes.is_empty() {
                // Nothing to read: there is no remote for an upstream to name.
                Upstream::None
            } else {
                upstream_remote(self.run, &self.remotes)
            }
        })
    }

    /// The current branch's upstream remote, e.g. `origin` for an upstream of `origin/main`; none
    /// when the branch has none, and also when that could not be read, which the fetch target it
    /// serves can do without.
    pub fn upstream(&self) -> Option<&str> {
        match self.upstream_read() {
            Upstream::Remote(name) => Some(name),
            Upstream::None | Upstream::Unknown => None,
        }
    }

    /// The URL to record as the project's remote: `origin`'s, else the upstream remote's, else the
    /// first remote listed, which by the scope order of `remotes` is a system or global one before
    /// any of the repository's own. A repository with no `origin` and no upstream, configured with
    /// only a local remote beside a global one, therefore records the global one's URL, which may
    /// have nothing to do with it; the combination is rare enough to leave.
    ///
    /// The inner `None` is a repository with no remote; the outer is "cannot tell", which is an
    /// upstream that could not be read in a repository with no `origin`, and leaves the recorded
    /// URL alone rather than guessing a remote.
    ///
    /// `origin` comes before the upstream because the recorded value says where the working tree
    /// came from, and must not move with the branch: in a fork workflow `main` tracks
    /// `upstream/main` while `feature` tracks `origin/feature`, and an upstream-first rule would
    /// rewrite the value on every branch switch.
    pub fn url(&self) -> Option<Option<String>> {
        let named = |name: &str| self.remotes.iter().find(|remote| remote.name == name);
        if let Some(origin) = named("origin") {
            return Some(Some(origin.url.clone()));
        }
        let upstream = match self.upstream_read() {
            Upstream::Unknown => return None,
            Upstream::Remote(name) => named(name),
            Upstream::None => None,
        };
        Some(
            upstream
                .or_else(|| self.remotes.first())
                .map(|remote| remote.url.clone()),
        )
    }
}

/// Reads the repository's remotes with `run`. An error is a read that could not be made, carrying
/// the failure that stopped it, which is not the same as a repository with no remote: the first
/// leaves the recorded URL alone, the second clears it.
pub fn inspect(run: RunGit) -> Result<Inspection, GitError> {
    let (remotes, ssh) = read_remote_configuration(run)?;
    Ok(Inspection {
        remotes,
        ssh,
        run,
        upstream: OnceCell::new(),
    })
}

/// The keys [`read_remote_configuration`] reads: each remote's URL, and `core.sshCommand`, in the
/// lower case `git config` matches and prints them in.
const REMOTE_CONFIGURATION_KEYS: &str = r"^(remote\..*\.url|core\.sshcommand)$";

/// The remotes in the repository's configuration, with the URL each is configured with, and its
/// `core.sshCommand`. The read takes `git`'s whole configuration — system, global, local and
/// worktree — the same set `git remote` and `git fetch` see, so a remote that only the user's
/// global configuration defines is found here as well as fetched from. `git config` applies no
/// `insteadOf` rewrite. The pattern ends in `.url` so a `pushurl` is not read as a remote.
///
/// `git config --get-regexp` answers "nothing matched" with exit status 1, and that status alone
/// means the repository has no remote, once a repository is confirmed to be there. Any other
/// failure — a different status, a timeout, a `git` killed by a signal, which also prints nothing —
/// is a read that could not be made, and is the error returned.
///
/// The confirmation covers only a result with no remote in it. Where there is no repository at all,
/// `git config` reads the global configuration and can exit 1 just the same, so an empty result is
/// checked with `git rev-parse --git-dir` before it is taken for a repository with no remote, which
/// keeps a damaged `.git` from clearing the recorded URL. A result with content is not confirmed
/// and cannot be traced to its source: both commands search the parent directories, so a damaged
/// `.git` can have the read answer for a repository enclosing this one. That case opens only while
/// the `.git` is damaged, and closes when it is repaired, at the next check. Reading the user's
/// global configuration is not that case — it is the parity with `git remote` described above, and
/// it holds for a healthy repository too.
fn read_remote_configuration(run: RunGit) -> Result<(Vec<Remote>, SshConfiguration), GitError> {
    let (remotes, ssh) = match run(&["config", "-z", "--get-regexp", REMOTE_CONFIGURATION_KEYS]) {
        Ok(output) => parse_remote_configuration(&output),
        Err(failure) if failure.code() == Some(1) => (Vec::new(), SshConfiguration::Unset),
        Err(failure) => {
            tracing::debug!(?failure, "reading the repository's remotes failed");
            return Err(failure);
        }
    };
    if remotes.is_empty() {
        if let Err(failure) = run(&["rev-parse", "--git-dir"]) {
            tracing::debug!(?failure, "no remote read, and no repository confirmed");
            return Err(failure);
        }
    }
    Ok((remotes, ssh))
}

/// The remotes and the `core.sshCommand` in the output of `git config -z --get-regexp`, whose
/// entries read `<key>\n<value>`, each ended by a NUL; a remote name may itself contain dots. A
/// remote configured with several URLs is read as its first, while `core.sshCommand` set more than
/// once is its last, as `git` takes it. A pure function over the text, so it is unit-tested with
/// literal strings.
fn parse_remote_configuration(output: &str) -> (Vec<Remote>, SshConfiguration) {
    let mut remotes: Vec<Remote> = Vec::new();
    let mut ssh = None;
    for entry in output.split('\0') {
        let Some((key, value)) = entry.split_once('\n') else {
            continue;
        };
        if key == "core.sshcommand" {
            ssh = Some(value.to_string());
            continue;
        }
        let Some(name) = key
            .strip_prefix("remote.")
            .and_then(|rest| rest.strip_suffix(".url"))
        else {
            continue;
        };
        let url = value.trim();
        if name.is_empty() || url.is_empty() || remotes.iter().any(|remote| remote.name == name) {
            continue;
        }
        remotes.push(Remote {
            name: name.to_string(),
            url: url.to_string(),
        });
    }
    (remotes, SshConfiguration::of(ssh))
}

/// The branch's upstream remote, e.g. `origin` for an upstream of `origin/main`. A branch with no
/// upstream is the ordinary case for a fresh one, and is [`Upstream::None`], not a failure.
fn upstream_remote(run: RunGit, remotes: &[Remote]) -> Upstream {
    match run(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ]) {
        Ok(output) => split_remote_branch(
            output.trim(),
            remotes.iter().map(|remote| remote.name.as_str()),
        )
        .map_or(Upstream::None, |(name, _)| {
            Upstream::Remote(name.to_string())
        }),
        Err(failure) if failure.code().is_none() => {
            tracing::debug!(?failure, "reading the branch's upstream failed");
            Upstream::Unknown
        }
        Err(_) => Upstream::None,
    }
}

/// Where a project's directory stands, as far as its remote is concerned.
enum Directory {
    /// There is no directory to see at the path: it is missing, on a volume that is not mounted,
    /// or replaced by something that is not a directory. None of those is evidence that the
    /// repository was taken out of git, so the recorded URL, which can still place the project on
    /// another machine, stays.
    Unseen,
    /// A directory that is not a git repository.
    NotARepository,
    Repository,
}

/// The one place that decides what a path is for the purposes of recording, so that "cannot see
/// it" is never taken for "is not a repository" by one path and not another.
fn classify(path: &Path) -> Directory {
    if !path.is_dir() {
        Directory::Unseen
    } else if !is_git_repo(path) {
        Directory::NotARepository
    } else {
        Directory::Repository
    }
}

/// What the project at `path` should have recorded, its repository's remotes read by `read`,
/// which runs only where there is a repository. The outer `None` is "cannot tell": the directory
/// cannot be seen, or the remotes could not be read. The inner is the answer: a directory that is
/// there but is not a repository, and a repository without a remote, have no URL, and that is no
/// error.
fn observe_with(
    path: &Path,
    read: impl FnOnce() -> Option<Option<String>>,
) -> Option<Option<String>> {
    match classify(path) {
        Directory::Unseen => None,
        Directory::NotARepository => Some(None),
        Directory::Repository => read(),
    }
}

/// The URL a repository's remotes, read with `run`, give; `None` where they could not be read.
fn url_with(run: RunGit) -> Option<Option<String>> {
    inspect(run).ok().and_then(|inspection| inspection.url())
}

fn observe(state: &AppState, path: &Path) -> Option<Option<String>> {
    observe_with(path, || match super::project_git(state, Pace::Scheduled) {
        Ok(git) => url_with(&runner(&git, path)),
        Err(err) => {
            tracing::debug!(%err, "no git to read the project's remotes with");
            None
        }
    })
}

/// The URL to record for the project at `path`; blocking, the caller runs it off the runtime. Both
/// kinds of "nothing to record" are `None` here: association stores nothing either way.
pub fn read_remote_url(state: &AppState, path: &Path) -> Option<String> {
    observe(state, path).flatten()
}

/// For the git status check, once it has found the project's directory is not a repository: clears
/// the recorded URL, unless the directory cannot be seen at all, in which case it says nothing
/// about the remote and the value stays.
pub fn record_for_non_repository(state: &AppState, project_id: &str, path: &Path) {
    if matches!(classify(path), Directory::NotARepository) {
        record_remote_url(state, project_id, None);
    }
}

/// For the git status check, once it has read the project's repository: records the URL the read
/// gives. A read that could not be made (`None`) leaves the recorded URL as it is.
pub fn record_inspected(state: &AppState, project_id: &str, inspection: Option<&Inspection>) {
    if let Some(url) = inspection.and_then(Inspection::url) {
        record_remote_url(state, project_id, url);
    }
}

/// Makes the stored remote URL of a project `url` and broadcasts the project when that changed
/// anything, the way any other change to a project reaches clients. A project that is gone, or a
/// store that cannot be read or written, is left alone: this runs behind the user's back, and a
/// missed update is picked up by the next one.
pub fn record_remote_url(state: &AppState, project_id: &str, url: Option<String>) {
    let Ok(Some(stored)) = state.store.get_project(project_id) else {
        return;
    };
    if stored.remote_url == url {
        return;
    }
    if let Err(err) = state
        .store
        .set_project_remote_url(project_id, url.as_deref())
    {
        tracing::debug!(%err, "recording the project's remote URL failed");
        return;
    }
    // Read back rather than sent as `stored` with the new URL: the user may have edited the
    // project in between, and an upsert carries the whole record.
    if let Err(err) = state.publish_project(project_id) {
        tracing::debug!(%err, "publishing the project's remote URL failed");
    }
}

/// Re-reads every project's remote URL from its directory and corrects what is stored, in a task
/// of its own so the daemon's start does not wait for it, [`BLOCKING_WIDTH`] directories at a
/// time. The git status check only visits the console a client is showing, so a project of a
/// console nobody has opened would otherwise keep a remote the user changed since, and a
/// configuration exported from it would name the wrong repository.
///
/// [`BLOCKING_WIDTH`]: crate::blocking_pass::BLOCKING_WIDTH
pub fn reconcile_remote_urls(state: &Arc<AppState>) {
    let projects = match state.store.list_projects() {
        Ok(projects) => projects,
        Err(err) => {
            tracing::warn!(%err, "listing projects to reconcile their remote URLs failed");
            return;
        }
    };
    let state = state.clone();
    tokio::spawn(async move {
        each_blocking(projects, move |project| {
            reconcile_project(&state, &project, |path| observe(&state, path))
        })
        .await;
    });
}

fn reconcile_project(
    state: &AppState,
    project: &Project,
    observe: impl Fn(&Path) -> Option<Option<String>>,
) {
    if let Some(url) = observe(Path::new(&project.path)) {
        record_remote_url(state, &project.id, url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::run::UserGit;
    use crate::protocol::{AgentId, Console, Event, ProjectSource};
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{
        app_state, fixture_git, git, isolated_git_env, redirecting_overrides, repo_pair, repo_with,
        ScratchDir, SYSTEM_GIT,
    };

    fn remote(name: &str, url: &str) -> Remote {
        Remote {
            name: name.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn remotes_are_read_from_the_url_keys_and_not_from_pushurls() {
        let (remotes, ssh) = parse_remote_configuration(
            "remote.origin.url\nhttps://example.com/o/r.git\0\
             remote.origin.pushurl\nssh://git@example.com/o/r.git\0\
             remote.my.fork.url\n/srv/git/r\0\
             remote.origin.url\nignored-second-value\0",
        );
        assert_eq!(
            remotes,
            vec![
                remote("origin", "https://example.com/o/r.git"),
                remote("my.fork", "/srv/git/r")
            ]
        );
        assert_eq!(ssh, SshConfiguration::Unset);
        assert_eq!(
            parse_remote_configuration(""),
            (Vec::new(), SshConfiguration::Unset)
        );
    }

    /// `core.sshCommand` is its last value, whole, a line break in it included, and an empty one
    /// is no setting.
    #[test]
    fn the_ssh_command_is_read_beside_the_remotes() {
        let (remotes, ssh) = parse_remote_configuration(
            "core.sshcommand\nssh -i first\0\
             remote.origin.url\nu-origin\0\
             core.sshcommand\nssh -i 'a key'\nsecond line\0",
        );
        assert_eq!(remotes, vec![remote("origin", "u-origin")]);
        assert_eq!(
            ssh,
            SshConfiguration::Command("ssh -i 'a key'\nsecond line".to_string())
        );
        assert_eq!(
            parse_remote_configuration("core.sshcommand\n\0").1,
            SshConfiguration::Unset
        );
    }

    /// A configuration with `core.sshCommand` and no remote still has its repository confirmed
    /// before it is taken for one with no remote.
    #[test]
    fn an_ssh_command_without_a_remote_is_confirmed_like_no_remote() {
        let confirmed = |there: bool| {
            move |args: &[&str]| match args[0] {
                "config" => Ok("core.sshcommand\nssh -i k\0".to_string()),
                _ if there => Ok(".git\n".to_string()),
                _ => Err(GitError::Exited {
                    code: Some(128),
                    stderr: "not a git repository".to_string(),
                }),
            }
        };
        let there = confirmed(true);
        let inspection = inspect(&there).expect("confirmed");
        assert!(inspection.remotes.is_empty());
        assert_eq!(
            inspection.ssh,
            SshConfiguration::Command("ssh -i k".to_string())
        );
        let missing = confirmed(false);
        assert!(inspect(&missing).is_err());
    }

    /// A remote's own name may hold a slash, so the upstream is split by the configured remotes.
    #[test]
    fn the_upstream_remote_is_the_longest_configured_name_that_begins_the_upstream() {
        let upstream = |names: &[&str], upstream: &str| {
            let reads = std::cell::Cell::new(0);
            let run = fake(names, Some(upstream), &reads);
            inspect(&run)
                .expect("listed")
                .upstream()
                .map(str::to_string)
        };
        assert_eq!(upstream(&["origin"], "origin"), Some("origin".to_string()));
        assert_eq!(
            upstream(&["team", "team/origin"], "team/origin"),
            Some("team/origin".to_string())
        );
        assert_eq!(
            upstream(&["team", "origin"], "team"),
            Some("team".to_string())
        );
        assert_eq!(upstream(&["origin"], "elsewhere"), None);
    }

    /// A runner answering from a fixed list of remotes and an upstream, counting how many times
    /// the upstream was asked for.
    fn fake<'a>(
        names: &[&str],
        upstream: Option<&str>,
        upstream_reads: &'a std::cell::Cell<u32>,
    ) -> impl Fn(&[&str]) -> Result<String, GitError> + 'a {
        let listing: String = names
            .iter()
            .map(|name| format!("remote.{name}.url\nu-{name}\0"))
            .collect();
        let upstream = upstream.map(|name| format!("{name}/main\n"));
        move |args| match args[0] {
            "config" if listing.is_empty() => Err(GitError::Exited {
                code: Some(1),
                stderr: String::new(),
            }),
            "config" => Ok(listing.clone()),
            "rev-parse" if args[1] == "--git-dir" => Ok(".git\n".to_string()),
            _ => {
                upstream_reads.set(upstream_reads.get() + 1);
                upstream.clone().ok_or(GitError::Exited {
                    code: Some(128),
                    stderr: "no upstream".to_string(),
                })
            }
        }
    }

    #[test]
    fn the_recorded_url_is_origins_then_the_upstreams_then_the_first() {
        let url = |names: &[&str], upstream: Option<&str>| {
            let reads = std::cell::Cell::new(0);
            let run = fake(names, upstream, &reads);
            let url = inspect(&run).expect("listed").url();
            (url, reads.get())
        };
        let some = |url: &str| Some(Some(url.to_string()));
        // Origin wins over the upstream, so the value does not follow the checked-out branch, and
        // the upstream is not even asked for.
        assert_eq!(
            url(&["fork", "origin"], Some("fork")),
            (some("u-origin"), 0)
        );
        assert_eq!(url(&["b", "a"], Some("a")), (some("u-a"), 1));
        // An upstream naming a remote that is not listed falls through rather than recording
        // nothing.
        assert_eq!(url(&["b", "a"], Some("gone")), (some("u-b"), 1));
        assert_eq!(url(&["b", "a"], None), (some("u-b"), 1));
        assert_eq!(url(&[], None), (Some(None), 0));
    }

    #[test]
    fn an_upstream_that_could_not_be_read_leaves_the_url_unless_origin_answers_alone() {
        let run_with = |names: &'static [&'static str]| {
            move |args: &[&str]| match args[0] {
                "config" => Ok(names
                    .iter()
                    .map(|name| format!("remote.{name}.url\nu-{name}\0"))
                    .collect()),
                _ => Err(GitError::Exited {
                    code: None,
                    stderr: "killed".to_string(),
                }),
            }
        };
        assert_eq!(inspect(&run_with(&["b", "a"])).unwrap().url(), None);
        assert_eq!(
            inspect(&run_with(&["b", "origin"])).unwrap().url(),
            Some(Some("u-origin".to_string()))
        );
        // A branch with no upstream is an exit status, and is a branch with no upstream.
        let no_upstream = |args: &[&str]| match args[0] {
            "config" => Ok("remote.b.url\nu-b\0remote.a.url\nu-a\0".to_string()),
            _ => Err(GitError::Exited {
                code: Some(128),
                stderr: "no upstream".to_string(),
            }),
        };
        assert_eq!(
            inspect(&no_upstream).unwrap().url(),
            Some(Some("u-b".to_string()))
        );
    }

    #[test]
    fn only_exit_status_one_means_no_remote() {
        let failing = |code: Option<i32>| {
            move |args: &[&str]| match args[0] {
                "rev-parse" => Ok(".git\n".to_string()),
                _ => Err(GitError::Exited {
                    code,
                    stderr: String::new(),
                }),
            }
        };
        assert_eq!(
            inspect(&failing(Some(1))).expect("no remote").remotes,
            vec![]
        );
        // A `git` killed by a signal exits with no status and prints nothing, which is not "no
        // remote" and must not clear a recorded URL.
        assert!(inspect(&failing(None)).is_err());
        assert!(inspect(&failing(Some(128))).is_err());
    }

    /// The inspection's one `git config` read gives the repository's `core.sshCommand` beside its
    /// remotes, from its own configuration.
    #[test]
    fn the_inspection_reads_the_repositorys_ssh_command() {
        let repo = repo_with("remote-ssh-command", &[(b"a.txt", b"one\n")]);
        let run = |args: &[&str]| {
            let output = fixture_git(&repo).args(args).output().expect("git runs");
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(GitError::Exited {
                    code: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                })
            }
        };
        assert_eq!(inspect(&run).expect("read").ssh, SshConfiguration::Unset);
        git(&repo, &["config", "core.sshCommand", "ssh -i /k"]);
        let inspection = inspect(&run).expect("read");
        assert!(inspection.remotes.is_empty());
        assert_eq!(
            inspection.ssh,
            SshConfiguration::Command("ssh -i /k".to_string())
        );
        git(
            &repo,
            &["remote", "add", "origin", "git@example.invalid:o/r.git"],
        );
        let inspection = inspect(&run).expect("read");
        assert_eq!(
            inspection.remotes,
            vec![remote("origin", "git@example.invalid:o/r.git")]
        );
        assert_eq!(
            inspection.ssh,
            SshConfiguration::Command("ssh -i /k".to_string())
        );
    }

    /// `git` through the fixture helper's isolated environment rather than the user's login shell.
    fn observe_in(dir: &Path) -> Option<Option<String>> {
        observe_with(dir, || {
            url_with(&|args| {
                let output = fixture_git(dir).args(args).output().expect("git runs");
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
                } else {
                    Err(GitError::Exited {
                        code: output.status.code(),
                        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                    })
                }
            })
        })
    }

    #[test]
    fn a_visible_directory_that_is_no_repository_has_no_url_and_an_unseen_one_cannot_tell() {
        let plain = ScratchDir::new("git-remote-plain");
        assert_eq!(observe_in(&plain), Some(None));
        assert_eq!(observe_in(&plain.join("not-mounted")), None);
        // A file where the directory was is no directory to see either.
        std::fs::write(plain.join("a-file"), "").expect("file");
        assert_eq!(observe_in(&plain.join("a-file")), None);
    }

    #[test]
    fn a_repository_without_a_remote_has_none_and_an_unreadable_one_cannot_tell() {
        let repo = repo_with("git-remote-none", &[(b"f.txt", b"one\n")]);
        assert_eq!(observe_in(&repo), Some(None));

        // A `.git` that is not a repository: the read fails, which says nothing about a remote.
        let broken = ScratchDir::new("git-remote-broken");
        std::fs::create_dir(broken.join(".git")).expect("fake .git");
        assert_eq!(observe_in(&broken), None);
    }

    #[test]
    fn the_configured_url_is_recorded_and_the_machines_rewrite_of_it_is_not() {
        let repo = repo_with("git-remote-raw", &[(b"f.txt", b"one\n")]);
        git(
            &repo,
            &["remote", "add", "origin", "https://example.com/o/r.git"],
        );
        git(
            &repo,
            &[
                "config",
                "url.ssh://git@example.com/.insteadOf",
                "https://example.com/",
            ],
        );
        // A separate push URL is not the remote's URL.
        git(
            &repo,
            &[
                "remote",
                "set-url",
                "--push",
                "origin",
                "ssh://git@push.example.com/o/r.git",
            ],
        );
        // The rewrite is really in force, so this is the value the test must not record.
        assert_eq!(
            git(&repo, &["remote", "get-url", "origin"]),
            b"ssh://git@example.com/o/r.git\n"
        );
        assert_eq!(
            observe_in(&repo),
            Some(Some("https://example.com/o/r.git".to_string()))
        );
    }

    #[test]
    fn the_url_does_not_move_with_the_branch_in_a_fork_workflow() {
        let repo = repo_with("git-remote-fork", &[(b"f.txt", b"one\n")]);
        git(
            &repo,
            &["remote", "add", "upstream", "https://example.com/up/r.git"],
        );
        git(
            &repo,
            &["remote", "add", "origin", "https://example.com/me/r.git"],
        );
        git(&repo, &["update-ref", "refs/remotes/upstream/main", "HEAD"]);
        git(
            &repo,
            &["branch", "--set-upstream-to=upstream/main", "main"],
        );
        git(&repo, &["checkout", "-q", "-b", "feature"]);
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/feature", "HEAD"],
        );
        git(
            &repo,
            &["branch", "--set-upstream-to=origin/feature", "feature"],
        );

        let on_feature = observe_in(&repo);
        git(&repo, &["checkout", "-q", "main"]);
        assert_eq!(observe_in(&repo), on_feature);
        assert_eq!(
            on_feature,
            Some(Some("https://example.com/me/r.git".to_string()))
        );

        // With no `origin`, the upstream's remote is the one recorded.
        git(&repo, &["remote", "remove", "origin"]);
        git(
            &repo,
            &["remote", "add", "zzz", "https://example.com/z/r.git"],
        );
        assert_eq!(
            observe_in(&repo),
            Some(Some("https://example.com/up/r.git".to_string()))
        );
    }

    #[test]
    fn the_read_keeps_the_users_configuration_locations_and_drops_other_git_variables() {
        let (ours, other) = repo_pair("git-remote");
        git(
            &other,
            &["remote", "add", "origin", "https://example.com/other.git"],
        );
        let scratch = ScratchDir::new("git-remote-env-config");
        let global = scratch.join("gitconfig");
        std::fs::write(
            &global,
            "[remote \"shared\"]\n\turl = https://example.com/shared.git\n",
        )
        .expect("global config");
        let mut env = isolated_git_env();
        // Kept: the repository has no remote of its own, so the URL can only come from here.
        env.insert(
            "GIT_CONFIG_GLOBAL".to_string(),
            global.to_string_lossy().into(),
        );
        // Dropped: honoured, they would send the read to the other repository.
        env.extend(
            redirecting_overrides(&other)
                .into_iter()
                .map(|(key, value)| (key.to_string(), value)),
        );
        let git = super::super::project_runner(UserGit::new(SYSTEM_GIT, env));
        let run = runner(&git, &ours);
        assert_eq!(
            inspect(&run).unwrap().url(),
            Some(Some("https://example.com/shared.git".to_string()))
        );
    }

    fn state_with_project(label: &str, remote_url: Option<&str>) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = app_state(label);
        state
            .store
            .insert_console(&Console {
                id: "console-1".to_string(),
                name: "Console".to_string(),
                workdir: dir.join("workdir").to_string_lossy().into_owned(),
                default_agent: AgentId::CLAUDE,
                project_session_agent: None,
                accounts: Default::default(),
                launch: Default::default(),
                project_session_launch: Default::default(),
                icon: None,
                console_session_prompt: None,
                created_at: 0,
                versions: Default::default(),
            })
            .expect("console");
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Project".to_string(),
                path: "/tmp/project-1".to_string(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: remote_url.map(str::to_string),
                accounts: Default::default(),
                launch: Default::default(),
                trust_consent: false,
                pinned: false,
                tags: Vec::new(),
                description: None,
                git_workflow: Default::default(),
                versions: Default::default(),
            })
            .expect("project");
        (state, dir)
    }

    fn stored_url(state: &AppState) -> Option<String> {
        state
            .store
            .get_project("project-1")
            .unwrap()
            .unwrap()
            .remote_url
    }

    #[test]
    fn a_changed_remote_url_is_stored_and_broadcast_and_an_unchanged_one_is_not() {
        let (state, _dir) = state_with_project("git-remote-record", Some("old"));
        let mut events = state.subscribe();

        record_remote_url(&state, "project-1", Some("old".to_string()));
        assert!(events.try_recv().is_err(), "nothing changed, nothing sent");

        record_remote_url(&state, "project-1", Some("new".to_string()));
        assert_eq!(stored_url(&state).as_deref(), Some("new"));
        match events.try_recv() {
            Ok(Event::ProjectUpserted { project }) => {
                assert_eq!(project.remote_url.as_deref(), Some("new"))
            }
            other => panic!("expected the project upserted, got {other:?}"),
        }

        record_remote_url(&state, "project-1", None);
        assert_eq!(stored_url(&state), None);
        assert!(matches!(
            events.try_recv(),
            Ok(Event::ProjectUpserted { .. })
        ));
    }

    #[test]
    fn reconciling_fills_corrects_and_clears_but_leaves_a_project_it_cannot_tell_about() {
        let (state, _dir) = state_with_project("git-remote-reconcile", None);
        let project = || state.store.get_project("project-1").unwrap().unwrap();

        reconcile_project(&state, &project(), |path| {
            assert_eq!(path, Path::new("/tmp/project-1"));
            Some(Some("found".to_string()))
        });
        assert_eq!(stored_url(&state).as_deref(), Some("found"));

        reconcile_project(&state, &project(), |_| Some(Some("moved".to_string())));
        assert_eq!(stored_url(&state).as_deref(), Some("moved"));

        reconcile_project(&state, &project(), |_| None);
        assert_eq!(stored_url(&state).as_deref(), Some("moved"));

        reconcile_project(&state, &project(), |_| Some(None));
        assert_eq!(stored_url(&state), None);
    }

    #[test]
    fn the_check_records_what_it_read_and_leaves_the_url_when_it_read_nothing() {
        let (state, _dir) = state_with_project("git-remote-check", Some("old"));
        let reads = std::cell::Cell::new(0);

        let run = fake(&["origin"], None, &reads);
        record_inspected(&state, "project-1", inspect(&run).as_ref().ok());
        assert_eq!(stored_url(&state).as_deref(), Some("u-origin"));

        // A read that could not be made changes nothing.
        record_inspected(&state, "project-1", None);
        assert_eq!(stored_url(&state).as_deref(), Some("u-origin"));

        // A repository that has lost its remote has the URL cleared.
        let run = fake(&[], None, &reads);
        record_inspected(&state, "project-1", inspect(&run).as_ref().ok());
        assert_eq!(stored_url(&state), None);
    }

    /// A `git` that is stopped never exited, so its failure carries no exit status.
    #[test]
    fn a_stopped_read_has_no_exit_status() {
        let repo = repo_with("git-remote-timeout", &[(b"a.txt", b"ours\n")]);
        // An alias standing in for a `git` that is slow to finish.
        let failure = crate::project_git::test_git()
            .with_limits(Limits::deadline(Duration::from_millis(100)))
            .read(&repo, ["-c", "alias.nap=!sleep 1", "nap"])
            .expect_err("it is stopped");
        assert_eq!(failure.code(), None);
        assert_eq!(failure.to_string(), "git did not finish within 100ms");
    }

    #[test]
    fn a_non_repository_clears_the_url_only_while_its_directory_can_be_seen() {
        let (state, dir) = state_with_project("git-remote-non-repo", Some("kept"));
        let plain = dir.join("plain");
        std::fs::create_dir(&plain).expect("directory");

        record_for_non_repository(&state, "project-1", &dir.join("not-mounted"));
        assert_eq!(stored_url(&state).as_deref(), Some("kept"));

        record_for_non_repository(&state, "project-1", &plain);
        assert_eq!(stored_url(&state), None);
    }
}
