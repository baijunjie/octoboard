> Severity: Major

## Symptom

The project Git status check runs `git status` in the user's repository without `GIT_OPTIONAL_LOCKS=0`, so it can
take `index.lock` and rewrite `.git/index`, which makes an agent's concurrent `git add` or `git commit` fail.

## Reproduction steps

1. In a scratch repository with one committed file `f`, run `touch f` (new modification time, same content).
2. Record `shasum .git/index`.
3. Run the same command the check runs, with the same environment it builds: `git status --porcelain=v2 --branch
   --untracked-files=no` (no `GIT_OPTIONAL_LOCKS` set).
4. Compare `shasum .git/index`: it has changed. Repeating steps 1–4 with `GIT_OPTIONAL_LOCKS=0` leaves it unchanged.
5. In Octoboard, the check runs this command per project of the shown console every five minutes and on demand; an
   agent running `git add`/`git commit` in that repository at the moment the check holds `index.lock` fails with
   "index.lock exists". This step is intermittent and depends on timing.

## Expected vs. actual

- Expected: the check only reads the repository and never takes its index lock — `docs/product/project-git-status.md`
  presents the check as a read of the branch state against its remote, and the daemon's other `git` reads keep the
  index untouched.
- Actual: `read_branch_header` runs `git status` with an environment that does not set `GIT_OPTIONAL_LOCKS=0`, so
  `git status` refreshes and rewrites `.git/index` whenever a tracked file's stat information changed.

## Environment

- `main` at 6c10ee5; macOS; git 2.50.1 (Apple Git-155).
- Any project in a Git repository whose console is shown; more likely while an agent is editing tracked files.

## Scope of impact

Every user with agents committing in projects Octoboard checks. The agent's git command fails and usually retries or
reports an error. Workaround: retrying the git command.

## Leads

- Verified: `apps/daemon/src/git_status.rs` `read_branch_header` calls `run_git_output` with `status --porcelain=v2
  --branch --untracked-files=no`; `run_git_output` clears the environment, applies the login-shell snapshot and sets
  `GIT_TERMINAL_PROMPT`/`GIT_SSH_COMMAND`, but sets no `GIT_OPTIONAL_LOCKS`.
- Verified: with git 2.50.1, plain `git status` rewrites `.git/index` after `touch` on a tracked file; with
  `GIT_OPTIONAL_LOCKS=0` it does not.
- Inferred: other `git` commands the same module runs (fetch, `merge --ff-only`) legitimately write; only the read
  steps are affected.

## Acceptance criteria

- [ ] The status check's read of the branch state leaves `.git/index` byte-identical (and its modification time
  unchanged) after a tracked file was touched with unchanged content.
- [ ] A test covers that case.
- [ ] The check's reported branch, upstream, ahead and behind are unchanged by the fix.
