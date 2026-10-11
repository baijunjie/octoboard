> Severity: Moderate

# Expanding a change of a path with a clean filter reveals the raw file, not what the patch counted

## Symptom

Where a path has a `clean` filter configured (Git LFS being the common case), expanding a diff's collapsed lines reads
the worktree side's raw bytes from disk, while the patch beside them was made from the filtered content — so the lines
revealed are not the lines the patch's numbers describe.

## Reproduction steps

Precondition: a project whose repository has a `clean` filter on some path. A minimal stand-in for Git LFS, which keeps
the reproduction to text:

```
git config filter.collapse.clean "sed -n '1,2p'"
printf 'one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n' > collapsed.txt
echo 'collapsed.txt filter=collapse' > .gitattributes
git add .gitattributes collapsed.txt && git commit -m "add collapsed.txt"
```

1. Change `collapsed.txt` in the worktree without staging it, so it shows under Unstaged.
2. Open the project pane's Git mode, Uncommitted view, and open that change in the file viewer.
3. Read the hunk header: `git diff` numbers the side from the filtered content, two lines here.
4. Press a separator of collapsed lines, or the separator's own text, to expand.
5. The lines that appear come from the eight-line file on disk, not from the two-line content the hunk header counted.

With a real Git LFS path the file on disk is the asset itself rather than the pointer, so step 5 instead fails on the
daemon's 4 MiB body limit, or draws the asset's bytes as text.

Nobody has run these steps against Octoboard; they are written from the code path under "Leads" and from the measured
`git` behaviour below.

## Expected vs. actual

- **Expected**: the lines an expansion reveals are the lines the patch left out, so that a diff and its revealed
  context describe one state — `docs/product/project-pane-git-mode.md`, "Expanding the collapsed lines" ("The first
  expansion of a diff reads both of its sides whole, and every later expansion of the same diff uses what that read
  returned"), which only holds if what is read is what the patch was made from. Where that cannot be delivered, the
  same section already has a presentation for it: the collapsed lines stay as they are and a note says so.
- **Actual**: the worktree side's raw file is read and drawn beside a patch made from the filtered content.

## Environment

- Any platform; no version dependence found.
- Needs a `clean` filter in effect for the path (`filter.<driver>.clean` plus a `filter=` attribute), which Git LFS
  installs for every path it tracks.
- Only the worktree side is affected, so only an unstaged change or an untracked file. A staged change and both sides
  of a branch comparison are read from blobs, which hold the filtered content already.

## Scope of impact

Anyone reviewing uncommitted changes in a repository using Git LFS, or any other `clean` filter, in the project pane's
Git mode. Workaround: stage the change and review it under Staged, where both sides come from blobs; or do not expand
the collapsed lines, since the patch itself is correct and nothing else in the view is affected.

## Leads

- **Verified (code read, twice, by separate reviews)**: `read_project_change_bodies` resolves the change through
  `Reader::whole` in `apps/daemon/src/browse/changes.rs`, which goes through `body()`; for a `Found::Disk` side that
  reads the file at its recorded version through `live::read_resolved`, i.e. the raw bytes. Nothing in that path applies
  or accounts for a filter, while the patch is made by `git diff`, which compares filtered content.
- **Verified (measured in a throwaway repository during this round's investigation)**: with
  `filter.collapse.clean = sed -n '1,2p'` configured for a path, the worktree file held 8 lines while `git diff`
  numbered that side 2 — the mismatch this ticket is about, observed at the `git` level rather than through Octoboard.
- **Verified (code read)**: the same read also sets that side's `lines` in the bodies reply from the raw bytes, so that
  count carries the same mismatch. The patch reply is not affected: it counts no worktree side at all, deliberately, and
  `apps/daemon/PROTOCOL.md`'s "Changes and comparisons" records why.
- **Inferred**: that a real Git LFS path hits the 4 MiB body limit or renders as binary, rather than showing wrong
  text. Follows from the asset being on disk in place of the pointer, but was not run.
- **Inferred**: that `git` offers no plumbing command applying a `clean` filter to a file and writing the result to
  stdout without storing an object — `git hash-object -w` stores one, which the Git mode's read-only rule forbids
  (`docs/product/project-pane-git-mode.md`, "Reading only"). This bears on which fixes are open and was not exhausted.

## Acceptance criteria

- [ ] Expanding a diff of a path with a `clean` filter in effect never shows lines that the patch's hunk headers do not
      describe — either by reading the filtered content, or by refusing the expansion through the path that already
      keeps the collapsed lines as they are and says so.
- [ ] A side's `lines` in a bodies reply agrees with what the patch was made from, or is absent.
- [ ] A test covers a path with a `clean` filter configured, over both an unstaged change and an untracked file.
- [ ] Nothing in the fix writes to the repository, including no object written to make a filtered copy readable.
- [ ] Where the fix refuses rather than delivers, `apps/daemon/PROTOCOL.md` and
      `docs/product/project-pane-git-mode.md` say so under the refusals each already lists.
