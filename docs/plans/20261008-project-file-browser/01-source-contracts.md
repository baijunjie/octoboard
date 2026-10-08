# Source identities and bounded read contracts

> Goal: establish the daemon foundations that make file and Git reads scoped, version-aware and bounded.
> Completion criteria: protocol contracts and focused daemon tests demonstrate source resolution for a plain
> directory, repository root, linked worktree and repository subdirectory; scoped file reads preserve bytes and
> reject escaped or unavailable sources. Declared budgets are enforced during reading, not only after buffering.
> Git invocation isolation is verified independently of a browser UI. Branch lists and change-list presentation
> are left to their own milestones.

## Technical design

- [ ] Source metadata distinguishes the registered project root, containing repository, worktree root and relative
  project scope according to the overview. Non-Git directories have a valid filesystem source without Git metadata.
- [ ] The daemon resolves project and worktree identifiers to roots it has validated. Client-supplied paths or
  branch labels cannot substitute for those identities. Worktree reads revalidate repository membership; removing
  a worktree or reusing its path for another repository invalidates the old source.
- [ ] Paths are represented without lossy conversion and resolved within the selected scope, including symlinks.
  The contract preserves filename identity separately from display text, rejects root escape, and distinguishes
  regular files from directories, links and unsupported special files. FIFO/device reads must not begin.
- [ ] The read contract distinguishes live files, index entries and committed blobs. Change identities additionally
  carry their group and old/new paths; an absent diff side is not an empty path that can resolve to a live file.
  A side restricted by project scope has its own state, distinct from absent, with neither a body read nor patch
  content from that side.
- [ ] Committed content and index-side content identify the immutable objects actually read. A comparison response
  identifies both sources and the versions used for its patch and bodies. Live content is a bounded read-time
  result; a detected change produces a retry/stale outcome instead of combining incompatible reads.
- [ ] Correlated replies carry enough source identity for a client to reject a response after project, worktree,
  mode, selected file or comparison changes. Connection/snapshot invalidation and content-version invalidation
  remain distinct.
- [ ] Results distinguish success, unavailable/deleted source, permission failure, unsupported content, stale source
  and exceeded limits. File content is classified without decoding arbitrary binary bytes as source text.
- [ ] Declare finite limits for file/patch bytes, directory/change entries, subprocess stdout/stderr, concurrent
  reads and retained content. Enforce byte limits while reading; a deadline alone is insufficient. Incomplete
  listings are marked, and an incomplete patch is never presented as a complete diff.
- [ ] On-demand content replies stay out of shared snapshots. Their transport and queue budgets protect ordinary
  control events; superseded work is cancelled or coalesced where possible and always subject to concurrency limits.
- [ ] Git reads use a controlled invocation contract: remove environment overrides that redirect the repository,
  worktree or index; disable external diff/textconv and color output; preserve bytes and literal paths. Local branch
  references are validated and resolved to commit identities rather than accepted as arbitrary revision expressions.

## Implementation plan

- [ ] Establish source resolution and metadata for each supported project-root shape, including a project reached
  through a symlink and a subdirectory mapped into another worktree.
- [ ] Provide the bounded live-file read foundation and define the source/version/error schema consumed by later
  file-tree and Git endpoints. Keep renderer-specific structures outside this contract.
- [ ] Establish the isolated Git invocation and byte-preserving output path that later discovery and diff reads
  will share, retaining non-interactive execution, deadlines and process cleanup.
- [ ] Verify paths containing spaces, newlines and pattern characters, non-UTF-8 filename identity, binary bodies,
  symlink escape, special files, missing roots and permission failures without returning another source's content.
- [ ] Verify Git environment overrides and configured diff/textconv helpers cannot redirect a read or execute the
  helper. Verify literal filenames are not interpreted as pathspec patterns.
- [ ] Verify configured byte and concurrency limits with oversized input and overlapping requests. Reaching a limit
  must stop further accumulation, release resources and leave ordinary control requests usable.
- [ ] Record protocol contracts and chosen budgets before downstream endpoints adopt them. Concrete Git version-race
  cases are verified with the change inspection and branch comparison milestones.

## Notes for the developer

**Reusable capabilities**

- The daemon protocol and UI daemon-client modules supply correlated requests; the UI store supplies snapshot epochs.
- The daemon environment-shell module supplies executable resolution, non-interactive process patterns, deadlines
  and process-group cleanup.

**Development notes**

- The existing Git runner inherits the shell environment and returns lossy UTF-8. Its process helper limits elapsed
  time but accumulates output without a byte ceiling. Reuse the relevant mechanisms, not those assumptions.
- The control connection has a shared outbound writer and starts requests independently. Bounded message counts
  alone do not bound queued bytes or the number of file reads in progress.
- Project association accepts ordinary directories, while existing repository detection checks for `.git` directly
  under the project path. Repository-subdirectory discovery must be intentional and consistent with the new scope.
- A snapshot epoch does not change when an agent stages, commits or overwrites a file. It cannot stand in for the
  content-version contract.

**Reference docs**

- `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`, `packages/ui/README.md`
- `docs/product/consoles-and-projects.md`, `docs/product/project-git-status.md`
- [Git environment variables](https://git-scm.com/docs/git#_environment_variables)
- [Git diff](https://git-scm.com/docs/git-diff), [Git worktrees](https://git-scm.com/docs/git-worktree)
