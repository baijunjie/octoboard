//! A project's recorded git remote URL: what counts as the project's remote, reading it from a
//! repository, and keeping the stored value true to it.
//!
//! One read serves every path that records it. Association (`add_project`) takes it once,
//! `reconcile_remote_urls` re-reads every project at daemon start, and the git status check reads
//! it in the pass where it already inspects the repository's remotes (see `git_status.rs`); all of
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

use crate::env_shell;
use crate::hostfs;
use crate::protocol::{Event, Project};
use crate::state::AppState;

/// Upper bound on one `git` read here. All of them read the repository's own files, so this is a
/// guard against a wedged repository (a dead network volume, a held lock), not a network
/// allowance; it is the browse reads' bound (`browse/budget.rs`'s `GIT_READ_TIMEOUT`) for the same
/// reason, and a read that outlasts it is a read that could not be made, which leaves the recorded
/// URL alone.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// How many directories are read at once when many are, at association of a parent directory and
/// at daemon start. Each read is a blocking thread that first waits for the login-shell snapshot
/// (held under one lock for up to ten seconds while the cache is cold) and then runs one `git`, on
/// the blocking pool every other part of the daemon shares: a few hundred repositories read at once
/// would hold that pool for as long as the slowest `git`, and a session opening or a trust key
/// press would queue behind them. A handful keeps the whole pass quick when the reads are fast, and
/// a directory that stalls holds one of these places for up to [`READ_TIMEOUT`] and no more.
pub(crate) const MAX_CONCURRENT_READS: usize = 4;

/// Why a `git` read did not produce output. `code` is the exit status, absent when `git` did not
/// run to an exit at all (it could not be started, timed out, or was killed by a signal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFailure {
    pub code: Option<i32>,
    pub message: String,
}

impl GitFailure {
    fn unrun(err: impl std::fmt::Display) -> Self {
        Self {
            code: None,
            message: err.to_string(),
        }
    }
}

/// What runs `git <args>` in a repository and returns its stdout, or why it did not.
pub type RunGit<'a> = &'a dyn Fn(&[&str]) -> Result<String, GitFailure>;

/// The runner for the repository at `path`: `git` in the user's shell environment, unable to
/// prompt, and held back from rewriting the repository's index.
pub fn runner(path: &Path) -> impl Fn(&[&str]) -> Result<String, GitFailure> + '_ {
    move |args| run_read(path, args)
}

fn run_read(path: &Path, args: &[&str]) -> Result<String, GitFailure> {
    let env = env_shell::cached_snapshot().map_err(|err| GitFailure::unrun(format!("{err:#}")))?;
    let git = env_shell::resolve_binary("git", &env)
        .map_err(|err| GitFailure::unrun(format!("{err:#}")))?;
    let mut command = read_command(Path::new(&git), &env, path, args);
    let output = crate::subprocess::run_with_timeout(&mut command, READ_TIMEOUT)
        .map_err(|err| GitFailure::unrun(format!("{err:#}")))?;
    if !output.status.success() {
        return Err(GitFailure {
            code: output.status.code(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The `git <args>` command to run in `path`, over `env` rather than a lookup of its own so a test
/// can point it at something else than the user's login shell.
fn read_command(
    git: &Path,
    env: &std::collections::HashMap<String, String>,
    path: &Path,
    args: &[&str],
) -> std::process::Command {
    // Of the `GIT_*` variables only those saying which of the user's own material to use are kept
    // (see `git_env::user_configuration_only`), and more is at stake here than elsewhere: what
    // this reads is written to the database and broadcast, not shown for a moment.
    let mut command =
        crate::git_env::non_interactive(git, crate::git_env::user_configuration_only(env));
    crate::git_env::read_only(&mut command);
    command.current_dir(path);
    command.args(args);
    command
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
                upstream_remote(self.run)
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
    /// first remote listed, which by the scope order of `remotes` is a system or global one before any of the
    /// repository's own. A repository with no `origin` and no upstream, configured with only a
    /// local remote beside a global one, therefore records the global one's URL, which may have
    /// nothing to do with it; the combination is rare enough to leave.
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

/// Reads the repository's remotes with `run`. `None` is a read that could not be made, which is
/// not the same as a repository with no remote: the first leaves the recorded URL alone, the
/// second clears it.
pub fn inspect(run: RunGit) -> Option<Inspection> {
    Some(Inspection {
        remotes: list_remotes(run)?,
        run,
        upstream: OnceCell::new(),
    })
}

/// The remotes in the repository's configuration, with the URL each is configured with. The read
/// takes `git`'s whole configuration — system, global, local and worktree — the same set `git
/// remote` and `git fetch` see, so a remote that only the user's global configuration defines is
/// found here as well as fetched from. `git config` applies no `insteadOf` rewrite. The pattern
/// ends in `.url` so a `pushurl` is not read as a remote.
///
/// `git config --get-regexp` answers "nothing matched" with exit status 1, and that status alone
/// means the repository has no remote, once a repository is confirmed to be there. Any other
/// failure — a different status, a timeout, a `git` killed by a signal, which also prints nothing —
/// is a read that could not be made.
///
/// The confirmation covers only an empty result. Where there is no repository at all, `git config`
/// reads the global configuration and can exit 1 just the same, so an empty result is checked with
/// `git rev-parse --git-dir` before it is taken for a repository with no remote, which keeps a
/// damaged `.git` from clearing the recorded URL. A result with content is not confirmed and cannot
/// be traced to its source: both commands search the parent directories, so a damaged `.git` can
/// have the read answer for a repository enclosing this one. That case opens only while the `.git`
/// is damaged, and closes when it is repaired, at the next check. Reading the user's global
/// configuration is not that case — it is the parity with `git remote` described above, and it
/// holds for a healthy repository too.
fn list_remotes(run: RunGit) -> Option<Vec<Remote>> {
    match run(&["config", "--get-regexp", r"^remote\..*\.url$"]) {
        Ok(output) => Some(parse_remote_urls(&output)),
        Err(failure) if failure.code == Some(1) => {
            let confirmed = run(&["rev-parse", "--git-dir"]);
            if let Err(failure) = &confirmed {
                tracing::debug!(?failure, "no remote read, and no repository confirmed");
            }
            confirmed.ok().map(|_| Vec::new())
        }
        Err(failure) => {
            tracing::debug!(?failure, "reading the repository's remotes failed");
            None
        }
    }
}

/// The remotes in the output of `git config --get-regexp`, whose lines read
/// `remote.<name>.url <value>`; a remote name may itself contain dots. A remote configured with
/// several URLs is read as its first. A pure function over the text, so it is unit-tested with
/// literal strings.
fn parse_remote_urls(output: &str) -> Vec<Remote> {
    let mut remotes: Vec<Remote> = Vec::new();
    for line in output.lines() {
        let Some((key, url)) = line.split_once(' ') else {
            continue;
        };
        let Some(name) = key
            .strip_prefix("remote.")
            .and_then(|rest| rest.strip_suffix(".url"))
        else {
            continue;
        };
        let url = url.trim();
        if name.is_empty() || url.is_empty() || remotes.iter().any(|remote| remote.name == name) {
            continue;
        }
        remotes.push(Remote {
            name: name.to_string(),
            url: url.to_string(),
        });
    }
    remotes
}

/// The branch's upstream remote, e.g. `origin` for an upstream of `origin/main`. A branch with no
/// upstream is the ordinary case for a fresh one, and is [`Upstream::None`], not a failure.
fn upstream_remote(run: RunGit) -> Upstream {
    match run(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ]) {
        Ok(output) => remote_from_upstream_ref(&output)
            .map_or(Upstream::None, |name| Upstream::Remote(name.to_string())),
        Err(failure) if failure.code.is_none() => {
            tracing::debug!(?failure, "reading the branch's upstream failed");
            Upstream::Unknown
        }
        Err(_) => Upstream::None,
    }
}

/// The remote name from an `@{upstream}` ref such as `origin/main` — a pure function over the text
/// so the split is unit-tested with literal strings rather than a real repository.
fn remote_from_upstream_ref(ref_text: &str) -> Option<&str> {
    ref_text.trim().split_once('/').map(|(remote, _)| remote)
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
    } else if !hostfs::is_git_repo(path) {
        Directory::NotARepository
    } else {
        Directory::Repository
    }
}

/// What the project at `path` should have recorded, read with `run`. The outer `None` is "cannot
/// tell": the directory cannot be seen, or the remotes could not be read. The inner is the answer:
/// a directory that is there but is not a repository, and a repository without a remote, have no
/// URL, and that is no error.
fn observe_with(path: &Path, run: RunGit) -> Option<Option<String>> {
    match classify(path) {
        Directory::Unseen => None,
        Directory::NotARepository => Some(None),
        Directory::Repository => inspect(run).and_then(|inspection| inspection.url()),
    }
}

fn observe(path: &Path) -> Option<Option<String>> {
    observe_with(path, &runner(path))
}

/// The URL to record for the project at `path`; blocking, the caller runs it off the runtime. Both
/// kinds of "nothing to record" are `None` here: association stores nothing either way.
pub fn read_remote_url(path: &Path) -> Option<String> {
    observe(path).flatten()
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
    if let Ok(Some(project)) = state.store.get_project(project_id) {
        state.broadcast(Event::ProjectUpserted { project });
    }
}

/// Runs `work` over `items` on blocking threads, at most [`MAX_CONCURRENT_READS`] at a time, and
/// returns what each returned; an item whose thread panicked is absent. The next item starts as
/// soon as any one finishes, so one that hangs holds back only its own place.
pub async fn read_bounded<I, T>(
    items: Vec<I>,
    work: impl Fn(I) -> T + Send + Sync + 'static,
) -> Vec<T>
where
    I: Send + 'static,
    T: Send + 'static,
{
    let work = Arc::new(work);
    let mut running = tokio::task::JoinSet::new();
    let mut done = Vec::new();
    for item in items {
        if running.len() >= MAX_CONCURRENT_READS {
            if let Some(Ok(result)) = running.join_next().await {
                done.push(result);
            }
        }
        let work = work.clone();
        running.spawn_blocking(move || work(item));
    }
    while let Some(result) = running.join_next().await {
        if let Ok(result) = result {
            done.push(result);
        }
    }
    done
}

/// Re-reads every project's remote URL from its directory and corrects what is stored, in a task
/// of its own so the daemon's start does not wait for it, [`MAX_CONCURRENT_READS`] directories at a
/// time. The git status check only visits the console a client is showing, so a project of a
/// console nobody has opened would otherwise keep a remote the user changed since, and a
/// configuration exported from it would name the wrong repository.
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
        read_bounded(projects, move |project| {
            reconcile_project(&state, &project, observe)
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

/// Splits a `scheme://userinfo@host...` URL into its scheme, its userinfo and what follows the
/// `@`. `None` for a URL with no userinfo, or of another shape such as `git@host:path`.
fn split_userinfo(url: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (userinfo, _) = rest[..authority_end].rsplit_once('@')?;
    Some((scheme, userinfo, &rest[userinfo.len() + 1..]))
}

/// Whether the userinfo of a `scheme://` URL is a credential: any of it for `http(s)` and `git`,
/// where the user part is routinely a token, and a password for the `ssh` family, whose plain user
/// (`ssh://git@host/...`) is how the remote is named, not a secret.
fn userinfo_is_credential(scheme: &str, userinfo: &str) -> bool {
    if scheme.to_ascii_lowercase().contains("ssh") {
        userinfo.contains(':')
    } else {
        true
    }
}

/// Whether `url` carries a credential in its userinfo, such as `https://user:token@host/repo`.
pub fn has_credentials(url: &str) -> bool {
    split_userinfo(url)
        .is_some_and(|(scheme, userinfo, _)| userinfo_is_credential(scheme, userinfo))
}

/// `url` with any credential in its userinfo removed, for showing a remote without its secret.
pub fn without_credentials(url: &str) -> String {
    let Some((scheme, userinfo, rest)) = split_userinfo(url) else {
        return url.to_string();
    };
    if !userinfo_is_credential(scheme, userinfo) {
        return url.to_string();
    }
    if scheme.to_ascii_lowercase().contains("ssh") {
        let user = userinfo.split(':').next().unwrap_or_default();
        return format!("{scheme}://{user}@{rest}");
    }
    format!("{scheme}://{rest}")
}

/// Whether `url` is one a clone may be made from: an `http(s)`, `ssh` (also as `git+ssh` and
/// `ssh+git`) or `git` URL, or the `user@host:path` shape, with a host that cannot read as an
/// option and no whitespace or control character anywhere in it. A local path, `file://`, the
/// helper transports (`ext::`) and the obsolete `ftp` are not: a clone from a document's URL must
/// reach a remote and never run a command or read a path of this machine. That is stricter than
/// adding a project by hand, which leaves the choice to git. Clones that a document or a tool call
/// asks for are held to it.
pub fn is_clonable_url(url: &str) -> bool {
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let lowered = url.to_ascii_lowercase();
    for scheme in [
        "https://",
        "http://",
        "ssh://",
        "git://",
        "git+ssh://",
        "ssh+git://",
    ] {
        if let Some(rest) = lowered.strip_prefix(scheme) {
            let authority = rest.split('/').next().unwrap_or("");
            return host_is_usable(authority);
        }
    }
    // `[user@]host:path`: no `/` before the first `:`, and no `:` or `//` right after it (`ext::`,
    // `file://`).
    let Some((host, path)) = url.split_once(':') else {
        return false;
    };
    host_is_usable(host)
        && !host.contains('/')
        && !path.is_empty()
        && !path.starts_with(':')
        && !path.starts_with("//")
}

/// Whether `authority` (`[user@]host[:port]`, the host being what follows the last `@`) has a host
/// and cannot be read as an option by `ssh` or `git`, whole or by its host.
fn host_is_usable(authority: &str) -> bool {
    let host = authority.rsplit('@').next().unwrap_or("");
    !host.is_empty() && !authority.starts_with('-') && !host.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Agent, Console, ProjectAccount, ProjectSource};
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{
        app_state, fixture_git, git, isolated_git_env, redirecting_overrides, repo_pair, repo_with,
        ScratchDir, SYSTEM_GIT,
    };

    /// A credential in the userinfo of a `scheme://` URL is told apart from the user an `ssh`
    /// remote is named by, and removed from what is shown; a URL without one is left as it is.
    #[test]
    fn credentials_in_a_remote_url_are_found_and_removed() {
        for (url, shown) in [
            (
                "https://user:token@example.com/r.git",
                "https://example.com/r.git",
            ),
            (
                "https://token@example.com/r.git",
                "https://example.com/r.git",
            ),
            (
                "ssh://git:secret@example.com:22/r.git",
                "ssh://git@example.com:22/r.git",
            ),
        ] {
            assert!(has_credentials(url), "{url}");
            assert_eq!(without_credentials(url), shown);
        }
        for url in [
            "https://example.com/a@b.git",
            "ssh://git@example.com/r.git",
            "git@example.com:owner/r.git",
            "/srv/repo",
        ] {
            assert!(!has_credentials(url), "{url}");
            assert_eq!(without_credentials(url), url);
        }
    }
    #[test]
    fn only_a_url_that_reaches_a_remote_may_be_cloned() {
        for url in [
            "https://github.com/o/r.git",
            "HTTP://example.com/r",
            "ssh://git@example.com/o/r.git",
            "git://example.com/r.git",
            "git+ssh://git@example.com/o/r.git",
            "ssh+git://example.com/o/r.git",
            "ssh://user@host:2222/o/r.git",
            "git@github.com:o/r.git",
            "host:path/r",
        ] {
            assert!(is_clonable_url(url), "{url}");
        }
        for url in [
            "",
            "/srv/git/r",
            "file:///srv/git/r",
            "ext::sh -c touch% /tmp/x",
            "ext::sh",
            "fd::3",
            "--upload-pack=touch",
            "-oProxyCommand=x:y",
            "https://",
            "https:///r",
            "ssh://-oProxyCommand=x/r",
            "https://example.com/r\n--upload-pack=x",
            "https://example.com/a b",
            "r.git",
            ":path",
            "host:",
            "a/b:c",
            "ftp://example.com/r.git",
            "ftps://example.com/r.git",
            // A leading `-` hidden behind the user part.
            "ssh://user@-host/r",
            "ssh://user@/r",
            "user@-host:path/r",
            "-user@host:path/r",
        ] {
            assert!(!is_clonable_url(url), "{url:?}");
        }
    }

    fn remote(name: &str, url: &str) -> Remote {
        Remote {
            name: name.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn remotes_are_read_from_the_url_keys_and_not_from_pushurls() {
        let remotes = parse_remote_urls(
            "remote.origin.url https://example.com/o/r.git\n\
             remote.origin.pushurl ssh://git@example.com/o/r.git\n\
             remote.my.fork.url /srv/git/r\n\
             remote.origin.url ignored-second-value\n",
        );
        assert_eq!(
            remotes,
            vec![
                remote("origin", "https://example.com/o/r.git"),
                remote("my.fork", "/srv/git/r")
            ]
        );
        assert_eq!(parse_remote_urls(""), Vec::new());
    }

    #[test]
    fn the_remote_name_splits_off_the_first_path_segment_of_an_upstream_ref() {
        assert_eq!(remote_from_upstream_ref("origin/main"), Some("origin"));
        // A branch name containing a slash of its own still yields the remote alone: a remote
        // name never contains one.
        assert_eq!(
            remote_from_upstream_ref("upstream/feature/thing"),
            Some("upstream")
        );
        assert_eq!(remote_from_upstream_ref("origin"), None);
    }

    /// A runner answering from a fixed list of remotes and an upstream, counting how many times
    /// the upstream was asked for.
    fn fake<'a>(
        names: &[&str],
        upstream: Option<&str>,
        upstream_reads: &'a std::cell::Cell<u32>,
    ) -> impl Fn(&[&str]) -> Result<String, GitFailure> + 'a {
        let listing: String = names
            .iter()
            .map(|name| format!("remote.{name}.url u-{name}\n"))
            .collect();
        let upstream = upstream.map(|name| format!("{name}/main\n"));
        move |args| match args[0] {
            "config" if listing.is_empty() => Err(GitFailure {
                code: Some(1),
                message: String::new(),
            }),
            "config" => Ok(listing.clone()),
            "rev-parse" if args[1] == "--git-dir" => Ok(".git\n".to_string()),
            _ => {
                upstream_reads.set(upstream_reads.get() + 1);
                upstream.clone().ok_or(GitFailure {
                    code: Some(128),
                    message: "no upstream".to_string(),
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
                    .map(|name| format!("remote.{name}.url u-{name}\n"))
                    .collect()),
                _ => Err(GitFailure {
                    code: None,
                    message: "killed".to_string(),
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
            "config" => Ok("remote.b.url u-b\nremote.a.url u-a\n".to_string()),
            _ => Err(GitFailure {
                code: Some(128),
                message: "no upstream".to_string(),
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
                _ => Err(GitFailure {
                    code,
                    message: String::new(),
                }),
            }
        };
        assert_eq!(
            inspect(&failing(Some(1))).expect("no remote").remotes,
            vec![]
        );
        // A `git` killed by a signal exits with no status and prints nothing, which is not "no
        // remote" and must not clear a recorded URL.
        assert!(inspect(&failing(None)).is_none());
        assert!(inspect(&failing(Some(128))).is_none());
    }

    /// `git` through the fixture helper's isolated environment rather than the user's login shell.
    fn observe_in(dir: &Path) -> Option<Option<String>> {
        observe_in_with_global(dir, None)
    }

    /// As [`observe_in`], with `global` as the user's global `git` configuration.
    fn observe_in_with_global(dir: &Path, global: Option<&Path>) -> Option<Option<String>> {
        observe_with(dir, &|args| {
            let mut command = fixture_git(dir);
            if let Some(global) = global {
                command.env("GIT_CONFIG_GLOBAL", global);
            }
            let output = command.args(args).output().expect("git runs");
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(GitFailure {
                    code: output.status.code(),
                    message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                })
            }
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
    fn a_remote_only_the_global_configuration_defines_is_found() {
        let repo = repo_with("git-remote-global", &[(b"f.txt", b"one\n")]);
        let scratch = ScratchDir::new("git-remote-global-config");
        let global = scratch.join("gitconfig");
        std::fs::write(
            &global,
            "[remote \"shared\"]\n\turl = https://example.com/shared/r.git\n",
        )
        .expect("global config");
        assert_eq!(observe_in(&repo), Some(None));
        assert_eq!(
            observe_in_with_global(&repo, Some(&global)),
            Some(Some("https://example.com/shared/r.git".to_string()))
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
        let run = |args: &[&str]| {
            let output = read_command(Path::new(SYSTEM_GIT), &env, &ours, args)
                .output()
                .expect("git runs");
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(GitFailure {
                    code: output.status.code(),
                    message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                })
            }
        };
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
                console_session_agent: Agent::Claude,
                default_agent: Agent::Claude,
                project_session_agent: None,
                claude_project_session_model: None,
                claude_project_session_effort: None,
                codex_project_session_model: None,
                codex_project_session_effort: None,
                grok_project_session_model: None,
                grok_project_session_effort: None,
                claude_account_id: None,
                codex_account_id: None,
                grok_account_id: None,
                claude_console_session_model: None,
                claude_console_session_effort: None,
                codex_console_session_model: None,
                codex_console_session_effort: None,
                grok_console_session_model: None,
                grok_console_session_effort: None,
                icon: None,
                console_session_prompt: None,
                created_at: 0,
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
                claude_account_id: ProjectAccount::Inherit,
                codex_account_id: ProjectAccount::Inherit,
                grok_account_id: ProjectAccount::Inherit,
                claude_session_model: None,
                claude_session_effort: None,
                codex_session_model: None,
                codex_session_effort: None,
                grok_session_model: None,
                grok_session_effort: None,
                trust_consent: false,
                pinned: false,
                tags: Vec::new(),
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
        let mut events = state.subscribe();
        let reads = std::cell::Cell::new(0);

        // A differing URL is written and broadcast.
        let run = fake(&["origin"], None, &reads);
        record_inspected(&state, "project-1", inspect(&run).as_ref());
        assert_eq!(stored_url(&state).as_deref(), Some("u-origin"));
        assert!(matches!(
            events.try_recv(),
            Ok(Event::ProjectUpserted { project }) if project.remote_url.as_deref() == Some("u-origin")
        ));

        // The same URL again sends nothing.
        record_inspected(&state, "project-1", inspect(&run).as_ref());
        assert!(events.try_recv().is_err());

        // A read that could not be made changes nothing.
        record_inspected(&state, "project-1", None);
        assert_eq!(stored_url(&state).as_deref(), Some("u-origin"));
        assert!(events.try_recv().is_err());

        // A repository that has lost its remote has the URL cleared.
        let run = fake(&[], None, &reads);
        record_inspected(&state, "project-1", inspect(&run).as_ref());
        assert_eq!(stored_url(&state), None);
        assert!(matches!(
            events.try_recv(),
            Ok(Event::ProjectUpserted { .. })
        ));
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
