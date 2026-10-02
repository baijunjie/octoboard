#!/usr/bin/env python3
"""Revert gate: blocks updates that quietly undo changes already merged into the target branch.

The typical cause is a squash that changed the base without merging the content (e.g. committing after
`git reset --soft <target branch>`), so old code gets squashed in as if it were new.
`git merge --ff-only` only checks the ancestry relation and fast-forwards all the same, so guarded branches
are checked by content whenever they move. The guarded ones are the target branches recorded by each
worktree branch (`git config branch.<branch>.worktreeTarget`), plus the resident guarded branches recorded
in `git config revert-gate.branch` (multi-valued, usually the main branch). Only the records are consulted,
not which branch the main working copy currently has checked out, so that rebasing or amending on your own
feature branch is not blocked either; a target branch with no record is therefore not guarded. Rules:

1. A non-fast-forward can only be allowed when none of the dropped commits were ever published
   (`pull --rebase`, `git rebase <branch>@{u}` replaying local unpushed commits on top of the latest remote
   commit, rewriting local commits that have not been pushed), and it still has to pass the content check
   of rule 2 — an unpushed commit in the same clone may also be one someone else just merged.
   A move that aligns with a remote-tracking branch and drops no local unpushed commit is allowed outright
   (published history cannot be held back locally); when the branch being moved is not checked out in any
   worktree and is not being rebased, a dropped unpushed commit that is still on another local branch does
   not count as dropped (first `git switch -c` to a new branch to carry the local commits, then
   `git branch -f` to move it back to the remote — git refuses to let `branch -f` touch a checked-out
   branch).
   This does not apply while the branch is checked out or being rebased: when you reset or rebase on it, or
   update-ref it from another worktree, another branch pointing at those commits is most likely only
   temporary (a scratch branch, a feature branch already fast-forward merged and about to be deleted), and
   deleting it loses the commits.
   Known gap: `switch -c` to a scratch branch, `branch -f` back, then switching back and deleting the
   scratch branch, or `switch --detach` first and then `branch -f` back (the feature branch still holds
   those commits) and deleting that branch afterwards, both have the same shape as the legitimate flows
   above; they are a deliberate bypass, the gate cannot tell them apart, and they are allowed.
2. Reject when any of the most recent non-merge commits on the branch before the move is undone by this
   move; the criteria are in find_reverts. How far back: at least the most recent WINDOW commits; when the
   move can be attributed to a feature branch that recorded its branch point
   (`git config branch.<branch>.worktreeBase`), extend to every commit after that branch point.
   The branch point cannot be found from git history alone — a squash that changed the base is exactly one
   that rewrote the parent to the latest target branch, so it has to be recorded when the branch is created.
   When the target branch was rebased while aligning with the remote, the branch point is no longer its
   ancestor, so counting starts from the merge base of the branch point and the target branch instead.
   When a local move lands above some remote-tracking branch tip and that tip is not below the position
   before the move (`git rebase <branch>@{u}`), commits on the tip (the published ones) are checked against
   the tip instead, and the position before the move is used only for commits not on the tip (the local
   unpushed ones):
   - Changes already undone on the tip were undone by someone else on the remote (a manual revert does not
     necessarily leave a `Reverts:`), cannot be held back locally, and do not count as undone by this move;
     undoing a change that is still live on the tip is rejected all the same, including within the same
     commit and the same file.
   - Commits someone else just pushed that are absent from the position before the move are included too:
     they are exactly what a conflict resolution keeping only the local side undoes, they cannot be found
     by checking against the position before the move alone, and would only be caught at push time, when
     redoing the work is already inconvenient.
   Deleting an add-only commit wholesale does not count as a revert, so undoing a purely additive commit is
   not caught, and neither is a dropped unpushed commit that only adds content — this keeps normal cleanups
   such as "delete a finished plan doc" or "drop temporary code" from being blocked.
3. When a commit message added by this move carries `Reverts: <sha>`, or git revert's default
   `This reverts commit <sha>.`, the corresponding commit is allowed — a deliberate revert must leave a
   record.

Usage:
  revert-gate.py reference-transaction <state>   called by hook.sh, stdin is the list of ref updates
  revert-gate.py pre-push <remote> <url>         called by hook.sh
  revert-gate.py check <target-branch> <branch>  manually check moving the target branch to that branch;
                                                 exit code 1 on a hit, exit code 2 when the check itself
                                                 fails (a non-fast-forward only prints a hint to rebase
                                                 first). Exit code 1 as well when the target branch is
                                                 behind or diverged from its upstream; with no upstream the
                                                 unique same-named branch on a remote is used instead, no
                                                 same-named branch means no check, several of them means
                                                 exit code 1 asking for an upstream to be set first. Based
                                                 on the last fetch, never goes online

In pre-push mode, a guarded branch's remote commit (the live value git gets from the remote when pushing)
that is missing locally is rejected outright: the content cannot be judged, and `push --force` is not
rejected by the remote, so letting it through would overwrite commits someone else just pushed.

In hook modes the exit code for a rejection is 3, and the hook entry points that call it treat only 3 as a
rejection; once installed into a clone those entry points are not updated with the repository, so this
convention must not change.
When the gate itself fails (including the script not running at all) the update is allowed: better to miss
one check than to let it wedge every ref update.
Set the environment variable REVERT_GATE_SKIP=1 to skip it temporarily, only for cases a human confirmed.
reference-transaction mode requires git 2.28+ (the hook only exists from 2.28 on); pre-push and check mode
do not.
"""

import os
import re
import subprocess
import sys
from collections import Counter

# How many commits to look back. A revert usually comes from a branch developed in parallel in the same
# period, so its base is not far behind; too large a value reads many more blobs on every merge. When a
# branch point is recorded, every commit after it is checked as well, but no more than MAX_DEPTH, so that a
# branch left unmerged for a long time cannot slow the check down enough to stall the merge.
WINDOW = 50
MAX_DEPTH = 1000

# Line-level criteria: the file only counts as restored when the share of C's added lines that disappeared
# and the share of its deleted lines that came back each reach RATIO.
# "Came back in bulk" also requires at least MIN_LINES recovered lines; a few stray matches are normal.
RATIO = 0.5
MIN_LINES = 3

# Files larger than this are only compared whole, without reading their content, so that big files are not
# read into memory on every merge.
MAX_BLOB = 4 * 1024 * 1024

# Lines made only of brackets, punctuation and whitespace repeat heavily in any file; counting them in would
# create false signals.
TRIVIAL_LINE = re.compile(r"^[\s{}()\[\];,.:<>/*#-]*$")

OVERRIDE = re.compile(
    r"^(?:Reverts:\s*|This reverts commit )([0-9a-f]{7,64})", re.MULTILINE | re.IGNORECASE
)

REJECTED = 3

# When blocked at the local layer, each command has already completed a different amount of work:
# merge / pull write the work tree and the index first and update the ref last; reset changes the index
# first (and the work tree too with --hard); rebase stops at its last step.
RESTORE_HINT = (
    "If a merge / pull was blocked, the working copy has been written to the branch content; restore it\n"
    "with `git reset --merge`: it only reverts the files written this time and keeps other uncommitted\n"
    "changes. Do not use it when a commit was blocked, it would discard the staged changes.\n"
    "If a reset was blocked, the index (and the work tree too with `--hard`) has been changed to the\n"
    "content of the target position: restore `--hard` with `git reset --merge`, `--mixed` with `git reset`.\n"
    "If a rebase was blocked, leave it with `git rebase --abort`.\n"
)

OID = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")

# Reading a blob in a partial clone may trigger a network fetch, which a hook must not do; content that
# cannot be obtained counts as unread.
GIT_ENV = dict(os.environ, GIT_NO_LAZY_FETCH="1")


def git(*args, check=True, stdin=None):
    result = subprocess.run(["git", *args], input=stdin, capture_output=True, check=False, env=GIT_ENV)
    if check and result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.decode(errors='replace').strip()}")
    return result


def text(data):
    return data.decode("utf-8", errors="surrogateescape")


def guarded_branches():
    """The set of guarded branch names; see the module docstring."""
    out = git("config", "--get-regexp", r"^(branch\..*\.worktreetarget|revert-gate\.branch)$", check=False).stdout
    return {line.split(" ", 1)[1] for line in text(out).split("\n") if " " in line}


def is_null(oid):
    return not oid or set(oid) == {"0"}


def is_ancestor(a, b):
    return git("merge-base", "--is-ancestor", a, b, check=False).returncode == 0


def has_commit(oid):
    return git("cat-file", "-e", f"{oid}^{{commit}}", check=False).returncode == 0


def rev_count(*args):
    return int(text(git("rev-list", "--count", *args).stdout).strip() or 0)


def batch_check(requests, fmt):
    """cat-file --batch-check; one output line per request. Requests must not contain newlines."""
    if not requests:
        return []
    request = "".join(f"{r}\n" for r in requests).encode("utf-8", "surrogateescape")
    out = git("cat-file", f"--batch-check={fmt}", stdin=request).stdout
    return text(out).split("\n")[:len(requests)]


def tree_entries(commit, paths):
    """The blob ids of paths in commit; None when absent (or not a regular file)."""
    paths = [p for p in dict.fromkeys(paths) if "\n" not in p]
    lines = batch_check([f"{commit}:{p}" for p in paths], "%(objectname) %(objecttype)")
    result = {}
    # A miss echoes the request itself plus " missing", and paths may contain spaces, so only the
    # "id + blob" shape is accepted.
    for path, line in zip(paths, lines):
        oid, _, kind = line.partition(" ")
        result[path] = oid if kind == "blob" and OID.match(oid) else None
    return result


def read_blobs(oids):
    """Read content by blob id; blobs over MAX_BLOB are not read and are absent from the result."""
    oids = [o for o in dict.fromkeys(oids) if o]
    wanted = []
    for oid, line in zip(oids, batch_check(oids, "%(objectsize)")):
        if line.isdigit() and int(line) <= MAX_BLOB:
            wanted.append(oid)
    if not wanted:
        return {}
    out = git("cat-file", "--batch", stdin="".join(f"{o}\n" for o in wanted).encode()).stdout
    blobs, pos = {}, 0
    for oid in wanted:
        eol = out.index(b"\n", pos)
        size = int(out[pos:eol].split()[-1])
        blobs[oid] = out[eol + 1:eol + 1 + size]
        pos = eol + 1 + size + 1
    return blobs


def lines_of(data):
    # Not splitlines: it also breaks at form feed, U+2028 and the like, which disagrees with git's lines.
    return Counter(line.strip() for line in text(data).split("\n") if not TRIVIAL_LINE.match(line))


def parse_raw(fields, i):
    """Parse one file change starting at field i of `-z --raw` output;
    returns ((id before, id after, path), next index)."""
    _, _, a, b, _ = text(fields[i])[1:].split(" ", 4)
    return ((None if is_null(a) else a, None if is_null(b) else b, text(fields[i + 1])), i + 2)


def tree_diff(a, b):
    """{path: (blob id in a, blob id in b)}, only for the files that differ between the two."""
    fields = git("diff-tree", "-r", "-z", "--no-renames", a, b).stdout.split(b"\0")
    result, i = {}, 0
    while i + 1 < len(fields) and fields[i].startswith(b":"):
        (x, y, path), i = parse_raw(fields, i)
        result[path] = (x, y)
    return result


def recent_commits(old, forks):
    """The commits to check: the most recent WINDOW on old, united with everything between old and the merge
    base of old and each branch point (at most MAX_DEPTH), non-merge commits only.
    Returns [(sha, subject, [(id before, id after, path)])]."""
    ranges = [[f"--max-count={WINDOW}", old]]
    for fork in forks:
        base = text(git("merge-base", fork, old, check=False).stdout).strip()
        if base:
            ranges.append([f"--max-count={MAX_DEPTH}", f"{base}..{old}"])
    subjects = {}
    for args in ranges:
        listing = text(git("log", "--no-merges", "--format=%H %s", *args).stdout)
        for line in listing.split("\n"):
            if line:
                sha, _, subject = line.partition(" ")
                subjects.setdefault(sha, subject)
    if not subjects:
        return []
    # In --stdin mode each commit prints its own id first, then its file list; --root compares the root
    # commit with the empty tree.
    out = git("diff-tree", "--stdin", "--root", "-r", "-z", "--no-renames",
              stdin="".join(f"{sha}\n" for sha in subjects).encode()).stdout
    fields = out.split(b"\0")
    changes, current, i = {sha: [] for sha in subjects}, None, 0
    while i < len(fields):
        field = text(fields[i])
        if OID.match(field.strip()):
            current, i = field.strip(), i + 1
        elif field.startswith(":") and current in changes and i + 1 < len(fields):
            change, i = parse_raw(fields, i)
            changes[current].append(change)
        else:
            i += 1
    return [(sha, subject, changes[sha]) for sha, subject in subjects.items()]


def overridden(old, new):
    messages = text(git("log", "--format=%B", f"{old}..{new}").stdout)
    return [m.lower() for m in OVERRIDE.findall(messages)]


def ratio(part, whole):
    return sum(part.values()) / sum(whole.values()) if whole else 1.0


def recorded_forks(branches):
    """The commit ids that the branch points recorded by these branches (branch.<branch>.worktreeBase)
    resolve to; branches with no record, or a record that will not resolve, are skipped."""
    forks = []
    for name in branches:
        fork = text(git("config", "--get", f"branch.{name}.worktreeBase", check=False).stdout).strip()
        if fork:
            oid = text(git("rev-parse", "--verify", "-q", f"{fork}^{{commit}}", check=False).stdout).strip()
            if oid:
                forks.append(oid)
    return forks


def branches_at(commit):
    """Local branch names pointing at commit. On a fast-forward merge new is the feature branch tip, which
    is how its recorded branch point is found."""
    out = git("for-each-ref", "--format=%(refname)", f"--points-at={commit}", "refs/heads/").stdout
    return [ref[len("refs/heads/"):] for ref in text(out).split()]


def find_reverts(old, new, forks=()):
    """Return the commits old → new undid: [(sha, subject, [(file, reason)])], newest first.

    A commit C counts as undone in two cases:
    - Wholesale: every file C touched and whose C changes were still live at old was restored by this move,
      and the lines C deleted were brought back. Deleting an add-only commit wholesale is normal cleanup
      and does not count.
    - Partial: some existing file C modified or deleted was restored and the lines C deleted came back in
      bulk — resolving every conflict to your own side during a rebase has exactly this shape.
    The line comparison only looks at the multiset of lines, not at positions, so it still works when the
    file was changed again by other commits after C.
    """
    touched = tree_diff(old, new)
    if not touched:
        return []
    allowed = overridden(old, new)
    candidates = [c for c in recent_commits(old, forks)
                  if any(path in touched for _, _, path in c[2])
                  and not any(c[0].startswith(a) for a in allowed)]
    if not candidates:
        return []
    # A file this move did not touch is the same in old and new, so the one in old is enough to tell
    # whether C's changes are still live.
    untouched = tree_entries(old, [p for c in candidates for _, _, p in c[2] if p not in touched])
    blobs = read_blobs({oid for c in candidates for before, after, p in c[2] if p in touched
                        for oid in (before, after, *touched[p])})

    def content(oid):
        return b"" if oid is None else blobs.get(oid)

    findings = []
    for sha, subject, changes in candidates:
        live, whole, restored_any, partial, reverted = 0, True, False, [], []
        for before, after, path in changes:
            at_old, at_new = touched[path] if path in touched else (untouched.get(path),) * 2
            if at_old == before:
                continue  # C's changes to this file were already undone before old, unrelated to this move
            live += 1
            if at_new == at_old:
                whole = False
                continue
            exact = at_new == before
            data = [content(o) for o in (before, after, at_old, at_new)]
            if None in data:
                # Content unread (too large, a submodule, or the object missing locally): compare whole
                # files only. What C deleted is unknowable here, so treat it as having changed or deleted
                # existing content.
                if not exact:
                    whole = False
                    continue
                reverted.append(path)
                if before is not None:
                    restored_any = True
                    partial.append((path, "the whole file is back at its previous version"))
                continue
            c_before, c_after, old_lines, new_lines = map(lines_of, data)
            alive = (c_after - c_before) & old_lines
            gone = (c_before - c_after) - old_lines
            lost, back = alive - new_lines, gone & new_lines
            if not (exact or ((alive or gone) and ratio(lost, alive) >= RATIO and ratio(back, gone) >= RATIO)):
                whole = False
                continue
            reverted.append(path)
            if back:
                restored_any = True
            # Only "deleted lines came back in bulk" counts: when C only adds, taking away what it added
            # (such as deleting temporary experimental code) is a normal change.
            if before is not None and sum(back.values()) >= MIN_LINES:
                partial.append((path, "the whole file is back at its previous version" if exact else
                                f"{sum(back.values())} of the {sum(gone.values())} lines it deleted came back, "
                                f"{sum(lost.values())} of the {sum(alive.values())} lines it added are gone"))
        if live and whole and restored_any:
            findings.append((sha, subject, [(p, "this commit's changes to this file were reverted")
                                            for p in reverted]))
        elif partial:
            findings.append((sha, subject, partial))
    return findings


def print_findings(findings):
    for c, subject, hits in findings:
        print(f"  {c[:10]} {subject}", file=sys.stderr)
        for f, why in hits:
            print(f"      {f}: {why}", file=sys.stderr)


def report(branch, old, new, findings, mode, remote_findings=(), aligning=False):
    """remote_findings are the published commits found by checking against the remote-tracking branch tip;
    aligning means this local move replayed the branch on top of the remote-tracking branch (aligning with
    the remote), in which case findings only holds local unpushed commits."""
    print(f"\n[revert-gate] Refusing to move {branch} to {new[:10]}: this update undoes changes made by "
          f"existing commits.\n", file=sys.stderr)
    if findings:
        print(f"Against {old[:10]} ({branch} before the move)"
              f"{', local unpushed commits only' if aligning else ''}:", file=sys.stderr)
        print_findings(findings)
    if remote_findings:
        print(f"Against {branch} on the remote-tracking branch (published commits):", file=sys.stderr)
        print_findings(remote_findings)
    if mode == "pre-push":
        cause = (f"The revert comes from a squash that changed the base, or from a conflict resolution that "
                 f"kept only one side: add a commit in the working copy that holds {branch} to bring the "
                 f"undone changes back, verify, then push; do not reset {branch}.")
    elif aligning:
        cause = ("Most likely the wrong side was kept while resolving conflicts when aligning with the "
                 "remote: if a rebase was blocked, leave it with `git rebase --abort` and rebase again, "
                 "resolving conflicts so that the changes from both sides are combined.")
    else:
        cause = ("The most common cause is a squash that changed the base without merging the content\n"
                 f"(e.g. committing after `git reset --soft {branch}`). On the branch, go back to before\n"
                 f"the squash, squash with `git reset --soft $(git merge-base HEAD {branch})` instead,\n"
                 f"then `git rebase {branch}` and resolve the conflicts faithfully.")
    print(f"""
{cause}
If the revert really is deliberate, confirm it and use `git revert` instead, or add one
`Reverts: <sha>` line per undone commit to the message of the commit that undoes it.
""", file=sys.stderr)
    if mode == "reference-transaction":
        print(RESTORE_HINT, file=sys.stderr)


def busy_branches():
    """Names of branches checked out in any worktree, or currently being rebased. During a rebase HEAD is
    detached, so the worktree listing does not show which branch it is on; read that from each worktree's
    rebase state directory instead."""
    names = set()
    for line in text(git("worktree", "list", "--porcelain").stdout).split("\n"):
        if line.startswith("branch refs/heads/"):
            names.add(line[len("branch refs/heads/"):])
    common = os.path.abspath(text(git("rev-parse", "--git-common-dir").stdout).strip())
    linked = os.path.join(common, "worktrees")
    gitdirs = [common] + ([os.path.join(linked, d) for d in os.listdir(linked)] if os.path.isdir(linked) else [])
    for gitdir in gitdirs:
        for state in ("rebase-merge", "rebase-apply"):
            try:
                with open(os.path.join(gitdir, state, "head-name"), encoding="utf-8", errors="surrogateescape") as f:
                    ref = f.read().strip()
            except OSError:
                continue
            if ref.startswith("refs/heads/"):
                names.add(ref[len("refs/heads/"):])
    return names


def remote_refs(branch):
    return text(git("for-each-ref", "--format=%(refname)", f"refs/remotes/*/{branch}").stdout).split()


def remote_tips(branch):
    return text(git("for-each-ref", "--format=%(objectname)", f"refs/remotes/*/{branch}").stdout).split()


def upstream_of(branch):
    """(upstream ref, how to write it in hints). With no upstream (or an upstream ref that does not exist),
    the unique same-named branch on a remote is used instead; with no or several same-named branches the
    ref is None and the hint is written `<remote>/<branch>`."""
    upstream = text(git("for-each-ref", "--format=%(upstream)", f"refs/heads/{branch}").stdout).strip()
    if upstream and git("rev-parse", "--verify", "-q", upstream, check=False).returncode == 0:
        return upstream, f"{branch}@{{u}}"
    refs = remote_refs(branch)
    if len(refs) == 1:
        return refs[0], refs[0][len("refs/remotes/"):]
    return None, f"<remote>/{branch}"


def upstream_ok(branch):
    """check mode only: print a hint and return False when branch is behind or diverged from its upstream.
    How the upstream is obtained is in upstream_of; no upstream means no check; with several same-named
    branches on remotes there is no telling which one counts, so hint to set an upstream and return False.
    Hook modes skip this: being behind the remote does not make a move undo existing changes, and blocking
    it would only get in the way of normal local work."""
    upstream, shown = upstream_of(branch)
    if upstream is None:
        if len(remote_refs(branch)) > 1:
            print(f"\n[revert-gate] {branch} has no upstream and several remotes have a branch of that "
                  f"name, so whether it is behind the remote was not checked; set an upstream with "
                  f"`git branch -u <remote>/{branch} {branch}` and run again.\n", file=sys.stderr)
            return False
        return True
    behind = rev_count(f"refs/heads/{branch}..{upstream}")
    if not behind:
        return True
    ahead = rev_count(f"{upstream}..refs/heads/{branch}")
    if ahead:
        print(f"\n[revert-gate] {branch} has diverged from the remote ({ahead} commits ahead, {behind} "
              f"behind): `git fetch` first, `git rebase {shown}` in the working copy that holds {branch}, "
              f"verify, then rebase the branch again.\n",
              file=sys.stderr)
    else:
        print(f"\n[revert-gate] {branch} is {behind} commits behind the remote: `git fetch` first, "
              f"update with `git merge --ff-only {shown}` in the working copy that holds {branch}, "
              f"then rebase again.\n",
              file=sys.stderr)
    return False


def tips_below(branch, old, new):
    """Remote-tracking branch tips below new but not below old; see rule 2 in the module docstring.
    A tip below old (the target branch is merely ahead of the remote, or a same-named branch on another
    remote is behind) means this move is not an alignment with the remote."""
    return [tip for tip in dict.fromkeys(remote_tips(branch))
            if tip != old and not is_ancestor(tip, old) and is_ancestor(tip, new)]


def reverted_on_tips(new, tips, forks):
    """Commits on each tip that new undid, checked against that tip (all of them published).
    forks decides how far back the checked range extends."""
    found = {}
    for tip in tips:
        for finding in find_reverts(tip, new, forks):
            found.setdefault(finding[0], finding)
    return list(found.values())


def check_move(branch, old, new, mode, forks=(), source=None):
    """Check one move of the guarded branch branch from old to new; returns True when it is allowed.
    mode is the gate's run mode; in check mode source is the name of the branch being merged in, used only
    in hints."""
    if is_null(old) or is_null(new) or old == new:
        return True
    local = mode == "reference-transaction"
    # Number of commits dropped and on no remote-tracking branch: an unpushed commit in the same clone may
    # also be one someone else just merged.
    unpushed = rev_count(old, f"^{new}", "--not", "--remotes")
    # Of those, only the ones not on another local branch either are truly lost, and that only counts for a
    # branch that is neither checked out nor being rebased; see rule 1 in the module docstring.
    # The branch itself has to be excluded — in the prepared phase it still points at old. A branch name
    # cannot contain glob characters, so it works directly as an --exclude pattern.
    stranded = unpushed
    if unpushed and local and branch not in busy_branches():
        stranded = rev_count(old, f"^{new}", "--not", "--remotes", f"--exclude={branch}", "--branches")
    if local and not stranded and new in remote_tips(branch):
        return True
    if not is_ancestor(old, new):
        if mode == "check":
            # In a precheck, a target branch that is not an ancestor of source almost always means source
            # has not been rebased onto the latest target branch commit.
            print(f"\n[revert-gate] {source} has not been rebased onto the latest {branch}; "
                  f"`git rebase {branch}` first.\n",
                  file=sys.stderr)
            return False
        # If even one dropped commit is already on a remote-tracking branch, this rewrites published history.
        published = rev_count(old, f"^{new}") - unpushed
        shown = upstream_of(branch)[1]
        if not local:
            print(f"\n[revert-gate] Refusing to rewrite remote {branch} from {old[:10]} to {new[:10]}: "
                  f"not a fast-forward, it would drop commits the remote already has. Do not force-push; "
                  f"`git fetch` first, then `git rebase {shown}` in the working copy that holds {branch}"
                  f" to replay the local unpushed commits on top of the remote, verify, then push.\n",
                  file=sys.stderr)
            return False
        if published:
            print(f"\n[revert-gate] Refusing to move {branch} from {old[:10]} to {new[:10]}: "
                  f"not a fast-forward, it would drop published commits, and {branch} must not rewrite "
                  f"published history.\n"
                  f"To align with the remote, `git fetch` first, then `git rebase {shown}` in the working "
                  f"copy that holds {branch} to replay the local unpushed commits on top of the remote.\n",
                  file=sys.stderr)
            print(RESTORE_HINT, file=sys.stderr)
            return False
    # find_reverts does not require old to be an ancestor of new: a non-fast-forward is checked against old
    # just the same, and the dropped local commits are checked as well.
    findings = find_reverts(old, new, forks)
    remote_findings, tips = [], []
    if local:
        tips = tips_below(branch, old, new)
        if tips:
            # Commits on the tip are checked against the tip instead; see rule 2 in the module docstring.
            # The parents of published commits already hit against old are passed in as branch points too:
            # when the tip is many commits ahead of old they may fall outside the WINDOW counted from the tip.
            published = [f[0] for f in findings if any(is_ancestor(f[0], tip) for tip in tips)]
            findings = [f for f in findings if f[0] not in published]
            remote_findings = reverted_on_tips(new, tips, [old, *forks, *(f"{sha}^" for sha in published)])
    if findings or remote_findings:
        report(branch, old, new, findings, mode, remote_findings, aligning=bool(tips))
        return False
    return True


def reference_transaction(state):
    lines = text(sys.stdin.buffer.read()).split("\n")
    if state != "prepared":
        return 0
    guarded = guarded_branches()
    for line in lines:
        parts = line.split()
        if len(parts) != 3 or not parts[2].startswith("refs/heads/"):
            continue
        branch = parts[2][len("refs/heads/"):]
        if branch not in guarded:
            continue
        old, new = parts[0], parts[1]
        # An update with no expected old value arrives with all zeros as the old value; at this point the
        # ref is already locked but still holds the old value.
        if is_null(old):
            old = text(git("rev-parse", "--verify", "-q", parts[2], check=False).stdout).strip()
        if not OID.match(old or "") or not OID.match(new):
            continue  # a symref value (ref:...) is not a commit
        forks = recorded_forks(b for b in branches_at(new) if b != branch)
        if not check_move(branch, old, new, "reference-transaction", forks=forks):
            return REJECTED
    return 0


def pre_push():
    guarded = guarded_branches()
    ok = True
    for line in text(sys.stdin.buffer.read()).split("\n"):
        parts = line.split()
        if len(parts) != 4 or not parts[2].startswith("refs/heads/"):
            continue
        branch = parts[2][len("refs/heads/"):]
        local, remote = parts[1], parts[3]
        if branch not in guarded or is_null(remote) or is_null(local):
            continue
        if not has_commit(remote):
            # No network: remote is the live remote value git got when pushing, so not having it locally
            # means the remote has commits that have not been fetched yet.
            print(f"\n[revert-gate] Refusing to push {branch}: remote {branch} has commits the local one "
                  f"does not. `git fetch` first, then `git rebase {upstream_of(branch)[1]}` in the working "
                  f"copy that holds {branch}, verify, then push.\n", file=sys.stderr)
            ok = False
            continue
        ok = check_move(branch, remote, local, "pre-push") and ok
    return 0 if ok else REJECTED


def main(argv):
    if os.environ.get("REVERT_GATE_SKIP") == "1":
        return 0
    mode = argv[1] if len(argv) > 1 else ""
    try:
        if mode == "reference-transaction":
            return reference_transaction(argv[2] if len(argv) > 2 else "")
        if mode == "pre-push":
            return pre_push()
        if mode == "check" and len(argv) == 4:
            old = text(git("rev-parse", "--verify", f"refs/heads/{argv[2]}^{{commit}}").stdout).strip()
            new = text(git("rev-parse", "--verify", f"{argv[3]}^{{commit}}").stdout).strip()
            forks = recorded_forks({argv[3], *branches_at(new)} - {argv[2]})
            if not upstream_ok(argv[2]):
                return 1
            return 0 if check_move(argv[2], old, new, "check", forks=forks, source=argv[3]) else 1
    except Exception as e:  # noqa: BLE001 — any surprise follows the failure policy in the module docstring
        print(f"[revert-gate] Check failed{'' if mode == 'check' else ', allowed anyway'}: {e!r}",
              file=sys.stderr)
        return 2 if mode == "check" else 0
    print(f"[revert-gate] Bad usage: {' '.join(argv[1:]) or '(no arguments)'}", file=sys.stderr)
    return 2 if mode == "check" else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
