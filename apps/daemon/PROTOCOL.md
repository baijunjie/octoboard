# octoboardd protocol

The only interface between a client and the daemon — the desktop application today, and in the server form the
owner's browser on another machine (see "Two deployment forms" in `docs/architecture.md`). There is no Tauri IPC
channel and no shared state between them (see "Why the daemon is split out" there), so everything the UI can do is in
here.

The daemon binds to `127.0.0.1` on an ephemeral port and prints `octoboardd listening on 127.0.0.1:<port>` on stdout as
its first line; it also writes the port to `$TMPDIR/octoboardd.port`. The application reads the stdout line.

## Who may connect

The daemon has no authentication: it trusts local processes and does not trust web pages, which can reach a loopback
port and, on a WebSocket handshake, are not subject to CORS. Every request on every route, the 404 fallback included, is
checked before any handler or WebSocket upgrade runs, and a request that fails the check gets `403`.

- **`Host`** must be a loopback name: `localhost`, `127.0.0.1` or `[::1]`, with any port, compared case-insensitively
  and exactly (`127.1` and the like are refused). A missing, repeated or malformed `Host` is refused. This is what stops
  DNS rebinding.
- **No `Origin` header** is allowed. Non-browser clients send none: scripts, the hook callback (`octoboardd hook`, or
  Claude Code's own HTTP hook), the MCP child process (`octoboardd mcp`), native applications.
- **An `Origin` header** (a browser always sends one on a WebSocket handshake and on a cross-origin `POST`) is allowed
  only from:
  - the packaged application's webview: `tauri://localhost`, or `http://tauri.localhost` / `https://tauri.localhost`;
  - an `http` or `https` page on a loopback host, any port — the dev servers;
  - the same origin as the `Host`.

  Anything else, including `null` and a malformed value, is refused.

A new client therefore either sends no `Origin`, or is served from one of the origins above. A client reaching the daemon
under any other host name is refused until the rule is widened (`host_allowed` in `apps/daemon/src/access.rs`).

## `GET /ws/control` — management and status

Text frames, JSON, one object per frame. Every client request may carry an `"id"` string; the daemon answers that
request on the asking socket with `ack` or `error` carrying the same `id`. State changes are broadcast to every
connected client regardless of who caused them.

The request id and the request's own fields share one flat object, so **no request field is named `id`**: what a
request acts on is named for its kind — `console`, `project`, `session` (and `console_id` / `project_id` where a record
is being placed under another). A field named `id` would collide with the envelope's and leave the daemon acting on
the request id as if it were a record id.

### Client to daemon

| `type` | Fields | Meaning |
|---|---|---|
| `create_console` | `name`, `default_agent`, `project_session_agent?`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `claude_console_session_model?`, `claude_console_session_effort?`, `codex_console_session_model?`, `codex_console_session_effort?`, `grok_console_session_model?`, `grok_console_session_effort?`, `claude_project_session_model?`, `claude_project_session_effort?`, `codex_project_session_model?`, `codex_project_session_effort?`, `grok_project_session_model?`, `grok_project_session_effort?`, `icon?`, `console_session_prompt?` | The daemon creates the console's working directory under `~/.octoboard/consoles/<id>/`. `icon` is validated first, as in the "Records" notes below: a failure is answered with `icon_not_an_image` or `icon_too_large`, and a blank value means unset. `console_session_prompt` is the user's own text added at the end of the console session's instruction file (see `docs/product/launching-agents.md`, "The console session's instruction file"): it is trimmed, and absent or blank means none. Each account field names the account this console should reference for that agent, by id; absent means the agent's default account. An id that names no account, or an account of another agent, is answered with `unknown_account`, and nothing is created. Console sessions inherit the console's `default_agent` unless an explicit agent is chosen when opening one. `project_session_agent` is the agent a project session falls back to when its project has no default agent; absent or `null` follows `default_agent`. The model and effort fields are the model and reasoning effort a console session is launched with, one pair per agent in that agent's own spelling; they are trimmed, and absent or blank means the agent's own default. `name` is trimmed and cannot be blank (`field_required`), and a model or effort is checked as in `update_project` (`invalid_launch_setting`); nothing is created when any is refused. The six `*_project_session_model` / `*_project_session_effort` fields are the model and effort a project session falls back to when its project sets none, trimmed, absent or blank meaning the agent's own default; they are checked like the console session ones. |
| `update_console` | `console`, `name?`, `default_agent?`, `project_session_agent?`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `claude_console_session_model?`, `claude_console_session_effort?`, `codex_console_session_model?`, `codex_console_session_effort?`, `grok_console_session_model?`, `grok_console_session_effort?`, `claude_project_session_model?`, `claude_project_session_effort?`, `codex_project_session_model?`, `codex_project_session_effort?`, `grok_project_session_model?`, `grok_project_session_effort?`, `icon?`, `console_session_prompt?` | `project_session_agent`: absent leaves it alone; an explicit `null` clears it, so project sessions follow `default_agent` again; an agent replaces it. For each account field independently: absent leaves the console's reference alone; an explicit `null` clears it, back to the agent's default account; an id is validated as in `create_console` and replaces the reference. A reference left alone is not checked again. `icon` follows the same absent / `null` / blank rule, clearing back to the default glyph. `console_session_prompt` follows the same absent / `null` / blank rule, clearing back to none; the console's instruction file is rewritten at once. A console session begun afterwards reads it on every agent; a resumed Claude Code or Codex session reads it too, while a Grok session never does within its life (`docs/agent-cli-reference.md`, "Resume re-injects everything"). Only sessions opened afterwards take a changed reference (see `Session.account_id`). Each model and effort field follows the same absent / `null` / blank rule, clearing back to the agent's own default; a console session that is opened or resumed afterwards inherits the console's current values unless it has its own override, and one already running keeps what it started with. Project sessions never read them (they read the `*_project_session_*` pair). Existing sessions keep their recorded agent. A model or effort that is being set, and is not blank, must be one the agent offers under the account its sessions would run with, by the list `list_agent_models` answers with, or the request is refused with `invalid_launch_setting`; a value left as it is is not checked again, and an agent whose list could not be read (`unavailable`, or no model in it) takes any value. `name` is trimmed and cannot be blank (`field_required`). The `*_project_session_model` / `*_project_session_effort` fields follow the same absent / `null` / blank rule and the same check, and are read at every launch by the project sessions whose project and override leave the value unset. |
| `delete_console` | `console` | Takes its projects and all their session records with it. Refused while any of them is still running |
| `add_project` | `console_id`, `source` (`local`\|`parent`\|`git`), `path?`, `remote_url?`, `name?`, `default_agent?`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `claude_session_model?`, `claude_session_effort?`, `codex_session_model?`, `codex_session_effort?`, `grok_session_model?`, `grok_session_effort?`, `detect_default_agent?`, `tags?` | `local` associates one directory; `parent` associates every git repository directly beneath `path`; `git` clones `remote_url`, any git remote, into `path` (used as the parent directory) and associates the clone; every added project records its repository's remote URL (`remote_url`, see the "Records" notes below): a clone keeps the URL it was given, and a `local` or `parent` project takes the one read from its directory, or none when the directory is not a repository or has no remote, which is not an error; `path` is required for `local` and `parent` only, and a `git` request without one clones into `default_clone_dir` of the settings. `github`, this source's former name, is still accepted in a request and in a stored project, and is always emitted as `git`. A `path` must be absolute or start with `~/`, and is stored lexically normalised (`.` and `..` folded, no trailing slash); a relative one is refused, because the trusted directories compare project paths and a relative path means nothing to them. Absent `default_agent` lets the daemon store one detected from each directory's marker files, or none (rules in `docs/product/consoles-and-projects.md`, "The project's default agent"); an absent `detect_default_agent` is true, and false stores none, for a client that has already shown the user the detection (`detect_directory_agent`, `probe_git_remote`) and sends what it showed. Absent `tags` means none, and each added project takes the same list, normalised as in the "Records" notes below. Each account field is a `ProjectAccount` (see the "Records" notes below) saying which account the project's sessions of that agent should read; absent or `null` means inheriting the console's reference for that agent, and `{"kind":"default"}` the agent's default account. An `account` kind that names no account, or an account of another agent, is answered with `unknown_account`, and nothing is cloned or stored. The model and effort fields are the model and reasoning effort the project's sessions are launched with, one pair per agent in that agent's own spelling; they are trimmed, and absent or blank means the console's project session value for that agent, else the agent's own default. Each added project takes the same values. A given model or effort is checked as in `update_project`, except for a project of `import_configuration`, whose document's values are stored as they are. A blank `name` is read as absent: a single project takes its directory's name, and a parent directory's projects always do. |
| `update_project` | `project`, `name?`, `default_agent?`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `claude_session_model?`, `claude_session_effort?`, `codex_session_model?`, `codex_session_effort?`, `grok_session_model?`, `grok_session_effort?`, `pinned?`, `tags?` | An absent `default_agent` leaves it alone; an explicit `null` clears it, so the project follows the console's project session agent again. For each account field independently: absent leaves the project's reference alone; an explicit `null` (or `{"kind":"inherit"}`) puts it back to inheriting the console's reference for that agent; `{"kind":"default"}` makes it the agent's default account; an `account` kind is validated as in `add_project` and replaces the reference, and a reference left alone is not checked again. Each model and effort field follows the absent / `null` / blank rule, clearing back to the console's project session value for that agent, else the agent's own default. Only sessions opened afterwards take a changed reference (see `Session.account_id`); a session that is opened or resumed afterwards inherits the project's current model and effort unless it has its own override, and one already running keeps what it started with. An absent `pinned` leaves the pin alone. An absent `tags` leaves the tags alone; a present array, even an empty one, replaces them wholesale; an explicit `null` is the same as absent. A model or effort that is being set, and is not blank, must be one the agent offers under the account its sessions would run with, by the list `list_agent_models` answers with, or the request is refused with `invalid_launch_setting`; a value left as it is is not checked again, and an agent whose list could not be read (`unavailable`, or no model in it) takes any value. `name` is trimmed and cannot be blank (`field_required`). |
| `delete_project` | `project`, `stop_sessions?`, `trash_directory?` | Removes the association and the project's session records, archived ones included; touches the directory only when `trash_directory` (absent means false) is true: the directory is then moved to the system Trash (recoverable, never deleted outright) after any running sessions are stopped and before the project is removed, so when the move fails the request fails with `project_directory_trash_failed` and the project is kept for a retry. A session started in the project while the directory moves is archived before the project is removed, even when `stop_sessions` is false. A directory that no longer exists is skipped and the project is removed as usual. The move uses `NSFileManager` on macOS (no Finder permission needed), so Finder's "Put Back" may be unavailable; the folder is recoverable from the Trash. Refused before anything is stopped, with the project kept, with `project_directory_protected` for the home directory, the root of a disk (`/` or a mount point), or a directory holding Octoboard's own data, and with `project_directory_shared` while another project's directory is the same as, inside, or holds this one, or another project's Git repository (a linked worktree's, say) lies inside it. Refused with `project_has_running_sessions` while the project has live sessions, unless `stop_sessions` (absent means false) is true: the running ones are then ended first, exactly as `archive_session` ends one (the deepest level of binding first — the sessions bound to a lead session, then that lead session, then the unbound ones — so an owner is never refused over them; a console session belongs to no project and is left running, and a lead session's binding to one goes with the lead session's record), and the project goes with them. A session being launched or resumed at that moment is not stoppable, so it refuses either way |
| `list_dir` | `path` | Answered with `dir_listing` on the asking socket |
| `detect_directory_agent` | `path` | Answered with `agent_detected`: the agent the directory at `path` is set up for (marker rules as for `add_project`), or null for none, several, or one known not to be installed. A `path` that is not absolute, does not exist or is not a directory is refused with `path_not_absolute`, `path_not_found` or `path_not_a_directory`, as for `add_project` |
| `probe_git_remote` | `remote_url` | Answered with `agent_detected` for the remote's top-level names, or refused with `git_remote_unreachable`. Does a shallow, blobless, checkout-less `git clone` into a scratch directory and lists its top level, so it proves the remote can be read with the user's credentials. Runs in the same non-interactive environment as a clone; the fetch is bounded by a 60 second deadline and the scratch directory is removed when the probe ends. An empty repository is reachable and answers null |
| `list_agent_models` | `agent`, `account?` | Answered with `agent_models`: the models `agent` currently offers, read from the catalogue of the account `account` names (an account id; absent is the default account). An `account` that does not exist or belongs to another agent is refused with `unknown_account`. Nothing else is a refusal: a missing binary, no network, a non-zero exit or output past 8 MiB, output that cannot be read or a command that runs past 15 seconds are an empty list with a reason (see "Agent models" below). Never touches a session or the stored accounts |
| `refresh_account_quotas` | `agent?`, `account?` | Answered with `ack` at once. Re-reads the plan quota of the accounts it selects whose last attempt is older than the polling interval (five minutes, longer for an account that keeps failing): `agent` limits it to one agent's accounts, `account` (an account id) to that one account, and with neither it covers every account of every available agent. An `account` that does not exist or belongs to another `agent` is refused with `unknown_account`. Each reading that finishes follows as `account_quotas_updated`; an account that is not due, or whose reading is already running, is left alone. See "Account quotas" below |
| `export_configuration` | `console?` | Answered with `configuration_exported`: the configuration document (see "Configuration export" below) of the console `console` names, or, when it is absent, of every console and the application settings. A `console` that does not exist is refused with `unknown_console` rather than answered with an empty document. A read of what is stored: it runs no `git` and touches no session |
| `inspect_configuration` | `document` | Answered with `configuration_inspected`: for each project of the configuration document `document` (see "Configuration import" below) the directories it could be placed in on this machine, and for each account reference whether it falls back to a default account. Changes nothing and runs no `git`. Its existence checks of paths together have a 15 second deadline; a project whose check did not come back in that time is offered nothing. A document that is not a configuration document, is of a version this daemon does not read, or cannot be read is refused with `configuration_not_recognised`, `configuration_version_unsupported` or `configuration_unreadable` |
| `import_configuration` | `document`, `resolutions?`, `apply_settings?` | Answered with `configuration_imported`: creates a new console for each console of `document` and associates each project with the directory its entry in `resolutions` chooses (see "Configuration import" below). A project with no entry is left out. The document is refused as `inspect_configuration` refuses it, and a resolution that names a console or project the document does not have, names one twice, or chooses a `directory` that is neither an absolute path nor starts with `~/`, is refused with `import_resolution_invalid`; all of that before anything is created, as is anything `create_console` would refuse of a console of the document (an avatar that is not an image, say). `apply_settings` (absent means false) also applies the document's application settings; asking for them of a document that carries none is refused with `import_settings_missing`, before anything is created. A project that cannot be placed after that is not an error of the request (see "Configuration import") |
| `get_project_source` | `project`, `slot?` | Where the project's files are read from: its resolved directory and, when it is inside a Git repository, the repository, the worktree holding it, its place in that worktree and every worktree of the repository, that one included. Answered with `project_source`. A browse request: see "Browsing a project" below for the rules every browse request shares (`slot`, budgets, its own outbound queue) |
| `list_project_dir` | `project`, `path`, `worktree?`, `slot?` | Lists one directory of the project's files on disk, `path` being a wire path relative to the project's directory (empty for the directory itself). With `worktree`, the same place in that worktree of the project's repository is listed instead. Answered with `project_dir`. A browse request |
| `search_project_files` | `project`, `query`, `slot?` | Searches file names recursively beneath the project's directory, including unopened subdirectories. A trimmed, case-insensitive basename substring, not a path expression. Answered with `project_file_matches`. A browse request |
| `read_project_file` | `project`, `path`, `worktree?`, `from?`, `slot?` | Reads one file of the project, `path` being a wire path relative to the project's directory. `from` picks the content: `{"kind":"live"}` (the default) the file on disk, `{"kind":"index"}` the blob staged for it, `{"kind":"branch","branch":…}` the blob at a local branch's tip, `{"kind":"commit","commit":…}` the blob in a commit given by its full id. With `worktree`, the disk and the index read are that worktree's. Answered with `project_file`. A browse request |
| `list_project_changes` | `project`, `worktree?`, `slot?` | Lists the uncommitted changes of the worktree holding the project's directory — or, with `worktree`, of that worktree of its repository — that touch the project: staged, unstaged, untracked and in conflict. Answered with `project_changes`. A browse request |
| `read_project_change` | `project`, `worktree?`, `change`, `slot?` | Reads one of those changes for its diff: `change` is `{group, old, new}`, `group` being `staged`, `unstaged` or `untracked` and each side `{"state":"present","path":…}`, `{"state":"absent"}` or `{"state":"out_of_scope"}` as the listing gave it. Answered with `project_change`. A browse request |
| `list_project_branches` | `project`, `slot?` | Lists the local branches of the project's repository, each with the object id its reference names: the commit at its tip. Answered with `project_branches`. A browse request |
| `compare_project_branches` | `project`, `left`, `right`, `slot?` | Resolves local branches `left` (the base) and `right`, each a branch name as a wire path, to the commits at their tips, finds the commit they forked at (their merge base), and lists the changes from that commit to `right`'s tip that touch the project, as `git diff left...right` does. Two branches with no commit in common are refused with `no_merge_base` (`no_merge_base_shallow` in a shallow clone). Answered with `project_comparison`. A browse request |
| `read_project_comparison_change` | `project`, `left`, `right`, `base`, `change`, `slot?` | Reads one change of a comparison for its diff: `left` and `right` are the comparison's `{branch, commit}` and `base` its merge base commit, as its reply gave them, and `change` is `{old, new}`, each side as in `read_project_change`. Answered with `project_comparison_change`. A browse request |
| `read_project_change_bodies` | `project`, `worktree?`, `change`, `old`, `new`, `slot?` | Reads the whole bodies of the two sides of a change `read_project_change` made a patch of, so a client can show the lines the patch left out. `change` is as in `read_project_change`; `old` and `new` are the versions that reply reported for its sides, `{"state":"present","source":…}` with the side's `source` as the reply gave it, or `{"state":"absent"}`. Answered with `project_change_bodies`. A browse request |
| `read_project_comparison_change_bodies` | `project`, `left`, `right`, `base`, `change`, `old`, `new`, `slot?` | The same for a change of a comparison: `left`, `right`, `base` and `change` are as in `read_project_comparison_change`, `old` and `new` as above. Answered with `project_comparison_change_bodies`. A browse request |
| `list_project_commits` | `project`, `base`, `tip`, `slot?` | Lists the commits reachable from `tip` and not from `base`, both full commit ids — a comparison's right commit and its merge base — newest first, each with its changes against its first parent that touch the project. Answered with `project_commits`. A browse request |
| `read_project_commit_change` | `project`, `commit`, `parent?`, `change`, `slot?` | Reads one change of a commit for its diff: `commit` and `parent` are as the commit list gave them (`parent` null or left out for a commit with no parent), and `change` is `{old, new}`, each side as in `read_project_change`. Answered with `project_commit_change`. A browse request |
| `read_project_commit_change_bodies` | `project`, `commit`, `parent?`, `change`, `old`, `new`, `slot?` | The same as `read_project_change_bodies` for a change of a commit: `commit`, `parent` and `change` are as in `read_project_commit_change`. Answered with `project_commit_change_bodies`. A browse request |
| `preview_console_session_title` | `console` | Answers with `console_session_title`: the current console name, used as the default title for new console sessions. Reads without consuming an ordinal; renaming the console can change the default before the caller submits. Refused with `unknown_console` for a console that does not exist |
| `open_session` | `console_id`, `project_id?`, `agent?`, `account?`, `model?`, `effort?`, `task?`, `title?`, `bound_to?`, `resume_agent_session_id?` | Omit `project_id` for a console session; a console may hold any number of them at once. `agent` chooses the session's agent; absent takes the project's default, else the console's `project_session_agent`, else its `default_agent`. A console session has no project, so absent takes the console's `default_agent`. The choice is recorded on the session and retained on resume and account switch. `account` is the account the session's agent reads, by id: absent leaves it to the project's reference for the session's agent, else the console's, else the agent's default account; an explicit `null` chooses the default account outright, whatever the project or console refers to. An id that names no account of the session's agent is refused with `unknown_account`. `model` and `effort` independently override the session's launch settings: absent inherits the project's current value, else the console's project session value (the console's console session value for a console session), an explicit `null` or blank string leaves the choice to the agent, and a string pins that trimmed value. Overrides are recorded on the session and reused on resume and account switch. For a console session, `title` supplies a custom title; absent or `null` uses the console's current name. `bound_to` names the session this (project) session should report to; absent means none. Ignored for a console session, which is never bound. A `bound_to` that names neither a console session of `console_id` nor a project session of `project_id` that is unbound or a lead session (see `Session.bound_to` below) is refused with `unknown_session`; a session bound to a lead session is refused as an owner, so bindings are never more than two levels deep, and so is a session still `starting`, since a launch that fails deletes its session whatever is bound to it. A `model` or `effort` that is a non-blank string is checked as in `update_project`, against the account the session will use, or the agent's default account; a blank `title` is read as absent. `resume_agent_session_id` is the agent's own id of a conversation started outside Octoboard (the one `--resume` takes), which the new session continues instead of starting one; the session is recorded with that id and as having a conversation, so a relaunch before its first prompt resumes it too. Trimmed, blank reads as absent. Only a project session takes one (`resume_for_console_session` otherwise), never together with `bound_to` (`conflicting_fields`); it must be a UUID (`resume_id_invalid`; stored lowercase and hyphenated whatever the case or form it was sent in), recorded as the agent's id by no session of any state (`conversation_in_session` for one of the same project, `conversation_outside_project` for any other, a console session included; checked again once the session is recorded, so a request that raced it is refused too), and have a record in the directory of the account the session will use, the pinned one or else the default account's from the login-shell snapshot (`conversation_to_resume_not_found`). Every refusal is before anything is recorded or launched. The reply (`session_opened`) comes as soon as the session is recorded and broadcast with `status` `starting`, before its agent process has been launched, which can take seconds; the launch goes on in the background, and starts only once the reply has been queued to the client, so a client never sees the session deleted before it has been told of it. When it succeeds the session moves to `working` (a `task` was given) or `idle`, as a `session_upserted`, unless the agent's own first hook has moved it on already. When it fails the session is deleted again (`session_deleted`) and a `launch_failed` `session_notice` says why; an `error` is never the reply to a launch that failed after the reply. Every refusal that can be known beforehand still comes as the reply's `error`. A `starting` session has no process: its terminal stream behaves as a dormant session's, and `archive_session` on it stands (the process is ended as soon as it is up), while `delete_session`, `resume_session` and `switch_session_account` are refused. The sessions an agent or a session request opens (`start_session`, an approving `answer_session_request`) are answered once the launch has settled instead, with its own error on failure |
| `resume_session` | `session` | Relaunches an `interrupted` or `archived` session through the agent's own resume mechanism, re-injecting everything. A session bound to an archived owner (a console session, or a project session that started it) relaunches its archived owners first, level by level from the topmost down — for a session bound to an archived lead session whose console session is archived too, the console session, then the lead session — stopping at the first owner that is not archived; an owner that is only `interrupted` is left alone. If an owner cannot be relaunched the whole request fails with that owner's own refusal: owners already relaunched stay up, and the rest, the session included, stay as they were. Resuming an owner relaunches nothing bound to it, and no other session bound to the same owners is relaunched |
| `archive_session` | `session` | Ends the process and archives the session. Archiving a session that has sessions bound to it (a console session, or a project session that started some) reaches every session under it — bound to it, or bound to a lead session bound to it — and goes by whether a process is running, not by status: refused with `session_has_running_sessions`, and nothing changed, while any session under it has a process (or is being launched, resumed or switched), and otherwise it archives every session under it that is not archived yet — the interrupted ones — along with the session itself, so nothing under it is left outside the archive. Archiving a session with nothing bound to it reaches no other session |
| `switch_session_account` | `session`, `account` | Moves the session to another account of its own agent: ends its process the way `archive_session` does (and archives nothing — the session reads `interrupted` while the process is down), copies the session's conversation record into the target account's directory, records the account and directory on the session, and relaunches it the way `resume_session` does, with every launch refusal applying unchanged. `account` is required: an id, or an explicit `null` for the default account, whose directory is resolved once from one login-shell snapshot that both the copy and the relaunch use (an absent `account` is `field_required`). The record is found by name under the root where the agent keeps its records and copied to the same path relative to the config directory, creating the directories in between and replacing an earlier copy; the original stays. The copy is built and checked in a transient `.octoboard-switch` directory at the target's root, which is removed again. A session with no conversation (`has_conversation` false) has nothing to copy, and the switch records the account and relaunches into a fresh conversation. Answered once the relaunched process has stayed up for a few seconds; an `error` answer means the session is on the account it had, and the `session_upserted` events that put it back there (recording it interrupted when the relaunch did not come up) were sent before that answer. Refused, with nothing recorded and the process left running: `session_already_starting` / `session_already_running` while a launch, resume or switch of it is under way, `session_archived`, `session_already_on_account`, `unknown_account` for an id that names no account of the session's agent, `grok_home_not_initialized` for a Grok Build target that was never run against (checked before anything is copied into it), and `conversation_not_found`. Refused after the process was ended, with nothing copied or recorded: `session_did_not_stop`. The session stays claimed against a resume or another switch until the answer is sent. Once the process has been ended and the conversation is being moved: `relocation_failed` when the copy does not complete, the launch's own refusals (`config_dir_unreachable`, `directory_unreachable`, `binary_not_found`, …) and `switch_did_not_come_up` when the relaunched process ends at once — each leaves the session `interrupted` on its previous `account_id` and `config_dir`, with the conversation in both directories |
| `inspect_session_records` | `sessions` | Inspects exactly these session ids and replies with `session_records_inspected`: aggregate `presence` is `present` when any session has a native record, otherwise `unknown` when an id is missing, a native id or account root cannot be resolved, or a listing is unreadable or linked, otherwise `absent`. An empty list is `absent`. Uses every known account and the default account as deletion's Trash scan does; Claude subagent directories and Grok session directories count even without a main transcript or usage ledger. This is a fresh inspection, not a promise about a later deletion |
| `delete_session` | `session`, `trash_agent_records?`, `only_if_unused?` | Removes Octoboard's record of one session and broadcasts `session_deleted`. Refused with `session_not_deletable` unless the session is archived or unused: no turn yet, bound to no owner and with no session bound to it. An unused session that is not archived has its process stopped once its record is gone; a turn landing at the same moment wins and the request is refused. So is a session being launched or resumed right now. Deleting an archived owner (a console session, or a project session that started some) deletes the archived sessions bound to it as well, at both levels — an archived lead session among them takes its own archived sessions along — each broadcast as `session_deleted` (a refusal deletes none); deleting an archived bound session leaves its owner alone. An archived session whose process is still ending can be deleted; one being resumed right now cannot. Unused sessions' agent records are automatically moved to the system Trash. With `trash_agent_records` (absent means false), used sessions' agent records deleted by the request are also moved to the system Trash after the deletion (recoverable, never deleted outright) — Claude Code's transcript with its subagent directory, Codex's rollout file, Grok's session directory — so the session can no longer be resumed in the agent itself. They are looked for wherever this machine's accounts could hold them: the session's own account, every account Octoboard knows of for the agent and the agent's default account (a session switched to another account leaves its original copy in the old one), and every copy in every Claude project directory. Only a record named after the session's agent session id the way the agent names it is taken, and the id and any path an agent's hook reported are never trusted beyond that: an id that is not the agent's id shape (a canonical UUID), a record that does not sit at the agent's exact layout depth, one under a linked directory, or a reported path outside the account's records directory, names nothing. A record is left when another remaining session refers to the same agent session id, and one that is missing or whose account location is unknown is skipped. A failure to move one is logged and never fails the request; a process just stopped may still rewrite its transcript afterwards. The project's directory is never touched. `only_if_unused` (absent means false) deletes this session only if it is still unused at the atomic delete; otherwise `session_not_deletable`. Native-record presence does not affect eligibility. This guard never falls back to deleting a used archived session or its group; any native records are moved to the Trash automatically |
| `delete_archived_sessions` | `console`, `project?`, `console_session?`, `trash_agent_records?` | With `project` (which must belong to `console`), deletes every archived session of that project; with `console_session` (a console session of `console`, else `unknown_session`), every archived session bound to it, an archived lead session's own archived sessions included, leaving the console session itself; with neither, every archived console session of the console together with the archived sessions under them, at both levels. Naming both `project` and `console_session` is `conflicting_fields`. Broadcasts `session_deleted` for each. A session that stopped being archived meanwhile (a resume got there first) is skipped, not an error. `trash_agent_records` (absent means false) moves the native records of every session this request deleted to the Trash; without it, native records are retained |
| `set_session_locked` | `session`, `locked`: boolean — locks or unlocks a session; broadcasts `session_updated` and replies `ack` |
| `set_session_pinned` | `session`, `pinned` | Pins or unpins a session; broadcasts `session_upserted` |
| `set_session_colour` | `session`, `colour` | Sets a console session's badge colour to one of the palette (see `Session.colour`); persisted, and broadcasts `session_upserted`. A `colour` outside the palette is refused with `unreadable_request`, and a project session with `internal_error` (the coordinator's `bail!`, with the reason as `detail`). A chosen colour counts as held for the next console session's assignment, like an assigned one |
| `send_message` | `session`, `text` | Writes a message into a running session. Refused while the session's own agent is waiting for the user: the message would be discarded and its trailing Enter would answer whatever dialog is up. A session that reads `waiting_user` only because a `request_session` call of its own is waiting takes a message as usual — that request is answered in its dialog, not at the terminal (see "Waiting for the answer" in `docs/product/session-requests.md`). The console session's own `send_message` tool holds such a message instead of refusing it — the user can be told to answer the prompt first, the console session cannot. Refused too, with `session_trust_pending`, while the session's agent is showing, or may still show, its folder-trust confirmation, since the trailing Enter would answer it: for Claude Code and Grok Build from launch until their first hook, which may be after another startup screen; for Codex, which runs none until its first prompt, until Octoboard's press of the confirmation, a hook, the person's answer in the terminal being seen to replace the confirmation, or about 30 seconds passing with none shown; the console session's tool, and every other message Octoboard writes, holds it until then instead |
| `rename_session` | `session`, `title` | The title is stored trimmed and cannot be blank (`field_required`) |
| `list_pages` | `console_session` | The console session's report panel pages, oldest first. Answered with `page_list` on the asking socket. A `console_session` that does not name a console session (one that names a project session, say) is refused with `unknown_session` |
| `submit_page` | `page`, `data` | A report panel form submission; `data` is the submitted form's fields as an object of strings, field name to value (a repeated name's values joined by `, `), in document order, including the pressed submit button's `name`/`value`. Written into the console session that pushed the page, found from the page itself, as a user message naming the page; held rather than refused while the console session is `waiting_user`, since the console session is not the one who has to answer that prompt. Refused when `page` is not its console session's newest page — history pages are read-only |
| `confirm_trust` | `session`, `remember`, `trust_parent_dir?` | The user's go-ahead to a `trust_prompt`, whichever agent is asking: Octoboard may press that session's own folder-trust confirmation, which it does by typing at the session's terminal after checking that the confirmation is still up — for Claude Code a Down and an Enter, once the cursor is seen to move to "Yes, I trust this folder"; for Codex an Enter, with the cursor seen on "1. Trust and continue" and nothing written into the session since the confirmation was sighted; for Grok Build a `y`, while no hook has run, the process is still there and nothing has been written into the session since the confirmation was sighted. Pressing it is what makes the agent record the trust in its own configuration, so its later launches do not ask; for Grok Build the entry Grok wrote into the session's own copy of its trust store is then carried over, unchanged, into the user's own store, and the press counts only once that is done. `remember` also records the project's permission (`Project.trust_consent`) once the confirmation has been pressed, so a later confirmation of any agent in that project is pressed without a prompt, and broadcasts the updated `project_upserted`. `trust_parent_dir` (absent means false) records the project's parent directory as trusted instead, once the confirmation has been pressed, and then `remember` adds nothing. Every project whose path lies under that directory is covered by it, for every agent — those already there, those added later by any means, repositories the console session clones or adds into it included — and each agent then applies that project's own configuration once it has recorded its trust. The confirmations already waiting under it are pressed at once. The daemon derives the directory from the session's project (it is the prompt's `trust_dir`); a client never names one. A parent that is the filesystem root, the user's home directory (however it is spelled or linked) or a directory containing it is refused, before anything is pressed, with `error` code `trust_directory_too_broad`; so is a project whose path is not absolute (`trust_path_not_absolute`), and so is any case where the home directory cannot be determined to check against (`trust_home_unknown`). Refused, with nothing recorded and nothing sent, unless `session` is a running project session that is still waiting at its own agent's confirmation and has not been answered (one that is no longer waiting is answered with `error` code `trust_not_waiting`, which a client shows nothing for); a failure to press once it was accepted (the confirmation is not as expected, or something else typed into the session meanwhile) is an `error` with code `trust_answer_failed`, and a Grok Build press whose entry could not be carried over is one with code `trust_not_carried_over`; either records no permission and is also broadcast as a `session_notice` so that it is seen even if the requesting dialog has closed. Octoboard never writes a trust decision into an agent's configuration itself; the one file it writes is the user's Grok Build trust store, with the entry Grok wrote |
| `remove_trusted_directory` | `path` | Stops trusting a directory (compared after lexical normalisation, so a trailing slash does not matter) and broadcasts `trusted_directories_updated`. Projects' own permissions, sessions already running and the trust each agent has already recorded are untouched. Removing one that is not trusted does nothing. Directories are added only by `confirm_trust` |
| `answer_session_request` | `request_id`, `approve` | The user's answer to a `session_request`. `approve` true starts the session the request describes — a project session in its project, or a console session in its console — as `open_session` would, with no `bound_to`: it has no owner and reports to the user, and the requesting session carries on unchanged, neither bound to it nor owning it. False refuses it and changes nothing (a client sends false when the dialog is dismissed). Approval is answered with `session_opened` carrying the new session, refusal with `ack`. The first answer wins. An answer for a request that has already been answered, or that the daemon does not know, gets `session_request_answered`. An approval for one that was withdrawn or timed out, or whose session's process ended before it could be carried out, gets `session_request_not_waiting`; a refusal for one gets the same code. An approval that fails while launching returns that launch's own error |
| `update_settings` | `auto_pull_repositories?`, `default_clone_dir?` | Each settable field absent means "leave it alone". `default_clone_dir` follows the path rules of `add_project`'s `path` (absolute or `~/`, stored lexically normalised, a relative one refused), and a blank value goes back to the built-in default; a refused value leaves the whole request unapplied. Broadcasts `settings_updated` only when something actually changed, as `remove_trusted_directory` does. A change that turns `auto_pull_repositories` on also starts the immediate fast-forward pass described in "Fast-forwarding when the setting is turned on" below, whose statuses follow as `project_git_status` broadcasts; an update that leaves the value as it was starts nothing. Turning it off starts nothing and undoes nothing |
| `refresh_git_status` | `console` | Checks every project of `console` against its remote, a few at a time; answered with `ack` at once, and the statuses follow as `project_git_status` broadcasts, one per project as its own check finishes. A project whose check is already running — this call raced ahead of an earlier one, or another client is watching the same console — is not started again, and nor is one whose last check completed less than a minute ago: several clients can each be polling this console on their own 5-minute interval and phase, and without this floor their sweeps would interleave into several checks per project every 5 minutes instead of one. Either way nothing is broadcast for the project that was skipped — it keeps whatever status it already had. An unknown console is `unknown_console`. See "Daemon behaviour, per project" below for what one project's check does |
| `open_project_path` | `project`, `path`, `worktree?` | Opens an existing project-relative wire path with its default application on the daemon's host; a directory opens in the file manager. `worktree` selects the same scoped live root as browsing; absent means the project's own directory. The path and its symlink target must remain inside that scope. Answers `ack` when the host accepts the request (Linux starts `xdg-open` detached, so later launcher failures cannot be observed). Invalid, missing and outside-scope paths use the browse errors; launch failures are `path_open_failed` |
| `reveal_project_path` | `project`, `path`, `worktree?` | Selects an existing file or directory in its containing folder on the daemon's host. Scope and path validation match `open_project_path`; a symlink is selected by its own name. macOS uses `open -R`; Linux requires `gdbus` and a file manager implementing `org.freedesktop.FileManager1.ShowItems`. Answers `ack` after the launcher succeeds; launch failures are `directory_unavailable` |
| `open_project_terminal` | `project` | Opens the system terminal at the project's directory, on the daemon's host. On macOS that is Terminal.app (`open -a Terminal`), which starts a shell in the directory. On Linux there is no single system terminal, so the first installed program in a fixed list that can be given the directory is used (a program that only inherits its working directory is used when none of those is installed). Answered with `ack` once the terminal has been asked to open, not when its window closes. Refused with `terminal_unavailable` when the directory does not exist, the operating system has no system terminal Octoboard can open, no emulator is installed, or the launch fails. An unknown project is `unknown_project` |
| `open_project_directory` | `project` | Opens the project's directory in the system file manager on the daemon's host: `/usr/bin/open -- <directory>` on macOS, or `xdg-open <directory>` on Linux (looked up in the user's login-shell `PATH`). On macOS, answered with `ack` once `open` exits successfully, bounded to 15 seconds. On Linux, `xdg-open` is detached into its own session and reaped in the background because it may run for the file manager's lifetime; `ack` means the process was spawned, and later launcher failures are not observed. Neither waits for the file manager window to close, and quitting the daemon does not close it. Refused with `directory_unavailable` when the path is not an existing absolute directory, the operating system is unsupported, the Linux login-shell environment or `xdg-open` is unavailable, the process cannot be spawned, or on macOS `open` fails or times out. An unknown project is `unknown_project` |
| `push_project_branch` | `project` | Pushes the project's current branch and only that branch, never with `--force` and never as a bare `git push` (which would follow `push.default`). A branch that has an upstream is pushed to that remote branch. A branch with no upstream is pushed to `origin` when the repository has one, otherwise to its only remote, and that push records the upstream; several remotes and no `origin` is `no_push_remote`, and nothing is pushed. The local branch header is re-read first (no fetch) and broadcast, so the push is shown against the current ahead count even for a commit nothing had noticed yet; then it publishes `pushing` activity during the request, retaining that branch, counts and error. It takes precedence over overlapping checks and pulls until every concurrent push ends; success and failure both restore the latest underlying activity. Answered with `ack` once `git push` has exited. A success is followed by `project_git_status` from a local re-read of the branch header, with no fetch, so the badge's ahead count can drop. Also refused, with nothing pushed: `not_a_git_repository`, `not_on_a_branch` (a detached `HEAD`), `no_commits_to_push` (a branch with no commit yet), `upstream_not_a_remote` (the upstream does not name a remote of this repository) and `git_push_failed` (`git`'s own message, or a branch name that cannot be placed in a refspec). An unknown project is `unknown_project` |
| `switch_project_branch` | `project`, `branch` | Switches the project's own worktree to an existing local branch; `branch` is its canonical wire name below `refs/heads/`, as listed by `list_project_branches`. Requires `.git` in the project directory itself. Validates the literal local reference before `git switch --no-guess -- <branch>`; never creates a branch, guesses a remote branch, forces, stashes, fetches, pushes or fast-forwards. Answered with `ack` after Git exits. Success publishes a local `project_git_status` re-read to every client. Failure is `unknown_project`, `not_a_git_repository`, or `git_switch_failed` with `detail` (Git's own message when it ran). Local changes that prevent switching, a branch checked out in another worktree, a deleted branch or invalid wire/reference name are errors. Uses the filtered, non-interactive Git environment below and a two-minute bound per call. Concurrent Git operations are not serialized with it |
| `restore_project_changes` | `project` | After confirmation in the client, restores staged and unstaged tracked changes in the project's own worktree to `HEAD`, including deleting staged additions absent from `HEAD`. Preserves untracked and ignored files, submodule worktrees and gitlinks, commits, the branch and other worktrees. Requires `.git` in the project directory itself, like the branch badge; repository subdirectory projects are refused. Answered with `ack` after Git exits; success publishes a local `project_git_status` re-read without fetching. Refused with `unknown_project`, `not_a_git_repository`, `restore_not_on_branch` (detached HEAD), `no_commits_to_restore` (unborn branch), `git_operation_in_progress` (merge, rebase, cherry-pick, revert or bisect), or `git_restore_failed` (`detail`). Each Git call is limited to two minutes and 16 MiB stdout / 64 KiB stderr; an inspection that exceeds a bound fails before restoration. A path occupied on disk after removal from the index, a directory where a tracked file belongs, or a symbolic link or file blocking a tracked ancestor is refused before restoration so untracked or ignored content is preserved. Uses the filtered Git environment below. Confirmation is a client responsibility; the request carries the confirmed destructive intent. Git can restore some paths before a later path fails; an error does not promise rollback. Concurrent Git operations, including those from running agents, are not serialized with this request |
| `pull_project_branch` | `project` | Checks one project against its remote and fast-forwards it right now, the same work `refresh_git_status` does per project (see "Daemon behaviour, per project" below), except that the one-minute floor does not apply and the fast-forward happens whether or not `auto_pull_repositories` is on, under the same rule (`upstream` set, `behind > 0`, `ahead == 0`). Answered with `ack` at once; the status follows as `project_git_status` broadcasts. A project whose check is already running gets one more check, with the fast-forward, as soon as that one ends, so the request ends in a fast-forward attempt; any number of requests meanwhile share that one rerun. A project whose check is only waiting for a place in a sweep, or in the fast-forward pass started by turning `auto_pull_repositories` on (at most four projects are checked at once, across every client) does not wait behind it: that check runs at once with the fast-forward, in its place, so the request costs one fetch and no rerun. The in-flight `checking` status is broadcast at once for the request, without the 400 ms delay a sweep's check has. The rerun is skipped too if the project has been removed meanwhile. A project that is not a git repository is reported as such (`repository: false`). An unknown project is `unknown_project` |
| `create_account` | `agent`, `name`, `config_dir` | Both fields are required: an account always has a name and a directory. `name` is trimmed and must be non-empty and unique within `agent` (compared trimmed, case-insensitively, including against the default account's own name); a collision is `account_name_taken`, naming the account it collides with. `config_dir` must be absolute or start with `~/`, and is stored lexically normalised; existence is not checked. Broadcasts `settings_updated` |
| `update_account` | `account`, `name?`, `config_dir?` | Renames the account, repoints it, or both, independently; either field absent leaves it alone. Validated as in `create_account`. Broadcasts `settings_updated` when something actually changed |
| `set_account_approval_reviewer` | `account` (id or `null`), `reviewer` (`user`, `auto_review` or `null`) | Sets the Codex account launch override; a null account names the Codex default account and a null reviewer follows the account's own configuration. An unknown account or one of another agent is `unknown_account`. Broadcasts `settings_updated` only when changed. Takes effect on the next launch or resume; running sessions keep their current reviewer |
| `delete_account` | `account` | Clears the reference of every console and project that refers to this account — putting each console back on its agent's default account and each project back to inheriting its console's, while a project whose reference is `default` names no account and is left as it is — then removes the account. A session holding it is left alone: it already carries its own copy of the directory it launches with. Broadcasts `settings_updated`, a `console_upserted` for every console whose reference was cleared, and a `project_upserted` for every project whose reference was cleared |
| `shutdown` | — | Terminates every session process (leaving them `interrupted`) and exits the daemon |

### Daemon to client

| `type` | Fields |
|---|---|
| `snapshot` | `hosts`, `consoles`, `projects`, `sessions`, `trusted_directories`, `settings`, `git_statuses`, `agent_availability`, `account_quotas`, `home_dir` — sent once, unprompted, when a control socket connects. `git_statuses` is every `GitStatus` the daemon currently holds, empty on a fresh start — carried here, like the trusted directories, so a reconnecting client never has to ask for it separately. `agent_availability` is always three entries, one per agent, each `not_determined` until the daemon's one-time login-shell snapshot for this run lands — see "Agent availability" below. `account_quotas` is the plan quota last read for each account of every available agent, empty until the first readings land — see "Account quotas" below. `home_dir` is the daemon host's home directory — not the browser's, the two may be on different machines — so a client can show a path under it as `~/...`; null when the daemon cannot determine one (`HOME` unset, relative or the filesystem root) |
| `agent_availability_updated` | `agent_availability` — the whole three-entry list, sent once, when the daemon's one-time determination of it lands; never again afterwards, since nothing re-determines it during a run |
| `account_quotas_updated` | `account_quotas` — the whole list, sent whenever a reading finishes or an account's reading is dropped (the account was repointed or deleted); see "Account quotas" below |
| `trusted_directories_updated` | `trusted_directories` — the whole list of trusted directory paths, sent when it changes |
| `settings_updated` | `settings` — the whole `Settings` record, sent when `update_settings` actually changes it, and also whenever the account list changes (`create_account`, `update_account`, `delete_account`, `set_account_approval_reviewer`, or a console dialog's save that repoints or mints one) |
| `console_upserted` / `project_upserted` / `session_upserted` | `console` / `project` / `session` — the whole record, under that key |
| `console_deleted` / `project_deleted` / `session_deleted` | `console` / `project` / `session` — the id of the record that was removed |
| `project_git_status` | `status` — one project's whole `GitStatus`, sent on every change, including every transition of `activity` (so an animated icon has something to follow) and every local re-read of the branch header (see "Daemon behaviour, per project"). Nothing is sent when a project is removed; the client drops its status along with it |
| `session_notice` | `session`, `code`, `params`, `message` — something about a session the user has to be told that no status field carries: an injected capability that will not apply, a setting of theirs Octoboard had to work around, a message Octoboard accepted and could not deliver. Broadcast when it is found, which may be at launch or at any point in the session's life; nothing stores it, so a client that connects later does not see it. See "Coded messages" |
| `trust_prompt` | `session`, `agent`, `project`, `path`, `trust_dir` — a running session of a project is at its agent's folder-trust confirmation, which is asking whether `path` is trusted, and the project has no permission of its own (`trust_consent`) and does not lie under any of `trusted_directories`. `agent` is the one asking: Claude Code's workspace-trust screen, Codex's "Trust this folder?" (shown only inside a git repository, and recorded by Codex for the repository's root), or Grok Build's "Do you trust the contents of this directory?" (shown only where the folder holds content its trust gates, such as an `AGENTS.md`). `trust_dir` is the directory `confirm_trust` with `trust_parent_dir` would trust — the project's parent — or null when there is none to offer (it would be the filesystem root, the home directory or one containing it, the home directory cannot be determined, or `path` is not absolute); a client offers the choice only when it is not null. Broadcast once per confirmation, when it is recognised in the session's terminal output. A Codex or Grok Build confirmation also stops waiting once anything is typed into the session from a terminal, which is the person answering it there (Codex runs no hook after that); its prompt is then not sent again, and a `confirm_trust` for it gets `trust_not_waiting`. When the person answers Grok Build's in the terminal, the entry Grok writes is carried into the user's own trust store all the same, with no permission of Octoboard's recorded; an answer that writes no entry (a decline) changes nothing, and a failure to carry one is a `trust_not_carried_over` notice. From launch, and while a confirmation is waiting, messages Octoboard itself writes into the session are held for as long as `send_message` would refuse one with `session_trust_pending`. Each client is also sent one for every confirmation still waiting under the same condition, with the same fields, right after every `snapshot` (on connect and on lag recovery), so a client that missed the broadcast is still asked; a client already holding the prompt ignores the repeat. A client that declines ("Not now") drops the prompt locally, and a later `snapshot` may ask again. With no client connected the confirmation simply stays for the person to answer in the terminal. The client answers with `confirm_trust`, or leaves the confirmation alone. A console session's confirmation is pressed by the daemon without a prompt and without recording anything, because its working directory is the console's own; a project session's is when it meets the condition above the other way round. A client whose queue holds prompts for projects under a directory that has just become trusted drops them |
| `session_request` | `request_id`, `session`, `role`, `console`, `project`, `agent`, `branches`, `requested_at` | A session called `request_session` and its call is waiting for the user's answer. `session` is the requesting session, which may be a console session or a project session bound or not. `role` is `project` or `console`, the kind of session it asked for; `console` is the console that session would be in, and `project` the project it would be in, or null for a console session. `agent` is the agent it would run. `branches` lists the branches an approval would hand to the new session, as wire paths below `refs/heads/`: branches the requesting session is responsible for in the project's repository, checked when it asked; empty when the request hands none over. `requested_at` is epoch ms and orders requests oldest first. Broadcast once when the call is made, and re-sent to a client after every `snapshot` (on connect and on lag recovery) for every request still waiting, oldest first, after the `trust_prompt`s. A client already holding it ignores the repeat |
| `session_request_closed` | `request_id`, `reason` | The request is no longer waiting, and every client closes its dialog. `reason` is `approved`, `refused`, `timed_out` or `withdrawn`. `approved` means the user approved; it is announced before the session is started, not after one was started. `withdrawn` means the requesting session's process ended, its call's connection to the daemon was dropped (including the agent cancelling the call over MCP), or the same session asked again |
| `console_session_title` | `id`, `console`, `title` — the reply to `preview_console_session_title`, the automatic title at the time of the read |
| `session_records_inspected` | `id`, `presence` — reply to `inspect_session_records`, with `presence` one of `present`, `absent`, `unknown` |
| `session_opened` | `id`, `session` — the reply to `open_session`, and to an approving `answer_session_request`, naming the session it started; for `open_session` it is `starting` (see there) |
| `agent_detected` | `id`, `agent` — the reply to `detect_directory_agent` and `probe_git_remote`; `agent` is an agent name or null |
| `agent_models` | `id`, `agent`, `account`, `models`, `unavailable` — the reply to `list_agent_models`; `agent` and `account` echo the request. `models` is `[{id, name, efforts}]` and `unavailable` is null or a coded reason (`{code, params, message}`, worded as an `error` is); see "Agent models" below |
| `dir_listing` | `id`, `path`, `entries`: `[{name, path, is_git_repo}]` — only directories are listed |
| `project_source` | `id`, `source` — the reply to `get_project_source`; `source` is a `ProjectSourceInfo` (see "Browsing a project") |
| `project_dir` | `id`, `project`, `worktree`, `path`, `root_id`, `entries`, `complete` — the reply to `list_project_dir`. `project`, `worktree` and `path` echo the request; `root_id` is the identity of the directory the listing was scoped to; `entries` are `BrowseEntry` records in byte order of their names' wire forms; `complete` is false when the listing was cut at a budget or an entry could not be read |
| `project_file_matches` | `id`, `project`, `query`, `paths`, `complete` — the reply to `search_project_files`. `project` and `query` echo the request; `paths` are canonical wire paths relative to the project, sorted in byte order. `complete` is false when traversal was cut short or an entry or subtree could not be read |
| `project_file` | `id`, `project`, `worktree`, `path`, `source`, `file` — the reply to `read_project_file`. `project`, `worktree` and `path` echo the request; `source` is the `ContentSource` the body was read from and `file` the `FileContent` |
| `project_changes` | `id`, `project`, `worktree`, `head`, `changes`, `complete` — the reply to `list_project_changes`. `project` and `worktree` echo the request; `head` is the commit the staged changes are against, null before the first commit; `changes` are `ChangeEntry` records in the order `git status` gives them; `complete` is false when the list was cut at a budget |
| `project_change` | `id`, `project`, `worktree`, `group`, `head`, `old`, `new`, `patch` — the reply to `read_project_change`. `project`, `worktree` and `group` echo the request; `head` is the commit a staged change was read against (null before the first commit, and for the other groups); `old` and `new` are `SideRead` records; `patch` is a `FileContent` holding the change's unified patch as `git` writes it, null when none is made |
| `project_branches` | `id`, `project`, `branches`, `complete` — the reply to `list_project_branches`. `branches` are `BranchInfo` records in byte order of their names; `complete` is false when the list was cut at a budget |
| `project_comparison` | `id`, `project`, `left`, `right`, `base`, `changes`, `complete` — the reply to `compare_project_branches`. `left` and `right` are `ComparisonEndpoint` records: each branch as the request named it and the commit it was resolved to; `base` is the full id of the two commits' merge base (the commit itself when both are at one); `changes` are `ChangeEntry` records of the `committed` group, in the order `git` gives them; `complete` is false when the list was cut at a budget |
| `project_comparison_change` | `id`, `project`, `left`, `right`, `base`, `old`, `new`, `patch` — the reply to `read_project_comparison_change`. `left`, `right` and `base` echo the request; the old side was read from `base` and the new side from `right`'s commit; `old`, `new` and `patch` are as in `project_change` |
| `project_change_bodies` | `id`, `project`, `worktree`, `group`, `old`, `new` — the reply to `read_project_change_bodies`. `project`, `worktree` and `group` echo the request; `old` and `new` are `SideRead` records, each `present` with its whole body in `file` or `absent`, never `out_of_scope` |
| `project_comparison_change_bodies` | `id`, `project`, `left`, `right`, `base`, `old`, `new` — the reply to `read_project_comparison_change_bodies`. `left`, `right` and `base` echo the request; `old` and `new` are as in `project_change_bodies` |
| `project_commits` | `id`, `project`, `base`, `tip`, `commits`, `complete`, `commits_complete` — the reply to `list_project_commits`. `base` and `tip` echo the request; `commits` are `CommitChanges` records, newest first; `complete` is false when their changes were cut at a budget, and `commits_complete` when there were more commits than are listed |
| `project_commit_change` | `id`, `project`, `commit`, `parent`, `old`, `new`, `patch` — the reply to `read_project_commit_change`. `commit` and `parent` echo the request; the old side was read from `parent` (the empty tree for none) and the new side from `commit`; `old`, `new` and `patch` are as in `project_change` |
| `project_commit_change_bodies` | `id`, `project`, `commit`, `parent`, `old`, `new` — the reply to `read_project_commit_change_bodies`. `commit` and `parent` echo the request; `old` and `new` are as in `project_change_bodies` |
| `configuration_exported` | `id`, `document` — the reply to `export_configuration`; `document` is a configuration document (see "Configuration export") |
| `configuration_inspected` | `id`, `settings?`, `consoles` — the reply to `inspect_configuration`; `settings` is what the document's application settings are and what they are on this machine now; one `InspectedConsole` per console of the document, in its order (see "Configuration import") |
| `configuration_imported` | `id`, `settings_applied?`, `settings_error?`, `consoles` — the reply to `import_configuration`; `settings_applied` holds the settings that were applied, and is absent when they were not asked for or could not be applied, in which case `settings_error` says why; one `ImportedConsole` per console of the document, in its order (see "Configuration import") |
| `page_list` | `id`, `console_session_id`, `pages` — oldest first. Pages are not in `snapshot`: one carries a whole HTML document, and only the console session on screen needs its pages, so the panel asks. Asking again after every `snapshot` is what keeps it correct across a `page_created` a lagging client never received: such a client is sent a fresh snapshot in place of the events it missed, on the socket it already has |
| `page_created` | `page` — the whole record. The console session pushed a page with `show_page` |
| `ack` | `id` |
| `error` | `code`, `params`, `message`, `id?` — a failure of a request (carrying its `id`) or, with no `id`, one that belongs to no request. See "Coded messages" |

### Coded messages

Everything the daemon sends for the user to read — every `error` and every `session_notice` — carries a stable `code`
and `params`, an object of named string values. A client words the message from them in its own language; the daemon
never translates. `message` is the same text in English: it is what a client shows for a code it does not know (a
daemon newer than the client), so it must always be shown whole, never parsed. A code never changes its meaning or its
params; a new reading gets a new code.

A param named `console`, `project` or `session` is that record's id, and a client shows the record's current name
instead (the raw value only when it does not know the record). A param named `count` is a number, sent as text, which
a client uses to pick the singular or plural wording. Every other param is text to be shown as is: a path, an agent's
name, an operating-system or `git` message that cannot be translated.

A failure that is not meant to be read in detail — an unexpected one with no meaning of its own — is `internal_error`
with the text as its `detail`.

A client also branches on some codes, instead of only showing them:

- `session_already_running` and `session_already_starting` both mean a launch was refused because the session is
  already running or being started. A client's own double click produces them and is not worth showing.
- `trust_directory_too_broad`, `trust_path_not_absolute` and `trust_home_unknown` all mean no parent directory can be
  offered to trust: nothing was answered and the dialog stays open.
- `trust_not_waiting` means nothing is wrong: the confirmation is no longer waiting, and a client shows nothing.
- `session_request_answered` means nothing is wrong and a client shows nothing; `session_request_not_waiting` is shown
  after an approval and ignored after a refusal.
- `request_superseded` means a browse request was given up for a newer one in its slot, which the client asked for;
  a client shows nothing.
- `source_changed` means what was being read changed while it was read; reading it again gets a consistent copy.

`error` codes:

| `code` | `params` | Meaning |
|---|---|---|
| `unreadable_request` | `detail` | The frame is not a request the daemon can read |
| `internal_error` | `detail` | Any other failure |
| `unknown_console` / `unknown_project` / `unknown_session` / `unknown_page` | `console` / `project` / `session` / `page` | The record is not there |
| `field_required` | `field` | A request lacks a field its kind needs, named as on the wire |
| `conflicting_fields` | `first`, `second` | A request names two fields that exclude each other, as on the wire |
| `invalid_launch_setting` | `agent`, `field`, `value` | A model or effort (`field` is `model` or `effort`) the agent does not offer, in `update_console`, `update_project`, `add_project` or `open_session` |
| `invalid_remote_url` | `url` | Raised by the console session's `add_project` tool only, so no client receives it: a `git` URL that is not an http(s), ssh or git remote, or carries credentials; `url` has any credential removed |
| `console_has_running_sessions` / `project_has_running_sessions` | — | Deleting needs the sessions archived first |
| `project_directory_shared` | `project` | Moving a deleted project's directory to the Trash would take another project's files or repository; `project` is that project's name |
| `project_directory_protected` | `path` | The directory is the home directory, the root of a disk, or holds Octoboard's data; the project is kept |
| `project_directory_trash_failed` | `path`, `detail` | The directory could not be moved to the Trash; the project is kept |
| `session_has_running_sessions` | `count`, `sessions`, `session_ids` | Archiving a session while sessions under it (bound to it, or to a lead session bound to it) have a process running, whatever their status; `count` is how many, at both levels, and `sessions` their titles, comma separated as plain fallback text. `session_ids` is a JSON array of their IDs, encoded as a string like every param, so clients can resolve current names and render each separately without parsing user-provided titles |
| `path_not_absolute` | `path` | A project path that is neither absolute nor `~`-relative |
| `path_not_found` | `path` | A directory the request names does not exist |
| `path_not_a_directory` | `path` | The path is not a directory |
| `path_already_exists` | `path` | A clone's target directory exists already |
| `directory_unreadable` | `path`, `detail` | A directory exists but cannot be read (on macOS, file access to its volume is not granted) |
| `directory_unreachable` | `path` | A launch's working directory is not reachable by the daemon |
| `no_repositories_found` | `path` | No git repository directly under a parent directory |
| `all_projects_already_added` | — | Every directory found is already a project of the console |
| `repository_name_missing` | `url` | A clone URL has no repository name in it |
| `git_clone_failed` | `detail` | `git clone` failed; `detail` is its own message |
| `git_remote_unreachable` | `detail` | `probe_git_remote` could not read the remote; `detail` is `git`'s own message |
| `config_dir_not_absolute` | `agent` | An account's config directory is neither absolute nor `~`-relative; `agent` is the agent's name |
| `config_dir_unreachable` | `agent`, `path` | A session's pinned config directory has gone, which refuses the launch — only a session that has a conversation on the agent's side (`Session.has_conversation`); a new session, or one with no conversation yet, launches into it instead (Claude Code creates the directory itself; Octoboard creates it for Codex, and refuses a new Codex session with this code when it cannot) |
| `grok_home_not_initialized` | `path` | A Grok Build session's pinned source home exists but Grok has never been run against it, so it carries no login and no session history for the per-session home to link; refused at launch on the same path as `config_dir_unreachable` |
| `unknown_account` | `account` | No account has this id, or the account belongs to another agent than the field naming it (a console's or project's per-agent reference, `open_session`'s `account`) |
| `account_name_taken` | `agent`, `name` | A requested account name collides with an existing one of the same agent (trimmed, case-insensitive) — including the default account's own name; `name` is the name of the account it collides with |
| `configuration_not_recognised` | — | The `format` of a configuration document is absent or is not `octoboard-configuration` |
| `configuration_version_unsupported` | `version`, `supported` | A configuration document's `version` is newer than the newest this daemon reads, `supported`, or is 0 |
| `configuration_unreadable` | `detail` | A configuration document is not a JSON object, has a `version` that is not a whole number, or cannot be read as a document of its version |
| `import_resolution_invalid` | `console`, `project` | A resolution of `import_configuration` names a project the document does not have, resolves one twice, or chooses a `directory` that is neither an absolute path nor starts with `~/`; `console` and `project` are the indexes it gave |
| `import_settings_missing` | — | `import_configuration` asked for the document's application settings and it carries none (a single console's export has none) |
| `import_choice_unavailable` | `choice` | Reported on a project of `configuration_imported`, not as a reply: the choice made for it cannot be used because the document gives it nothing to use for it (a path that is not absolute, a remote URL that may not be cloned, or none) |
| `icon_not_an_image` | — | A console's icon is not a `data:image/` URL |
| `icon_too_large` | `limit_kib` | A console's icon is longer than the limit, 256 KiB |
| `session_already_running` | `session` | The session is running already, or is not interrupted or archived |
| `session_already_starting` | `session` | The session is being started already |
| `session_already_on_account` | `session` | `switch_session_account` named the account the session is on |
| `session_archived` | `session` | `switch_session_account` for an archived session; reopening one is `resume_session`'s job |
| `conversation_not_found` | `agent`, `path` | `switch_session_account` found no conversation record of the session in the directory of the account it is on (`path`) |
| `conversation_to_resume_not_found` | `agent`, `path`, `id` | `open_session`'s `resume_agent_session_id` has no conversation record in the directory of the account the session will use (`path`) |
| `resume_id_invalid` | `id` | `open_session`'s `resume_agent_session_id` is not a UUID |
| `resume_for_console_session` | — | `open_session` was given a `resume_agent_session_id` without a `project_id` |
| `conversation_in_session` | `id`, `session` | `open_session`'s `resume_agent_session_id` is already the agent's id of a session of the same project, named by its id in `session` |
| `conversation_outside_project` | `id` | `open_session`'s `resume_agent_session_id` is already the agent's id of a session outside the project: of another project, or a console session |
| `relocation_failed` | `path`, `detail` | Copying the conversation into the target account's directory (`path`) did not complete |
| `session_did_not_stop` | `session` | `switch_session_account` ended the session's process and still did not see it gone after a generous wait; nothing was copied or recorded, and the process may still be running |
| `switch_did_not_come_up` | `session` | The relaunched process ended within moments, so the switch is reported as failed and the session stays on its previous account |
| `session_not_deletable` | `session` | Only an archived session, or one never used, can be deleted, and not while it is starting |
| `session_not_running` | `session` | A message or a go-ahead for a session with no running process |
| `session_waiting_for_user` | `session` | The session waits at a prompt only the user can answer, so a message is refused |
| `session_trust_pending` | `session` | The session's agent is showing, or may still show, its folder-trust confirmation, which the message's trailing Enter would answer, so a message is refused |
| `queued_messages_lost` | — | A message queued for a session could not be written in full, and it and what was behind it were dropped |
| `page_not_current` | — | A form can be submitted only from its console session's newest page |
| `trust_directory_too_broad` | `path` | The directory is the filesystem root, the home directory or one containing it |
| `trust_path_not_absolute` | `path` | The project's path is not absolute |
| `trust_home_unknown` | — | The home directory cannot be determined |
| `trust_not_waiting` | — | The trust confirmation is no longer waiting for an answer |
| `trust_answer_failed` | `agent`, `reason`, `reason_code`, `detail?` | Octoboard accepted the go-ahead but could not press the agent's trust confirmation; the user answers it in the terminal. See the reason codes below |
| `trust_not_carried_over` | `agent`, `path`, `reason`, `reason_code`, `detail?` | Grok Build's confirmation was pressed, but no trust entry for the folder reached the user's own store at `path`; `reason_code` says why (see the carry codes below). Nothing was written there and no permission was recorded, so Grok asks again on its next launch |
| `console_session_trust_not_asked` | — | A go-ahead for a console session's trust confirmation, which Octoboard presses without asking |
| `session_request_not_waiting` | `session` | An answer to a `session_request` whose session is no longer waiting for it (withdrawn, timed out, or its process ended before it could be carried out), so nothing was started |
| `session_request_answered` | — | The request has already been answered, from this client or another, or is unknown; nothing changed |
| `binary_not_found` | `binary` | The agent's binary (or `git`) is not on the shell's `PATH` |
| `shell_environment_timeout` | `shell`, `command`, `timeout` | The login shell did not finish printing its environment in time |
| `agent_not_available` | `agent` | `open_session`'s resolved agent has been determined unavailable (its binary does not resolve on the login shell's `PATH`); never raised while that determination is still pending. The console session's own `start_session` tool is refused the same way, as a tool error carrying this same text |
| `invalid_path` | `path` | A browse path is not a canonical wire path relative to the project (see "Wire paths"), or is too long for the operating system |
| `source_unavailable` | `path`, `detail` | The directory a browse request is scoped to — the project's own, or its place in another worktree — does not exist, is not a directory, or (in another worktree) is reached through a symbolic link; `path` is that directory's absolute path |
| `file_not_found` | `path` | Nothing is at the path in that source: no file on disk (a link leading nowhere or round in a loop included), no entry in the index or the commit |
| `permission_denied` | `path`, `detail` | The operating system refused to read the path or a directory on the way to it. `path` is absolute when the directory a request is scoped to cannot be read, and otherwise the request's own |
| `outside_scope` | `path` | The path, with its symbolic links followed, leads outside the project's directory (or the worktree), and is not read |
| `unsupported_file_type` | `path`, `file_type` | The path is not a regular file: `file_type` is `directory`, `file` (listing a file), `symlink` and `submodule` (in the index or a commit), `unmerged` (an index entry with conflicting stages, read from the index or as a change), `fifo`, `socket`, `device` or `other`. A FIFO, socket or device is never opened |
| `source_changed` | `path` | What was being read changed while it was read — the file, the directory being listed, or a change's index entry or file on disk while its patch was made — or a side whose version is no longer the one a body request named, or a symbolic link appeared on its resolved path; nothing that was read is returned |
| `limit_exceeded` | `limit`, `max`, `size?` | A browse budget was reached: `limit` is `file_bytes` (`size` the file's size, when known), `git_output`, `reply_bytes`, `pending_requests` or `patch_bytes`; `max` is the budget. See "Browse budgets" |
| `restore_not_on_branch` | — | Restoring changes was refused because HEAD is detached |
| `no_commits_to_restore` | — | Restoring changes was refused because the branch has no commit yet |
| `git_operation_in_progress` | — | Finish or abort the merge, rebase, cherry-pick, revert or bisect before restoring changes |
| `git_restore_failed` | `detail` | Git could not inspect or restore the project's tracked changes; `detail` is Git's message or the invocation failure |
| `not_a_git_repository` | `project` | An index, branch or commit read, or a `worktree`, for a project that is in no Git repository |
| `git_unavailable` | `detail` | The project's directory is inside a Git repository that `git` cannot read; `detail` is its message |
| `worktree_unavailable` | `worktree` | The worktree id names no worktree of the project's repository any more: removed, or its path now holds something else |
| `invalid_branch_name` | `branch` | Not a valid local branch name (a revision expression such as `main~1` or `@{-1}` is not one) |
| `unknown_branch` | `branch` | No local branch has that name |
| `invalid_commit` | `commit` | Not a full, lower-case hexadecimal object id |
| `unknown_commit` | `commit` | No commit has that id in the repository |
| `no_merge_base` | `left`, `right` | A `compare_project_branches` of two branches whose histories share no commit, so there is no point they forked at to compare from |
| `no_merge_base_shallow` | `left`, `right` | The same in a shallow clone (`git rev-parse --is-shallow-repository` says so), where the history that would show a common commit may be missing; a client words it as an incomplete history rather than unrelated branches |
| `invalid_change` | — | A `read_project_change` whose `change` names no side its group can read: both sides absent, an `out_of_scope` side outside the staged group, or an untracked change with an old side; a `read_project_change_bodies` or `read_project_comparison_change_bodies` whose change has no patch to expand: an untracked one, or one with a side outside the project |
| `clean_filtered` | `path`, `filter` | A body asked of a side read from the worktree whose path has a clean filter in effect — a `filter` attribute naming a driver that configures a `clean` or a `process` command, as Git LFS does for every path it tracks. `git` made the patch from what that filter produces, so the file's own bytes are not the lines the patch left out, and `filter` is the driver's name. Only a worktree side can be filtered this way: a blob holds the filtered content already |
| `git_failed` | `detail` | `git` failed on a browse read, or did not finish within its deadline; `detail` is its message |
| `terminal_unavailable` | `detail` | The system terminal could not be opened at a project's directory; `detail` says why |
| `directory_unavailable` | `detail` | The system file manager could not open a project's directory; `detail` says why |
| `not_on_a_branch` | — | A push was asked for while `HEAD` is detached. Nothing was pushed |
| `no_commits_to_push` | — | A push was asked for on a branch that has no commit yet. Nothing was pushed |
| `no_push_remote` | — | A branch with no upstream, and neither `origin` nor exactly one remote to send it to. Nothing was pushed |
| `upstream_not_a_remote` | — | The branch's upstream does not name a remote of this repository. Nothing was pushed |
| `git_switch_failed` | `detail` | Git could not switch to the requested existing local branch. Invalid canonical wire names and invalid or missing local references fail before switching; otherwise `detail` is Git's own message, the launch failure or timeout |
| `git_push_failed` | `detail` | `git push` failed, or the branch name cannot be placed in a refspec. `detail` is `git`'s own message when it ran. A name that is empty, starts with `-`, or contains `:`, NUL or a newline never runs `git`, and `detail` is then exactly `the branch name cannot be used in a refspec.` |
| `request_superseded` | — | A browse request was given up because a newer one took its slot |

`session_notice` codes:

| `code` | `params` | Meaning |
|---|---|---|
| `launch_failed` | `title`, `reason`, `reason_code`, and the failure's own params | The agent of a session opened with `open_session` could not be launched after the reply, so the session is gone again (if it could not be deleted it is left `interrupted`, and the notice is still sent); `title` is its title and `reason` the daemon's English account of why. `reason_code` is the `code` of the `error` the launch failed with (`binary_not_found`, `directory_unreachable`, `shell_environment_timeout`, … or `internal_error`), and that error's params (`binary`, `path`, `shell`, `command`, `timeout`, …) come alongside, so a client words the reason from its own text for that code and shows `reason` for a code it does not know |
| `claude_workspace_untrusted` | — | Claude Code is not yet trusted with the session's directory, so the project's own `allow` rules are ignored until its trust prompt is answered |
| `queued_messages_dropped` | — | Octoboard dropped what it had queued for the session; its input line may hold part of a message |
| `trust_answer_failed` | `agent`, `reason`, `reason_code?`, `detail?` | Octoboard could not press the agent's trust confirmation, so the user answers it in the terminal; `agent` names the agent, `reason` is the daemon's English account of why and `reason_code`, when the failure has one, names it (the `error` of the same code carries the same params) |
| `trust_not_carried_over` | `agent`, `path`, `reason`, `reason_code`, `detail?` | As the `error` of the same code: Grok Build's trust entry did not reach the user's own store, after Octoboard's press or after the person answered Grok's confirmation in the terminal (an answer that writes no entry, a decline, is not reported) |

`reason_code` values of `trust_answer_failed`, which a client words in place of `reason` and shows `reason` for
one it does not know:

| `reason_code` | `detail` | Meaning |
|---|---|---|
| `screen_gone` | — | The trust confirmation is no longer on the terminal (for Grok Build: its process has ended) |
| `cursor_not_at_start` | — | The cursor is not on the option it starts on (Claude Code's "No, exit", Codex's "1. Trust and continue"), or could not be found |
| `cursor_did_not_move` | — | Claude Code's cursor did not move to "Yes, I trust this folder" after the Down, so Enter was not sent |
| `terminal_not_settled` | — | The terminal did not settle, so Enter was not sent |
| `cursor_moved_away` | — | The cursor is no longer on the option that trusts the folder, so Enter was not sent |
| `screen_not_dismissed` | — | The confirmation did not go away after the key was sent (for Grok Build: it ran no hook within about 12 seconds of the key; a hook that comes later still has Grok's entry carried over, and a failure to carry it is then its own `trust_not_carried_over` notice) |
| `screen_redrawn` | — | The confirmation was drawn again after the key was sent |
| `input_touched` | — | Something else wrote into the terminal meanwhile, so the keys were not sent |
| `terminal_write_failed` | the system's own message | Writing to the terminal failed |

`reason_code` values of `trust_not_carried_over`, worded the same way:

| `reason_code` | `detail` | Meaning |
|---|---|---|
| `not_recorded` | — | No trust entry for the folder appeared in the session's copy of Grok's store within a few seconds of the hook that followed the answer |
| `store_locked` | — | The user's store stayed locked (its `.lock` file held) by another program |
| `store_unreadable` | — | The user's store could not be read as a trust store, or the entry could not be added without changing anything else in it, so it was left as it is |
| `store_io_failed` | the system's own message | Reading or writing the user's store failed |

### Records

```
Host    { id, name, kind: "local"|"ssh", ssh_config? }
Console { id, name, workdir, default_agent, project_session_agent?,   // null: project sessions follow default_agent
          claude_account_id?, codex_account_id?, grok_account_id?,
          claude_console_session_model?, claude_console_session_effort?,
          codex_console_session_model?, codex_console_session_effort?,
          grok_console_session_model?, grok_console_session_effort?,
          claude_project_session_model?, claude_project_session_effort?,      // the fallback of a project's own
          codex_project_session_model?, codex_project_session_effort?,        // unset values
          grok_project_session_model?, grok_project_session_effort?, icon?, console_session_prompt?, created_at }
Account { id, agent, name, config_dir, approval_reviewer }
Project { id, console_id, host_id, name, path, default_agent?, source, remote_url?,
          claude_account_id, codex_account_id, grok_account_id,   // each a ProjectAccount
          claude_session_model?, claude_session_effort?,
          codex_session_model?, codex_session_effort?,
          grok_session_model?, grok_session_effort?,
          trust_consent, pinned, tags: string[] }
Session { id, agent, agent_session_id?, console_id, project_id?, host_id,
          role: "console"|"project", origin: "console"|"user", title, terminal_title?, usage?: SessionUsage,
          status: "starting"|"working"|"waiting_user"|"idle"|"interrupted"|"archived",
          has_conversation, bound_to?, colour?: "<family>", ordinal?,
          account_id?, config_dir?, model?: string|null, effort?: string|null, pinned, locked, started_at, ended_at? }
SessionUsage { input_tokens, output_tokens, total_tokens, cached_input_tokens?, cache_write_input_tokens? }
Page    { id, console_session_id, html, anchor_message_id?, created_at }
Settings  { auto_pull_repositories, default_clone_dir, accounts: Account[], codex_default_approval_reviewer }
GitStatus { project, repository, branch?, detached, upstream?, ahead, behind,
            activity: "idle"|"checking"|"pulling"|"pushing", error? }
AgentAvailability { agent, availability: "not_determined"|"available"|"unavailable", default_account_dir? }
```

`Project.remote_url` is the git remote URL of the project's repository, recorded for every source and absent when
the directory is not a repository or has no remote. It is the URL as written in the configuration, before any
`url.<base>.insteadOf` rewrite, which is a setting of the machine and not of the repository; a clone starts from the
URL the request gave it and converges on the configured one at the next read. Of the remotes, `origin`'s is
recorded, else the branch's upstream remote's, else the first one listed (by configuration scope: system, global,
local, worktree): `origin` comes first so that the value says where the working tree came from and does not move
with the checked-out branch. It is written when the project is associated, re-read for every project once at daemon
start, and kept current by the git status check (step 3 of "Daemon behaviour, per project"); a change reaches
clients as `project_upserted`.

`Session.terminal_title` as a client receives it is the session's resolved title, not the terminal's own: the title the
agent's own records name the conversation with, else the task in the last terminal title with the agent's decorations
taken off (spinners, status glyphs, activity and approval markers, the agent's name, the project's name), else for Codex
a stand-in from the conversation's first prompt. It is absent when none of them says anything, and then clients use
`title` alone. So it can be set while the agent has sent no terminal title, and an empty or placeholder terminal title
does not by itself leave it absent. The daemon sends a `session_upserted` for a title change only when the resolved
title changes: a spinner turning or a status glyph switching in the terminal title changes nothing.

The store keeps the terminal title as the agent sent it, and the record titles beside it (the name and the first prompt
read from the agent's own files, which survive process exit, archiving and restart). The terminal title is the last
valid UTF-8 one emitted by the session's current process with OSC 0 or OSC 2, terminated by BEL or `ESC \`. The daemon
observes the PTY independently of attached clients, keeps at most 4096 title bytes, trims surrounding whitespace, drops the animated activity
glyphs it starts with (the set listed under "Leading activity glyphs" in `docs/product/session-titles.md`) and rejects
control characters. A title left blank by that clears it. It is stored independently of the user-managed `title`, retained
across archiving, process exit, daemon restart and resume until the next valid title, and the resolved title is carried
by `session_upserted` and snapshots. Unsupported OSC commands and titles inside other control strings have no effect;
raw terminal output is passed through unchanged. A repeated equal title causes no event. An older process cannot replace
the title once a successor has been registered for the same session.

`Session.usage` is the last valid cumulative token usage read from the agent's native records, persisted independently
of other session changes and retained when archived or restarted. Missing usage means unknown, not zero. Input includes
cached and cache-write input; the optional cache counts are subsets, and reasoning is already included in output. Clients
must not add these subsets again. Native totals are used for Codex and Grok; Claude totals sum input, cache input and output
once per native message id, including internal agents stored under that parent session's own `subagents` directory.
Codex reads `event_msg.payload.info.total_token_usage` on `token_count` records; Grok reads the persisted `usage.json`
ledger's `session` totals (the same ledger exposed by `grok usage`). These are usage counts, not current context size or
billing estimates. Inherited usage follows each agent's own resume/fork records. A change of native conversation id clears
the old usage; switching accounts with the same conversation preserves it.

One daemon collector checks live sources at a one-second interval, reading only newly completed JSONL lines and skipping
unchanged ledgers. Updates appear when the agent writes its own usage, which may be after a model call or turn completes;
streaming tokens are not estimated. Existing archived records are read once at daemon start if still available, and a live
session gets a final scan after its process exits. Missing, malformed or unsupported data leaves the last valid total intact.
Filesystem work runs on one blocking worker, source paths are indexed once per account in a pass, line parsing is capped at
8 MiB per record, and changed totals commit in one transaction before `session_upserted` events. Source identity is checked
again under the store lock so a stale scan cannot write a previous conversation's usage after a resume or account switch.

`Session.origin` is `console` for a session started by a session's `start_session` tool (a console session, or a
project session that is unbound or a lead session) and `user` for one the user opened, or that the user approved through
`request_session`.

`agent` is one of `claude`, `codex`, `grok`. An `Account` is a named config directory of one agent, kept
application-wide and referred to by id wherever a config directory is referred to — a console's per-agent field, a
session's own copy of it. `Account.name` is required and unique within `agent`, compared trimmed and case-insensitively
(not compared across agents); the default account's own name takes part in that comparison, even though it is not a row
here — see below. `Account.config_dir` is an absolute, lexically normalised path; existence is not checked when it is
set (Claude Code creates a missing directory on first run, and the daemon creates a missing Codex directory before
launching it; Grok Build is checked at launch instead). The whole account list travels as `Settings.accounts`, and a
client is told about any change to it through `settings_updated` — there is no separate account event.

`Account.approval_reviewer` is `null` (the default for older accounts), `user`, or `auto_review`, and is
settable only for Codex accounts. `Settings.codex_default_approval_reviewer` has the same values for the Codex default
account. A null override inherits the effective Codex home's `config.toml`. Non-null values become per-launch
`approvals_reviewer` CLI overrides on fresh launches and resumes, without editing that file, sandbox settings or the
approval policy. The override is looked up from the session's current account at launch time. A deleted account or a
legacy pinned directory with no account id inherits its original home's configuration rather than the default account's
override. These account settings stay local and are not included in configuration exports or changed by imports.

**Every agent also has a *default* account, which is not a row in `Settings.accounts` at all**: it is the state of
pinning nothing, named by `Console.*_account_id` and `Session.account_id` being unset. It cannot be created, renamed
or removed, and what it is shown as on screen is a client-side concern this protocol does not carry.

`Console.*_account_id` is the account each agent's sessions opened in this console read, by id; unset means that agent's
default account. `Project.*_account_id` is the same setting for the sessions opened in one project, and has one state
more than a console's: a `ProjectAccount`, an object tagged by `kind`, which is always present on a `Project` record.
`{"kind":"inherit"}` takes the console's reference for that agent (the state of a project that never set one, and the
one a deleted account leaves behind); `{"kind":"default"}` is the agent's default account even where the console
references a named account; `{"kind":"account","id":…}` is that account. A request may send `null`, or leave the field
out where absent is meaningful, for inherit; a bare string is not accepted. A session reads only its own agent's. The
account a session opens under is resolved, in descending priority, from the one chosen for that session
(`open_session`'s `account`), else the project's reference for the session's agent, else the console's, else the agent's
default account; a console session has no project, so its chain skips that step. A project whose reference is `default`
ends the chain at the agent's default account, and one whose reference is `inherit` falls through to the console's.
Claude Code is launched with `CLAUDE_CONFIG_DIR` set to the resolved directory and Codex with `CODEX_HOME`, each over
any value in the user's shell environment; unset leaves that environment as it is. For Claude Code, pointing it at
`~/.claude` is not the same as leaving it unset, because Claude Code reads its global config from `<dir>/.claude.json`
whenever the variable is set and from `~/.claude.json` otherwise — which is why the default account is never offered as
a second, pinned account of its own. For Grok it replaces `~/.grok` (or the shell's `GROK_HOME`) as the *source*
directory the session's private home is built from, so the user's config, login, trust store and session records come
from it; Grok itself still runs against that private home.
`Console.*_console_session_model` and `Console.*_console_session_effort` are the model and reasoning effort a console
session of the console is launched with, one pair per agent in that agent's own spelling, read from the console when
the session is opened or resumed; unset means the agent's own default. Only a console session reads them.
`Project.*_session_model` and `Project.*_session_effort` are the same for a session of one project, and are read by
that project's sessions alone. `Console.*_project_session_model` and `Console.*_project_session_effort` are the pair a
project session of the console falls back to for each value its project leaves unset; unset means the agent's own
default. A project session therefore resolves each value as: the session's override, else its project's, else its
console's project session value, else the agent's own default; a console session never reads the project session pair.
`Session.model` and `Session.effort` are independent launch overrides, fixed when the session is opened. An absent
field inherits the project's current setting (else the console's project session setting), or the console's console
session setting for a console session, read at every launch; an explicit `null` uses the agent's own default even
when the parent sets a value. A string pins that value. Every launch, resume and account switch uses these overrides
before the inherited values. Sessions stored before the fields existed keep inheriting, and an override survives
daemon restarts.
`Console.icon` is the console's custom avatar, a `data:image/...` URL of at most 256 KiB (the UI sends a 128x128
WebP or PNG); unset means the default glyph.
`Console.console_session_prompt` is the user's own text, stored trimmed, added after the built-in instructions in the
console's instruction file, inside user tags; unset means nothing is added. A configuration export carries it (see
"Configuration export"). The MCP tools neither read nor set it.
`Session.bound_to` is the id of the session this session reports to — a console session, or a project session of the
same project that can own sessions — or unset outside the orchestration. Set when the session is created, and afterwards
changed only by `transfer_session` (an owner hands a session of its own to another owner) and `bind_to_session` (an
unbound project session binds itself to an owner), both MCP tools; it is never cleared, so a bound session does not
become unbound again. Always unset for a console session itself, which is never bound. A **lead session** is a project
session that was opened unbound and later bound to a console session of its console; it keeps the sessions bound to it,
so its own `bound_to` names a console session while other project sessions' `bound_to` name it. Nothing in the record
sent to clients marks a lead session (the daemon keeps it apart), and a session becomes one by calling `bind_to_session`
with a console session; a `transfer_session` to another console session keeps it one, and one to a project session,
possible only while it owns no session, ends it. A session bound to a lead session owns none. `report` and the
synthesised report both deliver to this session; see "Reporting", "Lead sessions" and "Changing a session's owner" in
`docs/product/hub-orchestration.md`.
`Session.colour` is a console session's badge colour, one entry of a fixed palette of sixteen, each written as its family
name: `red`, `orange`, `amber`, `yellow`, `lime`, `green`, `teal`, `cyan`, `sky`, `blue`, `indigo`, `violet`,
`fuchsia`, `pink`, `brown` and `gray`. The ids are never shown to the user; what each one looks like is the client's.
A colour is assigned on creation (picked at random among the entries no other non-archived console session of its
console holds, or among all of them when every entry is held) and changed afterwards only by `set_session_colour`;
unset for a project session. The labels of the earlier palettes are rewritten when the store opens, so none is ever
sent: `olive` to `lime`, `jade` to `green`, `azure` to `sky`, `rose` to `pink`, the branch-only `purple` to `fuchsia`,
and every `<family>-<shade>` (`red-1` to `gray-3`) to its family.
`Session.ordinal` is a console session's place in its console's history — one past the highest ever used there, never
reused after archiving or deletion; unset for a project session.
`Session.account_id` is the account this session's own agent reads, by id, resolved as above when
the session is opened (unset means the default account), and changed afterwards only by a successful
`switch_session_account`. `Session.config_dir` is that account's directory at the same moment, written with it and
never alone: each agent keeps a conversation's transcript inside it, so `resume_session` relaunches with this value and
not the account's current one. It is unset for
sessions started on the default account, which resume under whatever the shell exports at that moment. A launch
whose pinned directory no longer exists is refused — for every agent — only when the session has a conversation to
resume (`config_dir_unreachable`); a new session, or one with no conversation yet, launches into it instead. Grok Build
additionally refuses a pinned source home that is not an initialized Grok home (`grok_home_not_initialized`),
whether or not there is a conversation to resume, since it cannot safely create one the way the other two agents can.
`trusted_directories` is a list of absolute, lexically normalised directory paths (no symlink is resolved). A project is trusted when its path equals one or lies below one, compared component by component (`/a/Project` does not cover `/a/Project2`). They are added by `confirm_trust` with `trust_parent_dir` and removed by `remove_trusted_directory`. The comparison looks at a project's path only, not at its `host_id`: there is one local host today, and a second would need its own set. A symlink is not followed when comparing, so a project reached through a link inside a trusted directory is trusted wherever the link points, and a project outside the directory is not, however it is linked from inside.
`Project.trust_consent` is true once the user has agreed, in the dialog a `trust_prompt` opens, that Octoboard may press any agent's trust confirmation for that project's directory without asking, and that press succeeded: it is the project form of the one permission every agent shares, a trusted directory being the other, and it is not any agent's own trust record. It is only ever set by `confirm_trust` with `remember`; `update_project` neither sets nor clears it, and removing the project removes it.
`pinned` (on a project and on a session) is the user's pin, false until set: by `update_project` for a project and `set_session_pinned` for a session. It is only a flag for the client to order by; archiving and resuming leave it alone.
`Session.locked` is false until the user sets it with `set_session_locked`. It survives reopening and account
switching and does not change ordering. Archive, delete and bulk delete refuse the operation with `session_locked`
(`session`: the locked session id) if the selected session or any session included by its cascade is locked. Removing
a console or project follows the same rule. A bound session's completed report is still delivered while it or its
archive group is locked, but it is not automatically archived. The MCP tools cannot change the lock. Changing a lock while any archive or delete operation is in progress is
refused with `session_lock_busy`; retry after that operation finishes.
`Project.tags` are the user's free-form labels, used by the client to filter the project list; there is no tag registry, so the tags in use are the distinct ones across projects. `add_project` and `update_project` normalise what they are given: each tag is trimmed, the empty ones are dropped, and a tag matching an earlier one ignoring case is dropped (the first spelling wins). The order is kept, and a tags list never fails validation. A project from before tags existed has none.
`Page.anchor_message_id` is stored and never read (see "Data model" in `docs/architecture.md`). Timestamps are
epoch milliseconds (the UI formats them).
`Settings` is the app-wide user settings the daemon stores, plus the account list — already a record that grows
without a new request shape or a new event for every addition, which is where `accounts` belongs too.
`auto_pull_repositories` (default `false`) governs step 5 below:
off, the periodic check still fetches the remote and reports how far ahead or behind the branch is, but never moves
it; on, a branch that is behind and can fast-forward is fast-forwarded. It never pushes, and it never merges
non-fast-forward. Turning it on also fast-forwards straight away, without waiting for the next check — see
"Fast-forwarding when the setting is turned on" below.
`GitStatus` is one project's live git state, derived from its working directory on request and never stored; see
"Daemon behaviour, per project" below for how it is built. `repository` is `false` when the project's directory is
not a git repository, and every other field is then at its empty value. `branch` is the branch name, the short
commit id when `detached`, or `null` when it cannot be read (an empty repository with no commit yet, or a failed
read). `upstream` is the configured upstream ref (e.g. `origin/main`), `null` when the branch has none — which is
also when `ahead` and `behind` are meaningless and both `0`. `activity` is `checking` while the remote is being
contacted and the status read, `pulling` while the branch is being fast-forwarded, and `idle` otherwise; it is a
tri-state rather than a boolean because the client words the two in-flight phases differently. `error` is the
verbatim, untranslatable `git` or operating-system message from the last failed step, cleared only by that same step
succeeding again — a fetch that fails does not stop the local read, so a status can carry both an error and usable
numbers. A step that runs past `GIT_COMMAND_TIMEOUT` produces neither: its message says only that it did not finish
in time.

### Agent availability

`AgentAvailability` is one agent's availability and what its default account currently resolves to, derived once per
daemon start from one login-shell snapshot — the same kind a launch takes — and held in memory, never in `Settings`:
it is written only by this determination, never by the user's own updates. `availability` has three states:
`not_determined` (what every run of the daemon begins with, for every agent, until the snapshot lands),
`available` (the agent's binary resolved on the snapshot's `PATH`) and `unavailable` (it did not). "No agent
available" is `unavailable` found on every agent, never `not_determined` on any of them — the two must not be
conflated, since the latter means nothing has been checked yet, not that nothing was found.

`default_account_dir` is the directory the agent's default account currently resolves to: the directory its own
variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GROK_HOME`) is exported to in that same snapshot, else the agent's own
usual default (see "Agent config directories" in `docs/product/consoles-and-projects.md`) under the snapshot's own
`HOME`. For Grok Build this is the *source* home a session's per-session home would be built from, never the
per-session home itself, which is Octoboard's own and exists only for the duration of a process. It is `null`
exactly while `availability` is `not_determined` — there is nothing to show yet, and this is what makes the
not-yet-determined state representable on the wire rather than implied by an absent field.

Determining this creates or removes no account: the default account exists for every agent by construction (it pins
nothing), so nothing here mints one, and a snapshot that does not complete — any of the three ways `env_shell`'s own
snapshot can fail — leaves every agent exactly as it started, `not_determined`, rather than being read as a failure
of the daemon's own start or as every agent being unavailable.

`open_session` refuses with `agent_not_available` when the session's resolved agent is `unavailable`; it refuses
nothing while that agent is still `not_determined`, since the launch's own refusal of a missing binary already
covers a binary that turns out not to be there. The console session's own `start_session` tool is refused through
the same path, as a tool error carrying the same reason.

### Agent models

`list_agent_models` is how a dialog learns which models to offer, so that no model name is written into a client.
Each `models` entry is `{id, name, efforts}`: `id` is what Octoboard passes to the agent as its model (the value of a
setting such as `claude_session_model`), `name` is what to show, and `efforts` is the reasoning-effort levels to offer
for it, in the agent's own spelling. It is never unknown: it is the model's own levels, in the catalogue's order, where
its catalogue entry gives them, else the agent's whole measured level set (Claude Code `low`, `medium`, `high`, `xhigh`,
`max`; Codex those and `ultra`; Grok Build `low`, `medium`, `high`, `xhigh`), which is safe because levels, unlike model
names, are not versioned. `[]` means the model takes none. A client offers exactly the levels listed. Because the
fallback is per agent rather than per model, and a catalogue can be incomplete, a listed level is one to offer, not a
promise: the agent can still refuse a level this model does not advertise, or quietly lower it (Claude Code lowers one
its resolved model does not support, so for it an entry is a level accepted, not one honoured).

Per agent (the measured CLI facts are in `docs/agent-cli-reference.md`, "Model and reasoning effort"):

- **Claude Code** is always answered, with no subprocess: `best`, `opus`, `sonnet`, `haiku`, each resolved by Claude
  Code itself to the newest model of its family (`best` is the newest model the account has, falling back to Opus, so
  none of the four can be refused), each with `low`, `medium`, `high`, `xhigh`, `max`.
- **Codex** runs `codex debug models` and keeps the entries whose `visibility` is `list`, by ascending `priority`;
  `id` is the `slug`, `name` the `display_name`, `efforts` the `supported_reasoning_levels` (the agent's set when the
  entry has none that can be read; an empty array is none).
- **Grok Build** runs `grok models` and takes the ids it prints, in its order; `name` and `efforts` come from the
  `models_cache.json` that command refreshes in the account's home, and an id the file does not know, or no readable
  file, gives the id as its name and Grok's set of efforts (an entry saying `supports_reasoning_effort: false` gives
  none). A model the file marks hidden is dropped, and if that drops every printed id the output counts as unreadable.

A catalogue belongs to an account, so the command runs in the environment a session of that account would get: the
login-shell snapshot, with the agent's configuration-directory variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
`GROK_HOME`) set to the account's directory when `account` names one, and left as the snapshot has it for the default
account.

The answer is cached in the daemon per agent and configuration directory, so two accounts never share one: a list that
was read for ten minutes, a failure for thirty seconds (long enough that reopening a dialog does not wait out a
timeout again, short enough that a login completed meanwhile is seen at once).

When there is no list, `models` is empty and `unavailable` says why; the request still succeeds, so a form that asked
stays usable. A client shows the reason and falls back to the agent's own default or a free-text field.

| `code` | `params` | Meaning |
|---|---|---|
| `models_binary_not_found` | `binary` | The agent's binary does not resolve on the login shell's `PATH` |
| `models_shell_environment_failed` | `detail` | The login-shell snapshot the command runs in could not be taken |
| `models_timed_out` | `agent` | The command did not finish within 15 seconds |
| `models_command_failed` | `agent`, `detail` | The command could not be run, exited non-zero, or printed more than 8 MiB; `detail` is the exit status and the last line of the first 4 KiB it wrote to stderr (which can end mid-line), or the reason it could not be run or kept running, such as `produced more than … bytes of output` |
| `models_unreadable_output` | `agent` | The command succeeded but printed no model that could be read |

### Account quotas

An `AccountQuota` is how much of one account's plan the agent's own CLI says is used. The daemon asks the CLI itself and
never reads a credential or calls a server of its own. It runs the CLI the way a session of the account would be run:
the login-shell snapshot, the agent's configuration-directory variable set to the account's directory (left as the
shell has it for the default account), and the home directory as working directory. The exception is that MCP servers,
plugins and hooks are left out where the CLI allows it without changing the sign-in (see "What a probe touches" in
`docs/agent-cli-reference.md`).
The commands and the replies they give are in the same document, "Plan quota". The readings are runtime state: held in
memory, never stored, never part of `get_settings` or an exported configuration.

```
AccountQuota { agent, account, fetched_at, plan, windows: [QuotaWindow], unavailable }
QuotaWindow  { kind, label, used_percent, resets_at, window_minutes }
```

- `account` is the account's id, `null` for the agent's default account. There is one entry per account of every
  available agent, the default account included, ordered by agent and then account with the default account first.
- `fetched_at` and `resets_at` are milliseconds since the epoch. `plan` is the plan as the agent names it (`max`,
  `plus`, `SuperGrok`), null when it does not say.
- `kind` is `session` (the five-hour window), `weekly`, `weekly_model` (a weekly window of the one model `label` names)
  or `credit_period` (a pool of credits over a billing period). Claude Code reports `session`, `weekly` and one
  `weekly_model` per model with its own weekly limit, Codex `session` and `weekly`, Grok Build one `credit_period`.
  `used_percent` is used, not remaining, normally 0 to 100 (a value above 100 is passed on as the agent gives it).
  `window_minutes` is the window's length where it is known; a Grok `credit_period`'s is the span of its current
  period.
- `windows` is empty while the account has never been read successfully. When a later attempt fails, the last good
  `windows`, `plan` and `fetched_at` are kept and `unavailable` is set beside them, so a client can show them as stale;
  the next success clears it. A reading that cannot be taken or read is a state of that account only: the agents mark
  these calls experimental, so nothing else is affected by it.

| `code` | `params` | Meaning |
|---|---|---|
| `quota_binary_not_found` | `binary` | The agent's binary does not resolve on the login shell's `PATH` |
| `quota_shell_environment_failed` | `detail` | The login-shell snapshot the command runs in could not be taken |
| `quota_timed_out` | `agent` | The command gave no reply within 15 seconds |
| `quota_command_failed` | `agent`, `detail` | The command could not be run, ended without replying, printed more than 8 MiB, or answered the request with an error; `detail` is the exit status and the last line of the first 4 KiB it wrote to stderr, or the error's message |
| `quota_unreadable_reply` | `agent` | The command replied, but not in a shape that holds a quota window |
| `quota_no_windows` | `agent` | The command replied and the account has no quota windows to report (an account without a subscription, say) |

Readings are kept per agent, account and configuration directory, so an account pointed at another directory never
inherits the old one's. `delete_account`, and `update_account` when it changes the directory, drop the account's reading
at once (a rename keeps it), and a reading still running for it is discarded when it lands. A reading is also dropped
when its account is no longer one of those the poller covers. A background poller started with the daemon reads every
account of every available agent about every five minutes, four CLIs at a time across the whole daemon. An account
whose reading ends unavailable is tried again after twice, four times and then eight times that interval, and a reading
still running is never started twice. A newly created account, or an agent found available after the daemon's start, is
read within the poller's next look, which comes every fifteen seconds. `refresh_account_quotas` runs the same path on
request for a client that wants fresher data when it opens a view: it only reads what is due, so asking on every open
costs nothing. Every change to the list is broadcast whole as `account_quotas_updated`.

### Configuration export

`export_configuration` produces the document a user carries to another machine. The reply, `configuration_exported`,
holds it under `document`; a client saves it as a file and does not build or alter it. The document is JSON and is the
input of the import as much as a file the user keeps, so its shape is part of this contract.

```
Document { format: "octoboard-configuration", version: 1,
           settings?: { auto_pull_repositories },     // present in a full export only; an older document
                                                      // may carry it as `auto_sync_repositories`, read alike
           consoles: DocumentConsole[] }
DocumentConsole { name, icon?, default_agent,
                  project_session_agent?,                           // absent in an older document: follows default_agent
                  claude_account?, codex_account?, grok_account?,   // each an AccountName
                  claude_console_session_model?, claude_console_session_effort?,
                  codex_console_session_model?, codex_console_session_effort?,
                  grok_console_session_model?, grok_console_session_effort?,
                  claude_project_session_model?, claude_project_session_effort?,   // absent in an older
                  codex_project_session_model?, codex_project_session_effort?,     // document: unset
                  grok_project_session_model?, grok_project_session_effort?,
                  console_session_prompt?,                          // absent in an older document: none
                  projects: DocumentProject[] }
DocumentProject { name, path, source: "local"|"parent"|"git", remote_url?, default_agent?, pinned, tags: string[],
                  claude_account, codex_account, grok_account,       // each a ProjectAccountRef
                  claude_session_model?, claude_session_effort?,
                  codex_session_model?, codex_session_effort?,
                  grok_session_model?, grok_session_effort? }
AccountName       { name }
ProjectAccountRef { kind: "inherit" } | { kind: "default" } | { kind: "account", name }
```

- **Scope.** With `console`, the document holds that console alone and no `settings`; without it, it holds every
  console and `settings`. The shape is the same either way: a full export is a list of consoles that happens to hold
  all of them. Consoles are ordered by creation time and a console's `projects` by name, with the stored id breaking
  a tie, so the order is the same on every export; the id itself is not in the document. `projects` is all of a
  console's projects, whether or not a project is a git repository.
- **Nothing is an id.** No console, project or account carries an `id`. A console's `workdir` is not in the document
  either: the daemon derives it from the console's id when it creates the console, so it would carry that id and the
  daemon's own layout, and an import creates its console, and so its directory, itself. A console's `*_account` is
  an `AccountName`, or `null` for the agent's default account. A project's is a `ProjectAccountRef`, which keeps the
  three states of `Project.*_account_id`: `inherit` the console's reference, `default` to pin the agent's default
  account even where the console names another, or the named account. A reference holds the account's name alone:
  its agent is the one whose field it sits in (`claude_account` is a Claude Code account), so the document has no
  second statement of it to contradict. An import matches the name without regard to case, since account names are
  unique within an agent only compared that way (`Work` and `work` are one name); matching exactly would send a
  reference to the default account for no better reason than how the name was typed.
- **Not exported.** An account's `config_dir`, and so the accounts themselves: the importing machine's own accounts
  are what a reference is rebound to. A reference whose account no longer exists, or belongs to another agent than
  its field, is dropped as deleting the account would have left it: `null` on a console, `inherit` on a project.
  Sessions, pages, transcripts, trust state (a project's `trust_consent`, the trusted directories) and
  `Settings.default_clone_dir` are not configuration and are not in the document; the last is the importing
  machine's own to choose, and an import places projects under it.
- **Machine-local paths.** `Project.path` is an absolute path of the exporting machine, as stored, and is only a
  starting point for an import to resolve against its own filesystem. It is the one machine-local path in the
  document.
- **`remote_url`** is what is recorded for the project (see "Records"); the export reads it from the stored record
  and runs no `git`. It is absent for a directory that is not a repository and for one with no remote.
- **Models and efforts** are exported as stored. They are free text read only when a session is launched, so a value
  the importing machine's agent does not offer is not an error here.
- **Version.** `format` tells a configuration document from any other JSON; `version` is an integer, 1 today. It is
  raised when a change would make a reader written for the older version misread a document: a field removed, or
  given another meaning. A field added is not such a change, and a reader ignores one it does not know. An import
  refuses a `format` it does not know, or a `version` newer than it supports, before creating anything.

### Configuration import

An import is two requests. `inspect_configuration` reads a document and answers what an import of it would meet on this
machine, changing nothing; `import_configuration` takes the document again, together with the choice made for each
project, and creates. A client sends the same `document` to both and does not alter it between them. The document is
untrusted input: its paths and URLs are data this machine checks, and nothing in it is acted on as it stands.

```
configuration_inspected { settings?: { auto_pull_repositories, current_auto_pull_repositories },
                          consoles: InspectedConsole[] }
InspectedConsole  { name, account_fallbacks: AccountFallback[], projects: InspectedProject[] }
InspectedProject  { name, path, remote_url?,            // as the document records them
                    offers: { exported_path?, default_clone_dir?, clone? },   // each the directory it would use
                    default?: "exported_path" | "default_clone_dir" | "clone",
                    account_fallbacks: AccountFallback[] }
AccountFallback   { agent, name }

import_configuration { document, resolutions?: ProjectResolution[], apply_settings?: bool }
ProjectResolution { console, project,                   // indexes into the document
                    choice: { kind: "exported_path" } | { kind: "default_clone_dir" } | { kind: "clone" }
                          | { kind: "directory", path } }

configuration_imported { settings_applied?: { auto_pull_repositories },
                         settings_error?: Failure,   // the settings asked for could not be applied
                         consoles: ImportedConsole[] }
ImportedConsole   { name, status: "created" | "failed",
                    console?,                           // the id of a created console
                    error?: Failure,                    // of a failed one
                    account_fallbacks: AccountFallback[],    // empty unless the console was created
                    projects: ImportedProject[] }       // all "not_attempted" for a failed console
ImportedProject   { name, status: "created" | "left_out" | "failed" | "not_attempted",
                    via?,                               // the choice that placed a created project
                    project?,                           // the id of a created project
                    error?: Failure,                    // of a failed one
                    account_fallbacks: AccountFallback[] }   // empty unless the project was created
Failure           { code, params, message }             // as an `error` reply carries them
```

- **Refused before anything is created.** The document is refused when it is not a configuration document
  (`configuration_not_recognised`), when its `version` is 0 or newer than this daemon reads
  (`configuration_version_unsupported`; checked before the rest of the body is read, since a newer version may be shaped
  in ways this one cannot read), or when it cannot be read (`configuration_unreadable`). Applying also refuses an
  impossible resolution (`import_resolution_invalid`), a request for settings the document does not carry
  (`import_settings_missing`), and whatever `create_console` would refuse of any console of the document
  (`icon_not_an_image`, `icon_too_large`). Past that point nothing is refused and nothing is undone: what goes wrong is
  reported in the result (below), because by then earlier consoles and projects exist and have been broadcast, and a
  rollback would contradict that an import always creates and goes through the ordinary association.
- **Four directories.** A project is placed in one of: the path the document records (`exported_path`); the last
  segment of that path under this machine's default clone directory (`default_clone_dir`; Settings, `~/Projects`
  unless set), the usual case of the same layout under another root; a fresh clone of the recorded `remote_url` into
  the default clone directory (`clone`); or a directory the caller names (`directory`, never offered). Inspecting
  offers only the choices the project's state allows, and each offer is the directory it would use. The recorded path
  is offered when it is absolute and is an existing directory; the derived path when it is an existing directory; the
  clone when the URL may be cloned from (below) and the directory the clone would create does not exist yet. The
  `default` is the first offered of the recorded path, the derived path and the clone, and absent when nothing is
  offered. A project with no offer is left out unless the caller points at a directory.
- **What may be cloned from.** `remote_url` is checked as data: an `http`, `https`, `ssh`, `git+ssh`, `ssh+git` or `git`
  URL, or the `host:path` form, with no whitespace or control character in it and a host that does not begin with `-`,
  nor does the part before it. A local path, a `file:` URL, `ftp` and the helper transports (`ext::`, `fd::`) are not
  offered and are refused when chosen (`import_choice_unavailable`), so a document cannot make the daemon run a command
  or read a path of this machine. This is stricter than `add_project`, which leaves the choice to git: a few transports
  git supports are not accepted here. The recorded path is likewise taken only if it is absolute (not relative, not
  starting with `~`), and the derived path is built from the last segment of the path after its `.` and `..` are
  resolved.
- **Accounts.** A reference is rebound to the account of the agent whose field holds it with the same name, compared
  without regard to case. One that matches no account is bound to that agent's default account (`null` on a console,
  `default` on a project, pinned rather than inheriting) and is listed in `account_fallbacks`: on the console for its
  own references, on the project for the project's. Inspecting lists what would fall back; the result lists what did,
  and a project's list is empty unless the project was created.
- **Applying.** Every console of the document is created new, with its own working directory, whether or not one of its
  name exists: console names are not unique. The projects of a console go through the ordinary association
  (`add_project`), so each is checked and broadcast exactly as one added by hand: the directory must exist, the tags and
  launch settings are normalised, and a directory already associated with that console is refused
  (`all_projects_already_added`, reported on the project). A project's pin, tags, default agent (none stays none and is
  not detected from the directory), account references, models and efforts come from the document; its `source` is `git`
  for a clone and `local` otherwise, and its `remote_url` is the one it was cloned from or the one read from its
  directory. Folder trust is not in the document: an imported project is untrusted like any other and goes through the
  trust flow when its first session opens.
- **Application settings.** A full export carries `settings`; a single console's does not. `settings` of the inspection
  is present when the document has them, with the document's value and the value this machine has now, so a client can
  show what applying would change. They are applied only when `import_configuration` is sent `apply_settings: true`,
  because `auto_pull_repositories` makes the daemon fast-forward the user's repositories on its own, which changes how
  this machine behaves. They are applied last, through the same path as `update_settings` (so they are normalised and
  `settings_updated` is broadcast when the value changes), and `settings_applied` in the result reports what was
  applied. Asking for them of a document that has none is refused (`import_settings_missing`) rather than ignored, so a
  request never silently does less than it was told to. Settings that could not be applied are `settings_error`, and the
  import is reported all the same.
- **In the result.** A project with no resolution is `left_out`. One whose directory cannot be obtained (the path is
  gone, a clone fails or its target exists, the choice has nothing to use) is `failed` with its `error`, and the rest
  carry on; its console is still created. A console that cannot be created (the disk is full, the data directory cannot
  be written) is `failed` with its `error` and no `console` id, its projects are `not_attempted`, and the next console
  is tried. The reply is `configuration_imported` in every one of these cases, with whatever was created before and
  after it, so a client never has to piece the outcome together from the broadcasts.
- **How long it takes.** Projects are placed one at a time, in document order, so a clone, which may run for minutes,
  holds one blocking thread and no document holds more. The reply therefore comes after the last clone has finished,
  which can be as long as all of them together (each is cut off after 30 minutes), and every console and project created
  meanwhile has already been broadcast. There is no request to cancel an import in progress.

### Daemon behaviour, per project

What `refresh_git_status` does for each of a console's projects, a few at a time, and what `pull_project_branch` does for
the one project it names; the daemon has no timer of its own for this, so no check runs until a client asks, and a
project already checked within the last minute is skipped entirely, except for `pull_project_branch`, to which that
one-minute floor does not apply (see `refresh_git_status` above):

1. If the directory is not a git repository, broadcast `repository: false`, `activity: idle`, and stop. Such a
   directory also has its recorded `remote_url` cleared (broadcasting `project_upserted` if it was set) — but only
   when the directory itself can be seen: one that is missing or unreachable, such as on an unmounted volume, keeps
   the value, since it says nothing about the remote.
2. Once the check has run for about 400 ms (at once for a `pull_project_branch`), set `activity` to `checking` and broadcast `project_git_status` (a check
   that ends sooner broadcasts only its result). That broadcast carries the branch, upstream,
   counts and error of the project's previous status (none before its first check), so the badge keeps showing them
   while the check runs; the check itself starts from a blank status, which the steps below fill in.
3. Read the repository's remotes with the URL each is configured with, `git config --get-regexp
   '^remote\..*\.url$'` (all of `git`'s configuration, the same set `git remote` and `git fetch` read), and make
   the project's recorded `remote_url` the one that read gives (see "Records" above): when it differs from the
   stored value, write it and broadcast `project_upserted`. A read that fails, or an upstream that could not be
   read when it was needed to choose between remotes, leaves the stored value as it is. This is the same call that
   tells which remote to fetch, not an extra one (a repository the call finds no remote in is asked once more, `git
   rev-parse --git-dir`, to tell it from a damaged `.git`; that confirmation covers only an empty result, since
   both commands search the parent directories, and what a damaged `.git` makes them read may come from the global
   configuration or an enclosing repository until it is repaired). Then, if the repository has no remote at all,
   skip the fetch — that is not an error. Otherwise run `git fetch --quiet` in the project's directory, in the
   environment "Git environment" describes (as is every `git` of these steps); a failure (offline, authentication, no
   upstream remote) records `error` and the check continues to the next step regardless, so the branch name and the
   last known numbers still show.
4. Read the local state in one call, `git status --porcelain=v2 --branch --untracked-files=no`, taking `branch`,
   `detached`, `upstream`, `ahead` and `behind` from its `# branch.*` header lines; `--untracked-files=no` skips
   enumerating the worktree's files, which this step never reads anyway.
5. If `auto_pull_repositories` is on (or the check is a `pull_project_branch`), and `upstream` is set, and
   `behind > 0`, and `ahead == 0`: set `activity` to `pulling` and broadcast, run `git merge --ff-only <upstream>`,
   then redo step 4 to pick up the new numbers. A failed merge records `error`; `git` itself refuses rather than
   clobbering local changes, which is why nothing here checks separately whether the worktree is clean.
6. Set `activity` to `idle` and broadcast.

### Re-reading the branch header when refs change

The daemon watches each git project's refs, so a commit made by an agent session or in a terminal reaches the badge
without waiting for the next poll. A project counts as a git project the way step 1 does: it has `.git` in its own
directory, which for a linked worktree is a file naming its git directory. What is watched is the git directory's
`HEAD` and, in the repository's common directory, `packed-refs` and everything under `refs/heads` and `refs/remotes`,
so a fetch or push from a terminal is seen too; the working tree is not watched. A change is debounced for a few
hundred milliseconds and answered with step 4 alone, published as `project_git_status`: nothing is fetched, and only
what the read produced is merged into the project's status, so the `activity` and `error` of an overlapping check,
pull or push are kept (this includes the refs that check's own fetch or fast-forward writes). A read that fails
records its own `error` and leaves the branch and counts as they were. A project is re-read by one `git` at a time,
a change arriving meanwhile owing it one more read, and at most four projects are read at once. A project whose `.git`
has gone is not re-read; its next check reports `repository: false`. Projects are watched from daemon start, from
`add_project`, and from the first check that finds one that was not a repository before, until they are deleted. A
watch that cannot be set up is logged and that project is left to the client's poll.

### Fast-forwarding when the setting is turned on

An `update_settings` that actually turns `auto_pull_repositories` on fast-forwards, concurrently and right away, every
project already known to be behind its upstream, rather than leaving them to the next `refresh_git_status`. The
request is acked as soon as the work is started; what each project ends up at follows as `project_git_status`
broadcasts.

- **Nothing is fetched**: no step here goes to the remote. The `GitStatus` records the daemon holds are used only to
  pick which projects are worth visiting, so this pass is **not scoped to one console** the way
  `refresh_git_status` is: every project the daemon currently holds a status for is visited, including projects of
  consoles a client showed earlier in this daemon's life. A project with no status yet has never been checked and is
  skipped — nothing is known to fast-forward it to.
- The **one-minute floor** on checks does not apply, since nothing contacts the remote, and the pass counts as no
  check of its own, so it never holds the next `refresh_git_status` off a project. The rule that a project is never
  worked on twice at once still holds across both: a project whose check is already running is skipped here and
  left to that check, which reads the setting itself (step 5), and while this pass is in a project a check of it is
  not started either.
- For each project visited, the gate is **re-read from the repository** before anything moves: step 4's local
  `git status` is run again and `upstream` set, `behind > 0`, `ahead == 0` re-applied to its fresh output, and the
  directory is confirmed to still be a git repository. A project failing any of those, and one whose re-read fails
  at all, is left alone with no broadcast — the previous check's `repository`, numbers and `error` stay as they
  were, for the next `refresh_git_status` to correct.
- Otherwise `activity` goes to `pulling` and is broadcast, `git merge --ff-only <upstream>` runs, step 4 is redone,
  and `activity` goes back to `idle` and is broadcast — as in step 5, including a refused merge recording `error`.
  An `error` the last check left is **not** cleared: it came from a step that contacted the remote, which this pass
  does not do.

Because a status is dropped only when its project or console is deleted, an `error` this pass records for a project
of a console no client is showing stays in the daemon's status for it until that console is refreshed again.

### Git environment

Every `git` the daemon runs in a project's repository or against its remote — the status check's fetch, `git status`
and fast-forward, `push_project_branch`, `switch_project_branch`, `restore_project_changes`, the reads of a project's remote URL, and every browse request — starts from
the user's own shell environment, so that it is their `git`, with their configuration and credentials, and is built
the same way:

- It cannot prompt: `GIT_TERMINAL_PROMPT=0` and `GIT_SSH_COMMAND=ssh -oBatchMode=yes` turn a missing credential, an
  unknown host key or a passphrase-protected key with no agent into an immediate failure, and the askpass helpers are
  removed.
- Every `GIT_*` variable of the shell is removed except those that say which of the user's own material to use and how
  to reach a remote with it: `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM` and `GIT_CONFIG_NOSYSTEM` for their
  configuration, and `GIT_SSL_CAINFO`, `GIT_SSL_CAPATH`, `GIT_SSL_CERT`, `GIT_SSL_KEY`, `GIT_SSL_CERT_TYPE`,
  `GIT_SSL_KEY_TYPE`, `GIT_SSL_VERSION`, `GIT_SSL_CIPHER_LIST`, `GIT_PROXY_COMMAND`, `GIT_HTTP_PROXY_AUTHMETHOD`,
  `GIT_PROXY_SSL_CAINFO`, `GIT_PROXY_SSL_CERT` and `GIT_PROXY_SSL_KEY` for the certificates, keys, TLS settings and
  proxy helper of a connection. That takes out everything that could point `git` at another repository, worktree,
  index, object store or ref namespace, configuration passed through the environment, and `GIT_SSL_NO_VERIFY`. Without
  it, a `GIT_DIR` exported in the shell would send a project's check to another repository and its fetch to that
  repository's remotes; with too little kept, a fetch that works from the user's shell fails here, and its message
  stays in the project's `error` for every later check.
- A command that only reads also gets `GIT_OPTIONAL_LOCKS=0` and `-c diff.autoRefreshIndex=false`, so it never
  rewrites the index of a repository an agent is working in.

Cloning a project and probing a remote (`probe_git_remote`) are the exception: they are started by the user on the spot,
run non-interactively, and pass the whole shell environment.

### Browsing a project

The browse requests — `get_project_source`, `list_project_dir`, `search_project_files`, `read_project_file`, `list_project_changes`,
`read_project_change`, `read_project_change_bodies`, `list_project_branches`, `compare_project_branches`,
`read_project_comparison_change`, `read_project_comparison_change_bodies`, `list_project_commits`,
`read_project_commit_change` and `read_project_commit_change_bodies` — read a project's files, their Git versions, a
worktree's changes, the changes between two branches and the commits a branch made since it forked on demand, for a
client showing them. Their replies
go to the asking socket only: nothing about them is broadcast, nothing is kept for a later client, and none of it is
part of `snapshot`, which carries no file content at all.

Every browse request re-resolves its project from the store and its worktree from the repository as they stand at
that moment. A client names a project by id and a worktree by the id `project_source` gave it, never by a directory:
nothing a client sends is used as a path to read from, apart from the relative `path` inside the project.

```
ProjectSourceInfo { project, root, resolved_root, root_id, git: GitSourceInfo?, git_error? }
GitSourceInfo     { repository, common_dir, worktree, scope, worktrees: WorktreeInfo[] }
WorktreeInfo      { id, root, main, head?, branch?, scope_present }
BrowseEntry       { name, kind: "file"|"directory"|"symlink"|"other", size?, version?,
                    target?: "file"|"directory"|"other"|"missing"|"outside" }
ContentSource     = { kind: "live", root_id, version }
                  | { kind: "index", worktree, blob }
                  | { kind: "commit", commit, branch?, blob }
FileContent       { size, kind: "text"|"binary", media_type?, text?, data? }
ChangeEntry       = { group: "staged"|"unstaged"|"untracked"|"committed", old: ChangeSide, new: ChangeSide }
                  | { group: "conflicted", path, conflict }
ChangeSide        = { state: "present", path, kind: "file"|"symlink"|"submodule", source: ContentSource }
                  | { state: "absent" }
                  | { state: "out_of_scope", repository_path }
SideRead          = { state: "present", path, kind, source: ContentSource, file: FileContent? }
                  | { state: "absent" }
                  | { state: "out_of_scope" }
BranchInfo        { name, commit, origin? }
BranchOriginInfo  { responsible_session, responsible_title, responsible_since }
ComparisonEndpoint { branch, commit }
CommitChanges     { commit, parent?, subject, changes: ChangeEntry[] }
```

#### Project sources

`root` is the project's path as stored, as a wire path; `resolved_root` is the directory it resolves to with its
symbolic links followed, which is the directory every live read of the project is held inside. A project associated
through a link therefore reads where the link points, and the stored path stays as the user chose it. `root_id` is
that directory's identity on its filesystem — its device and inode numbers, with its creation time where the
filesystem records one: it stays the same across a rename and changes when the directory is removed and another put in
its place. The other identities below are built the same way.

`git` describes the Git repository holding the project's directory, or is null when there is none. The daemon looks
for a `.git` entry in the directory and then in each directory above it, stopping below the home directory: the
home directory itself is never taken for a project's repository unless it is the project's own directory, so a home
directory kept under version control does not turn every ordinary project under it into a subdirectory of that
repository. When a `.git` is found but `git` cannot read the repository or list its worktrees (it is not installed,
the directory belongs to another user, the repository is damaged), `git` is null and `git_error` carries the reason,
`git`'s own message where there is one; a client shows Git review as unavailable either way, and the files can still be
browsed.

- `repository` is the repository's identity (that of its common git directory, `common_dir`), the same for every
  worktree of it.
- `worktree` is the identity of the worktree holding the project's directory — the checkout the directory is in,
  which may be a linked worktree, never whichever one has some branch checked out.
- `scope` is the project's directory relative to that worktree's root, as a wire path: empty for a project at the
  root, `packages/ui` for a project associated with that subdirectory. Git review of the project stays inside it.
- `worktrees` lists every worktree of the repository that is there now, the one holding the project included: each
  one's `id`, its `root`, whether it is the `main` worktree, the commit at its `HEAD` (null before the first commit),
  the branch checked out (a wire path, null when `HEAD` is detached; an unborn branch is named, with a null commit),
  and `scope_present`, whether the project's scope is a directory in it reached through no symbolic link (see
  below). A bare main repository, and a worktree `git` reports as prunable, are not listed.

A worktree's id is the identity of its own git directory — the common directory for the main worktree,
`<common>/worktrees/<name>` for a linked one — and is checked against the repository again on every request that
names it: the worktree must still be listed and its `.git` must still lead back to that git directory. A worktree
that was removed, one whose path now holds another checkout, and one removed and added again (which gets a new git
directory) are all `worktree_unavailable` under their old id. With `worktree`, a live read or listing is scoped to
the project's `scope` inside that worktree, never to the worktree's root. The scope there must be exactly that
directory: one that does not exist, and one reached through a symbolic link (`packages/ui` a link to `.` in that
worktree, which would otherwise widen the scope to the whole worktree without anyone choosing it), are both
`source_unavailable`. Choosing a worktree changes only where that request reads: it checks nothing out and moves no
session.

#### Wire paths

A path inside a project, an entry name, a branch name and the paths shown in `ProjectSourceInfo` all travel in one
canonical text form, because a Unix file name is bytes and a JSON string is not: valid UTF-8 as itself, except that
`%` is written `%25`, and each byte that is not part of valid UTF-8 as `%XX` in upper-case hexadecimal. The form is
the name's identity — two names are equal exactly when their forms are — and it is not display text: a client shows
a name by decoding the form to bytes and reading them as UTF-8 with replacement characters, and sends back the form
it was given. The daemon refuses with `invalid_path` a path not in the canonical form (a lower-case escape, an escape
of a byte that needed none), an absolute one, and one with an empty, `.` or `..` component; the empty path is the
project's directory itself. On macOS the file system stores only UTF-8 names, so a name that is not UTF-8 is met only
inside Git — in the index or a commit — and is read from there like any other.

The `path` param of a browse `error` is the request's own `path`, relative to the project's directory, whatever source
was read — except where the error is about the directory the request is scoped to (`source_unavailable`, and a
`permission_denied` on that directory), whose `path` is that directory's absolute path.

#### Reading

A live read or listing resolves the path inside its scope directory with every symbolic link followed. A link that
stays inside is read through; one that leads outside is `outside_scope` and nothing behind it is read; one that leads
nowhere, or round in a loop, is `file_not_found`. The resolved path has no link left in it, and it is opened refusing a
link in any of its components, so a link swapped in after the check makes the request `source_changed` instead of
redirecting it; a listing reads the directory it opened, not the path again. This holds on macOS and on Linux 5.6 and
later; elsewhere only the last component is held to it. A FIFO, a socket or a device is `unsupported_file_type` from
its metadata and is never read; one swapped in after that check is opened without blocking and refused before anything
is read from it. The file's identity, size, modification and change times are compared before and after the read, and
a file that changed in between is `source_changed`, never a body stitched from bytes on either side of the change. A
program writing the file in place can still leave it, for a moment, half written; a read in that moment returns the
file as it then is, as any reader would. `version` is the file's device and inode numbers, size, and modification
and change times at the read, as one opaque string, compared whole and never parsed: two equal versions mean nothing
about the file changed between the two reads. A read from disk has no deadline of its own: a volume that stops
answering holds the read until it answers or fails.

An index read looks up the path's entry in the worktree's index, and a branch or commit read looks up the path in the
commit's tree, both by listing exactly that path — never through a revision expression a file name could take part
in. A path that is a directory there, a symbolic link, a submodule, or an index entry with conflicting stages is
`unsupported_file_type`. A branch is a name below `refs/heads/`; it is checked as a reference name before anything is
read, so `main~1`, `main^`, `@{-1}` and the like are `invalid_branch_name`, not revisions, and it is looked up as
exactly `refs/heads/<name>` — never another reference that happens to end the same way — and resolved to the commit at
its tip, which the reply reports. A commit is accepted only as the full object id of a commit in the repository's
object format: a shorter prefix and the id of a tag are `invalid_commit`. `blob` is the id of the object whose bytes
were returned, and no replacement object stands in for it: the same id always names the same content.

`file` carries the body whole. `kind` is `text` when the bytes are valid UTF-8 with no NUL byte and JSON escaping at
most doubles them, and the body is then in `text`, as a string; anything else is `binary`, its bytes base64-encoded in
`data`, and the daemon never decodes it as text. The doubling rule keeps a file made mostly of control characters —
each of which JSON writes as six bytes — out of `text`; a log with colour escapes stays text. `media_type` is set only
for a binary body, as a hint from its first bytes, for the image formats the daemon recognises (`image/png`,
`image/jpeg`, `image/gif`, `image/webp`, `image/bmp`, `image/x-icon`, `image/avif`), and is null otherwise — an SVG,
being text, arrives as text with no `media_type`. `size` is the body's length in bytes.

A listing names each entry once, in byte order of the names' wire forms, with its `kind`. `size` is a regular file's
size and `version` the `version` a live read of it would report while it stays as it is, so a client holding a file's
body can tell from a listing alone whether it has changed since, a rewrite of the same size included. `target` says
what a symbolic link resolves to — a file, a directory, something else, nothing (`missing`) or something outside the
scope (`outside`, not looked at further). `complete` is false when the listing stopped at a
budget, and the entries then are those read before it stopped, not a chosen subset; it is false too when an entry
could not be read, which is then left out.

#### Searching file names

`search_project_files` searches the project's own directory and all real subdirectories, including hidden and
ignored entries, excluding every entry named `.git`. It matches only the basename, read as UTF-8 with replacement
characters, against the trimmed query using Unicode lowercase on both. An empty or whitespace-only query matches
all files, with the same traversal and result budgets. Returned paths retain their canonical wire encoding; their
byte order is the result order.

Directory symbolic links are not traversed, preventing cycles and duplicate subtrees. A symbolic link to a regular
file is a result only when its target stays inside the project. Files are named without reading their bodies.
The traversal uses the same no-follow directory opens as ordinary live reads. An unreadable entry or subtree makes
`complete` false while other subtrees continue; failure to open the project root fails the request.

The scan stops with `complete: false` at the traversal, depth, result, path-byte or elapsed-time budgets below.
Results are the matches reached before stopping, sorted afterwards, rather than the first matches of a complete
sorted traversal. Cancellation is checked between entries. Elapsed time is also checked between entries, so an
individual filesystem operation on a stalled volume can outlast the time budget. A search reserves the same
reply memory as one directory listing.

#### Telling a stale reply from a current one

A reply says exactly what it answers: `project`, `worktree` and `path` echo the request — a comparison's `left` and
`right` with the commits they were resolved to — and `source` (or `root_id`) identifies the directory, the index or the
commit the content came from and the version or object read. A client that
has since moved to another project, worktree, mode, file or comparison discards a reply that no longer matches what it
shows, and discards by request `id` as well: a request that finished just before a newer one in its slot arrived is
still answered with its real reply. Two kinds of change stay distinct: a reconnect, after which a client asks again
for everything it shows because the daemon keeps nothing from an earlier connection; and a change of content, which no
`snapshot` reports — an agent staging, committing or overwriting a file changes `version`, `blob` or `commit` in the
next reply, and nothing else.

#### Changes and comparisons

`list_project_changes` lists a worktree's uncommitted changes and `read_project_change` reads one of them for its diff;
`compare_project_branches` lists the changes between two branches and `read_project_comparison_change` reads one of
those (see "Comparing two branches" below). `read_project_change_bodies` and `read_project_comparison_change_bodies`
read the whole bodies such a diff was made from (see "Reading a change's bodies" below).

A change is identified by its group and both of its sides, not by a path alone:

- `staged` compares the commit at `HEAD` (the old side) with the worktree's index (the new side), and `unstaged` the
  index with the files on disk. `untracked` is a file on disk that Git neither tracks nor ignores; its old side is
  absent, so it is new content, never compared with an index entry it does not have. A file staged and changed again
  since is two changes, one per group, each read against its own baseline.
- A present side's `path` is a wire path relative to the project's directory, as every browse `path` is, and its
  `source` is the version listed: a commit's blob, the index's blob, or the file on disk with its `version`.
- An absent side — the old side of an added file, the new side of a deleted one, every old side before the first commit
  — carries no path at all, so it can never be resolved to a live file of the same name.
- A side whose path lies outside the project's scope (a rename into the project from elsewhere in the repository, or out
  of it) is `out_of_scope`, distinct from absent, and carries its `repository_path` — relative to the repository's root,
  since no path relative to the project can name it — for display only: no body is read for it, and no patch is made for
  the change, since a patch would carry that side's content in its hunks. That is a staged rename. A rename on disk is
  paired only for a path added with intent to add (`git add -N`); across the boundary, only its half inside the project
  is listed, as the unstaged deletion or addition it is there, and it is read alone.
- A present side's `kind` says what is there: a regular file, a symbolic link or a submodule. A directory is no side of
  a change: a path that is a directory in a source is absent there, and a change never reads what lies below its paths.
  Only a file side's body can be read; the patch shows a link's target as text and a submodule as its commit, the way
  `git` writes them. A type change — a file that became a link, say — is one change whose two sides have different
  kinds, and its patch holds two sections: the old side's removal and the new side's addition.
- A path in conflict is not a two-sided change. It is listed as `conflicted`, with its `path` and its `conflict` —
  `both_modified`, `both_added`, `both_deleted`, `added_by_us`, `added_by_them`, `deleted_by_us` or `deleted_by_them` —
  and is read as a live file with `read_project_file`, which shows its conflict markers. Read as a change, it is refused
  with `unsupported_file_type` (`unmerged`).

The listing is one `git status` of the whole worktree, so its entries all come from one reading of the index and a
rename is found against the rest of the repository. An entry is kept when either of its sides is inside the project's
scope; the project's directory itself is not inside it. Files the repository ignores are not changes and are not listed,
as `git status` does not list them; `list_project_dir` still shows them. A submodule's own uncommitted work belongs to
that repository: only a submodule checked out at another commit than the index records is a change here. The changes of
the project's scope are listed even when its directory is not on disk in that worktree, since a deletion is read from
the index and `HEAD`, never from the disk. A disk side is what is on disk when the list is made, reached through no
symbolic link — what lies beyond a link is, to Git, not there — and its `source` carries the `version` a live read of it
would report while it stays as it is. A list over its budget is cut and carries `complete: false`, as a listing does: at
most 10,000 entries or 2 MiB of paths as sent, the entries `git status` gave first.

`read_project_change` reads the change at the paths it names as they are when it is read; a side named absent is looked
up at the other side's path, so an addition that has since become a modification reads as one. A staged change is read
against the commit at `HEAD`, resolved first and named in the reply's `head` (the empty tree before the first commit; a
`HEAD` that names no commit otherwise is `git_failed`): its old side comes from that commit and its new side from the
index. An unstaged change's old side comes from the index — a path added with intent to add holds nothing there yet, and
its old side is `absent` — and its new side from the disk. A side no longer there is `absent`, and no side is ever read
from the disk in place of the index or a commit. The patch is made by `git diff` over exactly those paths and nothing
below them, with rename detection on and its presentation fixed whatever the user's configuration says, and the reply
carries the `source` of both sides it was made from; with both sides absent there is nothing to compare, and the patch
is empty. The index entries and the file on disk are looked at again once the patch is made, and a reply that may mix
versions is refused with `source_changed`; reading it again gets a consistent copy. A reply whose sources differ from
the listing's is the newer state, not an error: a change staged again, committed or overwritten since the listing reads
as it is now, and the next listing agrees with it.

A side's `file` is its body, carried only where no patch shows the content: the in-scope side of a change that crosses
the project's boundary, an untracked file, and both file sides of a binary change, whose patch `git` writes as one line.
A text change's content is in its patch alone. A present file side also carries `lines`, the number of lines its content
holds: the newlines in it, and one more when the last line is unterminated, so an empty side is 0. It is counted of the
very blob the patch was made from, so a hunk whose `start + count - 1` equals it is one that reaches that side's end,
and since the lines past a patch's last hunk are the same on both sides, one count is enough to tell how long that run
is. It is `null` where the daemon read no content for the side: a link or a submodule, every absent or out-of-scope
side, and a side read from the disk unless its body is carried, and either side of a patch with no hunk that no body is
carried for (a change of mode alone, a pure rename), where no separator bar needs a count. A patch with no hunk whose
bodies the reply does carry — a binary change — counts both sides from them all the same. A disk side is not counted
from the disk on purpose: `git` numbers the lines of its cleaned content (a configured clean filter, such as Git LFS's),
which the file's own lines need not match. The listing carries none, since it reads no content. A reply that carries a
side's body counts the body's bytes; for a file side that is an object of the index or a commit, whose patch shows its
content, one `git grep --no-textconv --no-color -G -c -e '' <old-blob> <new-blob> --` counts both blobs at once, named
by bare object id, as `<id>:<count>` per blob (an empty blob prints no line, and when neither blob prints one `git`
exits 1, which is not a failure). It takes no pathspec, so nothing of the worktree is searched and
`GIT_LITERAL_PATHSPECS` stays set. `--no-color` is the flag that cannot be left out: with `color.grep` set to `always`
the output wraps the id and the `:` in escape sequences, which no parse of `<id>:<count>` survives and which would turn
the read into `git_failed`; `--no-textconv` is `git grep`'s default and only guards against that changing; `-G` takes
the pattern type out of the user's hands, since a configured `grep.patternType` of `perl` on a `git` built without PCRE2
fails the command (the empty pattern counts every line under any type that works); the trailing `--` ends the revisions,
so that an id is never read as a path. Only a file side is ever named, since a submodule's commit given to it would have
`git` walk its tree. Ids are enough here, unlike for a diff, because a count of stored bytes depends on no attribute.
The patch is a `FileContent`, `text` or `binary` by the same rule as a body, so a patch that is not valid UTF-8 — of a
file in another encoding, or with a header naming a path that is not UTF-8 — is `binary`. A patch over its budget is
refused with `limit_exceeded` (`patch_bytes`), never cut: an incomplete patch is not presented as a diff. A change whose
paths nest — a rename from `foo` to `foo/bar`, or back — cannot keep what lies below the outer path out of a diff
restricted to its paths, so it is read in one of two ways. When both of its sides resolved to an object that is a file
or a symbolic link, in the staged group or a comparison, its patch is the diff `git` computes between the two versions
themselves, each named by revision and path: `git diff --no-ext-diff --no-textconv --no-color --no-relative
--src-prefix=a/ --dst-prefix=b/ <old-rev>:<old-path> <new-rev>:<new-path> --`, where the old revision is the pinned
`HEAD` commit (or the comparison's left commit) and the new one is the index (`:0`, so `:0:<new-path>`) or the
comparison's right commit. For a change that keeps its kind it is one section with the ordinary `index`, `---`, `+++`
and hunk lines, so its collapsed lines can be expanded; identical content is an empty patch, an executable-bit-only
change an `old mode` / `new mode` pair, a binary side one `Binary files … differ` line (so both bodies still come with
the reply), a link renamed to another link an `index … 120000` section, and a type change (a file that became a link)
the two sections `git` always writes for one, a removal and an addition, which the client joins. Nothing below the outer
path is given to `git`, listed or counted against a budget, and the patch is returned as `git` wrote it. Revision and
path are named, never bare object ids, so the repository's attributes (a path marked as not to be diffed, say) stay in
effect; the operands are followed by `--`, so a file in the worktree named like one cannot be taken for a path; and the
diff takes no pathspec, so `GIT_LITERAL_PATHSPECS` stays set. Every other such change keeps the earlier description: a
side that is absent, a submodule side (two of them, named that way, would be resolved to their commits and have `git`
diff the whole trees, content the change never named), and a nesting pair in the unstaged group, which cannot have two
present sides, since the index cannot hold a path as both a file and a directory. Its index lookup is still matched
exactly, and its patch keeps only the sections between its own two paths, made without rename detection: a diff
restricted to the pathspecs sees another set of rename candidates than the whole-repository `git status` that listed the
change, and could pair one of its paths with a third path below the outer one, whose section would be dropped. It shows
as its old path's removal and its new path's addition. What lies below the outer path still counts against the patch
budget there, so such a change with very much below it is refused with `limit_exceeded` rather than read. The index
entries of a nesting pair are looked up once per path, so that nothing below the outer path is listed or counted against
the metadata budget in either route; the two lookups are two reads of the index, and an entry that moved between them is
caught by the comparison of the index before and after the patch that answers `source_changed`. A `change` naming no
side its group can read — both sides absent, an `out_of_scope` side outside the staged group, an untracked change with
an old side — is `invalid_change`.

#### Reading a change's bodies

`read_project_change_bodies` and `read_project_comparison_change_bodies` return the whole old and new bodies a patch was
made from, for a client expanding the lines the patch collapsed. They are sent only when the user expands: the bodies
are not part of a patch reply, which carries a body only where no patch shows the content (a binary change), and not
part of any snapshot.

The request names the change as the patch request does (`change`, and `left` and `right` for a comparison) and adds the
versions the patch reply reported for its sides, `old` and `new`: `{"state":"present","source":…}` with the side's
`source` returned as the reply gave it, or `{"state":"absent"}`. The change is resolved exactly as for its patch — the
same sources, the same rules for a side named absent or added with intent to add, the same recheck of the index and the
file on disk — and the sides found are compared with the versions named before any body is read. A side that differs,
present where absent was named or the reverse, is refused with `source_changed`, and nothing is read: the client reads
the patch again, and so never shows a patch beside bodies of another revision. A body is then read from the very blob
or file version that matched, so the two bodies and the patch come from one state.

The reply carries `old` and `new` as `SideRead` records with the body in `file`, a `FileContent` as in `project_file`,
or `absent` for the side an added or deleted file does not have. The records also carry `lines`, counted from the very
bytes the body carries. A body is read with the file budget: one over 4 MiB is refused with `limit_exceeded`
(`file_bytes`, carrying its `size` when known), never cut, and so is the whole request, since half the bodies cannot
expand anything; the reply as a whole is held to the reservation of a change's diff (see "Browse budgets"). Because the
change is resolved by making its patch first, a change whose patch is itself over the patch budget is refused with
`limit_exceeded` (`patch_bytes`) before any body is read, exactly as its patch request is. A side that is a link or a
submodule has no body and is refused with `unsupported_file_type`. A side read from the worktree whose path has a clean
filter in effect is refused with `clean_filtered`, and so is the request: `git` numbered the patch from the content that
filter produces, the file's own bytes would be lines the patch does not describe, and no `git` command writes a path's
cleaned bytes out for a body to be read from. Both sides are put through that check once the versions the request names
have been compared — so a change already known to have moved on is still `source_changed` — and before either body is
read, so a refusal reads no body at all; a file that moves after the comparison answers `clean_filtered` rather than
`source_changed`, its expansion being one that can never be delivered. A change with no patch to expand — an untracked
file, or one with a side outside the project — is `invalid_change`, refused from the request alone before any side is
read. Reading changes nothing in the repository, takes no lock and refreshes no index, as no browse request does.

#### Comparing two branches

`list_project_branches` lists the repository's local branches — the references below `refs/heads/`, whichever
worktree has one checked out, or none — each with the full object id its reference names, which is the commit at its
tip. A broken branch, whose reference names an object the repository does not have or one that is not a commit, is
listed as it is rather than looked into, since asking `git` what a missing object is would fail the whole list; a
comparison of it is refused with `unknown_branch`, as a branch read is. At most 10,000 branches, or 2 MiB of names as
sent, are listed, the first in byte order; a list cut there carries `complete: false`.

A branch the daemon can name a responsible session for carries `origin`, a `BranchOriginInfo`: `responsible_session` is
the session working on the branch now (it may be gone), `responsible_title` is the title it had when it became
responsible, and `responsible_since` is when it did, in milliseconds. The session whose own git reported creating the
branch is responsible for it until the record is moved to another; nothing else about who created it is kept. A project
session's agent is started with `core.hooksPath` pointed, through `GIT_CONFIG_COUNT`, at hook dispatchers of the
daemon's, and when a reference transaction of any git in that agent's process tree commits a branch that did not exist
while the transaction was prepared, the `reference-transaction` dispatcher reports it to `POST /git-ref/:session`
(below) — a forced update of an existing branch is not a creation; every other hook, and that one too, runs the hook the
user's own configuration names. The report is recorded only for a repository the session's own project is in, and, where
the repository keeps reflog files, only while the branch's reflog still begins with its creation at the reported commit.
The command that made the branch is never read, so any command, alias or script counts alike; a branch no session's git
reported — one made in the user's own terminal, a copy (`git branch -c`), one made while the daemon was not running —
carries no `origin`. A branch renamed after it was recorded keeps its `origin` under the new name, which git's reflog
records. Nothing is injected, and nothing attributed, where the user's `git` is older than 2.36. Listing the branches is
also what removes the record of a branch that no longer exists, and at most 1,000 branches of a repository carry one:
past that, a record is dropped by the time its branch was created, oldest first. The record is kept in the daemon's
database only; nothing is written to the repository.

In a repository whose references are kept in reftable format there are no reflog files, so a record is taken at the
reported commit and time and cannot be held against its branch's birth: a branch deleted and made again under the same
name between two listings keeps the first one's `origin` until the name itself goes, and a rename is not followed. That
is the one place a record can outlive the branch it was made for by more than a second. The same holds anywhere for a
branch deleted and made again at the same commit within the same second, whose first reflog entry is then identical. Nor
does a record outlast its reflog: `git gc` drops reflog entries older than `gc.reflogExpire` (90 days by default), and a
branch whose first entry has gone loses its `origin` for good.

`compare_project_branches` compares the right branch's tip, as the new side, with the commit the two branches forked
at, as the old side — their merge base, as `git diff left...right` takes it — so it lists what the right branch has
changed since, and whatever only the left branch has done is not in it. Swapping the two compares the other branch's
changes. Each branch is checked and resolved to its commit as a branch read is (see "Reading": `invalid_branch_name`,
`unknown_branch`), and the reply reports both — `left: { branch, commit }`, `right: { branch, commit }` — and the merge
base's full id as `base`. Where there are several best common ancestors, the one `git merge-base` prints is used.
Two branches with no commit in common have no merge base, and are refused with `no_merge_base`
(`no_merge_base_shallow` in a shallow clone) rather than compared tip to tip, which would present the whole of one branch as a change of the other. Every change in the reply is between
`base` and the right commit, in the `committed` group, its present sides' `source` naming the commit and the blob (with
no `branch`, which the endpoints carry). Two branches at the same commit have no changes, and the reply says so by its
two equal commits and a `base` equal to them; a right branch that is an ancestor of the left one forks at its own tip
and has none either.

The listing compares the two commits' whole trees, with rename detection on, so a rename into or out of the project's
scope is found against the rest of the repository; an entry is kept when either side is inside the scope, and a side
outside it is `out_of_scope` with its `repository_path`, as in a worktree's listing. When the whole trees' comparison is
past its 8 MiB budget (far-apart branches of a large repository) and the project is below the repository's root, its
own directory is compared instead: a rename across the scope's boundary then reads as the addition or the deletion it
is inside, the way a rename on disk does in a worktree's listing, and nothing outside is named. A type change is one
change whose sides have different kinds; a submodule is a change only when the commits record it at different commits.
Nothing is read from any worktree's disk or index, so the project's folder does not have to hold a path for a
comparison to list or read it, and a comparison means the same whichever worktree is chosen for the uncommitted
changes. A list over its budget is cut and carries `complete: false`, as a change list is: at most 10,000 entries or
2 MiB of paths as sent.

`read_project_comparison_change` names the comparison it belongs to by both endpoints and its `base`, as the
comparison's reply gave them, and the daemon reads from `base` and the right commit only: each `commit` and `base` must
be the full id of a commit in the repository (`invalid_commit`, `unknown_commit`) and each `branch` a branch name
(`invalid_branch_name`), but the branches are not
looked up again. A branch moved or deleted since its comparison was listed therefore never repoints the comparison; it
reads as it was until the comparison is asked for again, which resolves the branches' new tips, or is refused with
`unknown_branch` for a branch that is gone. A commit the repository no longer has — a deleted branch's, once pruned — is
`unknown_commit`, and an object missing from the repository fails the read with `git_failed`; nothing is read in its
place. Otherwise the change is read as a staged one is, its old side from `base` and its new side from the
right commit: a side named absent is looked up at the other side's path, a side outside the scope gets no body and the
change no patch, the patch is made between the two commits over exactly the change's paths (paths that nest read as
"Changes and comparisons" describes), a side's `file` is carried where no patch shows its content, a file side carries
its `lines` by the rule "Changes and comparisons" gives (a commit's blob is counted like an index's), and both sides
absent is `invalid_change`. Since a commit's content never changes, nothing is looked at again once the patch is made,
and the reply never mixes versions. The daemon does not check that the change a read names is one the comparison listed:
like a `read_project_file` of a commit, a read is held only to the project's scope and the two commits it names.

#### The commits of a branch

`list_project_commits` lists, one by one, the commits a comparison's right branch has made since the fork: those
reachable from `tip` and not from `base`, as `git log base..tip` gives them, newest first in its order. Each of `base`
and `tip` must be the full id of a commit in the repository (`invalid_commit`, `unknown_commit`); no branch is looked
up, so like a comparison's changes, a commit list never moves with a branch. A `tip` equal to `base` has no commits.
Each `CommitChanges` names the commit, its first parent (`null` for a commit with no parent, which a branch has only
where it merged in an unrelated history), its subject line — read as UTF-8 with replacement characters and cut to 200
characters — and its changes against that parent that touch the project, in the `committed` group, its present sides'
`source` naming the parent's or the commit's blob. A merge commit's changes are those against its first parent, what
it took in; a commit with no parent is compared with the empty tree. A commit that changed nothing in the project is
listed with no changes. The changes are found as a comparison's are (see "Comparing two branches" above): the whole
trees are compared with rename detection on, a side outside the scope is `out_of_scope`, and when that output is past
its 8 MiB budget, a project below the repository's root is listed within its own directory instead — and then only the
commits that touch it are listed — while a project at the root is refused (`git_output`). At most 100 commits are
listed, the newest, and `commits_complete` is false when there were more; their changes together are held to one change
list's budgets, at most 10,000 entries or 2 MiB of paths, and past that the last commit listed holds only the changes
read before the budget ran out, the older commits are left out (as is the commit the budget ran out in when none of its
changes fit, so no commit is listed as having changed nothing that did), and `complete` is false. Nothing is read from any
worktree's disk or index.

`read_project_commit_change` and `read_project_commit_change_bodies` read one change of a commit as
`read_project_comparison_change` and its bodies read one of a comparison, with `parent` (or the empty tree, for none) in
place of `base` and `commit` in place of the right commit: each of `commit` and `parent` must be the full id of a
commit in the repository, but whether `parent` is the commit's own parent is not asked, a read being held only to the
project's scope and the commits it names.

#### Git invocation

Every `git` a browse request runs gets the daemon's shared git environment ("Git environment" above), and on top of it:

- `GIT_LITERAL_PATHSPECS=1`, so a file name is never a pattern — except where a change is read: its index lookups and
  its diff match each path exactly and nothing below it with `:(literal)<path>` and `:(exclude,glob)<path>/**` (the
  path's glob characters escaped) (see "Changes and comparisons" for a change whose paths nest), and so run without it.
  The diff between two versions of a nesting pair, and the line count of a change's blobs, take no pathspec at all and
  keep the setting;
  `GIT_NO_REPLACE_OBJECTS=1`, so an object id names the object stored under it; `GIT_NO_LAZY_FETCH=1`, so a partial
  clone's missing object fails the read instead of going to the network; `GIT_OPTIONAL_LOCKS=0`, so `git status` never
  takes the index lock to write back what it refreshed; and discovery capped at the home directory as above.
- Every command runs with `--no-pager -c core.fsmonitor=false -c diff.autoRefreshIndex=false`, and every diff-family
  command with `--no-ext-diff --no-textconv --no-color`: a configured external diff, textconv filter or file-system
  monitor is a program that would run during the read and, for the first two, rewrite what is returned; and `git diff`,
  comparing the index with the disk, would otherwise take the index lock and rewrite the index for a file whose
  timestamps moved but whose content did not, which `GIT_OPTIONAL_LOCKS` does not prevent. No browse request writes to
  the repository. Clean and smudge filters stay in effect, since they define what a file's content in the repository is.
- Output is read as bytes, bounded while it is read, under a deadline, and the whole process group is killed on a
  deadline, an output budget or a cancel. A `git` whose output is still held open by something it started after it
  has exited fails rather than handing back output that may stop short.

This needs Git 2.36 or later, for `git worktree list -z`. `GIT_NO_LAZY_FETCH` came with Git 2.45; an older Git ignores
it, and can still fetch a partial clone's missing object during a read. Telling a branch not born yet from a broken one,
for a staged change's `HEAD`, takes `git show-ref --exists` (Git 2.43); before it, a broken branch is taken for one not
born yet, and its staged changes are compared with the empty tree.

#### Browse budgets

| Budget | Value | Why |
|---|---|---|
| File body | 4 MiB | Of the tracked files measured across seven real projects (about 3,000 files), 99.9 % were under 2.4 MB; the largest text file was a 2.3 MB string catalog and the largest image 2.6 MB. Only one file was larger than this budget, a 13 MB `.wasm` — not something anyone opens in a viewer |
| Patch, per change | 4 MiB | 99 % of the measured commits' whole patches were under about 1.2 MB, so one file's patch under this is the overwhelming case |
| Directory entries | 10,000 | Counted on disk across the same projects' worktrees, ignored directories included: the largest held 6,418 entries (a Gradle cache) and 4,536 (an icon package in `node_modules`); 99.99 % held under 1,600 |
| Listing names | 2 MiB, as sent | Bounds a listing whose names are few but long, counted as JSON writes them; 10,000 names of typical length are a small fraction of it |
| Search entries visited | 250,000 | Counts matching and nonmatching entries across the traversal |
| Search matches | 2,000 | Bounds a recursive result list before it is cut short |
| Search paths | 2 MiB, as sent | Bounds escaped path bytes in a recursive result |
| Search directory depth | 128 open directory levels | Bounds descriptors and traversal stack; deeper subtrees are skipped and the search marked incomplete |
| Search elapsed time | 5 s, checked between entries | Bounds traversal work while allowing the current filesystem operation to finish |
| Change entries | 10,000 | The largest measured commit changed 704 files and 99 % changed under 170; a worktree's pending changes are smaller still. A change list past it is cut |
| Change paths | 2 MiB, as sent | Bounds a change list whose entries are few but whose paths are long, counted as JSON writes them, the repository paths of sides outside the project included. A change list past it is cut |
| Branches | 10,000 | A repository usually has a handful to tens of local branches, and one kept for years with every branch left behind a few thousand. A branch list past it is cut |
| Branch names | 2 MiB, as sent | Bounds a branch list whose branches are few but whose names are long, counted as JSON writes them. A branch list past it is cut |
| Branch origins, per repository | 1,000 | Which session is working on a branch, kept in the daemon's database (see "Comparing two branches"). In a repository with more branches than this the ones a listing reaches first keep theirs and the rest carry none; a record past the cap is dropped oldest first, by the time its branch was created. A branch list reserves 768 bytes for each one it attaches, as JSON writes it |
| Origin title | 80 characters | The responsible session's title as the record snapshots it when the session becomes responsible, so the badge can name it once it is gone; a longer title is cut to this |
| `git for-each-ref` output for a branch list | 4 MiB | One branch past the budget is asked for, each a line of about 90 bytes beside its name, so this is room for names averaging some 300 bytes. Past it the branch list is refused (`git_output`) |
| Commits, per commit list | 100 | A branch under review holds a handful to a few tens of commits since it forked; each one listed is a section of a client's change list, so past this a list stops being a way to read a branch commit by commit. The newest are listed |
| Commit subject | 200 characters | A subject is the commit message's first paragraph joined into one line, which nothing bounds; a longer one is cut to this |
| `git log` output for a commit list | 8 MiB | As a comparison's `git diff-tree`, and for the same reason; past it a project below the repository's root is listed within its own directory, and a project at the root is refused (`git_output`). That second run can double a commit list's worst-case time: two runs of up to the 30 s `git` deadline each |
| `git diff-tree` output for a comparison | 8 MiB | As a change list's `git status`, it covers the whole tree, so a rename into the project is found against the rest of the repository; a record is about 100 bytes and its paths. Past it a project below the repository's root is compared within its own directory instead (see "Comparing two branches"), and a project at the root is refused (`git_output`), as is a directory's own comparison past it. That second run can double a comparison's worst-case time: two runs of up to the 30 s `git` deadline each |
| `git status` output for a change list | 8 MiB | The status covers the whole worktree, since a rename into the project is only found against the rest of the repository; a record is about 120 bytes and its path, so this holds tens of thousands of changes. Past it the change list is refused (`git_output`) rather than built from part of the output |
| `git` metadata output | 1 MiB | A `worktree list` entry is about 200 bytes; one path's index or tree entry is one line; a change's line counts are one line per object |
| `git` stderr kept | 64 KiB | A `git` message is a few hundred bytes; the rest of a longer one is read and dropped |
| `git` deadline | 30 s | Browse reads are local, so this only catches a wedged repository (a dead network volume, a held lock) |
| Reads at once, daemon-wide | 4 | A viewer, its neighbour and a couple of directory expansions in parallel; more waits its turn. A read from disk has no deadline, so a volume that stops answering can hold these |
| Outstanding requests, per connection | 16 | Waiting, being read, or queued until their reply is written. One more is refused with `limit_exceeded` at once, so a flooding client cannot grow the daemon's backlog |
| Reserved per request | about 12 MiB for a file or a project source, about 7.8 MiB for a listing, about 11.9 MiB for a branch list, about 26.6 MiB for a change list or a comparison, about 26.9 MiB for a commit list, about 24.3 MiB for a change of either, or for the bodies of one | Each request reserves its worst case before it starts. For a file that is three times the file body (a text body held as read and serialized, JSON escaping at most doubling it; a binary body held base64-encoded and serialized again) and room for the rest of the frame; for a listing, its names and the rest of its entries, each held as read and serialized; for a branch list, the `git for-each-ref` output while it is read, its branches at 128 bytes each beside their names and the origins attached to at most 1,000 of them at 768 bytes each, held as built and serialized; for a change list, the `git status` output while it is read and its entries, at 768 bytes each beside their paths, held as built and serialized, and for a comparison the same with its `git diff-tree` output, and for a commit list the same with its `git log` output and its commits at 1,536 bytes each beside their changes; for a change of either kind, two bodies as a file reserves them beside a patch of at most 64 KiB, a body coming with a patch only when the patch carries no content (a text patch alone, at most 4 MiB, needs no more than one body). The reservation shrinks to the serialized reply and is given back once the reply is written to the socket. A reply that would be larger than its reservation is refused (`reply_bytes`) |
| Bytes held for replies, per connection | about 48 MiB | Four file reads at their worst, so one window gets every read the daemon runs at a time |
| Bytes held for replies, daemon-wide | about 96 MiB | Twice one connection's share, so a client that stops reading cannot hold every other connection's reads up |
| Writing one frame | 30 s | Any frame on the control socket, not only a browse reply. On a loopback connection a client that takes no bytes for this long has stopped reading; its connection is closed, giving back what its queued replies held, and it reconnects to a fresh `snapshot` and asks again |

A request takes its reservation in the order it came, out of the connection's share and then the daemon's: a large one
waiting for room — a change list or a change at its worst, beside reads already holding theirs — holds back the smaller
ones that came after it until it starts. A window reading a change holds two of these at once where it expands one: the
patch and the bodies are separate requests, and two reservations of a change's class are more than a connection's share,
so the later one waits for the earlier to give its reservation back, and whatever was queued behind it waits too.

#### Transport

Browse replies leave on a queue of their own, and a connection's writer sends a control event (a broadcast, the reply
to any other request, any `error`) before a browse reply waiting at the same moment, so a burst of file bodies delays a
status change by at most the one frame being written. Every failure of a browse request is an `error` on the control
queue, carrying the request's `id`. A request refused for being past the outstanding bound waits for room on that
queue before the next frame of the connection is read, so a client flooding the socket without reading it is slowed
down rather than buffered for. A failure, like a reply, keeps its place among the connection's outstanding requests
until it is on the queue.

A client whose socket accepts none of a frame's bytes for 30 seconds is disconnected, whether or not it asked for
anything: a client that stopped reading would otherwise hold every frame queued for it, and the reply bytes those hold,
for as long as its socket stays open. A client that is only slow loses nothing it cannot get back — it reconnects, and the `snapshot` it
is sent then carries the current state.

A browse request may carry a `slot`, any string the client picks; slots belong to one connection. A newer browse
request in the same slot cancels the older one still outstanding — its file read stops, its `git` is killed — and the
older one is answered `request_superseded`. A client gives one slot to each thing that shows one result at a time (the
file a viewer shows, say) and none to requests that should all complete. Closing the connection cancels whatever it
had outstanding.

## `GET /ws/term/:session` — terminal stream

- Daemon to client: **binary** frames carrying raw PTY output. On attach the session's ring buffer is replayed first,
  then live output follows. Once the ring has dropped its oldest output, the replay opens with escape sequences that
  restore the terminal's current modes (alternate screen, mouse reporting, focus reporting, bracketed paste,
  application cursor and keypad, a hidden cursor, disabled line wrap), since programs announce those only once.
- Client to daemon: **binary** frames are raw PTY input, written through verbatim. **Text** frames are JSON control:
  `{"type":"resize","cols":N,"rows":N}`. Supported dimensions are 2–1,000 columns and 1–500 rows; other sizes are
  ignored and logged. Input and resizing are refused once the session's preview has been frozen.
- Attaching to a `starting` session, which has no process yet, is answered as a dormant session with nothing kept:
  the socket closes straight away. A client waits for the session to leave `starting` (its `session_upserted`)
  before attaching.
- Attaching to a dormant session sends its saved terminal preview as one binary frame, when kept, and then closes
  the socket; with nothing kept it closes straight away. A session frozen for archiving or another controlled stop
  already answers this way while its process is still exiting. These connections accept no input. The UI reloads
  the preview on entering dormancy and displays it read-only while offering resume or reopen.
- The daemon continuously parses terminal output independently of the raw live replay. Before a controlled stop
  (archiving, switching accounts or daemon shutdown), it freezes the active buffer, including up to 10,000 rows of
  scrollback when the normal buffer is active, before exposing the dormant status or sending the exit signal. The
  alternate buffer has no scrollback. Shutdown rendering cannot replace that snapshot. An agent exiting or crashing
  on its own instead leaves its final parsed screen.
- A snapshot is ordinary scrolling terminal text with ANSI cell colours and styles, without alternate-screen or input
  reporting modes. It is independent of the live ring's 2 MiB byte window and can wrap to the reader's width. Normal
  buffer content reflows when the live terminal is resized. A preview does not reconstruct the agent's full
  conversation or content already overwritten within its alternate screen.
- Each end replaces the previous preview; starting a new process removes it, and deleting the session deletes it.
  Older saved files containing raw PTY output remain replayable as-is. A session whose process ended before previews
  were kept, or with the daemon killed outright, has none, even if an earlier process left some.
- The daemon takes backpressure from each attached client: a client that stops draining for longer than the daemon's
  grace window is dropped (its socket closes) rather than allowed to make the daemon buffer without bound. A dropped
  client reconnects and gets the ring buffer replayed.

## `POST /hook/:session` — hook callback

Body is the hook's stdin payload verbatim, or empty. The event is read from the payload
(`hook_event_name` / `hookEventName`) rather than the URL, because the injected hook script takes no arguments —
Codex derives a hook's trust hash from its handler definition, so a per-event command string would need a hash per
event as well. Answers `200` with an empty body at once, before the event is applied, and never
steers the agent (an empty `200` is Claude Code's "no decision"): the injected hook script is shaped to fail silently
and fast, because every agent surfaces a failing hook to the user. The event is then applied on a queue of the
session's own, so a session's events take effect in the order they arrived; at most 256 wait per session, and past that the newest event takes the place of the one last in line (the event being applied is never touched).

The caller is `octoboardd hook`, except for a Claude Code of version 2.1.63 or later, whose `--settings` carry a
`"type": "http"` hook that posts to this route itself (`http://127.0.0.1:<port>/hook/<session>`), so a hook event costs
no process. It sends the same JSON body, and carries no credential: the route has none, and what keeps a web page from
reaching it is the `Host` and `Origin` check above. Claude Code's HTTP hook is synchronous, which is why the answer
does not wait for the event to be applied; a daemon that is unreachable, or does not answer within the hook's 3 s
timeout, shows a non-blocking error notice in Claude Code's transcript where the command form would have swallowed
it. A Claude Code whose settings carry an `allowedHttpHookUrls` that leaves this URL out would skip the hook
silently, so such a launch (the user's settings, the project's two, and the managed settings file are read) is given
the command form instead; a policy delivered by MDM profile or the claude.ai console cannot be read, and is a known
limitation.

## `POST /git-ref/:session` — a session's git reporting branches it created

Not part of the application's interface. This is how `octoboardd git-ref`, which a project session's
`reference-transaction` hook dispatcher starts in the background, reports a committed transaction's created branches:

```
{ cwd, git_dir?, refs: [{ name, oid }], unix_time }
```

`cwd` is the directory git ran the hook in and `git_dir` the `GIT_DIR` it set, if any (relative to `cwd` when
relative), both wire paths; `refs` holds each created reference's full name (a wire path) and the object id it was
created at; `unix_time` is when the report was made, in seconds. Answers `200` with an empty body, or `400` for a body
that does not parse, and records the report afterwards as "Comparing two branches" describes; the reporter ignores the
answer.

## `POST /mcp/:token` — Octoboard's MCP tools

Not part of the application's interface. This is how a session's own MCP server — the
`octoboardd mcp` child process each adapter registers — runs a tool call against the daemon; the
desktop application never calls it.

The token in the path is what says which session is calling. It is issued per launch, dropped when
that process goes away, and never sent by the caller as an argument, so a child that rewrote its
arguments still cannot act on another session. An unrecognised token is refused without detail.

Request body is `{"tool": "<name>", "arguments": {…}}`. The answer is always `200` with
`{"ok": true, "result": {…}}` or `{"ok": false, "error": "<prose>"}`: a refused call is an outcome
the model is meant to read and act on, not a transport failure. Which tools a session may call
follows from its role and whether it has the role of a bound session as its binding stands now (a lead session, bound
to a console session while keeping sessions of its own, keeps the tools of an unbound project session), and is checked
here as well as in the child. A binding changes while a session runs, so every answer also carries `bound`, whether the
session has the role of a bound one after the call: the `octoboardd mcp` child, started with `--bound`, switches the
catalogue it announces to match and sends `notifications/tools/list_changed` when it changes.

A `request_session` call waits for the user's answer, up to 8 minutes, below the 10-minute timeout the
`octoboardd mcp` child gives every call it forwards. A third limit is the agent's own on a tool call, and the three are
kept in this order, 8 minutes < 10 minutes < the agent's as Octoboard sets it:

- the answer limit is the shortest so that a call ends with an outcome the daemon chose, with room left under the
  forwarding limit for an approval given at the last moment to be carried out;
- the forwarding limit is what ends any call the daemon does not answer, and is generous because a `git clone` can take
  minutes;
- an agent that has a limit of its own has it **set** to the forwarding limit plus a minute when the session launches,
  so that the agent's limit is a known value above the forwarding one rather than a default that may fall either side
  of it (Codex's is measured and shorter; Grok Build's is documented as longer, and measurement only bounds it from
  below). Setting it regardless of where that default falls is worth it because giving up costs more for Grok Build —
  it cancels the call, which withdraws the request, while Codex only loses the call's result (see each agent's section
  in `docs/agent-cli-reference.md`). Claude Code has no such limit to set: it moves a long call to the background
  instead of ending it. The margin covers a call in normal operation: the forwarding limit bounds each read on the
  connection rather than the whole call, so a daemon that stalls partway through a response could still carry a call
  past it.

When the agent cancels that call (`notifications/cancelled`), the
child closes that call's connection to the daemon, which withdraws the request, and the agent gets a tool error saying
the call was cancelled. A cancelled call of any other tool is left to finish in the daemon. The waiting, the endings and
the one waiting request a session may have, are described in `docs/product/session-requests.md`
("Waiting for the answer", "How a request ends").

### The tools of each role

| Tool | Console session | Unbound or lead project session | Bound project session |
|---|---|---|---|
| `list_projects`, `add_project`, `update_project`, `get_console`, `update_console`, `get_settings`, `update_settings`, `list_archived`, `show_page` | yes | — | — |
| `start_session`, `send_message`, `get_session`, `archive_session`, `reopen_session`, `rename_session`, `set_session_pinned` | yes | yes | — |
| `list_sessions`, `share_info` | yes | yes | yes |
| `archive_self` | yes | yes (refused once bound, a lead session included, which wraps up with `report`) | — |
| `transfer_session` | yes | yes | — |
| `bind_to_session` | — | yes (refused once bound, a lead session included) | — |
| `request_session` | yes | yes | yes |
| `report`, `get_project` | — | yes (`report` is refused until bound) | yes |

Every tool that changes a record runs the function the control request of the same name runs, so its
refusals (blank names, a model the agent does not offer, an account of another agent) and the events it
broadcasts are the same, and a write tool answers with the record as stored. The shapes shared by several tools:

- `accounts` is an object keyed by agent (`claude`, `codex`, `grok`); an agent left out is left alone. For a project
  each value is `inherit`, `default` or an account's id or name; for a console `default` (or `null`) or an id or name.
  A name is read among that agent's accounts only, an id of another agent's account is `unknown_account`, and results
  carry ids. The default account of an agent is the id `default` in `get_settings`.
- `launch` is an object keyed by agent of `{"model": …, "effort": …}`; a field left out is left alone, `null` or blank
  is the agent's own default.

`archive_self` takes no arguments and archives the caller, which only a session that reports to nobody does. It is
refused, with nothing changed, while the caller, or any session under it (bound to it, or bound to a lead session
bound to it), is locked, and while any session under it is not archived, running or interrupted alike. The refusal
names those sessions: an owner archives its sessions once their tasks are done, so an unarchived one means its task is
not finished. For one bound to a lead session, which the caller cannot archive itself, it advises waiting for the lead
session to finish it or archiving the lead session. The answer is sent before the process is ended.

Scope: a console session reads and changes only its own console and its projects; `get_console` never returns the avatar
(`has_icon` says whether there is one) and `update_console` cannot set it, nor read or set the console's
`console_session_prompt`. A project session's `get_project` takes no `project`, and returns its own project only.
`rename_session` and `set_session_pinned` follow the rules of `send_message`: only a session bound to the caller, never
the caller's own and never a console session. Nothing here deletes a project or console, creates or deletes another
console, changes a project's path, source, host, remote or `trust_consent`, or writes an account or a trusted directory.

- `list_projects` answers each project with `project`, `name`, `host`, `path`, `source`, `remote_url`, `default_agent`,
  `pinned`, `tags`, `trust_consent`, `accounts`, `launch` and `sessions`. `get_project`, `update_project` and
  `add_project` answer with the same entries without `sessions`. A `remote_url` is always given without the
  credentials it carries, as `https://example.com/r.git` for `https://user:token@example.com/r.git`.
- `add_project` takes `tags`, `accounts`, `launch` and `pinned` besides its location. A `git` `remote_url` must be an
  http(s), ssh or git URL (the shapes a configuration import clones from) without credentials in it, else
  `invalid_remote_url` and nothing is cloned.
- `update_project` takes `project` (id, or a name that names one project) and any of `name`, `tags` (replaces the
  list), `add_tags` and `remove_tags` (matched ignoring case; `tags` beside either is `conflicting_fields`), `pinned`,
  `default_agent` (`null` follows the console's `project_session_agent`, else its `default_agent`), `accounts` and
  `launch`.
- `get_console` answers `name`, `default_agent` (console sessions), `project_session_agent` (`null` follows
  `default_agent`), `accounts`, `launch` (the console session model and effort of each agent),
  `project_session_launch` (the pair a project session falls back to), `has_icon` and `workdir`. `update_console`
  takes `name`, `default_agent`, `project_session_agent` (`null` follows `default_agent` again), `accounts`, `launch`
  and `project_session_launch`, and a changed account reaches only console sessions opened afterwards, while a
  changed model or effort reaches the calling session on its next launch unless it was opened with its own. A changed
  `project_session_launch` reaches project sessions at their next launch, unless their project or the session sets
  its own.
- `get_settings` answers `auto_pull_repositories`, `default_clone_dir`, `accounts` (`id`, `agent`, `name`, `default`;
  never a config directory) and `trusted_directories`. `update_settings` takes `auto_pull_repositories` and
  `default_clone_dir` (blank resets it) through the function `update_settings` runs, so enabling the automatic pull
  fast-forwards the projects already behind at once.
- `start_session` takes `account` (id or name, `default`), `model` and `effort` besides the brief; the account is read
  among the accounts of the agent the session will be of.
- `request_session` takes `kind` (`project` or `console`), `project`, `console`, `brief` and an optional `branches` —
  and the overrides of `start_session` — and waits for the user's answer. `branches` is a list of local branch names in
  the project's repository to hand to the new session; each must be one the caller is responsible for (see
  `BranchOriginInfo`), and a call that names one it is not responsible for, names any for a console session, or names
  any for a project that is not in a git repository, is refused before the user is asked. On approval, once the session
  has started, their responsible session becomes the new session, a branch whose responsible session changed meanwhile
  being reported in the outcome instead, and a move that cannot be recorded being reported there too, with the caller
  still responsible for the branches; nothing moves on any other ending. A project session is in the named project,
  which defaults to the caller's own, and may belong to any console; a console session is in the named console,
  defaulting to the caller's own. The new session has no owner and the caller carries on unchanged. See
  `docs/product/session-requests.md`.
- `transfer_session` takes `session` (one of the caller's), `to` (the new owner) and `note`, and needs no approval:
  `Session.bound_to` changes, the session and the new owner are told in messages, and the note reaches the new owner.
  `bind_to_session` takes `owner` and `summary`, which reaches the owner as the caller's first report. Both refuse, in
  prose and changing nothing, an owner that breaks the rules a binding keeps; see "Changing a session's owner" in
  `docs/product/hub-orchestration.md`.
- `list_sessions` answers every running session but the caller's, in every console and project, console sessions
  included, with `owner`, `owner_kind`, `yours` and `your_owner`; `get_session` and `share_info` reach any session,
  whichever console or project it is in, while the tools that act on a session (`send_message`, `rename_session` and so
  on) stay with the session's owner. See `docs/product/information-between-sessions.md`.
