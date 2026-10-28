# What every session reads

The read-only tools of Octoboard's MCP server (see `docs/product/hub-orchestration.md`). Every session — a console
session or project coordinator or worker — is offered the same ones, and each reads the same for every caller: any
session, of any project or console, and the projects and archives of any console — except the reads of a project's
repository, its changes and its branches, which reach only the caller's own console's projects (see "Reading a project's
changes" and "Listing a project's branches" below).

| Tool | Arguments | What it does |
|---|---|---|
| `get_session` | `session?`, `output_bytes?` | The record of any session, of any console, or with `session` left out of the caller itself, which is how a session learns its own id — status, title, agent, whether it is a console session, its console and project, which session owns it and whether that owner is a console session or a project session (or that it is independent), whether it is the caller's, timestamps, its account, the model and effort it is seen running with, whether it is pinned or locked, the names of the branches it is responsible for, and the rest of "Session records" below — plus a tail of what it has printed, and, for an interrupted or archived session, whether its conversation can still be resumed. |
| `list_sessions` | `console?`, `project?`, `owner?`, `yours?`, `kind?`, `branch?`, `status?` | The running sessions other than the caller — or with `status` the interrupted ones, or both — in every console and project, optionally narrowed. See `docs/product/information-between-sessions.md`. |
| `read_session_history` | `session?`, `limit?`, `before?` | What any session's conversation held, or with `session` left out the caller's own, a page at a time from the latest. See "Reading a session's history" below. |
| `get_session_branches` | `session?` | The git branches any session is responsible for, each with its project, worktree, target branch and fork point, since when the session is responsible for it, and what archiving the session would do with it. See "Reading the branches a session is responsible for" in `docs/product/branch-responsible-session.md`. |
| `list_project_changes` | `project?`, `worktree?`, `branch?`, `base?`, `base_remote?`, `commit?`, `limit?`, `offset?` | What has changed in a project of the caller's own console, without running `git` there: the uncommitted changes of one of its worktrees, what a branch has changed since it forked from a base together with its commits, or one of those commits; a page at a time. See "Reading a project's changes" below. |
| `list_project_branches` | `project?`, `limit?`, `offset?` | The local branches of a project of the caller's own console, without running `git` there, each with its tip, its worktree and the session responsible for it; a page at a time. See "Listing a project's branches" below. |
| `read_project_diff` | `project?`, `path`, `old_path?`, `group?`, `worktree?`, `commit?`, `parent?`, `offset?`, `max_bytes?` | The diff of one change `list_project_changes` lists, a window of it at a time. See "Reading a project's changes" below. |
| `list_agent_models` | `agent`, `account?` | The models an agent offers and the reasoning-effort levels of each. See "Reading the models an agent offers" below. |
| `list_consoles` | — | Every console, each as `console` (its id), `name` and `yours` (whether it is the caller's own console). The id, or a name that names one console, is what a `console` argument takes. |
| `list_projects` | `console?`, `project?`, `refresh?` | The projects of a console: the caller's own in full, another's only as far as needed to target one of its projects; optionally one project, and optionally with their git status checked first. See "Listing a console's projects" below. |
| `list_archived` | `console?`, `project?`, `owner?`, `yours?`, `kind?`, `branch?`, `check_resumable?`, `limit?`, `before?` | The archived sessions of every console, console sessions included, whoever owns them, each with its owner, newest archived first, optionally narrowed, a page at a time. See "Listing archived sessions" below. |
| `get_account_quotas` | `agent?`, `refresh?` | Each account's plan quota, optionally read again first. See "Reading quotas from a session" in `docs/product/account-quotas.md`. |

The reads are not narrowed by role because sessions of any kind can work in the same project, and so the same working
directory, at once — two console sessions can dispatch into it, say — and one that could not see the others' sessions
would collide with them. That holds for a worker as much as for a coordinator, so a worker is offered every read too, at
the cost of a few more tool descriptions in its context. What stays narrower is the reach of the reads of a project's
repository, held to the caller's own console (see "Reading a project's changes" below), a read of settings —
`get_project` is a project session's read of its own project (see `docs/product/agent-settings-tools.md`), and
`get_console` and `get_settings` are the console session's (see "The console session's tools" in
`docs/product/hub-orchestration.md`) — and `list_pages`, the read of the report panel pages the caller pushed itself,
offered with the other report panel tools to a console session and to a project coordinator (see "Reading back the pages
pushed" in `docs/product/report-panel.md`). What a session may do to another is narrower too (see "Which sessions an
owner drives" in `docs/product/hub-orchestration.md`). The tool descriptions, the console session's instructions and
every project session's role description tell it that a session it does not own is somebody else's and is to be left
alone, whatever it can read of it.

## Listing a console's projects

`list_projects` takes an optional `console`, by id or by a name that names one console (`list_consoles` lists them);
a name that names none, or more than one, is refused. Left out, it is the caller's own console — for a project session,
the console its project is in. Both answers carry `console`, the id of the console listed, and `projects`:

- **The caller's own console**: each project with its settings (see "A project's settings" in
  `docs/product/agent-settings-tools.md`) and the sessions currently running in it — every session of the console,
  whichever session owns it — each a session record as in "Session records" below.
- **Another console**: each project only as `project` (its id), `name`, `description` and `tags` — what
  `request_session` and `request_resume_session` need to target one of its projects — with no sessions and no
  settings.

`project` narrows either answer to one project of the console listed, named by its id or by a name that names one
project in that console; anything else is refused. It is how a console session reads a single project, `get_project`
being the project sessions' own.

`refresh` (`true` or `false`, `false` when left out) checks the git status of the projects listed against their
remotes before answering, as the application's own check does, so that a session need not run `git` to learn where a
repository stands (see "Reading the status from a session" in `docs/product/project-git-status.md`). It is refused
with another console, whose projects are listed without a git status. The answer then carries `git_refresh` at its top
level. The tool's description tells the agent to ask for it only when a status is too old to decide on.

## Reading a project's changes

`list_project_changes` and `read_project_diff` show what has changed in a project's git repository without the caller
running `git` there: `git status` and `git diff` rewrite the repository's index, and can fail a command another session
working there is running. They are the project pane's Git mode read for a session: the same reads, so what they list is
what the change list lists (see "What is listed" in `docs/product/project-pane-git-change-list.md`) — for a project that
is a subdirectory of its repository, only the changes that touch the project's folder — and, like the Git mode, they
never write to the repository (see "Reading only" in `docs/product/project-pane-git-mode.md`).

**Which project.** `project` names it by id, or by a name that names one project (looked for among the caller's own
console's projects first). Left out, it is a project session's own project; a console session has to give it. Only a
project of the caller's own console can be read: one of another console is refused, as `list_projects` lists another
console's projects without their git status — even when the worktree or branch asked about came from a session of
that other console. A project that is not in a git repository, or one whose repository `git` cannot read, is refused
with the reason.

### Listing the changes: `list_project_changes`

The arguments choose one of three views:

- **Neither `worktree` nor `branch`**: the uncommitted changes of the worktree holding the project's folder — not
  necessarily the caller's own worktree.
- **`worktree`**: the uncommitted changes of that worktree of the project's repository, named by its path (as
  `get_session_branches` gives it) or by the branch checked out in it. One that names no worktree of the repository is
  refused, the reason listing the worktrees there are.
- **`branch`**: what the local branch `branch` has changed since it forked from `base` — as `git diff base...branch`
  shows it — and the commits it has made since. `base` is a local branch, or with `base_remote: true` a
  remote-tracking one such as `origin/main`, as the repository last fetched it; nothing contacts the remote. Unlike the
  Git mode, which compares with the branch the main working copy has checked out, the base is the caller's to name, or,
  left out, the **target branch recorded for `branch`** — the local branch a session's git created it from, as
  `get_session_branches` and `list_project_branches` give it as `target_branch` (see "Target branches and fork points"
  in `docs/product/revert-gate.md`). For a branch with none recorded, or to compare with another, the tool's
  description suggests the target branch of the session's report, or the branch the project's folder has checked out
  (`git_status.branch` in `list_projects`). With `commit` besides — the full id of one of the commits the branch view
  lists — the answer is that one commit's changes against its first parent; a commit not among them is refused, the
  reason saying whether it may be an older one than the 100 listed, or one of those left out where the daemon cut the
  list short.

Refused: `branch` without `base` when no target branch is recorded for it, the reason saying a target branch is
recorded only for a branch a session's git created from another local branch; `base_remote` without `base`, the
recorded target being a local branch; `base` or `commit` without `branch`; and `worktree` together with `branch`. When
`base` is left out and the comparison with the recorded target fails — that branch no longer exists, say — the reason
names the recorded target branch it compared with. The changes come a page at a time: `limit` is a whole number from 1
to 1000, 200 when left out, and `offset` one of 0 or more, 0 when left out; a value outside those is refused. Reading a
worktree that is changing twice can give two different lists, so a page of one read is not a page of the next.

Every answer carries:

| Field | Contents |
|---|---|
| `project` | The project's id. |
| `view` | `uncommitted`, `branch` or `commit`. |
| `changes` | The page of changes. |
| `total` | How many changes there are altogether. |
| `offset` | Where `changes` starts. |
| `next_offset` | Where the next page starts, `null` on the last page. |
| `complete` | `false` when the daemon cut the list short at its own limits (see "What the list says instead" in `docs/product/project-pane-git-change-list.md`), so `total` and the changes past the cut are not known. |

The uncommitted views add `worktree`: `path` and `branch` as plain text, as `get_session_branches` gives them (`branch`
`null` when `HEAD` is detached or unborn), and `head`, the commit the staged changes are against (`null` before the
first commit). The branch view adds `branch`, `base` (the branch compared with: the recorded target branch when `base`
was left out), `branch_commit` and `base_commit` (the full ids of the two tips), `merge_base` (the commit they forked
at; the changes are those between it and `branch_commit`), `commits` — newest first, at most 100, each `commit`,
`parent` (its first parent, `null` for a root commit), `subject` and `files` (how many of the project's files it
changed) — and `commits_complete`, `false` when the branch made more commits than are listed or the daemon cut the
last one's changes short. The commit view adds the commit's `commit`, `parent` and `subject`.

Each change is:

- `group`, in the uncommitted views only: `staged`, `unstaged`, `untracked` or `conflicted`. A path can be in two
  groups, as a file staged and changed again since is in the change list.
- `status`: `added`, `deleted`, `modified`, `renamed` or `type_changed`, or for a conflicted path how it conflicts —
  `both_modified`, `both_added`, `both_deleted`, `added_by_us`, `added_by_them`, `deleted_by_us` or `deleted_by_them`.
- `path`: relative to the project's folder, the removed path for a deletion. A name that is not UTF-8 or holds a `%` is
  spelled with `%XX` escapes, which is how `read_project_diff` takes it back.
- `old_path`: a rename's former path.
- `kind`: only for a `symlink` or a `submodule`; for a type change, the new side's.
- `outside_project`: only for a rename whose other end lies outside the project's folder — `side` (`old` or `new`) and
  that end's `repository_path`, from the repository's root. Such a change is `renamed` without an `old_path`, and inside
  the project it amounts to an addition or a deletion.
- `lines_added` and `lines_removed`: `null` where `git` counts none — a binary file, a link, a submodule, a change with
  a side outside the project, a very large change, and a conflicted path.

### Reading one change's diff: `read_project_diff`

`path` (required) names the file as the change lists it, and `old_path` a rename's former path. The change is then
either:

- **an uncommitted one**: `group` — `staged`, `unstaged` or `untracked`; a conflicted path has no diff to read — with
  `worktree`, as for `list_project_changes`, for a worktree other than the one holding the project's folder;
- **one between two commits**: `commit`, the full id of the commit holding the new version, and `parent`, that of the
  commit holding the old one; left out, the old version is nothing, as for a root commit. For a commit of the branch
  view, its `commit` and `parent`; for the whole branch since it forked, the answer's `branch_commit` and `merge_base`.

Refused: neither `group` nor `commit`, or both; `parent` with `group`; `worktree` with `commit`; `old_path` for an
untracked file; a `commit` or `parent` that is not a full id of a commit there is; and a `path` or `old_path` that is
not a path inside the project — absolute, with `.` or `..`, with an empty component, or the project's folder itself.

A window of the diff comes back at a time: `max_bytes` is a whole number from 1 to 131072 (128 KiB), 32768 (32 KiB) when
left out, and `offset`, 0 or more and 0 when left out, is where in the text to start; an `offset` past the end is
refused. The answer is:

| Field | Contents |
|---|---|
| `project`, `path`, `old_path` | The project's id and the file named; `old_path` only when given. |
| `format` | What `text` holds: `patch`, the unified diff as `git` writes it — a file changed only in its mode, or only renamed, has a patch with no hunk, and an empty `text` means the path differs in nothing there; `file`, the whole content of an untracked file, which has no diff, every line of it added; `binary`, a binary change, `text` being `git`'s one line saying the binary files differ for a patch, and `null` for an untracked file; `none`, a link, submodule or side outside the project with no content to show, `text` `null`. |
| `text` | The window. It ends at the end of a line where one lies in it, and always on a whole character. |
| `size_bytes` | The full size of the diff or file in bytes, as `git` has it. |
| `offset` | Where the window starts. |
| `truncated`, `next_offset` | Whether more follows, and the `offset` to read on from; `next_offset` is `null` when nothing does. |
| `note` | `null`, unless something about `text` is worth knowing: a patch that is not plain text has the bytes that are not UTF-8 shown as U+FFFD — so `offset` and `next_offset`, which count the bytes of the text returned, then differ from `size_bytes` — and a `none` says why there is nothing to show. |

An uncommitted change is read as it is now, so reading on after it has changed can give a text that does not follow on.
A patch, or a file shown whole, larger than 4 MiB is refused rather than cut, the reason saying it is bounded by the
daemon and not by an argument.

### Sharing the daemon's reads with the window

Each read waits for the same capacity the windows' reads of a repository wait for, as one window's requests do. The
reads of every session together are held to one window's share of it, so a window's own reads keep their share however
many sessions are reading, and a call can wait while others are read. A call that is dropped stops its `git`.
`list_project_branches` (see "Listing a project's branches" below) shares the same capacity the same way.

## Listing a project's branches

`list_project_branches` lists the local branches of a project's git repository — every one of them, whichever session,
if any, is responsible for it — without the caller running `git` there. It reads the branch list the project pane's
Git mode reads for its branch selector (see "The branch selector" in `docs/product/project-pane-git-mode.md`), and
like the Git mode it never writes to the repository. The project is named, and held to the caller's own console, as in
"Reading a project's changes" above, and a project that is not in a git repository, or whose repository `git` cannot
read, is refused with the reason. It is how a session finds who holds a branch, or that the session that held it is
gone, without reading every session.

The branches come in byte order of their names, a page at a time: `limit` is a whole number from 1 to 1000, 200 when
left out, and `offset` one of 0 or more, 0 when left out; a value outside those is refused. The answer is:

| Field | Contents |
|---|---|
| `project` | The project's id. |
| `branches` | The page of branches. |
| `total`, `offset`, `next_offset` | How many branches there are, where the page starts, and where the next one starts (`null` on the last page). |
| `complete` | `false` when the daemon cut the list short at its own limits, so the branches past the cut are not listed. |
| `upstreams_complete` | `false` when the lookup of the main working copy's remote branch went past the daemon's limits, so that branch may lack its `upstream`, and the answer its `default_branch`. |
| `default_branch` | The repository's remote default branch, as `branch` and `commit` (see "Committed changes" in `docs/product/project-pane-git-change-list.md`), given only when the branch the main working copy has checked out has no `upstream` and the remote default branch was found; `null` otherwise. |

Each branch is:

- `branch`, its name as plain text, and `tip`, the full id of the commit it names.
- `upstream`: for the branch the repository's main working copy has checked out, the remote branch the Git mode
  compares it with (see "Committed changes" in `docs/product/project-pane-git-change-list.md`), as `branch` and
  `commit`; `null` when it has none, and for every other branch.
- `worktree`: `null` when no worktree has the branch checked out, else `path`, `main` (whether it is the repository's
  main working copy) and `holds_project_directory` (whether it is the worktree holding the project's folder).
- `origin`: `null` when the branch has no responsible session, else what its record holds (see "The responsible
  session" in `docs/product/branch-responsible-session.md`):
  - `session`, the responsible session's id, which can name a session that no longer exists;
  - `title`, the created name recorded for it when it became responsible;
  - `session_state`: `live` (it has a process), `interrupted`, `archived`, or `gone` when no record of the session is
    left;
  - `since`, when it became responsible, in milliseconds since the epoch — the branch's creation by its git, or a
    handover;
  - `target_branch` and `fork_point`, the branch it was created from (`null` when it was created from something that is
    no local branch) and the commit it was created at (see "Target branches and fork points" in
    `docs/product/revert-gate.md`).

Listing the branches brings the repository's records up to date first, as the Git mode's listing does (see "What the
record is" in `docs/product/branch-responsible-session.md`), so `origin` is as the records stand after that. A branch
whose responsible session is `gone` cannot be handed over: `handover_branches` takes branches only from the caller or
a session it owns (see "Handing branches between sessions of an owner" in
`docs/product/branch-responsible-session.md`), and no session can be named for one that no longer exists.

## Listing archived sessions

`list_archived` lists archived sessions of every console, project sessions and console sessions alike, whichever
session owns them; an interrupted session is not archived, and is listed by `list_sessions` with `status` (see
"Narrowing the list of sessions" in `docs/product/information-between-sessions.md`). It answers with `sessions`, each a
session record as in "Session records" below, `total`, `older` and `next`. Its optional arguments narrow the list, and
a session is listed only when it matches every one given:

- `console`: only the sessions of that console, named by its id or by a name that names one console.
- `project`: only the sessions of that project, named by its id, in any console, or by its name — looked for among the
  caller's own console's projects first, else among every console's. Given with `console`, the project is looked for
  among that console's projects only, by id or by name, and one that is not among them is refused.
- `owner`: only the sessions that were bound to that session when they were archived, named by its id.
- `yours`: `true` lists only the sessions that were bound to the caller when they were archived, `false` only those
  that were not.
- `kind`: `console` for console sessions only, `project` for project sessions only. `kind: console` together with a
  `project` is refused, a console session being in no project.
- `branch`: only the sessions responsible for the local branch of that name, as their `branches` name it (see "Session
  records" below).
- `limit`: how many sessions to return, a whole number from 1 to 200, 50 when left out.
- `before`: the `next` of the previous page, passed back unchanged; only the sessions after that position are
  returned. Left out, the list starts from the most recently archived.

The **most recently archived come first** — by when they were archived, then by id — at most `limit` of them. `total`
counts every session that matches, and `older` how many of them come after the page returned. `next` is a cursor for
the page after this one, to pass as `before`, and `null` when `older` is 0, so pages follow until `older` is 0. A
cursor is a position in that order, not a session: it stays good when the session it was taken from is reopened,
archived again or deleted between two calls. The tool's description tells the agent to narrow the filters — `yours`,
`owner` or `project` — where it can, an unfiltered list being long. A `console`, `project` or `owner` that names
nothing, a name that names more than one console or project, a `kind` of any other value, a `limit` out of range and a
`before` that is not a cursor the daemon made, a session id say, are refused, with the reason in prose, and nothing is
listed.

`check_resumable` (`true` or `false`, `false` when left out) adds `resumable` to each session listed, as `get_session`
gives it (see "Session records" below). It is off by default because it looks through the agents' records once for
each session listed.

## Reading a session's history

`read_session_history` reads the conversation of any session, running, interrupted or archived, from the agent's own
record — the same record the history view reads (see `docs/product/session-history.md`), read afresh on each call. It
holds the user's messages, the messages Octoboard wrote into the session — a brief, a report, shared information, a
notice — and the agent's final reply to each turn. Unlike the view, it hides none of Octoboard's messages, marking them
as Octoboard's instead; everything else the view leaves out (see "What the history lists" there), tool calls among it,
is left out here too.

It takes an optional `session`, any session, the caller itself when left out, and two more optional arguments, which
page the history by entry count from the most recent entry backwards:

- `limit`, how many entries to return: a whole number from 1 to 100, 20 when left out;
- `before`, an entry's `index`: only the entries before it are returned. A whole number, 0 or more, and not past the
  number of entries the history has. Left out, the latest entries are returned.

Anything else, and a session that does not exist, is refused. The answer is:

| Field | Contents |
|---|---|
| `session` | The session's id. |
| `unavailable` | `true` when the agent's record of the conversation cannot be found or read; `total` and `older` are then 0 and `entries` empty. A session that has had no conversation answers `false` with no entries. |
| `total` | How many entries the whole history has. |
| `older` | How many entries come before the page. |
| `entries` | The page, oldest first. Each entry is `index` (its position in the whole history, from 0), `speaker` (`user`, `octoboard` or `agent`), `text`, `turn` and `at` (milliseconds since the epoch, `null` where the record gives no time). A `user` or `octoboard` entry opens a turn, numbered from 1, and the `agent` reply after it shares its number; a turn with no reply has no `agent` entry. |

The first entry's `index`, passed as `before`, reads the page before. **Indices hold only while entries are
appended.** A compacted Grok session shows only the turns since the compaction, and a Claude Code rewind can replace
entries, so a `total` smaller than one read earlier means the history was rewritten, and the reader starts again from
the latest page. In a running session, the last `agent` entry of the turn still open can be a progress note rather than
a final reply.

## Reading the models an agent offers

`list_agent_models` takes `agent` (`claude`, `codex` or `grok`) and an optional `account`: an account of that agent by
id or by name, or `default`; left out, the agent's default account. An account that does not exist, or belongs to
another agent, is refused. It lists what the model pickers list for that account (see "The models offered" and "The
reasoning effort offered" in `docs/product/model-and-reasoning-effort.md`), from the same answers the pickers reuse, so
a call runs nothing while an answer is fresh. The answer is `agent`, `account` (the account's id, or `default`),
`models` and `unavailable`:

- each model is `id` — what a `model` argument takes — `name` and `efforts`, the levels an `effort` argument takes for
  that model;
- `unavailable` is `null`, or, when the agent cannot be asked, `code` and `message` saying why (as in "While the list
  is being fetched, and when there is none" there), with `models` empty.

The description of every `model` and `effort` a tool takes — `new_session`, `request_session`, `request_resume_session`,
`handoff_session`, `reopen_session`, and the `launch` settings of `add_project`, `update_project` and `update_console` —
points to it: a value given there must be one it lists, under the rules in "The Advanced section" in
`docs/product/model-and-reasoning-effort.md`.

## Session records

`get_session` returns the session's most recent output in `recent_output`, with terminal escape sequences and
cursor-control bytes stripped and runs of blank lines collapsed, so it is readable prose rather than a rendered frame.
How much is set by `output_bytes`, a whole number from 1 to 65536 (64 KiB), counted in bytes of the raw output before
anything is stripped; left out, it is 8192 (8 KiB). A value outside those bounds, or one that is not a whole number, is
refused rather than clamped, so a caller is never left believing it read more than it did. Output shorter than what is
asked for is returned whole. For a session whose process has ended, an archived or interrupted one, that is the output
the daemon kept when the process ended, so the field reads the same whether the session is running or not. It is `null`
only where the daemon never kept any output — no process of the session has printed anything yet, it ended with the
daemon killed outright before the output was kept, or a new process has since taken the session over and discarded it. A
session that is waiting for the user carries a note saying so.

Every session record the session tools give — `list_projects`' sessions, `get_session`, `list_sessions`,
`list_archived`, and the record `rename_session` and `set_session_pinned` answer with — carries `owner` (the id of the
session it is bound to, or null when it is independent — always null for a console session, which is never bound),
`owner_kind` (`console` or `project`, the kind of session that parent is; null when independent) and `yours` (whether
that parent is the caller). `orchestration_role` is `coordinator` or `worker`: it describes the ability to create and
manage child sessions, separately from the parent relationship. Console sessions read `coordinator`; project sessions
use the roles described in `docs/product/hub-orchestration.md`. A coordinator can have a parent and no children.

Session records sent to windows also expose `orchestration_role`; their parent field remains `bound_to`. Tool names,
arguments such as `owner`, the `owner_kind` field and the compatibility flag `lead` keep their existing names.
Clients reading an older record without `orchestration_role` cannot infer a project session's role from a non-null
parent alone: a coordinator can also have a parent.

Each record also carries `background_tasks`: for a Claude Code session working in background, the work it left running,
each entry with `id` and, where Claude Code gives them, `type` and `description`; an empty list for every other session.
Such a session's `status` is `idle`: it takes a message like any session awaiting instructions, but its work is not over
(see "Working in background" in `docs/product/session-statuses.md`).

Each of these records also carries `context`, the session's context size as the window shows it (see "Session context
size" in `docs/product/session-context.md`): an object with `tokens`, plus `window` and `used_percent` — the share of
the window in use as a whole percentage, by the agent's own accounting as in "The context-size chip" there — only once
a context window is known, both keys left out otherwise. It is `null` while the context size is unknown: before the
agent's records show one, and for Claude Code after a compaction until the next request.

```json
"context": {"tokens": 120000, "window": 200000, "used_percent": 60}
```

The `status` these records carry is the one the window shows, one of `starting`, `working`, `waiting_user`, `idle`,
`interrupted` and `archived` (see `docs/product/session-statuses.md`): a running session reads `waiting_user` while a
folder-trust confirmation that needs the user, a Codex action title or a screen only the user can answer holds it there,
whatever its agent last reported (see "What the statuses are derived from" in `docs/product/session-statuses.md`), and
one that a change of its account's sharing has ended and is about to resume reads `starting` (see "When a change is
carried out" in `docs/product/sharing-the-default-configuration.md`). Each record also carries `queued_messages` and
`writes_held`, which say how many messages are waiting to be written into the session — what is kept for an
interrupted session until its resume included: information, and for an owner the reports and notices (see "Reports
and notices for an interrupted owner" in `docs/product/reporting-to-owners.md`) — and what keeps them from being
written whatever its `status` reads (see "Messages held until a session can take them" in
`docs/product/message-delivery.md`).

Each record also carries the facts the tools that act on a session depend on, so a caller can check them beforehand:

- `account`, the account the session runs on — the account's id, or `default` for its agent's default account, as
  `get_account_quotas` and `switch_session_account` name accounts — and `account_name`, that account's name: for the
  default account the name the user gave it, else `Default` (see "The default account" in
  `docs/product/agent-accounts.md`); for an account removed since, its id. The account's directory is never given.
  `switch_session_account` refuses the account the session is on already.
- `observed_model` and `observed_effort`, the model and reasoning effort the agent's records show the session running
  with, each `null` until the records show one, and `default_effort`, the level the agent runs that model at by itself:
  given only where the records name the model but no effort and that level can be worked out, else `null` (see "The
  model and effort a session is shown running with" in `docs/product/model-and-reasoning-effort.md`).
- `pinned`, whether the session is pinned (see "Pinning" in `docs/product/order-of-projects-and-sessions.md`), and
  `locked`, whether the user locked it (see "Locking a session" in `docs/product/sessions.md`); `archive_session`
  refuses a locked session.
- `branches`, the names of the git branches the session is responsible for (see "The responsible session" in
  `docs/product/branch-responsible-session.md`): an empty list when it is responsible for none, and always for a
  console session. These are the branches `handover_branches`, and the `branches` of `new_session` and
  `request_session`, need the caller to be responsible for; where each is checked out is `get_session_branches`'
  answer. Reading a record runs no `git`.

And each record carries what tells sessions apart and what an owner decides on:

- `display_title`, the session's displayed name as the user's session list shows it, the agent's task title before
  `title` (see "Displayed names" in `docs/product/session-titles.md`), so that a session the user calls by that name
  can be matched to a record.
- `agent_session_id`, the agent's own id of the conversation, `null` until the agent has reported one. It is what
  `request_resume_session` takes as its `conversation` (see `docs/product/requests-to-resume-a-conversation.md`), which
  the tool's description still says to use only when the user asked for that conversation.
- `origin`: `console` for a session started with `new_session`; `user` for one the user opened and for one opened
  through `request_session` or `request_resume_session`. A session that takes another's place in a handoff carries the
  original's.
- `has_conversation`: whether a turn was ever taken in the session. It does not say that the agent still holds the
  conversation's record; a `reopen_session` that finds it gone is refused (`conversation_unresumable`).
- `resumable`, in `get_session`'s record of an interrupted or archived session only — and in `list_archived`'s with
  `check_resumable` (see "Listing archived sessions" above): whether the agent still holds the conversation, looked up
  in its own records under the session's account as a reopen looks it up (see "When a conversation can no longer be
  resumed" in `docs/product/archiving-and-resuming-sessions.md`). `true` when it does, and for a session with no
  conversation, which a reopen starts afresh; `false` when the conversation is proven gone, so that `reopen_session`
  would refuse it; `null` when that cannot be told, which `reopen_session` lets through. It is read in the daemon's own
  environment rather than a launch's, so in rare cases a reopen can find otherwise. A running session's record has no
  `resumable`.
- `model` and `effort`, the session's own launch choices (see `docs/product/model-and-reasoning-effort.md`): a key is
  absent when the session follows its project's, else its console's, setting, and `null` when the session leaves the
  value to the agent's own default. They differ from `observed_model` and `observed_effort`, which say what the agent's
  records show.
- `lead`, a compatibility flag that is true for a project coordinator with a console parent (see "Coordinators with a
  parent session" in `docs/product/hub-orchestration.md`), and `owned_sessions`, how many sessions are bound to it in
  any status, archived ones included — the count that decides whom it can be bound to (see "Who can own a session" in
  `docs/product/changing-session-owners.md`).
