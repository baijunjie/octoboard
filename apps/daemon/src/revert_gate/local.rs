//! The local layer: a `reference-transaction` update of a branch, held against the fork points of
//! the branches it takes in. See `docs/product/revert-gate.md`, "A local update of a guarded
//! branch" and "A merge into a branch that is not guarded".

use anyhow::Result;

use super::content::{find_reverts, Finding};
use super::git::{
    branches_at, busy_branches, creation_point, is_ancestor, is_null, is_space, lossy,
    parents_and_tips, remote_tips, rev_count, short, tips_at, upstream_of, Git, Tip,
};
use super::report::{report, RESTORE_HINT};
use super::{Inputs, Route};
use crate::git::object_id::is_object_id;

/// `None` to allow the transaction, or the message that rejects it.
pub(super) fn decide(
    git: &Git,
    inputs: &Inputs,
    state: &str,
    stdin: &[u8],
) -> Result<Option<String>> {
    if state != "prepared" {
        return Ok(None);
    }
    for line in lossy(stdin).split('\n') {
        let parts: Vec<&str> = line
            .split(is_space)
            .filter(|part| !part.is_empty())
            .collect();
        let [old, new, refname] = parts[..] else {
            continue;
        };
        let Some(branch) = refname.strip_prefix("refs/heads/") else {
            continue;
        };
        let guarded = inputs.guarded.contains(branch);
        // A branch that is not guarded is only checked for a merge into it, which moves an
        // existing branch to another commit; a creation resolves to no old value below.
        if !guarded && (is_null(new) || old == new) {
            continue;
        }
        // An update with no expected old value arrives with all zeros as the old value; at this
        // point the ref is already locked but still holds the old value.
        let old = if is_null(old) {
            git.line_lenient(["rev-parse", "--verify", "-q", refname])?
        } else {
            old.to_string()
        };
        if !is_object_id(&old) || !is_object_id(new) {
            continue; // a symref value (`ref:...`) is not a commit
        }
        let mut own = Vec::new();
        let forks = if guarded {
            own = own_tips(git, inputs, branch)?;
            let within = || target_side(git, &own, &old, new);
            forks_of_others(git, inputs, new, branch, &within)?
        } else {
            match merged_forks(git, inputs, branch, &old, new)? {
                Some(forks) => forks,
                None => continue,
            }
        };
        if !guarded {
            own = own_tips(git, inputs, branch)?;
        }
        if let Some(message) = check_move(git, Route::Local, branch, &old, new, &forks, &own)? {
            return Ok(Some(message));
        }
    }
    Ok(None)
}

/// The names of the local branches at `commit` other than `moved`.
fn other_branches_at(git: &Git, commit: &str, moved: &str) -> Result<Vec<String>> {
    Ok(branches_at(git, commit)?
        .into_iter()
        .filter(|name| name != moved)
        .collect())
}

/// The tips of the remote-tracking branches `branch` is published on: the same-named ones, and its
/// upstream, unless that is a guarded branch or its recorded target. A guarded `release` cut from
/// `origin/main` is not published there, and a pull from it must not pass as its own.
fn own_tips(git: &Git, inputs: &Inputs, branch: &str) -> Result<Vec<String>> {
    let target = inputs.targets.get(branch);
    remote_tips(git, branch, &|name| {
        inputs.guarded.contains(name) || target.is_some_and(|target| target == name)
    })
}

/// Where the target side of a local move from `old` to `new` is: `old`, and the branch's own
/// remote-tracking tips (`own`) that `new` contains, which can be ahead of `old`; a tip it does
/// not contain is not what this move takes in.
pub(super) fn target_side(git: &Git, own: &[String], old: &str, new: &str) -> Result<Vec<String>> {
    let mut side = vec![old.to_string()];
    for tip in own {
        if is_ancestor(git, tip, new)? {
            side.push(tip.clone());
        }
    }
    Ok(side)
}

/// Where the target side of a move is, worked out only when a fork point needs it.
pub(super) type Side<'a> = &'a dyn Fn() -> Result<Vec<String>>;

/// The commit ids of the fork points, see [`fork_points_of`], of the branches at `commit` other
/// than `moved`; `within` is where the target side of the move is.
pub(super) fn forks_of_others(
    git: &Git,
    inputs: &Inputs,
    commit: &str,
    moved: &str,
    within: Side,
) -> Result<Vec<String>> {
    fork_points_of(git, inputs, &other_branches_at(git, commit, moved)?, within)
}

/// The fork points, as for [`forks_of_others`], of the branches that the move of the unguarded
/// `branch` from `old` to `new` merges into it, or `None` when the move merges nothing, which is
/// an ordinary move of one's own branch and is not checked. It merges when it is a fast-forward to
/// the tip of another local or a remote-tracking branch, or a merge commit with `old` as its first
/// parent and the tip of one as another parent. Branches that cannot overwrite what `branch` is
/// for are not counted (see `docs/product/revert-gate.md`, "A merge into a branch that is not
/// guarded"): its recorded target, any guarded branch, and `branch` itself, whether local or on a
/// remote.
fn merged_forks(
    git: &Git,
    inputs: &Inputs,
    branch: &str,
    old: &str,
    new: &str,
) -> Result<Option<Vec<String>>> {
    let target = inputs.targets.get(branch);
    let counts = |tip: &Tip| {
        (tip.remote || tip.name != branch)
            && !inputs.guarded.contains(&tip.name)
            && Some(&tip.name) != target
    };
    let (parents, tips) = parents_and_tips(git, new)?;
    let mut merged: Vec<String> = Vec::new();
    if tips.iter().any(counts) && is_ancestor(git, old, new)? {
        merged.extend(tips.into_iter().filter(counts).map(|tip| tip.name));
    }
    // The first parent is the branch's own side.
    if let [first, merging @ ..] = &parents[..] {
        if first == old {
            for parent in merging {
                merged.extend(
                    tips_at(git, parent)?
                        .into_iter()
                        .filter(counts)
                        .map(|tip| tip.name),
                );
            }
        }
    }
    if merged.is_empty() {
        return Ok(None);
    }
    merged.sort();
    merged.dedup();
    let within = || target_side(git, &own_tips(git, inputs, branch)?, old, new);
    Ok(Some(fork_points_of(git, inputs, &merged, &within)?))
}

/// The commit ids of the fork points of `branches`: the one recorded for each, else the commit
/// its reflog says it was created at, when that is an ancestor of one of `within` (the target
/// side of the move): a branch created on someone else's unmerged work, or from a remote-tracking
/// branch, has a creation point that is not where it left the target, and the files of that other
/// work would be held against it. A branch with neither, or a record that will not resolve, is
/// skipped. A guarded branch is a target branch, not a source: its creation says nothing about
/// what it merged, so it gets no reflog fallback.
pub(super) fn fork_points_of(
    git: &Git,
    inputs: &Inputs,
    branches: &[String],
    within: Side,
) -> Result<Vec<String>> {
    let mut forks = Vec::new();
    let mut side: Option<Vec<String>> = None;
    for name in branches {
        let Some(fork) = inputs.fork_points.get(name).filter(|fork| !fork.is_empty()) else {
            if inputs.guarded.contains(name) {
                continue;
            }
            if let Some(oid) = creation_point(git, name)? {
                if side.is_none() {
                    side = Some(within()?);
                }
                for tip in side.iter().flatten() {
                    if is_ancestor(git, &oid, tip)? {
                        forks.push(oid);
                        break;
                    }
                }
            }
            continue;
        };
        let oid =
            git.line_lenient(["rev-parse", "--verify", "-q", &format!("{fork}^{{commit}}")])?;
        if !oid.is_empty() {
            forks.push(oid);
        }
    }
    Ok(forks)
}

/// Remote-tracking branch tips below `new` but not below `old`; see "A local update of a guarded
/// branch" in `docs/product/revert-gate.md`. A tip below `old` (the target branch is merely ahead
/// of the remote, or a same-named branch on another remote is behind) means this move is not an
/// alignment with the remote.
fn tips_below(git: &Git, own: &[String], old: &str, new: &str) -> Result<Vec<String>> {
    let mut tips = Vec::new();
    for tip in own {
        if tips.contains(tip) || tip == old {
            continue;
        }
        if !is_ancestor(git, tip, old)? && is_ancestor(git, tip, new)? {
            tips.push(tip.clone());
        }
    }
    Ok(tips)
}

/// Commits on each tip that `new` undid, checked against that tip (all of them published).
/// `forks` decides how far back the checked range extends; `bases` is the branch's actual fork
/// point — `forks` here is mixed with `old` and parent commits, so it cannot be used as one.
fn reverted_on_tips(
    git: &Git,
    new: &str,
    tips: &[String],
    forks: &[String],
    bases: &[String],
) -> Result<Vec<Finding>> {
    let mut found: Vec<Finding> = Vec::new();
    for tip in tips {
        for finding in find_reverts(git, tip, new, forks, bases)? {
            if !found.iter().any(|known| known.sha == finding.sha) {
                found.push(finding);
            }
        }
    }
    Ok(found)
}

/// Checks one move of the guarded branch `branch` from `old` to `new`: `None` when it is allowed,
/// else the message that rejects it. `own` are the tips of the remote-tracking branches `branch` is
/// published on (see [`own_tips`]); none for a push, which has the remote's own value.
pub(super) fn check_move(
    git: &Git,
    route: Route,
    branch: &str,
    old: &str,
    new: &str,
    forks: &[String],
    own: &[String],
) -> Result<Option<String>> {
    if is_null(old) || is_null(new) || old == new {
        return Ok(None);
    }
    // Number of commits dropped and on no remote-tracking branch: an unpushed commit in the same
    // clone may also be one someone else just merged.
    let not_new = format!("^{new}");
    let unpushed = rev_count(git, &[old, &not_new, "--not", "--remotes"])?;
    // Of those, only the ones not on another local branch either are truly lost, and that only
    // counts for a branch that is neither checked out nor being rebased; see the header of the
    // parent module. The branch itself has to be excluded — in the prepared phase it still points
    // at `old`. A branch name cannot contain glob characters, so it works directly as an
    // `--exclude` pattern.
    let mut stranded = unpushed;
    if route == Route::Local && unpushed > 0 && !busy_branches(git)?.contains(branch) {
        let exclude = format!("--exclude={branch}");
        stranded = rev_count(
            git,
            &[old, &not_new, "--not", "--remotes", &exclude, "--branches"],
        )?;
    }
    if route == Route::Local && stranded == 0 && own.iter().any(|tip| tip == new) {
        return Ok(None);
    }
    if !is_ancestor(git, old, new)? {
        if route == Route::Push {
            return Ok(Some(format!(
                "\n[revert-gate] Refusing to rewrite remote {branch} from {} to {}: not a \
                 fast-forward, it would drop commits the remote already has. Do not force-push; \
                 `git fetch` first, then `git rebase {}` in the working copy that holds {branch} \
                 to replay the local unpushed commits on top of the remote, verify, then push.\n\n",
                short(old),
                short(new),
                upstream_of(git, branch)?.1
            )));
        }
        // If even one dropped commit is already on a remote-tracking branch, this rewrites
        // published history.
        if rev_count(git, &[old, &not_new])? > unpushed {
            let shown = upstream_of(git, branch)?.1;
            return Ok(Some(format!(
                "\n[revert-gate] Refusing to move {branch} from {} to {}: not a fast-forward, it would \
                 drop published commits, and {branch} must not rewrite published history.\n\
                 To align with the remote, `git fetch` first, then `git rebase {shown}` in the working \
                 copy that holds {branch} to replay the local unpushed commits on top of the remote.\n\n\
                 {RESTORE_HINT}\n",
                short(old),
                short(new)
            )));
        }
    }
    // `find_reverts` does not require `old` to be an ancestor of `new`: a non-fast-forward is
    // checked against `old` just the same, and the dropped local commits are checked as well.
    // `bases` only ever gets the actual fork points: the forks used by `reverted_on_tips` are
    // mixed with other reference points and cannot be used as fork points.
    let mut findings = find_reverts(git, old, new, forks, forks)?;
    let mut remote_findings = Vec::new();
    let tips = if route == Route::Local {
        tips_below(git, own, old, new)?
    } else {
        Vec::new()
    };
    if !tips.is_empty() {
        // Commits on the tip are checked against the tip instead; see "A local update of a
        // guarded branch" in `docs/product/revert-gate.md`. The parents of published commits
        // already hit against `old` are passed in as fork points too: when the tip is many
        // commits ahead of `old` they may fall outside the window counted from the tip.
        let mut published = Vec::new();
        for finding in &findings {
            for tip in &tips {
                if is_ancestor(git, &finding.sha, tip)? {
                    published.push(finding.sha.clone());
                    break;
                }
            }
        }
        findings.retain(|finding| !published.contains(&finding.sha));
        let tip_forks: Vec<String> = [old.to_string()]
            .into_iter()
            .chain(forks.iter().cloned())
            .chain(published.iter().map(|sha| format!("{sha}^")))
            .collect();
        remote_findings = reverted_on_tips(git, new, &tips, &tip_forks, forks)?;
    }
    if findings.is_empty() && remote_findings.is_empty() {
        return Ok(None);
    }
    Ok(Some(report(
        branch,
        old,
        new,
        &findings,
        &remote_findings,
        route,
        !tips.is_empty(),
    )))
}
