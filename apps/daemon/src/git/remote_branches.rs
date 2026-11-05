//! Where a local branch's remote-tracking branch and a repository's remote default branch are
//! looked up with `git`: the branch it is set to track, else its namesake on a configured remote,
//! and the branch a remote's `HEAD` names. Only the references asked for are read, by exact name,
//! and every output is held to a budget of its own, so a repository with thousands of
//! remote-tracking references is no slower and a lookup past a budget gives up rather than grows.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::git::isolated::IsolatedGit;
use crate::git::run::GitError;
use crate::processes::subprocess::BoundedFailure;
use crate::protocol::UpstreamInfo;
use crate::wire_path;

/// The most bytes of reference names one upstream lookup asks `git for-each-ref` for: the same
/// branch's name and the `HEAD` on each of the repository's remotes, so room for some one thousand
/// remotes, or fewer where the host's command line holds less than 64 KiB of arguments. The output
/// is about these names again with an object id on each line, plus what lies below a name asked
/// for (`refs/remotes/origin/a` also names `refs/remotes/origin/a/b`), which is not bounded here.
/// This and the two budgets below are recorded in `apps/daemon/PROTOCOL.md`'s "Browse budgets"
/// section, and change together with it.
pub const UPSTREAM_PATTERN_BYTES: usize = {
    let wanted = 64 * 1024;
    let fits = crate::platform::proc::ARGUMENT_BYTES;
    if wanted < fits {
        wanted
    } else {
        fits
    }
};

/// The most stdout the `git remote` of an upstream lookup may print.
pub const UPSTREAM_REMOTES_STDOUT: usize = 32 * 1024;

/// The most stdout the `git for-each-ref` of an upstream lookup may print.
pub const UPSTREAM_LOOKUP_STDOUT: usize = 128 * 1024;

/// The repository's remote default branch, by the rule the branch list's `default_branch` follows
/// ([`find_remote_branches`]), without listing the repository's branches. `None` for a repository
/// with none, or whose remotes are past a budget.
pub fn remote_default_branch(
    env: &IsolatedGit,
    dir: &Path,
    cancel: &AtomicBool,
) -> Result<Option<UpstreamInfo>, GitError> {
    // `HEAD` has no same-named branch on a remote, so only the remotes' own `HEAD`s are looked up.
    Ok(find_remote_branches(env, dir, (b"HEAD", b""), true, cancel)?.default_branch)
}

/// What [`find_remote_branches`] found.
#[derive(Default)]
pub struct Found {
    /// The remote-tracking branch to compare the local branch with.
    pub upstream: Option<UpstreamInfo>,
    /// The repository's remote default branch, looked for only when asked to and the branch has no
    /// upstream.
    pub default_branch: Option<UpstreamInfo>,
    /// False when a lookup was past a budget, and until the lookups have run to the end.
    pub complete: bool,
}

/// The remote-tracking branch to compare local branch `name` with, and, when `with_default` and
/// there is none, the repository's remote default branch; and whether the lookup was complete.
///
/// The upstream is the one the branch is set to track (`configured`, the reference `git` resolved
/// it to, empty for none), else the branch of its name on a configured remote, preferred by
/// [`preferred`]. An upstream that is another local branch, or whose reference is gone, is none. A
/// reference is a remote's branch only as `refs/remotes/<remote>/<name>` for a configured
/// `<remote>`, so `refs/remotes/origin/backup/main` is not `main` on a remote `origin/backup` that
/// was never added.
///
/// The default branch is what `refs/remotes/<remote>/HEAD` names (left by `git clone` or `git
/// remote set-head`) for a configured `<remote>`, preferred by [`preferred`] among the remotes
/// whose `HEAD` is a symbolic reference to a branch the repository has below `refs/remotes/`.
///
/// Only the references asked for are looked up, by exact name: first the upstream and, when it is
/// none, `git remote` and, in one run, the same name and the `HEAD` of each remote, so a
/// repository with thousands of remote-tracking references is no slower, and a branch tracking one
/// costs one `git` run. A lookup that passes a budget ([`UPSTREAM_PATTERN_BYTES`] of names
/// asked for, the output of `git remote` or of a lookup) gives up with what it has found so far,
/// the upstream if that was found, and says it was not complete.
pub fn find_remote_branches(
    env: &IsolatedGit,
    dir: &Path,
    (name, configured): (&[u8], &[u8]),
    with_default: bool,
    cancel: &AtomicBool,
) -> Result<Found, GitError> {
    let mut found = Found::default();
    if configured.starts_with(b"refs/remotes/") {
        let Some(stdout) = lookup(env, dir, vec![configured.to_vec()], cancel)? else {
            return Ok(found);
        };
        if let Some(upstream) = upstream_of(&tips(&stdout), configured) {
            found.upstream = Some(upstream);
            found.complete = true;
            return Ok(found);
        }
    }
    // A branch named `HEAD` has no same-named branch on a remote: that reference is the remote's
    // own `HEAD`.
    let same_name = name != b"HEAD";
    if !same_name && !with_default {
        found.complete = true;
        return Ok(found);
    }
    let stdout = match env.run(dir, "remote", &[], UPSTREAM_REMOTES_STDOUT, cancel) {
        Ok(stdout) => stdout,
        Err(GitError::Stopped(BoundedFailure::StdoutExceeded(_))) => return Ok(found),
        Err(err) => return Err(err),
    };
    let remotes = || {
        stdout
            .split(|&b| b == b'\n')
            .filter(|remote| !remote.is_empty())
    };
    let named = |remote: &[u8]| [b"refs/remotes/", remote, b"/", name].concat();
    let head = |remote: &[u8]| [b"refs/remotes/", remote, b"/HEAD"].concat();
    let mut asked = Vec::new();
    for remote in remotes() {
        if same_name {
            asked.push(named(remote));
        }
        if with_default {
            asked.push(head(remote));
        }
    }
    let Some(lookup_stdout) = lookup(env, dir, asked, cancel)? else {
        return Ok(found);
    };
    let tips = tips(&lookup_stdout);
    found.complete = true;
    if same_name {
        let remote =
            preferred(remotes().filter(|&remote| tips.contains_key(named(remote).as_slice())));
        found.upstream = remote.and_then(|remote| upstream_of(&tips, &named(remote)));
    }
    if with_default && found.upstream.is_none() {
        let default_on = |remote: &[u8]| default_of(&tips, &head(remote));
        let remote = preferred(remotes().filter(|&remote| default_on(remote).is_some()));
        found.default_branch = remote.and_then(default_on);
    }
    Ok(found)
}

/// The remote to take a branch from among those that have it: `origin`, else the first in byte
/// order of the names.
fn preferred<'a>(remotes: impl Iterator<Item = &'a [u8]>) -> Option<&'a [u8]> {
    remotes.min_by_key(|&remote| (remote != b"origin", remote))
}

/// What a lookup printed, by reference: the object id each reference it was asked for names, and
/// the reference it is a symbolic reference to (empty for none).
type Tips<'a> = HashMap<&'a [u8], (&'a [u8], &'a [u8])>;

/// The lines of a lookup's output, `<object id> <reference> <symbolic target>`, indexed by
/// reference.
fn tips(stdout: &[u8]) -> Tips<'_> {
    stdout
        .split(|&b| b == b'\n')
        .filter_map(|line| {
            let mut fields = line.splitn(3, |&b| b == b' ');
            Some((
                fields.next()?,
                fields.next()?,
                fields.next().unwrap_or_default(),
            ))
        })
        .map(|(oid, refname, symref)| (refname, (oid, symref)))
        .collect()
}

/// The tip of remote-tracking reference `refname`, when `tips` (a lookup's answer) has it.
fn upstream_of(tips: &Tips, refname: &[u8]) -> Option<UpstreamInfo> {
    let (commit, _) = tips.get(refname)?;
    let below = refname.strip_prefix(b"refs/remotes/")?;
    Some(UpstreamInfo {
        name: wire_path::encode(below),
        commit: String::from_utf8_lossy(commit).into_owned(),
    })
}

/// The branch the remote's `HEAD`, `refname`, is a symbolic reference to, when `tips` (a lookup's
/// answer) has it and the branch is below `refs/remotes/`; the object id printed for a symbolic
/// reference is its target's.
fn default_of(tips: &Tips, refname: &[u8]) -> Option<UpstreamInfo> {
    let (commit, symref) = tips.get(refname)?;
    let below = symref
        .strip_prefix(b"refs/remotes/")
        .filter(|name| !name.is_empty())?;
    Some(UpstreamInfo {
        name: wire_path::encode(below),
        commit: String::from_utf8_lossy(commit).into_owned(),
    })
}

/// The `--format` of a lookup.
const FORMAT: &[u8] = b"--format=%(objectname) %(refname) %(symref)";

/// Asks `git for-each-ref` for the references `names` by their exact names, printed as [`FORMAT`],
/// and gives back what it printed. `None` when the names are past [`UPSTREAM_PATTERN_BYTES`],
/// or the output passes its budget.
fn lookup(
    env: &IsolatedGit,
    dir: &Path,
    names: Vec<Vec<u8>>,
    cancel: &AtomicBool,
) -> Result<Option<Vec<u8>>, GitError> {
    if names.is_empty() {
        return Ok(Some(Vec::new()));
    }
    if names.iter().map(|name| name.len() + 1).sum::<usize>() > UPSTREAM_PATTERN_BYTES {
        return Ok(None);
    }
    let mut args: Vec<&[u8]> = vec![FORMAT];
    args.extend(names.iter().map(Vec::as_slice));
    match env.run(dir, "for-each-ref", &args, UPSTREAM_LOOKUP_STDOUT, cancel) {
        Ok(stdout) => Ok(Some(stdout)),
        Err(GitError::Stopped(BoundedFailure::StdoutExceeded(_))) => Ok(None),
        Err(err) => Err(err),
    }
}
