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
| `update_console` | `console`, `name?`, `default_agent?`, `project_session_agent?`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `claude_console_session_model?`, `claude_console_session_effort?`, `codex_console_session_model?`, `codex_console_session_effort?`, `grok_console_session_model?`, `grok_console_session_effort?`, `claude_project_session_model?`, `claude_project_session_effort?`, `codex_project_session_model?`, `codex_project_session_effort?`, `grok_project_session_model?`, `grok_project_session_effort?`, `icon?`, `console_session_prompt?` | `project_session_agent`: absent leaves it alone; an explicit `null` clears it, so project sessions follow `default_agent` again; an agent replaces it. For each account field independently: absent leaves the console's reference alone; an explicit `null` clears it, back to the agent's default account; an id is validated as in `create_console` and replaces the reference. A reference left alone is not checked again. `icon` follows the same absent / `null` / blank rule, clearing back to the default glyph. `console_session_prompt` follows the same absent / `null` / blank rule, clearing back to none; the console's instruction file is rewritten at once. A console session begun afterwards reads it on every agent; a resumed Claude Code or Codex session reads it too, while a Grok session never does within its life (`docs/agent-cli-reference.md`, "Injecting Octoboard into each agent"). Only sessions opened afterwards take a changed reference (see `Session.account_id`). Each model and effort field follows the same absent / `null` / blank rule, clearing back to the agent's own default; a console session that is opened or resumed afterwards inherits the console's current values unless it has its own override, and one already running keeps what it started with. Project sessions never read them (they read the `*_project_session_*` pair). Existing sessions keep their recorded agent. A model or effort that is being set, and is not blank, must be one the agent offers under the account its sessions would run with, by the list `list_agent_models` answers with, or the request is refused with `invalid_launch_setting`; a value left as it is is not checked again, and an agent whose list could not be read (`unavailable`, or no model in it) takes any value. `name` is trimmed and cannot be blank (`field_required`). The `*_project_session_model` / `*_project_session_effort` fields follow the same absent / `null` / blank rule and the same check, and are read at every launch by the project sessions whose project and override leave the value unset. |
| `delete_console` | `console` | Takes its projects and all their session records with it, and stops each project's processes (see "Process trees"). Refused while any of them is still running |
| `add_project` | `console_id`, `source` (`local`\|`parent`\|`git`), `path?`, `remote_url?`, `name?`, `default_agent?`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `claude_session_model?`, `claude_session_effort?`, `codex_session_model?`, `codex_session_effort?`, `grok_session_model?`, `grok_session_effort?`, `detect_default_agent?`, `tags?`, `description?`, `git_workflow?` | `local` associates one directory; `parent` associates every git repository directly beneath `path`; `git` clones `remote_url`, any git remote, into `path` (used as the parent directory) and associates the clone; every added project records its repository's remote URL (`remote_url`, see the "Records" notes below): a clone keeps the URL it was given, and a `local` or `parent` project takes the one read from its directory, or none when the directory is not a repository or has no remote, which is not an error; `path` is required for `local` and `parent` only, and a `git` request without one clones into `default_clone_dir` of the settings. A `path` must be absolute or start with `~/`, and is stored lexically normalised (`.` and `..` folded, no trailing slash); a relative one is refused, because the trusted directories compare project paths and a relative path means nothing to them. A directory the console already has a project at is not associated again, whichever way it is written: a path through a symbolic link, or in another case on a case-insensitive file system, names the same directory (compared by device and inode, the stored path staying as it was given), except while the console's existing projects cannot be looked at within the deadline, when only the normalised text is compared; a path that is the same text once normalised is the same directory even where nothing can be read at it. An absent or blank `description` means none; it is stored trimmed, and ignored for a `parent` directory that adds more than one project, as `name` is. Absent `default_agent` lets the daemon store one detected from each directory's marker files, or none (rules in `docs/product/projects.md`, "The project's default agent"); an absent `detect_default_agent` is true, and false stores none, for a client that has already shown the user the detection (`detect_directory_agent`, `probe_git_remote`) and sends what it showed. Absent `tags` means none, and each added project takes the same list, normalised as in the "Records" notes below. Each account field is a `ProjectAccount` (see the "Records" notes below) saying which account the project's sessions of that agent should read; absent or `null` means inheriting the console's reference for that agent, and `{"kind":"default"}` the agent's default account. An `account` kind that names no account, or an account of another agent, is answered with `unknown_account`, and nothing is cloned or stored. The model and effort fields are the model and reasoning effort the project's sessions are launched with, one pair per agent in that agent's own spelling; they are trimmed, and absent or blank means the console's project session value for that agent, else the agent's own default. Each added project takes the same values. A given model or effort is checked as in `update_project`, except for a project of `import_configuration`, whose document's values are stored as they are. `git_workflow` is an object of the project's own git workflow settings (`development`: `in_place`, `branch` or `worktree`; `rebase_before_merge`, `delivery`, `revert_gate`); the object, or any item in it, absent or `null` follows the application-wide value, and a `development` that is none of the three or a `delivery` that is none of `ask`, `merge` or `pull_request` is refused as a malformed request, before anything is cloned or stored. `rebase_before_merge` and `delivery` apply only while the project's resolved `development` is not `in_place`, and their values are kept while it is; `revert_gate` applies whatever `development` is. Each added project takes the same values. A blank `name` is read as absent: a single project takes its directory's name, and so does each of several found under a parent directory; an explicit `name` applies when a parent directory finds exactly one repository. An explicit name is limited to 100 Unicode scalar values, and a description to 2,000, counted after trimming; an overlong field is refused with `field_too_long` before anything is cloned or stored. Ignored text for several parent-directory projects is not checked, and directory-derived names and imported configuration values retain their full text. |
| `update_project` | `project`, `name?`, `default_agent?`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `claude_session_model?`, `claude_session_effort?`, `codex_session_model?`, `codex_session_effort?`, `grok_session_model?`, `grok_session_effort?`, `pinned?`, `tags?`, `description?`, `git_workflow?` | An absent `default_agent` leaves it alone; an explicit `null` clears it, so the project follows the console's project session agent again. For each account field independently: absent leaves the project's reference alone; an explicit `null` (or `{"kind":"inherit"}`) puts it back to inheriting the console's reference for that agent; `{"kind":"default"}` makes it the agent's default account; an `account` kind is validated as in `add_project` and replaces the reference, and a reference left alone is not checked again. Each model and effort field follows the absent / `null` / blank rule, clearing back to the console's project session value for that agent, else the agent's own default. Only sessions opened afterwards take a changed reference (see `Session.account_id`); a session that is opened or resumed afterwards inherits the project's current model and effort unless it has its own override, and one already running keeps what it started with. An absent `pinned` leaves the pin alone. An absent `tags` leaves the tags alone; a present array, even an empty one, replaces them wholesale; an explicit `null` is the same as absent. An absent `description` leaves it alone; an explicit `null` or a blank string clears it, and any other text is stored trimmed. A model or effort that is being set, and is not blank, must be one the agent offers under the account its sessions would run with, by the list `list_agent_models` answers with, or the request is refused with `invalid_launch_setting`; a value left as it is is not checked again, and an agent whose list could not be read (`unavailable`, or no model in it) takes any value. `name` is trimmed and cannot be blank (`field_required`). A changed name is limited to 100 Unicode scalar values, and a changed description to 2,000, after trimming (`field_too_long`); no text is truncated. A legacy overlong value left alone or resubmitted unchanged is accepted, so unrelated settings remain editable. `git_workflow` is an object of the project's own git workflow choices (`development`, `rebase_before_merge`, `delivery`, `revert_gate`), and per item: absent leaves it alone, an explicit `null` goes back to following the application-wide value in `Settings.git_workflow`, and a value sets the project's own; an unknown `development` or `delivery` is refused when the request is parsed. `rebase_before_merge` and `delivery` apply only while the project's resolved `development` is not `in_place`, and their values are kept while it is; `revert_gate` applies whatever `development` is. |
| `delete_project` | `project`, `stop_sessions?`, `trash_directory?`, `force_remove?`, `inspection?` | Removes the association and the project's session records, archived ones included; touches the directory only when `trash_directory` (absent means false) is true: the directory is then moved to the system Trash (recoverable, never deleted outright) after any running sessions are stopped and before the project is removed, so when the move fails the request fails with `project_directory_trash_failed` and the project is kept for a retry. A session started in the project while the directory moves is archived before the project is removed, even when `stop_sessions` is false. A directory that no longer exists is skipped and the project is removed as usual. The move uses `NSFileManager` on macOS (no Finder permission needed), so Finder's "Put Back" may be unavailable; the folder is recoverable from the Trash. Refused before anything is stopped, with the project kept, with `project_directory_protected` for the home directory, the root of a disk (`/` or a mount point), or a directory holding Octoboard's own data, and with `project_directory_shared` while another project's directory is the same as, inside, or holds this one, or another project's Git repository (a linked worktree's, say) lies inside it. Directories are told apart by device and inode, never by their paths' text, so a symbolic link, a case-only respelling, a bind mount or a macOS firmlink naming one directory makes no difference. Another project's path, the home directory's or Octoboard's data directory's, with nothing at it or that cannot be read for want of permission, is compared by its text instead, as written and, where it resolves, as resolved. Any other path that cannot be resolved — this project's own folder when it cannot be read, an I/O error, a file system that reports no inode, a look that gives no answer within a few seconds — counts as overlapping: another project's refuses with `project_directory_shared` naming that project, and the project's own, the home directory's or Octoboard's data directory's with `project_directory_trash_failed`; the root of a disk is refused with `project_directory_protected` before either. With `trash_directory` and `inspection`, the token an `inspect_project_removal` refusal carried, the request is refused with that inspection's error, even once the path that decided it resolves, so a removal confirmed after an inspection that listed nothing takes nothing along; the token is then used up. With the token a `project_removal_inspected` carried, the worktrees that inspection held back are left even once whether they are or hold another project's folder can be told, and reported as left; that token is kept until the project is removed, so a request that fails before the removal stands, and is sent again with it, holds them back too. A request without `inspection`, or with a token no longer kept (used, its project removed, pushed out by newer ones, the daemon restarted), looks for itself. Another project at something other than a directory (a file, or a link to one) is in the way only when the directory holding its path, or the directory holding what a link there resolves to, is or lies in this one; a project whose own path is a file or a link that leads nowhere is moved as itself and refused, with `project_directory_shared`, only when another project's directory holds it, another project is at that same entry or at a link to it, or another project's path could not be looked at. Refused with `project_has_running_sessions` while the project has live sessions, unless `stop_sessions` (absent means false) is true: the running ones are then ended first, exactly as `archive_session` ends one (the deepest level of binding first — the sessions bound to a project coordinator, then that project coordinator, then the independent ones — so a parent is never refused over them; a console session belongs to no project and is left running, and a project coordinator's binding to one goes with the project coordinator's record), and the project goes with them. A session being launched or resumed at that moment is not stoppable, so it refuses either way. The project's processes (the trees no session owns: kept under it, and left behind in its folder; see "Process trees") are stopped once its sessions are, before the folder moves, whether or not `trash_directory` is set. Without `trash_directory` no branch or worktree is touched. With it, what `inspect_project_removal` describes goes too, after the project is removed and once its sessions' processes have ended: the outside worktrees are moved to the Trash whatever they hold, except one that is or holds another project's folder, or of which that cannot be told (reported as left, `failed`), each looked at again just before they are moved, so one that has come to be so since the request stays too; a `locked` one only when `force_remove` (absent means none) names it as `locked` and without uncommitted changes it did not show, and unlocked once it is in the Trash where the repository outlives the folder — and where the repository outlives the folder its worktree records are pruned and the branches no worktree still has checked out are deleted with `git branch -D`; one still checked out in a worktree that was to go and did not, other than a locked one that was kept, is reported. The request itself reads no worktree's status and makes no merge check. A failure there never fails the request; it is reported as `session_branches_not_removed`, or as `project_branches_not_determined` when what goes along could not be found out at all |
| `inspect_project_removal` | `project` | Replies with `project_removal_inspected`: in `processes`, the project's processes, which `delete_project` stops with or without `trash_directory` (`ProcessTree`s with no `session_id`, see "Process trees"; `list_processes` with the `project_id` lists the same trees, which a client can ask for when the inspection is refused); and what `delete_project` with `trash_directory` would also remove now, as `SessionBranchItem`s (see "Branches and worktrees bound to a session"). When the folder holds the repository (it is the main worktree), that is every linked worktree of the repository outside the folder, bound to a session or not (`session` null, and `branch` null for one with a detached `HEAD`), since none works once the repository is gone. When the repository outlives the folder (the folder is part of a bigger repository, or a linked worktree), it is the linked worktrees bound to the project's sessions outside the folder and the bound branches, but not a branch checked out in a worktree that stays. A worktree that is or holds another project's folder never counts, nor does the main worktree, and one of which that cannot be told (as in "Branches and worktrees bound to a session") is held back: listed among the items as `failed` with `cause` `project_folder_unknown`, as one that stays. The answer then carries `inspection`, a token the worktrees it held back are kept under, for the `delete_project` confirmed after it to send back, so that it leaves them even once they can be told; an answer that held none back carries none. `state` says whether a worktree has uncommitted changes and whether a branch is merged, both of which go either way; a `locked` worktree, whose `uncommitted` says whether it has changes, goes only when `delete_project`'s `force_remove` names it, and a branch it has checked out only with it. Refused as `delete_project` with `trash_directory` would be (`project_directory_protected`, `project_directory_shared`, or `project_directory_trash_failed` when a path it depends on cannot be resolved), each of which a client takes as nothing to list rather than as a failure, and `unknown_project` for an unknown project. A refusal because a path could not be resolved (`project_directory_trash_failed`, or `project_directory_shared` naming a project whose path could not be) is kept under a token of its own, which the error carries as its `inspection` param, for the `delete_project` confirmed after it to send back (see there); any other refusal carries none. The daemon keeps the four newest tokens of either kind per project at most, drops them with the project or its console, and keeps none for an inspection that ends after its project is gone |
| `list_dir` | `path` | Answered with `dir_listing` on the asking socket |
| `detect_directory_agent` | `path` | Answered with `agent_detected`: the agent the directory at `path` is set up for (marker rules as for `add_project`), or null for none, several, or one known not to be installed. A `path` that is not absolute, does not exist or is not a directory is refused with `path_not_absolute`, `path_not_found` or `path_not_a_directory`, as for `add_project` |
| `probe_git_remote` | `remote_url` | Answered with `agent_detected` for the remote's top-level names, or refused with `git_remote_unreachable`. Does a shallow, blobless, checkout-less `git clone` into a scratch directory and lists its top level, so it proves the remote can be read with the user's credentials. Runs in the same non-interactive environment as a clone; the fetch is bounded by a 60 second deadline and the scratch directory is removed when the probe ends. An empty repository is reachable and answers null |
| `list_agent_models` | `agent`, `account?` | Answered with `agent_models`: the models `agent` currently offers, read from the catalogue of the account `account` names (an account id; absent is the default account). An `account` that does not exist or belongs to another agent is refused with `unknown_account`. Nothing else is a refusal: a missing binary, no network, a non-zero exit or output past 8 MiB, output that cannot be read or a command that runs past 15 seconds are an empty list with a reason (see "Agent models" below). Never touches a session or the stored accounts |
| `locate_conversation_to_resume` | `project`, `agent_session_id` | Answered with `conversation_located`: where a conversation is held, for the new-session dialog of `project`'s Resume tab. A session that records the id comes first, whatever its agent: an archived session of `project`, another session of `project` (not archived), or a session outside `project` (another project's, or a console session, in any state). Otherwise the agents' own records are searched in the default account and every stored account of each agent that is available and added, in the directory a launch of that account would use (the default account's from the shell snapshot, as `open_session` takes it); when several hold it, the one whose record was written to last wins. A shell snapshot that fails skips the default accounts, and is returned as the error only when nothing was found anywhere. An `agent_session_id` that is not a UUID is refused with `resume_id_invalid`, a `project` that does not exist with `unknown_project`. Reads the disk only |
| `start_account_login` | `agent`, `account?` (an account id; absent or null is the agent's default account) | Signs the account in with its agent's own CLI, run in the background (no terminal, no window) against the account's config directory; see "Account login" below. Answered with `ack` once the CLI is running; progress and outcome follow as `account_login_updated`. Refused: `unknown_account`, `agent_not_available`, `shell_environment_timeout`, `binary_not_found`, `login_already_running` (this account has a login running), `login_codex_busy` (a Codex login runs for any account) |
| `cancel_account_login` | `agent`, `account?` | Ends the account's running login and kills the CLI. Answered with `ack`; `account_login_updated` with `cancelled` follows. Refused: `login_not_running`; it looks only at the logins running, so a login whose account was just deleted can still be cancelled |
| `refresh_account_quotas` | `agent?`, `account?` | Answered with `ack` at once. Re-reads the plan quota of the accounts it selects whose last attempt is older than the polling interval (five minutes, longer for an account that keeps failing), and also those whose last attempt is over 30 seconds old and which hold no windows because the account is not signed in or has no quota to report (a sign-in done outside Octoboard shows up at once): `agent` limits it to one agent's accounts, `account` (an account id) to that one account, and with neither it covers every account of every available agent. An `account` that does not exist or belongs to another `agent` is refused with `unknown_account`. Each reading that finishes follows as `account_quotas_updated`; any other account that is not due, or whose reading is already running, is left alone. See "Account quotas" below |
| `export_configuration` | `console?`, `consoles?` (`one`\|`all`\|`none`), `include_settings?`, `include_accounts?`, `include_paths?` | Answered with `configuration_exported`: the configuration document (see "Configuration export" below) holding the regions chosen. `consoles` says which consoles and projects: `one` (the console `console` names), `all` or `none`; absent it is `one` when `console` is given and `all` otherwise, and `console` is read for `one` alone. `include_settings` says whether the application settings are in it; absent it is true exactly when every console is exported. `include_accounts` says whether the account structure is, never a credential; absent it is false. Each region is independent of the others. A request that would export nothing (`consoles` `none`, no settings, no accounts) is refused with `export_empty`, `consoles` `one` without a `console` with `field_required`, and a `console` that does not exist with `unknown_console` rather than answered with an empty document. `include_paths` is a bool, true when absent; false exports each project's `path` as its last segment alone and drops a `remote_url` that cannot be cloned from (see "Configuration export"); it never touches the accounts, whose directories are always written. A read of what is stored: it runs no `git` and touches no session |
| `inspect_configuration` | `document` | Answered with `configuration_inspected`: for each project of the configuration document `document` (see "Configuration import" below) the directories it could be placed in on this machine, for each account reference that matches no account here (an account the document's accounts would create is listed too; the client tells them apart from `accounts`), and, when the document carries accounts, what importing each of them would do. Changes nothing and runs no `git`. Its existence checks of paths together have a 15 second deadline; a project whose check did not come back in that time is offered nothing. A document that is not a configuration document, is of a version this daemon does not read, or cannot be read is refused with `configuration_not_recognised`, `configuration_version_unsupported` or `configuration_unreadable` |
| `import_configuration` | `document`, `resolutions?`, `apply_settings?`, `apply_accounts?`, `account_resolutions?`, `apply_consoles?` | Answered with `configuration_imported`: creates a new console for each console of `document` and associates each project with the directory its entry in `resolutions` chooses (see "Configuration import" below). A project with no entry is left out. The document is refused as `inspect_configuration` refuses it, and a resolution that names a console or project the document does not have, names one twice, or chooses a `directory` that is neither an absolute path nor starts with `~/`, is refused with `import_resolution_invalid`; all of that before anything is created, as is anything `create_console` would refuse of a console of the document (an avatar that is not an image, say). `apply_settings` (absent means false) also applies the document's application settings; asking for them of a document that carries none is refused with `import_settings_missing`, before anything is created. `apply_accounts` (absent means false) also creates the document's accounts, before the consoles, so their references bind to them; asking for them of a document that carries none is refused with `import_accounts_missing`, before anything is created. `account_resolutions` (read only with `apply_accounts`) are `{ agent, account, directory? \| leave_out? }` entries, `account` being the index among the document's accounts of that `agent`: it gives the account another `directory` (absolute or starting with `~/`) or leaves it out (`leave_out: true`). One that names an account the document lacks, names one twice, gives both or neither of the two choices, or gives a malformed `directory` is refused with `import_resolution_invalid` (`agent` and `account` in its params), before anything is created. `apply_consoles` (absent means true, so a client that never sends it keeps importing the consoles) says whether the document's consoles and projects are created at all; false creates no console or project, and `resolutions` are not read (one naming a project the document lacks is no refusal), so only the accounts and the settings asked for are applied and `consoles` in the reply is empty. A project or account that cannot be created after that is not an error of the request (see "Configuration import") |
| `get_project_source` | `project`, `slot?` | Where the project's files are read from: its resolved directory and, when it is inside a Git repository, the repository, the worktree holding it, its place in that worktree and every worktree of the repository, that one included. Answered with `project_source`. A browse request: see "Browsing a project" below for the rules every browse request shares (`slot`, budgets, its own outbound queue) |
| `list_project_dir` | `project`, `path`, `worktree?`, `slot?` | Lists one directory of the project's files on disk, `path` being a wire path relative to the project's directory (empty for the directory itself). With `worktree`, the same place in that worktree of the project's repository is listed instead. Answered with `project_dir`. A browse request |
| `search_project_files` | `project`, `query`, `include_ignored?`, `slot?` | Searches file names recursively beneath the project's directory, including unopened subdirectories. A trimmed, case-insensitive basename substring, not a path expression. Files Git ignores, and ignored folders with all beneath them, are left out unless `include_ignored` (absent means false) is true — see "Searching file names". Answered with `project_file_matches`. A browse request |
| `read_project_file` | `project`, `path`, `worktree?`, `from?`, `slot?` | Reads one file of the project, `path` being a wire path relative to the project's directory. `from` picks the content: `{"kind":"live"}` (the default) the file on disk, `{"kind":"index"}` the blob staged for it, `{"kind":"branch","branch":…}` the blob at a local branch's tip, `{"kind":"commit","commit":…}` the blob in a commit given by its full id. With `worktree`, the disk and the index read are that worktree's. Answered with `project_file`. A browse request |
| `list_project_changes` | `project`, `worktree?`, `slot?` | Lists the uncommitted changes of the worktree holding the project's directory — or, with `worktree`, of that worktree of its repository — that touch the project: staged, unstaged, untracked and in conflict. Answered with `project_changes`. A browse request |
| `read_project_change` | `project`, `worktree?`, `change`, `slot?` | Reads one of those changes for its diff: `change` is `{group, old, new}`, `group` being `staged`, `unstaged` or `untracked` and each side `{"state":"present","path":…}`, `{"state":"absent"}` or `{"state":"out_of_scope"}` as the listing gave it. Answered with `project_change`. A browse request |
| `list_project_branches` | `project`, `slot?` | Lists the local branches of the project's repository, each with the object id its reference names: the commit at its tip, and the remote-tracking branch to compare it with when the repository has one. Answered with `project_branches`. A browse request |
| `compare_project_branches` | `project`, `left`, `left_remote?`, `left_empty?`, `right`, `slot?` | Resolves local branches `left` (the base) and `right`, each a branch name as a wire path, to the commits at their tips (`left` is a remote-tracking branch, named below `refs/remotes/` such as `origin/main`, when `left_remote` is true; with `left_empty`, `left` is ignored and the comparison is against the empty tree), finds the commit they forked at (their merge base), and lists the changes from that commit to `right`'s tip that touch the project, as `git diff left...right` does. Two branches with no commit in common are refused with `no_merge_base` (`no_merge_base_shallow` in a shallow clone). Answered with `project_comparison`. A browse request |
| `read_project_comparison_change` | `project`, `left`, `right`, `base`, `change`, `slot?` | Reads one change of a comparison for its diff: `left` and `right` are the comparison's `{branch, commit}` and `base` its merge base commit, as its reply gave them, and `change` is `{old, new}`, each side as in `read_project_change`. Answered with `project_comparison_change`. A browse request |
| `read_project_change_bodies` | `project`, `worktree?`, `change`, `old`, `new`, `slot?` | Reads the whole bodies of the two sides of a change `read_project_change` made a patch of, so a client can show the lines the patch left out. `change` is as in `read_project_change`; `old` and `new` are the versions that reply reported for its sides, `{"state":"present","source":…}` with the side's `source` as the reply gave it, or `{"state":"absent"}`. Answered with `project_change_bodies`. A browse request |
| `read_project_comparison_change_bodies` | `project`, `left`, `right`, `base`, `change`, `old`, `new`, `slot?` | The same for a change of a comparison: `left`, `right`, `base` and `change` are as in `read_project_comparison_change`, `old` and `new` as above. Answered with `project_comparison_change_bodies`. A browse request |
| `list_project_commits` | `project`, `base`, `tip`, `slot?` | Lists the commits reachable from `tip` and not from `base`, both full commit ids — a comparison's right commit and its merge base, or, for a comparison from the empty tree, the empty tree's id, which lists every commit reachable from `tip` — newest first, each with its changes against its first parent that touch the project. Answered with `project_commits`. A browse request |
| `read_project_commit_change` | `project`, `commit`, `parent?`, `change`, `slot?` | Reads one change of a commit for its diff: `commit` and `parent` are as the commit list gave them (`parent` null or left out for a commit with no parent), and `change` is `{old, new}`, each side as in `read_project_change`. Answered with `project_commit_change`. A browse request |
| `read_project_commit_change_bodies` | `project`, `commit`, `parent?`, `change`, `old`, `new`, `slot?` | The same as `read_project_change_bodies` for a change of a commit: `commit`, `parent` and `change` are as in `read_project_commit_change`. Answered with `project_commit_change_bodies`. A browse request |
| `open_session` | `console_id`, `project_id?`, `agent?`, `account?`, `model?`, `effort?`, `task?`, `title?`, `bound_to?`, `resume_agent_session_id?`, `terminal_appearance?` | Omit `project_id` for a console session; a console may hold any number of them at once. `agent` chooses the session's agent; absent takes the project's default, else the console's `project_session_agent`, else its `default_agent`. A console session has no project, so absent takes the console's `default_agent`. The choice is recorded on the session and retained on resume and account switch. `account` is the account the session's agent reads, by id: absent leaves it to the project's reference for the session's agent, else the console's, else the agent's default account; an explicit `null` chooses the default account outright, whatever the project or console refers to. An id that names no account of the session's agent is refused with `unknown_account`. `model` and `effort` independently override the session's launch settings: absent inherits the project's current value, else the console's project session value (the console's console session value for a console session), an explicit `null` or blank string leaves the choice to the agent, and a string pins that trimmed value. Overrides are recorded on the session and reused on resume and account switch. For a console session, `title` supplies a custom title; absent or `null` uses the console's current name. `bound_to` names the session this (project) session should report to; absent means none. Ignored for a console session, which is always independent. A `bound_to` that names neither a console session of `console_id` nor a project coordinator of `project_id` (see `Session.bound_to` below) is refused with `unknown_session`; a project worker is refused as a parent, so bindings are never more than two levels deep, and so is a session still `starting`, since a launch that fails deletes its session whatever is bound to it. A `model` or `effort` that is a non-blank string is checked as in `update_project`, against the account the session will use, or the agent's default account; a blank `title` is read as absent. `resume_agent_session_id` is the agent's own id of a conversation started outside Octoboard (the one `--resume` takes), which the new session continues instead of starting one; the session is recorded with that id and as having a conversation, so a relaunch before its first prompt resumes it too. Trimmed, blank reads as absent. Only a project session takes one (`resume_for_console_session` otherwise), never together with `bound_to` (`conflicting_fields`); it must be a UUID (`resume_id_invalid`; stored lowercase and hyphenated whatever the case or form it was sent in), recorded as the agent's id by no session of any state (`conversation_in_session` for one of the same project, `conversation_outside_project` for any other, a console session included; checked again once the session is recorded, so a request that raced it is refused too), and have a record in the directory of the account the session will use, the pinned one or else the default account's from the login-shell snapshot (`conversation_to_resume_not_found`). Every refusal is before anything is recorded or launched. The reply (`session_opened`) comes as soon as the session is recorded and broadcast with `status` `starting`, before its agent process has been launched, which can take seconds; the launch goes on in the background, and starts only once the reply has been queued to the client, so a client never sees the session deleted before it has been told of it. When it succeeds the session moves to `working` (a `task` was given) or `idle`, as a `session_upserted`, unless the agent's own first hook has moved it on already. When it fails the session is deleted again (`session_deleted`) and a `launch_failed` `session_notice` says why; an `error` is never the reply to a launch that failed after the reply. Every refusal that can be known beforehand still comes as the reply's `error`. A `starting` session has no process: its terminal stream behaves as a dormant session's, and `archive_session` on it stands (the process is ended as soon as it is up), while `delete_session`, `resume_session` and `switch_session_account` are refused. The sessions an agent or a session request opens (`new_session`, an approving `answer_session_request`) are answered once the launch has settled instead, with its own error on failure. `terminal_appearance` is `light` or `dark`, the appearance the asking client's window has resolved to, sent so a Grok Build launch matches that terminal. Absent — a session an agent opens, a session request, or a client that sends none — leaves Grok following the operating system. It is not recorded on the session. |
| `resume_session` | `session`, `terminal_appearance?` | Relaunches an `interrupted` or `archived` session through the agent's own resume mechanism, re-injecting everything. A session bound to an archived parent (a console session, or a project session that started it) relaunches its archived parents first, level by level from the topmost down — for a session bound to an archived project coordinator whose console session is archived too, the console session, then the project coordinator — stopping at the first parent that is not archived; a parent that is only `interrupted` is left alone. Before anything is relaunched, the session and each parent about to be relaunched are looked up in the agent's own records, in the session's own account only (a copy left in another account does not count). A session whose conversation record is proven gone is refused with `conversation_unresumable` and nothing is relaunched; an archived parent whose record is proven gone is relaunched into a fresh conversation instead (a new agent session id, `has_conversation` false until its first turn, what is left of the old conversation's records in the agent's directories moved to the Trash) and the session is resumed as usual. A record that cannot be proven gone is not a refusal (an account directory that cannot be read, or that holds no records directory at all, included): the launch goes on and applies its own refusals, `config_dir_unreachable` for a pinned account directory that is gone. A Codex session whose rollout sits only in the account's `archived_sessions/` has `codex unarchive` run first, under the session's account: the session's own rollout before any parent is relaunched (`codex_unarchive_failed` when it fails, the whole group left as it was), an archived Codex parent's just before its own relaunch (a failure there is that parent's own refusal, below). If a parent cannot be relaunched the whole request fails with that parent's own refusal: parents already relaunched stay up, and the rest, the session included, stay as they were. Resuming a parent relaunches nothing bound to it, and no other session bound to the same parents is relaunched. `terminal_appearance` is as in `open_session`, and applies to this relaunch only; an archived parent relaunched on the way is left to follow the operating system |
| `archive_session` | `session`, `force_remove?`, `keep?`, `keep_process_ids?` | Ends the process and archives the session. This user control request also ends and archives every unarchived session under it: those bound directly to it, and those bound to its project coordinators, including manually bound sessions and descendants reached through an archived coordinator. The deepest descendants are archived first, then their parents. Already archived sessions keep their archive timestamps; unrelated sessions are untouched. All locks are checked before any session is changed; a locked session anywhere in the group refuses the request with `session_locked`. A session still launching is archived immediately and its process ends as soon as it comes up. Agent tools and reports keep their separate safeguard: they refuse a parent archive with `session_has_running_sessions` while a descendant has a process or launch claim. Once the processes of the sessions it archived have ended, the branches and worktrees bound to them are removed as "Branches and worktrees bound to a session" below says, together with the ones `force_remove` (absent means none) names; the reply does not wait for that. Then every client is told, as `session_branches_not_removed`, of what is still there except, unless `force_remove` names it, a branch checked out in a worktree that stays or a project's folder, and what `keep` names in the state it stands in. `keep_process_ids` (absent means none) are the ids of process trees (see "Process trees") the user chose to keep running: they are made the user's before the processes are ended (a console session's trees are not: it keeps nothing, so an id naming one is logged and the tree stopped), so the end of the session does not stop them; they stay kept even if the archive is then refused, for instance with `session_locked`. Every process attributed to the session's launches that is not kept is stopped as the sessions' processes end (see "Process trees") |
| `switch_session_account` | `session`, `account`, `terminal_appearance?` | Moves the session to another account of its own agent: ends its process the way `archive_session` does (and archives nothing — the session reads `interrupted` while the process is down), copies the session's conversation into the target account's directory, records the account and directory on the session, and relaunches it the way `resume_session` does, with every launch refusal applying unchanged. `account` is required: an id, or an explicit `null` for the default account, whose directory is resolved once from one login-shell snapshot that both the copy and the relaunch use (an absent `account` is `field_required`). The record is found by name under the root where the agent keeps its records (for Codex a rollout compressed to `.jsonl.zst` counts too, and is copied as it is). Once the process is down, what the agent keeps beside the record is added: for Claude Code the conversation's `projects/<slug>/<id>/` directory and `file-history/<id>/`, where they exist; for Codex, every rollout of the thread (its first rollout and, when a turn was reverted, each later history segment; the located record is the chain's head) — a Codex subagent's rollout, named by its own id, is left behind. Each item is copied to the same path relative to the config directory, creating the directories in between and replacing an earlier copy; the originals stay. For Codex, once the items are in place the target's database (`state_<n>.sqlite` in its directory, or where its `config.toml` `sqlite_home` or the launch environment's `CODEX_SQLITE_HOME` puts it) has its `threads` row of the session updated to name the carried head where the row leads to another file (`rollout_path` only: Codex resumes the rollout the row names, and would follow an older one; a `state_<n>.sqlite` that links to a file counts); it is the only write to the agent's own state besides the copy, and it is `relocation_failed` (the conversation is already copied by then, and the message says so) when where the database is cannot be determined (an unreadable or invalid `config.toml`, a relative `CODEX_SQLITE_HOME`, a directory that cannot be listed) or it cannot be written to or stays busy for 5 seconds; a `~` in `sqlite_home` is the launch environment's `HOME`. The items are built and size-checked in a transient `.octoboard-switch` directory at the target's root and put in place together: if one fails, those already placed are removed and the earlier copies put back. The directory is removed again, except when an earlier copy could not be put back, in which case it holds that copy until the daemon next starts or the session is next switched into that account. A session with no conversation (`has_conversation` false) has nothing to copy, and the switch records the account and relaunches into a fresh conversation. Answered once the relaunched process has stayed up for a few seconds; an `error` answer means the session is on the account it had, and the `session_upserted` events that put it back there (recording it interrupted when the relaunch did not come up) were sent before that answer. Refused, with nothing recorded and the process left running: `session_already_starting` / `session_already_running` while a launch, resume or switch of it is under way, `session_archived`, `session_already_on_account`, `unknown_account` for an id that names no account of the session's agent, `grok_home_not_initialized` for a Grok Build target that was never run against (checked before anything is copied into it), `account_sharing_in_progress` while a change of the sharing of the session's account or of the target runs, and `conversation_not_found`. Refused after the process was ended, with nothing copied or recorded: `session_did_not_stop`. The session stays claimed against a resume or another switch until the answer is sent. Once the process has been ended and the conversation is being moved: `relocation_failed` when the copy does not complete, the launch's own refusals (`config_dir_unreachable`, `directory_unreachable`, `binary_not_found`, …) and `switch_did_not_come_up` when the relaunched process ends at once — each leaves the session `interrupted` on its previous `account_id` and `config_dir`, with the conversation in both directories. `terminal_appearance` is as in `open_session` |
| `set_session_model` | `session`, `model?`, `effort?`, `terminal_appearance?` | Changes the session's model and reasoning effort, which become its own overrides: each is as in `open_session` (absent keeps what the session has, an explicit `null` or blank string leaves the choice to the agent, a string pins that trimmed value), and at least one is given (`field_required` otherwise, with `field` `model`). They are checked against what the session's agent offers under the session's own account, as `open_session` checks them (`invalid_launch_setting`). One mechanism serves every agent: a session with a running process (`working`, `waiting_user` or `idle`) has it ended the way `archive_session` ends it (and archives nothing), the values recorded, and is relaunched on the same account the way `resume_session` relaunches it, resuming its conversation (a session with no conversation yet starts a fresh one), with every launch refusal applying unchanged. A turn the restart cut off, or one that had stopped on the plan's usage limit, is continued as after `switch_session_account` (the relaunch is given the same prompt to continue, decided from the agent's record in the same way); otherwise the session comes back `idle`, any turn in progress cut off, and a prompt its agent was waiting on ended with the process. The process trees its process started are carried over to the relaunch as for `switch_session_account`, and kept under the session's project when the restart fails after it ended the process (see "process trees" below). From the moment the restart is decided until the answer, every client is shown the session `starting` (each change announced with a `session_upserted`, and a `snapshot` carries it), and the session stays claimed against a launch, a resume, a switch or another change; once the process is down, the `observed_model` and `observed_effort` its records showed are cleared, and what the agent records after the relaunch fills them again. Answered once the relaunched process has stayed up for a few seconds. An `interrupted` session has the values recorded for its next resume and is not launched; a session whose overrides are already those is answered with nothing written or restarted. Refused, with nothing changed: `unknown_session`, `session_archived` (an archived session is reopened with other values by the `reopen_session` tool), `session_already_starting` while it is being launched, resumed, switched, handed off, restarted for this or for a change of its account's sharing, having its branches handed over or its owner changed, `account_sharing_in_progress`, and `invalid_launch_setting`. Refused after the process was asked to end, with nothing recorded: `model_change_did_not_stop`. Once the process is down, the launch's own refusals and `model_change_did_not_come_up` (the relaunched process ended at once) leave the session `interrupted` with the model and effort it had, the `session_upserted` events that say so sent before the answer. `terminal_appearance` is as in `open_session` |
| `hand_off_session` | `session`, `agent`, `account?`, `model?`, `effort?`, `terminal_appearance?` | Hands the session off to a new session of another agent, which takes its whole place: the same kind (project or console), project or console, coordinator or worker role and parent session; its created name is the original's displayed name (task title｜created name, or the created name alone under the rules in "Displayed names" in `docs/product/session-titles.md`); the branches the session is responsible for name the new session instead; every session the original owns, archived ones included, is bound to the new one. The conversation does not move (the agents' records cannot resume one another): the new session starts with an opening prompt that gives the path of a handoff package built from the original's records (`handoff` in `apps/daemon/README.md`) and tells it to read it first; the original's own brief is not kept anywhere, so it is not repeated (the package holds the conversation, whose first user message is that brief). `agent` is required; `account`, `model` and `effort` are as in `open_session`. Order: every refusal first; then the package is written and the new session started, answered with nothing moved if that fails (the package is removed again); then the binding, the branches and the owned sessions move together, and each moved session that has a process, the parent (when it has one that is not archived; an interrupted one has the message kept for its resume) and the new session are told through the ordinary message delivery; the original is archived last, its screen kept as `archive_session` keeps it, and none of the sessions that moved with it. The original's message queue is held from the moment the handoff claims it: messages for it are accepted and queued, none is written; once the new session has taken its place the queue (including what was already in its outbox) is forwarded to it before the original is archived, a handoff that fails before the new session has started writes the queue to the original as usual, and one that fails after the place has moved (the original's archiving failing, say) sends it to the new session. The held queue is kept if the original's process ends meanwhile. The new session also carries over the original's pinned mark. The package is removed when the new session is deleted, and the daemon removes packages (and half-written files) that name no session at startup. Answered with `session_opened` for the new session, `working` with the opening prompt. Refused, with nothing changed: `unknown_session`, `session_already_starting` (a launch, resume or switch of the original is under way), `session_archived`, `session_working` (the session is in the middle of a turn or still starting; waiting for instructions, waiting for the user and interrupted are allowed), `session_locked`, `handoff_to_same_agent` (the refusal points to `switch_session_account`), `agent_not_available`, `agent_not_added`, `unknown_account` for an account that is not of `agent`, `account_disabled`, `invalid_launch_setting`, and what `open_session` refuses for a project session's parent. Failing after the new session is running — the coordinator's parent relationship no longer applying, or the original's archiving failing — is answered with an `error` that names the new session, which stays running. `terminal_appearance` is as in `open_session` and applies to the receiving session's launch |
| `inspect_session_records` | `sessions` | Inspects exactly these session ids and replies with `session_records_inspected`: aggregate `presence` is `present` when any session has a native record, otherwise `unknown` when an id is missing, a native id or account root cannot be resolved, or a listing is unreadable or linked, otherwise `absent`. An empty list is `absent`. Looks where deletion's Trash scan does (see `delete_session`), the default account only where that scan would take from it; Claude subagent directories and Grok session directories count even without a main transcript or usage ledger. This is a fresh inspection, not a promise about a later deletion |
| `read_session_history` | `session` | Replies with `session_history`: what the session's own agent record says the conversation was, for the history view of an archived session. An id with no session is `unknown_session`. It is read fresh from the agent's record (found the way the handoff package finds it, in every account that could hold it, and for a Codex thread that `codex archive` moved out of `sessions/`, in the account's `archived_sessions/`) and nothing is stored. A session that has had no conversation has an empty history. A record that is not found or cannot be read is not an error: the reply carries `unavailable`. Grok keeps only a summary of what came before a compaction, so the history of a compacted Grok session shows the turns since the compaction only |
| `inspect_session_branches` | `sessions`, `processes?` | Replies with `session_branches_inspected`: the branches and linked worktrees bound to exactly these session ids, as "Branches and worktrees bound to a session" below describes, each with where it stands now. An id with no session is skipped. Runs no `git` when none of the sessions is responsible for a branch. With `processes` true, the reply also carries `processes`: the process trees of those of these sessions that are running whose top has run for at least 30 seconds, what an archive of them would stop (see "Process trees"). This is a fresh inspection, not a promise about a later archive or deletion |
| `list_processes` | `session_id` or `project_id` | Replies with `processes_listed`: with `session_id`, the process trees of the session's current launch; with `project_id`, those of every running session of the project (each tree carries its `session_id`) followed by the project's trees no session owns, which carry none: its leftovers and the trees kept under it (`kept: true`), oldest first. Ports are read when the list is computed. Clients poll it on a timer while they show it; a list of the same session or project computed less than 2 seconds before answers again, unless a `stop_processes` or `keep_processes` through the daemon, or the end of a launch, came in between. A `session_id` of a session that has no process is refused with `session_not_running`; an unknown session or project with `unknown_session` or `unknown_project`. See "Process trees" |
| `stop_processes` | `ids` | Replies with `processes_stopped`: re-verifies each tree id against the ledger (a tree of a launch being followed, a leftover of a local project, or a kept tree) and stops the trees that still qualify; a kept tree stopped is the user's no more. An id that does not, because the tree is gone or was never one, is left out of the reply, not an error. See "Process trees" |
| `keep_processes` | `ids` | Replies with `processes_kept`: re-verifies each tree id as `stop_processes` does (a tree of a launch being followed that is not being ended, or a leftover of a local project), and the trees that still qualify become the user's, with everything that descends from them: no session owns them, the daemon never stops them unless a `stop_processes` names them or their project is removed, and `list_processes` of their project lists them with `kept: true` among its processes (the project of the session whose tree it was, or the local project a leftover is in). A console session's tree is not kept, as it belongs to no project, and is left out of the reply like an id that names no tree. Ids that name no such tree are left out of the reply. See "Process trees" |
| `delete_session` | `session`, `trash_agent_records?`, `only_if_unused?`, `force_remove?`, `keep?` | Removes Octoboard's record of one session and broadcasts `session_deleted`. Refused with `session_not_deletable` unless the session is archived or unused: no turn yet, with no session bound to it, and either independent or opened by the user (`origin` `user`); a session an agent started with `new_session` (`origin` `console`) was dispatched by its parent, which handed it work and expects it to exist, so deleting it would silently cancel that task, and it is archived instead. An unused session that is not archived has its process stopped once its record is gone; a turn landing at the same moment wins and the request is refused. So is a session being launched or resumed right now. Deleting an archived parent (a console session, or a project session that started some) deletes the archived sessions bound to it as well, at both levels — an archived project coordinator among them takes its own archived sessions along — each broadcast as `session_deleted` (a refusal deletes none); deleting an archived child session leaves its parent alone. An archived session whose process is still ending can be deleted; one being resumed right now cannot. Unused sessions' agent records are automatically moved to the system Trash. With `trash_agent_records` (absent means false), used sessions' agent records deleted by the request are also moved to the system Trash after the deletion (recoverable, never deleted outright) — Claude Code's transcript with its subagent directory, Codex's rollout file, Grok's session directory — so the session can no longer be resumed in the agent itself. They are looked for wherever this machine's accounts could hold them, since a session switched to another account leaves its original copy in the old one: the session's own account, every account Octoboard knows of for the agent, and every copy in every Claude project directory. The agent's default account is looked in only for a session on the default account, or on an account sharing the default configuration whose last change of it is complete, whose records live there; a session on any other account never removes anything from the default account's directory, however it is spelled or linked. Only a record named after the session's agent session id the way the agent names it is taken, and the id and any path an agent's hook reported are never trusted beyond that: an id that is not the agent's id shape (a canonical UUID), a record that does not sit at the agent's exact layout depth, one under a linked directory, or a reported path outside the account's records directory, names nothing. A record is left when another remaining session refers to the same agent session id, and one that is missing or whose account location is unknown is skipped. A failure to move one is logged and never fails the request; a process just stopped may still rewrite its transcript afterwards. The project's directory is never touched. `only_if_unused` (absent means false) deletes this session only if it is still unused at the atomic delete; otherwise `session_not_deletable`. Native-record presence does not affect eligibility. This guard never falls back to deleting a used archived session or its group; any native records are moved to the Trash automatically. After the deletion the branches and worktrees bound to every session it deleted are removed and reported as for `archive_session`, `force_remove` and `keep` included |
| `delete_archived_sessions` | `console`, `project?`, `console_session?`, `trash_agent_records?`, `force_remove?`, `keep?` | With `project` (which must belong to `console`), deletes every archived session of that project; with `console_session` (a console session of `console`, else `unknown_session`), every archived session bound to it, an archived project coordinator's own archived sessions included, leaving the console session itself; with neither, every archived console session of the console together with the archived sessions under them, at both levels. Naming both `project` and `console_session` is `conflicting_fields`. Broadcasts `session_deleted` for each. A session that stopped being archived meanwhile (a resume got there first) is skipped, not an error. `trash_agent_records` (absent means false) moves the native records of every session this request deleted to the Trash; without it, native records are retained. The branches and worktrees bound to every session it deleted are removed and reported as for `archive_session`, `force_remove` and `keep` included |
| `set_session_locked` | `session`, `locked`: boolean — locks or unlocks a session (locking an archived one is refused with `session_archived`; unlocking is not); broadcasts `session_updated` and replies `ack` |
| `set_session_pinned` | `session`, `pinned` | Pins or unpins a session; broadcasts `session_upserted` |
| `set_session_colour` | `session`, `colour` | Sets a console session's badge colour to one of the palette (see `Session.colour`); persisted, and broadcasts `session_upserted`. A `colour` outside the palette is refused with `unreadable_request`, and a project session with `internal_error` (the coordinator's `bail!`, with the reason as `detail`). A chosen colour counts as held for the next console session's assignment, like an assigned one |
| `send_message` | `session`, `text` | Writes a message into a running session. Refused while the session's own agent is waiting for the user: the message would be discarded and its trailing Enter would answer whatever dialog is up. A session that reads `waiting_user` only because a screen its agent raised is recognised on its terminal (the `agent_dialog` hold in "Messages held until a session can take them" in `docs/product/message-delivery.md`) takes the message into its queue and is answered with success; it is written once the screen is gone. A session that reads `waiting_user` only because a `request_session` call of its own is waiting takes a message as usual — that request is answered in its dialog, not at the terminal (see "Waiting for the answer" in `docs/product/session-requests.md`). The console session's own `instruct_session` tool holds such a message instead of refusing it — the user can be told to answer the prompt first, the console session cannot. Refused too, with `session_trust_pending`, while the session's agent is showing, or may still show, its folder-trust confirmation, since the trailing Enter would answer it: for Grok Build from launch until its first hook, and for Claude Code until both its first hook has arrived and its terminal has bracketed paste on at that hook or after it, either of which may be after another startup screen; for Codex, which runs none until its first prompt, until Octoboard's press of the confirmation, a hook, the person's answer in the terminal being seen to replace the confirmation, or its first terminal title with none shown; the console session's tool, and every other message Octoboard writes, holds it until then instead. Refused with `field_required` (`field`: `text`) when nothing but whitespace is left of `text` once its control characters and the characters Claude Code treats as invisible are removed (see "Messages held until a session can take them" in `docs/product/message-delivery.md`). Answered once the message is written or queued and, for a Claude Code session at its prompt, once the few-second check that its agent sent it has ended; a message left unsent is not refused, but reported to every client as a `message_not_sent` notice |
| `interrupt_session` | `session` | Stops the turn the session's agent is running, as the user pressing its interrupt key in the terminal would, and replies with `session_interrupted`; no ownership is checked. The process and its conversation stay, and the session goes back to `idle`. The key is the agent's own, written on its own and never joined to a message: one Esc for Claude Code and Codex, Ctrl-U then Ctrl-C for Grok Build (which ignores Esc, and whose Ctrl-C clears text in its composer instead of cancelling the turn). A draft in Grok's composer is cleared first with Ctrl-U, one press at a time until the screen shows the composer empty, and the note says so (Grok may let the user undo it with ctrl+z); a draft that does not clear gets no Ctrl-C. The input box is read off the screen with faint text left out, such as Claude Code's placeholder in a fresh session's empty box. It is written only while the session reads `working` by its hooks and its terminal title shows the turn running, checked right before each key, so a turn that ends meanwhile gets no key. When Claude Code or Grok Build puts the interrupted prompt back in its input box (a key before the model has shown anything), the daemon clears it with Ctrl-U, one press at a time while the screen shows text there, as long as the box was empty before the interrupt key and nothing else has been written meanwhile; text left there is reported in the note. Messages Octoboard has for the session are held from the first key until the reply, so none joins the restored text, and are written afterwards. A cancel the user makes in the terminal is left as the agent leaves it. The reply waits up to five seconds for the turn to end, which counts even when a message the agent had queued starts the next turn at once. A session not in a turn (at its prompt, with or without background work) is answered with `interrupted: false` and nothing is written; so is a session another `interrupt_session` is already interrupting. Refused, with nothing written: `session_not_running` for a session that is archived or interrupted, has no process, or is still being launched or not yet past its startup screens; `session_trust_pending` while its agent shows its folder-trust confirmation; `session_waiting_for_user` while its agent waits on the user (a permission prompt or question, a screen of its own recognised on the terminal, Codex's `Action Required` title) or a `request_session` call of its own holds its hand up; `session_busy` while it is being switched to another account, handed off, having its branches handed over, given another parent or restarted for a change of its account's sharing. An id that names no session is `unknown_session` |
| `rename_session` | `session`, `title` | The title is stored trimmed and cannot be blank (`field_required`) |
| `release_session` | `session` | Makes a project session independent from its parent, whoever it is, and broadcasts `session_upserted`: `bound_to` is cleared and the session is an ordinary independent project coordinator, the user's own. A project coordinator keeps the sessions bound to it, and branches stay with their session. Allowed whether the session is running or not, and refused for a console session, an independent session, and a session, or the parent it is bound to, that is being started, resumed, handed off, switched to another account or having its parent changed. The refusals are `console_session_never_bound`, `session_not_bound`, `session_binding_changed` (the binding moved between the check and the write), `session_changing` and `session_owner_changing`; an id that names no session is `unknown_session`. A running session is told in a message, queued while it cannot take one, that it has no parent and which tools it has; one with no process is told when it next starts, ahead of any instruction of that launch, as the notice every change of parent leaves for a session that was not running (see `release_session` under the MCP tools). The former parent is told in a message, queued while it cannot take one, that the user released the session, so it can no longer instruct, drive or archive it (it can still tell it something with the MCP tool `send_message`) and should treat any work it handed the session as left with the user; a parent with no process is not told, and a later attempt by it to drive the session is refused as not its own |
| `list_pages` | `console_session` | A session's report panel pages, oldest first. Answered with `page_list` on the asking socket. `console_session` is the session that pushed the pages, a console session or a project session; one that names no session is refused with `unknown_session` |
| `submit_page` | `page`, `data` | A report panel form submission; `data` is the submitted form's fields as an object of strings, field name to value (a repeated name's values joined by `, `), in document order, including the pressed submit button's `name`/`value`. Written into the session that pushed the page, found from the page itself, as a user message naming the page; held rather than refused while that session is `waiting_user`, since the session is not the one who has to answer that prompt. Refused when `page` is not its session's newest page — history pages are read-only — and, as an unknown page, when its session has the worker role and so no tool to read the submission (its pages are withdrawn when it becomes one, below) |
| `confirm_trust` | `session`, `remember`, `trust_parent_dir?` | The user's go-ahead to a `trust_prompt`, whichever agent is asking: Octoboard may press that session's own folder-trust confirmation, which it does by typing at the session's terminal after checking that the confirmation is still up — for Claude Code a Down and an Enter, once the cursor is seen to move to "Yes, I trust this folder"; for Codex an Enter, with the cursor seen on "1. Trust and continue" and nothing written into the session since the confirmation was sighted; for Grok Build a `y`, while no hook has run, the process is still there and nothing has been written into the session since the confirmation was sighted. Pressing it is what makes the agent record the trust in its own configuration, so its later launches do not ask; for Grok Build the entry Grok wrote into the session's own copy of its trust store is then carried over, unchanged, into the user's own store, and the press counts only once that is done, or, when no hook comes to carry it (a Grok policy that runs none), once the confirmation is seen to have gone from the screen; a hook that comes later still carries the entry. `remember` also records the project's permission (`Project.trust_consent`) once the confirmation has been pressed, so a later confirmation of any agent in that project is pressed without a prompt, and broadcasts the updated `project_upserted`. `trust_parent_dir` (absent means false) records the project's parent directory as trusted instead, once the confirmation has been pressed, and then `remember` adds nothing. Every project whose directory really lies under that directory (see the `trusted_directories` notes under "Records") is covered by it, for every agent — those already there, those added later by any means, repositories the console session clones or adds into it included — and each agent then applies that project's own configuration once it has recorded its trust. The confirmations already waiting under it are pressed at once. The daemon derives the directory from the session's project (it is the prompt's `trust_dir`); a client never names one. A parent that is the filesystem root, the user's home directory (however it is spelled or linked) or a directory containing it is refused, before anything is pressed, with `error` code `trust_directory_too_broad`; so is a project whose path is not absolute (`trust_path_not_absolute`), so is any case where the home directory cannot be determined to check against (`trust_home_unknown`), and so is a project whose parent cannot be checked within a few seconds, as on a mount that does not answer (`trust_directory_unreachable`); a client keeps its dialog open for each of these four. Refused, with nothing recorded and nothing sent, unless `session` is a running project session that is still waiting at its own agent's confirmation and has not been answered (one that is no longer waiting is answered with `error` code `trust_not_waiting`, which a client shows nothing for); a failure to press once it was accepted (the confirmation is not as expected, or something else typed into the session meanwhile) is an `error` with code `trust_answer_failed`, and a Grok Build press whose entry could not be carried over is one with code `trust_not_carried_over`; either records no permission and is also broadcast as a `session_notice` so that it is seen even if the requesting dialog has closed. Octoboard never writes a trust decision into an agent's configuration itself; the one file it writes is the user's Grok Build trust store, with the entry Grok wrote |
| `remove_trusted_directory` | `path` | Stops trusting a directory (compared after lexical normalisation, so a trailing slash does not matter) and broadcasts `trusted_directories_updated`. Projects' own permissions, sessions already running and the trust each agent has already recorded are untouched. Removing one that is not trusted does nothing. Directories are added only by `confirm_trust` |
| `answer_session_request` | `request_id`, `approve`, `agent?`, `account?`, `model?`, `effort?` | The user's answer to a `session_request`. `approve` true does what the request's `action` says. For `start` it starts the session the request describes — a project session in its project, or a console session in its console — as `open_session` would, with no `bound_to`: it has no parent and reports to the user, and the requesting session carries on unchanged, neither bound to it nor owning it. For `reopen` and `resume` it brings back the session `target` as `resume_session` does, without an instruction, and its parent and everything bound to it stay as they are; archived parents are reopened first, topmost down, and a name the request carried for an archived session is then given as `rename_session` does (a name that cannot be given is told to the requester and does not undo the reopen). A target that is already running or starting when the answer arrives is left alone and counts as done, as does one that `resume_session` refuses with `session_already_running`, or with `session_already_starting` for the target's own launch or after which the target is no longer dormant; a `session_already_starting` that is a claim for an account switch, a handoff, a handover of branches or a binding change on the target, or for an archived parent, while the target is still dormant, is not done: the answer fails with that refusal's own error and the requester is told the approval could not be carried out. A name the request carried that was not given because the target was not archived by then is told as `renamed: false` in the result. For `resume_record` it opens a session as `open_session` does on the record the request names, with no `bound_to`, so that it reports to the user. For `switch` it runs the parent's `switch_session_account` on the session `target` and for `handoff` its `handoff_session`, each with the values the user ended the dialog on: `agent` (a handoff's receiving agent) and `account` (a switch's target account, a handoff's receiving account; an explicit `null` is the agent's default account), and `model` and `effort`, each absent where the request's own value stands, an explicit `null` for the agent's own default and a string to pin it. The other values of `agent`, `account`, `model` and `effort` are ignored for the other actions. A switch records a model or effort that is given as the session's own override, checked as `open_session` checks them against the account the session is switched to, before the relaunch that reads it; the previous overrides are put back when the switch fails. A handoff's account left out stands as the request showed it, for the agent the request named; for another agent it is settled as a `handoff_session` call naming that agent with no `account` would settle it. The call's `model` and `effort` stand for the agent it named only. The result of either tool names what was done: a handoff's carries `account`, `account_name` and, where set, `model` and `effort`, a switch's carries `account`, `account_name` and `model` and `effort` where the session has an override, and the `note` says when the user chose differently from what was asked. Everything the operations refuse is refused at that moment as for the tool without a dialog, and the call returns the tool's result. False refuses it and changes nothing (a client sends false when the dialog is dismissed). Approval is answered with `session_opened` carrying the new session, or for `reopen`, `resume` and `switch` the target as it then stands, and for `handoff` the receiving session; refusal with `ack`. The first answer wins. An answer for a request that has already been answered, or that the daemon does not know, gets `session_request_answered`. An approval for one that was withdrawn or timed out, or whose session's process ended before it could be carried out, gets `session_request_not_waiting`; a refusal for one gets the same code. An approval that fails while launching returns that launch's own error |
| `update_settings` | `auto_pull_repositories?`, `auto_delete_unresumable_archives?`, `auto_approve_session_requests?`, `auto_approve_resume_requests?`, `auto_approve_account_switches?`, `auto_approve_handoffs?`, `ask_questions_as_choices?`, `interface_language?`, `auto_select_account?`, `auto_switch_on_usage_limit?`, `sidebar_order_by_status?`, `auto_start_quota_windows?`, `default_clone_dir?`, `git_workflow?`, `claude_compatibility?` | Each settable field absent means "leave it alone". Turning `auto_delete_unresumable_archives` on starts no scan; the next scheduled one (at most a day away) deletes again. Turning `auto_approve_session_requests`, `auto_approve_resume_requests`, `auto_approve_account_switches` or `auto_approve_handoffs` on or off changes no request already waiting: the first is read when a `request_session` call is made, the second when a `request_resume_session` call is made, the third when a `switch_session_account` call is made, the fourth when a `handoff_session` call is made. Turning `ask_questions_as_choices` on or off changes no session already open: it is read when a session is launched (see the description of the setting below). `interface_language` is a language tag the interface offers (`ar`, `de`, `en`, `es`, `fr`, `hi`, `id`, `it`, `ja`, `ko`, `pt-BR`, `ru`, `th`, `tr`, `vi`, `zh-Hans`, `zh-Hant`); any other tag is refused, leaving the whole request unapplied. Turning `auto_select_account` on or off changes no session already open: it is read when `new_session`, `request_session` or `handoff_session` leaves the account out, and then the session runs on the account of its agent whose weekly quota resets soonest and still has some left, or, when none does, on the account it would have taken while the setting is off. Turning `auto_switch_on_usage_limit` on or off changes no session: it is read at the end of a session's failed turn (see "Switching a session whose turn hit the usage limit" below), and has no bearing on how a new session picks its account or on whether an agent's request for a switch is asked about. `default_clone_dir` follows the path rules of `add_project`'s `path` (absolute or `~/`, stored lexically normalised, a relative one refused), and a blank value goes back to the built-in default; a refused value leaves the whole request unapplied. `git_workflow` is an object of the four git workflow settings (`development`: `in_place`\|`branch`\|`worktree`, `rebase_before_merge`, `delivery`: `ask`\|`merge`\|`pull_request`, `revert_gate`), each optional; `rebase_before_merge` and `delivery` apply only while the effective `development` is not `in_place` (the stored values are kept when it is); `revert_gate` applies whatever `development` is; an item left out is left alone, and an unknown `development` or `delivery` is refused when the request is parsed, as any bad enum value is. Broadcasts `settings_updated` only when something actually changed, as `remove_trusted_directory` does. A change that turns `auto_pull_repositories` on also starts the immediate fast-forward pass described in "Fast-forwarding when the setting is turned on" below, whose statuses follow as `project_git_status` broadcasts; an update that leaves the value as it was starts nothing. Turning it off starts nothing and undoes nothing. Turning `auto_start_quota_windows` on looks over the readings already held at once, as described in "Account quotas" below. `claude_compatibility` is an object keyed by agent wire name (`codex`, `grok`) with a boolean each; an agent left out is left alone, and `claude` or any other key is refused, leaving the whole request unapplied. It is read when a session starts, resumes or switches account (see the paragraph on `Settings.claude_compatibility` below) |
| `refresh_git_status` | `console` | Checks every project of `console` against its remote, a few at a time; answered with `ack` at once, and the statuses follow as `project_git_status` broadcasts, one per project as its own check finishes. A project whose check is already running — this call raced ahead of an earlier one, or another client is watching the same console — is not started again, and nor is one whose last check completed less than a minute ago: several clients can each be polling this console on their own 5-minute interval and phase, and without this floor their sweeps would interleave into several checks per project every 5 minutes instead of one. Either way nothing is broadcast for the project that was skipped — it keeps whatever status it already had. An unknown console is `unknown_console`. See "Daemon behaviour, per project" below for what one project's check does |
| `open_project_path` | `project`, `path`, `worktree?` | Opens an existing project-relative wire path with its default application on the daemon's host; a directory opens in the file manager. `worktree` selects the same scoped live root as browsing; absent means the project's own directory. The path and its symlink target must remain inside that scope. Answers `ack` when the host accepts the request (Linux starts `xdg-open` detached, so later launcher failures cannot be observed). Invalid, missing and outside-scope paths use the browse errors; launch failures are `path_open_failed` |
| `reveal_project_path` | `project`, `path`, `worktree?` | Shows an existing path in the file manager on the daemon's host: a file is selected in its containing folder, a directory is opened. Scope and path validation match `open_project_path`; a symlink is always selected by its own name, whatever it points at. macOS uses `open -R` to select and `open -a Finder` to open a directory, except that a directory macOS types as a package (`Foo.app`, `Foo.xcodeproj`, which opening would launch) is selected in its parent, as is one whose type cannot be read; Linux requires `gdbus` and a file manager implementing `org.freedesktop.FileManager1`, calling `ShowItems` to select and `ShowFolders` to open a directory. Answers `ack` after the launcher succeeds; launch failures are `directory_unavailable` |
| `probe_path_links` | `project?`, `paths` | Asks, for each path the session's terminal showed, what it names on the daemon's host. Answered with `path_links`, one entry per path in order, `none` for anything not reachable. A path is absolute, starts with `~/` (the daemon's home), or is relative. It is reachable when it exists and, once symbolic links are resolved, lies inside a root: the project's directory or its place in a worktree of the same repository, or a temp root (`/tmp`, `/private/tmp` and the daemon's own temp directory, `$TMPDIR`; one that is `/` or holds the home directory is not used). An absolute path must also start with a root as written (or as the project's directory is stored), not only resolve into one. A relative path is tried against each project root and is reachable only when exactly one has it; one found in several is `none`. `project` absent means a relative path resolves to nothing; a project whose directory cannot be resolved leaves its paths to the temp roots alone. What the system would launch rather than show (a file with an executable bit, a path with a component of a launchable extension: `.app` and other bundles, `.command`, shell scripts, `.pkg`, `.url`/`.webloc`-style shortcuts and the like, compared without case) is never opened by a link click: in a temp root it is not reachable, and in a project a folder of that kind is not either, while a file is, with `viewer_only` true, to be shown by the file viewer alone (`open_path_link` refuses it). Inside a temp root the path and every directory from the root down to it must also belong to the daemon's user and not be writable by group or others. At most 64 paths, else `limit_exceeded` (`limit` `probed_paths`); an unknown project is `unknown_project`. The project's roots, which cost a `git` call, are looked up only for a path that is not under a temp root, or one gone from under it, and are reused for a few seconds; at most four probes run at once. A file inside the project or one of its worktrees (not a temp root) also carries a `view`, where to read it with `read_project_file`: the innermost root holding it (a worktree kept below the project's directory is its own), `worktree` (absent for the project's own directory) and its wire `path` there. No file is read to answer: whether the viewer can show the file (text or an image, within the read cap) is for the client to find out by reading it, and it opens one it cannot show in its default application unless `viewer_only`. A project directory carries a `view` naming its entry file: a regular file directly in it named `README` or `README.*`, else `index.<ext>`, ignoring case, `.md` first, then `.markdown`, then other names alphabetically (links are not followed); only the directory's listing is read, never a file. Temp-root files and folders, and project folders with no entry file, have no `view` |
| `open_path_link` | `project?`, `path` | Opens a path `probe_path_links` found reachable, resolving it again by the same rules (with the project's roots looked up afresh): a file with its default application, a folder in the file manager. Answers `ack` when the host accepts the request; a path spelled inside a root where nothing is any more (deleted, moved or renamed since it was probed) is refused with `file_not_found`, a path outside every root (spelled outside them, or resolving outside them through a link) with `outside_scope` and `action` `open`, a read the system denied with `permission_denied`, any other path that is not reachable, or one that is `viewer_only`, with `path_link_refused` and the `cause`, a launch failure with `path_open_failed`, an unknown project with `unknown_project` |
| `create_shell_terminal` | `project`, `worktree?`, `path?` | Starts a shell terminal for the project: in the project's directory, or, with `worktree`, in the project's place in that worktree of its repository — the same scoped root `open_project_path` and browsing resolve, so a worktree that is not one of the project's repository's is refused with `worktree_unavailable` and a scope missing there with `source_unavailable`. A non-empty `path`, a wire path as for `open_project_path`, starts the shell in that folder below the root instead (the record's `cwd` is the resolved folder); it is validated like `open_project_path`'s, and one that resolves to something other than a folder is refused with `path_not_a_directory`. Answered with `shell_terminal_created`, after `shell_terminal_upserted` has been broadcast. See "Shell terminals" below |
| `rename_shell_terminal` | `shell_terminal`, `title` | Names the terminal; the shell's own titles no longer change it. The title is stored trimmed and cannot be blank (`field_required`). Broadcasts `shell_terminal_upserted` |
| `close_shell_terminal` | `shell_terminal` | Ends the terminal's shell if it still runs (see "Shell terminals" below) and removes the terminal, broadcasting `shell_terminal_deleted`; answered with `ack` once the shell is gone. An exited terminal is removed the same way |
| `open_project_terminal` | `project` | Opens the system terminal at the project's directory, on the daemon's host. On macOS that is Terminal.app (`open -a Terminal`), which starts a shell in the directory. On Linux there is no single system terminal, so the first installed program in a fixed list that can be given the directory is used (a program that only inherits its working directory is used when none of those is installed). Answered with `ack` once the terminal has been asked to open, not when its window closes. Refused with `terminal_unavailable` when the directory does not exist, the operating system has no system terminal Octoboard can open, no emulator is installed, or the launch fails. An unknown project is `unknown_project` |
| `open_project_directory` | `project` | Opens the project's directory in the system file manager on the daemon's host: `/usr/bin/open -- <directory>` on macOS, or `xdg-open <directory>` on Linux (looked up in the user's login-shell `PATH`). On macOS, answered with `ack` once `open` exits successfully, bounded to 15 seconds. On Linux, `xdg-open` is detached into its own session and reaped in the background because it may run for the file manager's lifetime; `ack` means the process was spawned, and later launcher failures are not observed. Neither waits for the file manager window to close, and quitting the daemon does not close it. Refused with `directory_unavailable` when the path is not an existing absolute directory, the operating system is unsupported, the Linux login-shell environment or `xdg-open` is unavailable, the process cannot be spawned, or on macOS `open` fails or times out. An unknown project is `unknown_project` |
| `push_project_branch` | `project` | Pushes the project's current branch and only that branch, never with `--force` and never as a bare `git push` (which would follow `push.default`). A branch that has an upstream is pushed to that remote branch. A branch with no upstream is pushed to `origin` when the repository has one, otherwise to its only remote, and that push records the upstream; several remotes and no `origin` is `no_push_remote`, and nothing is pushed. The local branch header is re-read first (no fetch) and broadcast, so the push is shown against the current ahead count even for a commit nothing had noticed yet; then it publishes `pushing` activity during the request, retaining that branch, counts and error. It takes precedence over overlapping checks and pulls until every concurrent push ends; success and failure both restore the latest underlying activity. Answered with `ack` once `git push` has exited. A success is followed by `project_git_status` from a local re-read of the branch header, with no fetch, so the badge's ahead count can drop. Also refused, with nothing pushed: `not_a_git_repository`, `not_on_a_branch` (a detached `HEAD`), `no_commits_to_push` (a branch with no commit yet), `upstream_not_a_remote` (the upstream does not name a remote of this repository) and `git_push_failed` (`git`'s own message, or a branch name that cannot be placed in a refspec). An unknown project is `unknown_project` |
| `switch_project_branch` | `project`, `branch` | Switches the project's own worktree to an existing local branch; `branch` is its canonical wire name below `refs/heads/`, as listed by `list_project_branches`. Requires `.git` in the project directory itself. Validates the literal local reference before `git switch --no-guess -- <branch>`; never creates a branch, guesses a remote branch, forces, stashes, fetches, pushes or fast-forwards. Answered with `ack` after Git exits. Success publishes a local `project_git_status` re-read to every client. Failure is `unknown_project`, `not_a_git_repository`, or `git_switch_failed` with `detail` (Git's own message when it ran). Local changes that prevent switching, a branch checked out in another worktree, a deleted branch or invalid wire/reference name are errors. Uses the filtered, non-interactive Git environment below and a two-minute bound per call. Concurrent Git operations are not serialized with it |
| `restore_project_changes` | `project` | After confirmation in the client, restores staged and unstaged tracked changes in the project's own worktree to `HEAD`, including deleting staged additions absent from `HEAD`. Preserves untracked and ignored files, submodule worktrees and gitlinks, commits, the branch and other worktrees. Requires `.git` in the project directory itself, like the branch badge; repository subdirectory projects are refused. Answered with `ack` after Git exits; success publishes a local `project_git_status` re-read without fetching. Refused with `unknown_project`, `not_a_git_repository`, `restore_not_on_branch` (detached HEAD), `no_commits_to_restore` (unborn branch), `git_operation_in_progress` (merge, rebase, cherry-pick, revert or bisect), or `git_restore_failed` (`detail`). Each Git call is limited to two minutes and 16 MiB stdout / 64 KiB stderr; an inspection that exceeds a bound fails before restoration. A path occupied on disk after removal from the index, a directory where a tracked file belongs, or a symbolic link or file blocking a tracked ancestor is refused before restoration so untracked or ignored content is preserved. Uses the filtered Git environment below. Confirmation is a client responsibility; the request carries the confirmed destructive intent. Git can restore some paths before a later path fails; an error does not promise rollback. Concurrent Git operations, including those from running agents, are not serialized with this request |
| `pull_project_branch` | `project` | Checks one project against its remote and fast-forwards it right now, the same work `refresh_git_status` does per project (see "Daemon behaviour, per project" below), except that the one-minute floor does not apply and the fast-forward happens whether or not `auto_pull_repositories` is on, under the same rule (`upstream` set, `behind > 0`, `ahead == 0`). Answered with `ack` at once; the status follows as `project_git_status` broadcasts. A project whose check is already running gets one more check, with the fast-forward, as soon as that one ends, so the request ends in a fast-forward attempt; any number of requests meanwhile share that one rerun. A project whose check is only waiting for a place in a sweep, or in the fast-forward pass started by turning `auto_pull_repositories` on (at most four projects are checked at once, across every client) does not wait behind it: that check runs at once with the fast-forward, in its place, so the request costs one fetch and no rerun. The in-flight `checking` status is broadcast at once for the request, without the 400 ms delay a sweep's check has. The rerun is skipped too if the project has been removed meanwhile. A project that is not a git repository is reported as such (`repository: false`). An unknown project is `unknown_project` |
| `create_account` | `agent`, `name`, `config_dir` | Both fields are required: an account always has a name and a directory. `name` is trimmed and must be non-empty and unique within `agent` (compared trimmed, case-insensitively, including against the default account's own name); a collision is `account_name_taken`, naming the account it collides with. `config_dir` must be absolute or start with `~/`, and is stored lexically normalised; existence is not checked. The normalised directory must not equal another account's of the same `agent`, nor the default account's directory (as the daemon resolved it; until that is determined, the conventional `~/.claude`, `~/.codex` or `~/.grok`); compared lexically without following symlinks, folding case on macOS (a case-insensitive file system) and byte-exact elsewhere, the stored text staying as typed, and a collision is `config_dir_taken`, naming the account using it. Broadcasts `settings_updated` |
| `update_account` | `account`, `name?`, `config_dir?` | Renames the account, repoints it, or both, independently; either field absent leaves it alone. Validated as in `create_account`. Broadcasts `settings_updated` when something actually changed. Pointing a sharing account (or one whose last change of sharing is incomplete) at another directory turns its sharing off first, in the directory it was carried out on, with the same refusals as `delete_account`; the account then does not share |
| `add_agent` | `agent` | Puts the agent at the end of `Settings.added_agents`. An undecided list is filled first, once availability is determined (announced by `settings_updated`), so a refusal can follow a change. Refused: `agent_not_installed` when the agent has been determined unavailable, `agent_availability_pending` while availability is not determined yet (the list is not decided until it is, so it cannot be edited before), `agent_already_added`. Broadcasts `settings_updated` |
| `remove_agent` | `agent` | Takes the agent off `Settings.added_agents`; an undecided list is filled first, as for `add_agent`, and is refused with `added_agents_undecided` while it stays undecided. Its accounts stay in `Settings.accounts` untouched, and the consoles, projects and sessions that use it are left as they are. Answered with `ack` and no `settings_updated` of its own when the agent is not on the list; otherwise broadcasts `settings_updated` |
| `reorder_agents` | `agents` | Puts `Settings.added_agents` in the given order, which carries the whole list: it must name exactly the agents on it, each once (a permutation of the current list). An undecided list is filled first, as for `add_agent`. Refused: `agent_order_invalid` when an agent is missing, repeated or not added, `added_agents_undecided` while the list stays undecided. A request that leaves the order as it is is answered with `ack` and no `settings_updated` of its own; otherwise broadcasts `settings_updated`. The order is the one every client lists agents in |
| `reorder_accounts` | `agent`, `accounts: (string|null)[]` | Sets `Settings.account_order[agent]` for display. The list must name every owned account of the agent and its default (`null`), each exactly once; missing, repeated, unknown or other-agent accounts are refused with `account_order_invalid`. A changed order broadcasts `settings_updated`; an unchanged order answers with `ack` only. Does not affect automatic quota-based account selection |
| `set_default_account_name` | `agent`, `name` | Names the agent's default account (`Settings.default_account_names`). `name` is trimmed; a blank name, or `Default` in any letter case, clears it, back to the client's own word for the default. Refused with `account_name_taken` when another account of the `agent` has the name (trimmed, case-insensitive); the same check refuses `create_account` and `update_account` a name the default account has been given. The word the daemon reserves for the default (`Default`) stays refused to accounts either way. Broadcasts `settings_updated` only when the stored name changed. The directory of the default account is not editable |
| `set_account_disabled` | `agent`, `account` (id or `null`), `disabled` | Turns an account off for new work, or back on; a null `account` names the agent's default account (`Settings.disabled_default_accounts`, by wire name), a stored one is `Account.disabled`. A disabled account is skipped by automatic account selection and refused by `open_session`, the handoff and `switch_session_account` as `account_disabled`; sessions already on it are untouched, and nothing turns it back on but this request. No usage window is started automatically on a disabled account (see `auto_start_quota_windows`); turning an account back on, with that setting on, starts at once the window its held reading shows unstarted, as turning the setting on does. Refused with `unknown_account` for an id that is no account of the `agent`. Broadcasts `settings_updated` only when the flag changed |
| `set_codex_approval_reviewer` | `reviewer` (`user`, `auto_review` or `null`) | Sets the Codex approval reviewer for every Codex account, the default account included; a null reviewer follows each account's own configuration. Broadcasts `settings_updated` only when changed. Takes effect on the next launch or resume; running sessions keep their current reviewer |
| `set_claude_approval_mode` | `mode` (`auto`, `manual`, `bypass` or `null`) | Sets the Claude Code approval mode for every Claude Code account, the default account included; a null mode follows each account's own configuration. Broadcasts `settings_updated` only when changed. Takes effect on the next launch or resume; running sessions keep their current mode. The daemon does not itself ask for confirmation of `bypass`; the client does (see "Approval modes" in `docs/product/per-agent-settings.md`) |
| `set_account_sharing` | `account` (id or `null`), `share` | Sets whether the account shares the agent's default configuration (`Account.share_default_config`; see "Sharing the default configuration" below) and carries the change out at once, answering `ack` when it has ended. A null account names the default account, which is refused with `default_account_not_shareable`, as is turning sharing on for an account whose directory resolves to the default account's; an unknown account is `unknown_account`. Refused, with nothing recorded or stopped: `account_in_use` while sessions of the account work under its directory (see `account_sharing`'s `in_use`), or one of its idle sessions turned working before the change ended it, or it is being signed in, and `account_sharing_in_progress` while a change of it runs. Otherwise the account is locked, its idle sessions' processes are ended (what they started is carried over to the resumed process, as for `switch_session_account`; see "Process trees"), the choice is recorded with `Account.share_default_config_incomplete` set and `settings_updated` broadcast, the change runs, and those sessions are resumed on their conversations, whatever its outcome, before the account is let go; `account_sharing` tells the progress throughout (see "Sharing the default configuration" below). A change that fails part-way is answered with `account_sharing_failed`; nothing is rolled back and the account stays incomplete. Asking again for the value recorded retries an incomplete change and does nothing otherwise. Broadcasts `settings_updated` again when the change completes |
| `delete_account` | `account` | Clears the reference of every console and project that refers to this account — putting each console back on its agent's default account and each project back to inheriting its console's, while a project whose reference is `default` names no account and is left as it is — then removes the account. An account that shares the default configuration, or whose last change of it is incomplete, has its sharing turned off first, at once and under the account's lock as `set_account_sharing` does; that is refused with `account_still_sharing` while sessions of it run or start or it is being signed in, with `account_sharing_in_progress` while a change of it runs, and with `account_sharing_failed` (sharing then stays turned off, incomplete) when it does not complete, each leaving the account in place. A session holding it is left alone: it already carries its own copy of the directory it launches with. Broadcasts `settings_updated`, a `console_upserted` for every console whose reference was cleared, and a `project_upserted` for every project whose reference was cleared |
| `shutdown` | — | Terminates every session process (leaving them `interrupted`) and exits the daemon |

### Daemon to client

| `type` | Fields |
|---|---|
| `snapshot` | `hosts`, `consoles`, `projects`, `sessions`, `trusted_directories`, `settings`, `git_statuses`, `agent_availability`, `account_quotas`, `account_logins`, `account_sharing`, `home_dir`, `shell_terminals` — sent once, unprompted, when a control socket connects. `git_statuses` is every `GitStatus` the daemon currently holds, empty on a fresh start — carried here, like the trusted directories, so a reconnecting client never has to ask for it separately. `agent_availability` is always three entries, one per agent, each `not_determined` until the daemon's one-time login-shell snapshot for this run lands — see "Agent availability" below. `account_logins` is the logins that have started and not ended, empty when none runs — see "Account login" below. `account_sharing` is the sharing state of every account in `Settings.accounts`, as `account_sharing` sends it. `account_quotas` is the plan quota last read for each account of every available agent, empty until the first readings land — see "Account quotas" below. `home_dir` is the daemon host's home directory — not the browser's, the two may be on different machines — so a client can show a path under it as `~/...`; it is `HOME`, or the user's entry in the system user database when `HOME` is unset or relative, and null when the home settled on that way is the filesystem root. `shell_terminals` is every `ShellTerminal` the daemon holds, running or exited, oldest first; with the records it is all a client needs to rebuild its terminals, so one that lagged loses nothing by the fresh snapshot it is sent |
| `agent_availability_updated` | `agent_availability` — the whole three-entry list, sent once, when the daemon's one-time determination of it lands; never again afterwards, since nothing re-determines it during a run |
| `account_quotas_updated` | `account_quotas` — the whole list, sent whenever a reading finishes or an account's reading is dropped (the account was repointed or deleted); see "Account quotas" below |
| `trusted_directories_updated` | `trusted_directories` — the whole list of trusted directory paths, sent when it changes |
| `account_login_updated` | `login` — an `AccountLogin`, the account's whole login state, sent when it starts, moves on or ends; see "Account login" below |
| `account_sharing` | `account`, `in_use?`, `idle_sessions?`, `progress?`, `failure?` — an account's whole sharing state, sent whenever any of it changes; each field is absent when it has nothing to say. `in_use` is `{sessions, signing_in}`: how many of the account's sessions work under its directory — in the middle of a turn, idle with background work running, waiting for the user's answer (a permission prompt, a question, a session request, or a screen of its agent's that only the user can answer, such as Claude Code's "Resume this conversation?"), holding a message of Octoboard's that is queued, being written, or was left unsent in its input box (`message_not_sent`), or claimed (being started, resumed, switched or handed off, or having their parent changed) — and whether it is being signed in: what `account_in_use` refuses a change for. `idle_sessions` is how many of its sessions are idle — at their prompt waiting for the user's next message, with nothing running in the background — which a change ends and resumes. `progress` is `{share, phase, done, total, item?}` while a change runs: its direction; `phase`, one of `waiting` (another account of the same agent is changing), `stopping` (ending the idle sessions), `changing` (the entries) and `resuming` (resuming those sessions), in that order, `waiting` and `stopping` left out when there is nothing to wait for or stop and `resuming` when nothing was stopped; the entries processed out of `total`, none before `changing` and all of them once `resuming`; and `item`, the entry being processed (an item of the catalog or, turning sharing off, the agent's id of a conversation being taken back), present only while `changing`, from the first entry on, absent while the change waits for short CLI reads. `failure` is `{share, cause, items, detail}`, the last change that failed since the daemon started (`cause` and `items` as in `account_sharing_failed`, `items` as an array), cleared when the next change records its choice (one refused or stopped before that leaves it). An account the client has been told nothing of is free; see "Sharing the default configuration" below |
| `settings_updated` | `settings` — the whole `Settings` record, sent when `update_settings` actually changes it, and also whenever the account list changes (`create_account`, `update_account`, `delete_account`, `set_account_disabled`, `set_default_account_name`, `reorder_accounts`, or a console dialog's save that repoints or mints one), and whenever `Settings.added_agents` changes (`add_agent`, `remove_agent`, `reorder_agents`, or the one-time fill described with `Settings.added_agents` under "Records"), and when `set_codex_approval_reviewer` or `set_claude_approval_mode` changes its value |
| `console_upserted` / `project_upserted` / `session_upserted` | `console` / `project` / `session` — the whole record, under that key |
| `console_deleted` / `project_deleted` / `session_deleted` | `console` / `project` / `session` — the id of the record that was removed |
| `project_git_status` | `status` — one project's whole `GitStatus`, sent on every change, including every transition of `activity` (so an animated icon has something to follow) and every local re-read of the branch header (see "Daemon behaviour, per project"). Nothing is sent when a project is removed; the client drops its status along with it |
| `shell_terminal_upserted` | `shell_terminal` — the whole `ShellTerminal`, sent when one is created and whenever its title, `state` or `busy` changes |
| `shell_terminal_deleted` | `shell_terminal` — the id of a terminal that was closed, or ended with its project's removal |
| `shell_terminal_created` | `id`, `shell_terminal` — the reply to `create_shell_terminal`, the record of the terminal it started, also broadcast as `shell_terminal_upserted` |
| `session_notice` | `session`, `code`, `params`, `message` — something about a session the user has to be told that no status field carries: an injected capability that will not apply, a setting of theirs Octoboard had to work around, a message Octoboard accepted and could not deliver. Broadcast when it is found, which may be at launch or at any point in the session's life; nothing stores it, so a client that connects later does not see it. See "Coded messages" |
| `trust_prompt` | `session`, `agent`, `project`, `path`, `trust_dir` — a running session of a project is at its agent's folder-trust confirmation, which is asking whether `path` is trusted, and the project has no permission of its own (`trust_consent`) and its directory does not really lie under any of `trusted_directories` (see "Records"). `agent` is the one asking: Claude Code's workspace-trust screen, Codex's "Trust this folder?" (shown only inside a git repository, and recorded by Codex for the repository's root), or Grok Build's "Do you trust the contents of this directory?" (shown only where the folder holds content its trust gates, such as an `AGENTS.md`). `trust_dir` is the directory `confirm_trust` with `trust_parent_dir` would trust — the project's parent, as `path` spells it — or null when there is none to offer (it would be the filesystem root, the home directory or one containing it, the home directory cannot be determined, or `path` is not absolute), or when trusting it would not cover the project: `path` goes through a symbolic link that leads out of its parent, or the project's directory cannot be resolved within a few seconds; a client offers the choice only when it is not null. Broadcast once per confirmation, when it is recognised in the session's terminal output. A Codex or Grok Build confirmation also stops waiting once anything is typed into the session from a terminal, which is the person answering it there (Codex runs no hook after that); its prompt is then not sent again, and a `confirm_trust` for it gets `trust_not_waiting`. When the person answers Grok Build's in the terminal, the entry Grok writes is carried into the user's own trust store all the same, with no permission of Octoboard's recorded; an answer that writes no entry (a decline) changes nothing, and a failure to carry one is a `trust_not_carried_over` notice. From launch, and while a confirmation is waiting, messages Octoboard itself writes into the session are held for as long as `send_message` would refuse one with `session_trust_pending`. Each client is also sent one for every confirmation still waiting under the same condition, with the same fields, right after every `snapshot` (on connect and on lag recovery), so a client that missed the broadcast is still asked; a client already holding the prompt ignores the repeat. A client that declines ("Not now") drops the prompt locally, and a later `snapshot` may ask again. With no client connected the confirmation simply stays for the person to answer in the terminal. The client answers with `confirm_trust`, or leaves the confirmation alone. A console session's confirmation is pressed by the daemon without a prompt and without recording anything, because its working directory is the console's own; a project session's is when it meets the condition above the other way round. A client does not judge which prompts a trusted directory covers: that is the daemon's decision, made on the file system. It drops a queued prompt once a `session_upserted` shows that session with a status other than `waiting_user`, which is how a confirmation answered meanwhile — by Octoboard under a directory that has just become trusted, in the terminal, or by another client — leaves its queue, and when a `session_notice` with code `trust_answer_failed` arrives for that session, since the confirmation is then left to the person in the terminal. This relies on the daemon publishing the session as `waiting_user` before it broadcasts its prompt, and not broadcasting a prompt for a confirmation that is no longer waiting by then |
| `session_request` | `request_id`, `session`, `action`, `role`, `console`, `project`, `agent`, `target`, `owner`, `reopened_owners`, `account`, `conversation`, `branches`, `model?`, `effort?`, `requested_at` | A session called `request_session` or `request_resume_session`, or a parent called `switch_session_account` or `handoff_session` while `auto_approve_account_switches` (for a switch) or `auto_approve_handoffs` (for a handoff) is off, and its call is waiting for the user's answer. `session` is the requesting session, which may be a console session or a project session with or without a parent. `action` says what an approval does: `start` (a new session with no parent, from the requester's brief), `reopen` (the archived session that holds a conversation), `resume` (the interrupted session that holds it), `resume_record` (a new session with no parent, on an agent's own record of a conversation no session holds), `switch` (the parent's session `target` moved to another account of its agent) or `handoff` (the parent's session `target` handed off to a new session of another agent); a client that does not know a value does not offer the request, and the daemon's timeout ends the call. `role` is `project` or `console`, the kind of session it asked for, or of the session brought back, switched or handed off; `console` is the console that session would be in, and `project` the project it would be in, or null for a console session. `agent` is the agent it would run, or for `switch` the target's own. For `switch` and `handoff`, `target` is the session an approval acts on, which the requester owns, `account` the account asked for (a handoff's settled when the call was made; null for the agent's default account), and for `handoff` `model` and `effort` are what the requester asked for (absent when left out, null for the agent's own default); they are shown to the user, who may change the account, agent, model and effort. For `reopen` and `resume`, `target` is the session an approval brings back, `owner` the session it reports to (null when it reports to the user, and an approval leaves it so), and `reopened_owners` the archived parents an approval brings back with it, topmost first, as they stood when the request was made; for `resume_record`, `account` is the account that holds the record (null for the agent's default account) and `conversation` the conversation's id as the agent writes it. Each of these is null, or empty for the list, where the action does not use it. The request of a `start`, `reopen`, `resume` or `resume_record` carries nothing the requester wrote, and the name an archived session would be given is not sent. `branches` lists the branches an approval would hand to the new session, as wire paths below `refs/heads/`: branches the requesting session is responsible for in the project's repository, checked when it asked; empty when the request hands none over. `requested_at` is epoch ms and orders requests oldest first. Broadcast once when the call is made, and re-sent to a client after every `snapshot` (on connect and on lag recovery) for every request still waiting, oldest first, after the `trust_prompt`s. A client already holding it ignores the repeat |
| `session_request_closed` | `request_id`, `reason` | The request is no longer waiting, and every client closes its dialog. `reason` is `approved`, `refused`, `timed_out` or `withdrawn`. `approved` means the user approved; it is announced before the session is started, not after one was started. `withdrawn` means the requesting session's process ended, its call's connection to the daemon was dropped (including the agent cancelling the call over MCP), or a later request from the same session replaced it (see "How a request ends" in `docs/product/session-requests.md`) |
| `session_records_inspected` | `id`, `presence` — reply to `inspect_session_records`, with `presence` one of `present`, `absent`, `unknown` |
| `session_interrupted` | `id`, `session`, `interrupted`, `status`, `note` — reply to `interrupt_session`. `interrupted` is `true` when the key was written and the turn ended within the wait (the session may already be `working` on a message its agent had queued); `status` is the session's status by then, and `note` says in prose what the caller is to know: that there was nothing to interrupt, that no key was sent because the terminal does not show a turn running, which keys went in when not all did, that the session still reads `working`, that a draft in Grok's composer was cleared, what may have become of the messages Octoboard wrote into the session during the cancelled turn, with their approximate count (Grok Build keeps them queued and runs them next as a new turn; Claude Code may have taken some into the cancelled turn and runs the rest next; Codex steered them into the turn the cancel threw away, so unless it had acted on them they are to be sent again), and when the input box still holds text afterwards — a prompt the agent put back that could not be cleared, or text that was there before the keys |
| `session_history` | `id`, `session`, `messages`, `unavailable?` — reply to `read_session_history`. `messages` is the conversation in order, each `{speaker, text, turn, at?}`: `speaker` is `user` (typed by the user), `octoboard` (written into the session by Octoboard: a brief, a report, shared information, a "Message from Octoboard" notice) or `agent` (the agent's final reply to its turn). `turn` counts from 1; a `user` or `octoboard` message opens a turn and the `agent` message after it shares its number, and a turn with no reply has no `agent` message. `at` is milliseconds since the epoch when the record dated the line. Tool calls, reasoning, a turn's earlier replies, slash-command expansions and interruption notes are left out; a slash command the user invoked is a `user` message of the command as typed (`/name args`). `unavailable` is `true` (and `messages` empty) when the agent's record is not found or cannot be read; absent otherwise, so an empty `messages` without it is a session nothing was said in (including one that has had no conversation) |
| `session_branches_inspected` | `id`, `items`, `processes?` — reply to `inspect_session_branches`, a list of `SessionBranchItem` (see "Branches and worktrees bound to a session"); `processes`, a list of `ProcessTree` (see "Process trees"), is present only when the request asked for them and there are some |
| `processes_listed` | `id`, `trees` — reply to `list_processes`, a list of `ProcessTree` |
| `processes_stopped` | `id`, `ids` — reply to `stop_processes`: the ids of the trees it stopped |
| `processes_kept` | `id`, `ids` — reply to `keep_processes`: the ids of the trees it kept |
| `project_removal_inspected` | `id`, `items`, `processes`, `inspection?` — reply to `inspect_project_removal`: `processes`, a list of `ProcessTree`, the project's processes its removal stops; `items`, a list of `SessionBranchItem`, the worktrees it held back among them, and the token they are kept under, when it held any back |
| `session_opened` | `id`, `session` — the reply to `open_session`, to an approving `answer_session_request` and to `hand_off_session`, naming the session it started; for `open_session` it is `starting` (see there) |
| `agent_detected` | `id`, `agent` — the reply to `detect_directory_agent` and `probe_git_remote`; `agent` is an agent name or null |
| `conversation_located` | `id`, `location` — the reply to `locate_conversation_to_resume`; `location` is tagged by `kind`: `archived_session` and `live_session` (with `session`, the id of the Octoboard session that records the conversation), `outside_project` (with `session`, the id of the session outside the project that records the conversation), `record` (with `agent` and `account`, an account id or null for the agent's default account) or `not_found` |
| `agent_models` | `id`, `agent`, `account`, `models`, `unavailable` — the reply to `list_agent_models`; `agent` and `account` echo the request. `models` is `[{id, name, efforts}]` and `unavailable` is null or a coded reason (`{code, params, message}`, worded as an `error` is); see "Agent models" below |
| `dir_listing` | `id`, `path`, `entries`: `[{name, path, is_git_repo}]` — only directories are listed |
| `project_source` | `id`, `source` — the reply to `get_project_source`; `source` is a `ProjectSourceInfo` (see "Browsing a project") |
| `project_dir` | `id`, `project`, `worktree`, `path`, `root_id`, `entries`, `complete` — the reply to `list_project_dir`. `project`, `worktree` and `path` echo the request; `root_id` is the identity of the directory the listing was scoped to; `entries` are `BrowseEntry` records in byte order of their names' wire forms; `complete` is false when the listing was cut at a budget or an entry could not be read |
| `project_file_matches` | `id`, `project`, `query`, `paths`, `ignored?`, `complete` — the reply to `search_project_files`. `project` and `query` echo the request; `paths` are canonical wire paths relative to the project, sorted in byte order; `ignored` (present only when not empty) are the `paths` Git ignores, which only a search with `include_ignored` can have. `complete` is false when traversal was cut short or an entry or subtree could not be read |
| `project_file` | `id`, `project`, `worktree`, `path`, `source`, `file` — the reply to `read_project_file`. `project`, `worktree` and `path` echo the request; `source` is the `ContentSource` the body was read from and `file` the `FileContent` |
| `project_changes` | `id`, `project`, `worktree`, `head`, `changes`, `complete` — the reply to `list_project_changes`. `project` and `worktree` echo the request; `head` is the commit the staged changes are against, null before the first commit; `changes` are `ChangeEntry` records in the order `git status` gives them; `complete` is false when the list was cut at a budget |
| `project_change` | `id`, `project`, `worktree`, `group`, `head`, `old`, `new`, `patch` — the reply to `read_project_change`. `project`, `worktree` and `group` echo the request; `head` is the commit a staged change was read against (null before the first commit, and for the other groups); `old` and `new` are `SideRead` records; `patch` is a `FileContent` holding the change's unified patch as `git` writes it, null when none is made |
| `project_branches` | `id`, `project`, `branches`, `complete` — the reply to `list_project_branches`. `branches` are `BranchInfo` records in byte order of their names; `complete` is false when the list was cut at a budget; `default_branch` is an `UpstreamInfo`, absent when there is none to give (see "Comparing two branches"); `upstreams_complete` is false, and otherwise absent, when the upstream lookup was past a budget and the main working copy's branch may lack its `upstream`, or the list its `default_branch`, as a result |
| `project_comparison` | `id`, `project`, `left`, `right`, `base`, `changes`, `complete` — the reply to `compare_project_branches`. `left` and `right` are `ComparisonEndpoint` records: each branch as the request named it (with `remote: true` for a remote-tracking `left`, and `empty: true` for a `left` that is the empty tree) and the commit it was resolved to; `base` is the full id of the two commits' merge base (the commit itself when both are at one); `changes` are `ChangeEntry` records of the `committed` group, in the order `git` gives them; `complete` is false when the list was cut at a budget |
| `project_comparison_change` | `id`, `project`, `left`, `right`, `base`, `old`, `new`, `patch` — the reply to `read_project_comparison_change`. `left`, `right` and `base` echo the request; the old side was read from `base` and the new side from `right`'s commit; `old`, `new` and `patch` are as in `project_change` |
| `project_change_bodies` | `id`, `project`, `worktree`, `group`, `old`, `new` — the reply to `read_project_change_bodies`. `project`, `worktree` and `group` echo the request; `old` and `new` are `SideRead` records, each `present` with its whole body in `file` or `absent`, never `out_of_scope` |
| `project_comparison_change_bodies` | `id`, `project`, `left`, `right`, `base`, `old`, `new` — the reply to `read_project_comparison_change_bodies`. `left`, `right` and `base` echo the request; `old` and `new` are as in `project_change_bodies` |
| `project_commits` | `id`, `project`, `base`, `tip`, `commits`, `complete`, `commits_complete` — the reply to `list_project_commits`. `base` and `tip` echo the request; `commits` are `CommitChanges` records, newest first; `complete` is false when their changes were cut at a budget, and `commits_complete` when there were more commits than are listed |
| `project_commit_change` | `id`, `project`, `commit`, `parent`, `old`, `new`, `patch` — the reply to `read_project_commit_change`. `commit` and `parent` echo the request; the old side was read from `parent` (the empty tree for none) and the new side from `commit`; `old`, `new` and `patch` are as in `project_change` |
| `project_commit_change_bodies` | `id`, `project`, `commit`, `parent`, `old`, `new` — the reply to `read_project_commit_change_bodies`. `commit` and `parent` echo the request; `old` and `new` are as in `project_change_bodies` |
| `configuration_exported` | `id`, `document` — the reply to `export_configuration`; `document` is a configuration document (see "Configuration export") |
| `configuration_inspected` | `id`, `settings?`, `accounts?`, `consoles` — the reply to `inspect_configuration`; `settings` is what the document's application settings are and what they are on this machine now; `accounts` is what importing the document's accounts would do (present when the document carries them); one `InspectedConsole` per console of the document, in its order (see "Configuration import") |
| `configuration_imported` | `id`, `settings_applied?`, `settings_error?`, `accounts?`, `consoles` — the reply to `import_configuration`; `settings_applied` holds the settings that were applied, and is absent when they were not asked for or could not be applied, in which case `settings_error` says why; `accounts` is what came of the accounts and is present only when `apply_accounts` was sent; one `ImportedConsole` per console of the document, in its order (see "Configuration import") |
| `page_list` | `id`, `console_session_id`, `pages` — oldest first. Pages are not in `snapshot`: one carries a whole HTML document, and only the session on screen needs its pages, so the panel asks (and so does the window for a project session it has selected, to know whether the Report panel has anything to show). Asking again after every `snapshot` is what keeps it correct across a `page_created` a lagging client never received: such a client is sent a fresh snapshot in place of the events it missed, on the socket it already has |
| `path_links` | `id`, `links` — one `{path, kind, view?, viewer_only?}` per asked path, in order, `path` as asked, `kind` `file`, `directory` or `none`, `view` (`{worktree?, path}`) and `viewer_only` (present only when true) as `probe_path_links` describes |
| `page_created` | `page` — the whole record. A session pushed a page with `show_page` |
| `pages_deleted` | `console_session_id` (the session that pushed them), `pages` — the ids withdrawn with `delete_pages`, or by the daemon when the session took the worker role (see "MCP tools"), never empty. A client drops them from its list for that session, and applies it idempotently: it may name an id the client does not hold, one it already dropped or one whose `page_created` has not arrived yet (a later `pages_deleted` then drops that one) |
| `ack` | `id` |
| `error` | `code`, `params`, `message`, `id?` — a failure of a request (carrying its `id`) or, with no `id`, one that belongs to no request. See "Coded messages" |

### Coded messages

Everything the daemon sends for the user to read — every `error` and every `session_notice` — carries a stable `code`
and `params`, an object of named string values. A client words the message from them in its own language; the daemon
never translates. `message` is the same text in English: it is what a client shows for a code it does not know (a
daemon newer than the client), so it must always be shown whole, never parsed. A code never changes its meaning or its
params; a new reading gets a new code.

A param named `console`, `project`, `session`, `owner` (a session) or `shell_terminal` is that record's id, and a
client shows the record's current name instead (the raw value only when it does not know the record). A param named
`count` is a number, sent as text, which a client uses to pick the singular or plural wording. Every other param is
text to be shown as is: a path, an agent's name, an operating-system or `git` message that cannot be translated.

A failure that is not meant to be read in detail — an unexpected one with no meaning of its own — is `internal_error`
with the text as its `detail`.

A client also branches on some codes, instead of only showing them:

- `session_already_running` and `session_already_starting` both mean a launch was refused because the session is
  already running or being started. A client's own double click produces them and is not worth showing.
- `trust_directory_too_broad`, `trust_path_not_absolute` and `trust_home_unknown` mean no parent directory can be
  offered to trust, and `trust_directory_unreachable` that it could not be checked in time: in all four, nothing was
  answered and the dialog stays open.
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
| `unknown_console` / `unknown_project` / `unknown_session` / `unknown_page` / `unknown_shell_terminal` | `console` / `project` / `session` / `page` / `shell_terminal` | The record is not there |
| `field_required` | `field` | A request lacks a field its kind needs, named as on the wire |
| `field_too_long` | `field`, `limit` | A project name or description exceeds its character limit after trimming; `limit` is the decimal character count |
| `conflicting_fields` | `first`, `second` | A request names two fields that exclude each other, as on the wire |
| `invalid_launch_setting` | `agent`, `field`, `value` | A model or effort (`field` is `model` or `effort`) the agent does not offer, in `update_console`, `update_project`, `add_project` or `open_session` |
| `invalid_remote_url` | `url` | Raised by the console session's `add_project` tool only, so no client receives it: a `git` URL that is not an http(s), ssh or git remote, or carries credentials; `url` has any credential removed |
| `console_has_running_sessions` / `project_has_running_sessions` | — | Deleting needs the sessions archived first |
| `project_directory_shared` | `project` | Moving a deleted project's directory to the Trash would take another project's files or repository, or might, since that project's path could not be resolved (an I/O error other than a refused permission, no inode, no answer in time); `project` is that project's name |
| `project_directory_protected` | `path` | The directory is the home directory, the root of a disk, or holds Octoboard's data; the project is kept |
| `project_directory_trash_failed` | `path`, `detail`, `cause?` | The directory could not be moved to the Trash, or it, the home directory or Octoboard's data directory could not be resolved (the directory itself unreadable, an I/O error, no inode, no answer in time) to tell what moving it would take along; the project is kept. `cause` is `unanswered` where one of them gave no answer in time, `unreadable` where permission to read the directory itself (or what a link there leads to) was refused, and absent otherwise (another I/O error, a file system that reports no inode) |
| `session_has_running_sessions` | `count`, `sessions`, `session_ids` | Archiving a session while sessions under it (bound to it, or to a project coordinator bound to it) have a process running, whatever their status; `count` is how many, at both levels, and `sessions` their titles, comma separated as plain fallback text. `session_ids` is a JSON array of their IDs, encoded as a string like every param, so clients can resolve current names and render each separately without parsing user-provided titles |
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
| `grok_sandbox_unsupported` | `source`, `profile` | A Grok Build launch would run under Grok's sandbox, which the per-session home cannot support (the sandbox lets Grok write only to its home, the working directory and temp directories, with links resolved, and the home's `sessions/` and `auth.json` are links back into the source home); refused at launch before the per-session home is built. `profile` is the non-`off` profile and `source` what sets it: `GROK_SANDBOX` in the launch environment, or the file (the system directory's `requirements.toml`, a resumed session's `summary.json`, the home's `config.toml`, the system directory's `managed_config.toml`) it is read from. macOS managed preferences are not read |
| `unknown_account` | `account` | No account has this id, or the account belongs to another agent than the field naming it (a console's or project's per-agent reference, `open_session`'s `account`) |
| `account_name_taken` | `agent`, `name` | A requested account name collides with an existing one of the same agent (trimmed, case-insensitive) — including the default account's own name, the word `Default` or the custom name the user gave it (`set_default_account_name` is refused for an owned account's name by the same code); `name` is the name of the account it collides with |
| `login_already_running` | `agent` | `start_account_login` for an account that has a login running |
| `login_codex_busy` | — | `start_account_login` for a Codex account while a Codex login runs for any account: Codex's browser flow holds one fixed local port |
| `login_not_running` | `agent` | `cancel_account_login` for an account with no login running |
| `config_dir_taken` | `agent`, `name` | A requested account config directory, once normalised, equals that of another account of the same agent (symlinks not followed; case folded on macOS) — including the default account's directory; `name` is the name of the account using it |
| `agent_not_installed` | `agent` | `add_agent`'s agent has been determined unavailable (its binary does not resolve on the login shell's `PATH`) |
| `agent_already_added` | `agent` | `add_agent`'s agent is on `Settings.added_agents` already |
| `agent_availability_pending` | `agent` | `add_agent` was asked before which agents are installed is determined. Once `agent_availability_updated` has landed it is no longer refused for this reason; a run whose snapshot failed refuses until the next start |
| `account_order_invalid` | — | `reorder_accounts` did not name every owned account of the agent and its default exactly once; the account list may have changed since the client read it |
| `agent_order_invalid` | — | `reorder_agents` named agents that are not exactly the added ones, each once |
| `added_agents_undecided` | — | `reorder_agents` or `remove_agent` was asked while the list is undecided (availability not determined); no longer refused for this reason once it is, and a run whose snapshot failed refuses until the next start |
| `configuration_not_recognised` | — | The `format` of a configuration document is absent or is not `octoboard-configuration` |
| `configuration_version_unsupported` | `version`, `supported` | A configuration document's `version` is newer than the newest this daemon reads, `supported`, or is 0 |
| `configuration_unreadable` | `detail` | A configuration document is not a JSON object, has a `version` that is not a whole number, or cannot be read as a document of its version |
| `import_resolution_invalid` | `console`, `project` or `agent`, `account` | A resolution of `import_configuration` names a project the document does not have, resolves one twice, or chooses a `directory` that is neither an absolute path nor starts with `~/`; `console` and `project` are the indexes it gave. An account resolution (`account_resolutions`) that names an account the document lacks, resolves one twice, gives both or neither of `directory` and `leave_out`, or gives a `directory` that is neither an absolute path nor starts with `~/` is refused the same way, with `agent` (its display name, such as `Claude Code`) and `account` (the index it gave) in place of the two |
| `import_settings_missing` | — | `import_configuration` asked for the document's application settings and it carries none |
| `import_accounts_missing` | — | `import_configuration` asked for the document's accounts (`apply_accounts`) and it carries none |
| `export_empty` | — | `export_configuration` selected nothing: `consoles` is `none` and neither the settings nor the accounts are included |
| `import_choice_unavailable` | `choice` | Reported on a project of `configuration_imported`, not as a reply: the choice made for it cannot be used because the document gives it nothing to use for it (a path that is not absolute, a remote URL that may not be cloned, or none) |
| `icon_not_an_image` | `console_name?` | A console's icon is not a `data:image/` URL. `console_name` names the console when the refusal is of an imported configuration (`import_configuration`), which may carry several |
| `icon_too_large` | `limit_kib`, `console_name?` | A console's icon is longer than the limit, 256 KiB. `console_name` as for `icon_not_an_image` |
| `session_already_running` | `session` | The session is running already, or is not interrupted or archived |
| `session_already_starting` | `session` | The session is being started already |
| `session_already_on_account` | `session` | `switch_session_account` named the account the session is on |
| `session_archived` | `session` | `switch_session_account`, `set_session_model`, `hand_off_session` or `set_session_locked` with `locked: true` for an archived session; reopening one is `resume_session`'s job |
| `session_working` | `session` | `hand_off_session` for a session in the middle of a turn, or still starting; `handover_branches` and `handover_processes` for a `from` that is another session and is working, starting or waiting for the user |
| `handoff_to_same_agent` | `session`, `agent` | `hand_off_session` to the agent the session already runs on; moving to another account of it is `switch_session_account` |
| `conversation_not_found` | `agent`, `path` | `switch_session_account` found no conversation record of the session in the directory of the account it is on (`path`) |
| `conversation_unresumable` | `agent`, `session` | `resume_session` (and the `reopen_session` tool) found that the agent no longer has the conversation record of the session, so the launch could only fail: Claude Code's `<id>.jsonl`, or a Codex rollout, is gone from the session's own account. Nothing is reopened and every session keeps its status and `ended_at`. A record that cannot be proven absent (an unresolved default account, an account directory that cannot be read or has no records directory, a linked records directory, a Grok Build session folder missing locally) launches as before. Not raised for an archived parent, which is relaunched into a fresh conversation, nor for a session with no conversation (`has_conversation` false) |
| `codex_unarchive_failed` | `detail` | Resuming a Codex session whose rollout is in the account's `archived_sessions/` first runs `codex unarchive` there, and that failed; `detail` is its own message. The session is left as it was |
| `conversation_to_resume_not_found` | `agent`, `path`, `id` | `open_session`'s `resume_agent_session_id` has no conversation record in the directory of the account the session will use (`path`) |
| `resume_id_invalid` | `id` | `open_session`'s `resume_agent_session_id` is not a UUID |
| `resume_for_console_session` | — | `open_session` was given a `resume_agent_session_id` without a `project_id` |
| `conversation_in_session` | `id`, `session` | `open_session`'s `resume_agent_session_id` is already the agent's id of a session of the same project, named by its id in `session` |
| `conversation_outside_project` | `id`, `session` | `open_session`'s `resume_agent_session_id` is already the agent's id of a session outside the project: of another project, or a console session; `session` is the id of that holder |
| `relocation_failed` | `path`, `stage`, `cause`, `detail`, `kept_in?` | Moving the conversation into the target account's directory (`path`) did not complete. `stage` says where: `copy` (the items could not be built or put in place; nothing of the target changed), `thread_record` (the conversation was copied, but the target's Codex thread database could not be updated) or `kept_staging` (the copy failed and an older item could not be put back: it is kept in `kept_in` until the daemon next starts or the session is next switched into that account). `cause` says why in a word a client can word a remedy for: `permission_denied` (not allowed to read or write a file or directory, or a read-only volume), `busy` (a file in use, or a database locked for longer than the 5 seconds waited), `no_space` (the volume or a quota is full), `config_unreadable` (the target's `config.toml` cannot be read or is not valid TOML), `sqlite_home` (where the target's thread database is cannot be told: a relative `CODEX_SQLITE_HOME`, or a `sqlite_home` starting with `~` with no `HOME` in the launch environment), `symlink` (a record to copy holds a symbolic link, which is never copied), `not_restored` (`kept_staging`: an older item could not be put back) or `other`. `detail` is the system's own text of the cause alone |
| `default_account_not_shareable` | `account?`, `name?` | `set_account_sharing` for the default account (no params), or turning sharing on for an account whose directory resolves to the default account's (`account`, `name`); nothing was recorded |
| `account_sharing_failed` | `account`, `name`, `cause`, `items`, `detail` | An account's change of sharing did not complete — answering `set_account_sharing`, or refusing a `delete_account`, or an `update_account` pointing it at another directory, that turned sharing off first. Nothing was rolled back: the account stays `share_default_config_incomplete` until the user asks again for the recorded value or for the opposite one. `items` is a JSON array of `{item, left, cause, detail}`, encoded as a string: each entry of the account's directory that was not brought in line, what it was left as (`unchanged`, `backed_up` — renamed to its backup with nothing linked in its place, `partly_moved` — some of its conversation history in the default directory and the rest in the account's, `linked`, `absent`), why in a word a client can explain in plain language (`cause`, always sent), and why in the system's own text (`detail`); empty when the operation could not start at all (`detail` says why). A `cause` is `permission_denied` (not allowed to change a file or directory, or a read-only volume), `busy` (a file in use by another process, or a database locked by one), `not_found` (a file or directory gone), `no_space` (the volume or a quota full, or a file too large), `conflict` (something in the way or not what it has to be: an entry already there, a link leading elsewhere, a directory where a file is expected or the other way round), `default_moved` (the agent's default directory is no longer the one the links lead into or sharing was started into: turn sharing off, then on again, to share the new one) or `other`. The `cause` param says the same of the change as a whole: the cause its entries share (`other` when they differ), or why it could not start when there are none. An entry about a conversation that turning sharing off could not take back names the agent's id of it as `item` and adds `sessions`, the ids of the sessions recording it; `unchanged` there means it is still in the default directory |
| `account_still_sharing` | `account`, `name` | `delete_account`, or `update_account` pointing it at another directory, for an account that shares the default configuration, or whose last change of it is incomplete, while sessions of it work under its directory (as `account_sharing`'s `in_use` counts them), one of its idle sessions turned working before turning sharing off ended it, or it is being signed in; its sharing has to be turned off first. Nothing was stopped or changed |
| `account_in_use` | `account`, `name`, `sessions`, `signing_in` | `set_account_sharing` while `sessions` (a count) of the account's sessions work under its directory, as `account_sharing`'s `in_use` counts them, or one of its idle sessions turned working before the change ended it, or while it is being signed in (`signing_in`, `true` or `false`). Nothing was stopped or recorded |
| `sharing_session_did_not_stop` | `account`, `name`, `session` | A `set_account_sharing`, or a `delete_account` or `update_account` repointing the account, ended one of the account's idle sessions (`session`) and still did not see it gone after a generous wait: the change did not start and nothing was recorded; the sessions it ended are resumed, and `session` is told as `sharing_resume_failed` unless it ends meanwhile |
| `account_sharing_in_progress` | `account`, `name` | A change of the account's sharing is running: refusing `open_session` for it, a launch or resume of one of its sessions, `switch_session_account` from or to it (normally before the session's process is ended; when the change locked the target just after the switch's last check, the relaunch is refused instead and the session is left `interrupted` on its source account, with nothing lost), a handoff from it, a second `set_account_sharing` for it, and `delete_account` or `update_account` repointing it |
| `session_did_not_stop` | `session` | `switch_session_account` ended the session's process and still did not see it gone after a generous wait; nothing was copied or recorded, and the process may still be running |
| `switch_did_not_come_up` | `session` | The relaunched process ended within moments, so the switch is reported as failed and the session stays on its previous account |
| `model_change_did_not_stop` | `session` | `set_session_model` ended the session's process to restart it and still did not see it gone after a generous wait; nothing was recorded, and the process may still be running |
| `model_change_did_not_come_up` | `session` | The process `set_session_model` relaunched ended within moments, so the change is reported as failed and the session is left `interrupted` with the model and effort it had |
| `session_not_deletable` | `session` | Only an archived session, or one never used, can be deleted, and not while it is starting |
| `session_not_running` | `session` | A message or a go-ahead for a session with no running process; an `interrupt_session` for one that is archived or interrupted, has no process, or is still being launched or not yet past its startup screens; a `list_processes` for a session with no process |
| `console_session_never_bound` | `session` | `release_session` for a console session, which is always independent |
| `session_not_bound` | `session` | `release_session` for a session that is not bound to any parent |
| `session_not_bound_to_owner` | `session`, `owner` | `release_session` by a parent (the MCP tool) for a session bound to another parent |
| `session_binding_changed` | `session` | `release_session` found the session's binding changed between checking it and writing the release; nothing was released |
| `session_changing` | `session` | A change of the session's parent (`bind_to_session`, `transfer_session`, `release_session`) is refused because the session itself is being started, resumed, handed off, switched to another account, having its branches handed over or having its parent changed; nothing was changed |
| `session_owner_changing` | `session`, `owner` | A change of the session's parent is refused because the parent it gains or loses the session for is being started, resumed, handed off, switched to another account or having its parent changed; nothing was changed |
| `session_waiting_for_user` | `session` | The session waits at a prompt only the user can answer, so a message is refused; an `interrupt_session` is refused for the same, and while a `request_session` call of the session's own holds its hand up |
| `session_trust_pending` | `session` | The session's agent is showing, or may still show, its folder-trust confirmation, which the message's trailing Enter would answer, so a message is refused; an `interrupt_session` is refused while the confirmation is up |
| `session_busy` | `session` | An `interrupt_session` for a session that is being switched to another account, handed off, having its branches handed over, given another parent, or restarted for a change of its account's sharing; nothing was sent |
| `queued_messages_lost` | — | A message queued for a session could not be written in full, and it and what was behind it were dropped |
| `page_not_current` | — | A form can be submitted only from its session's newest page |
| `trust_directory_too_broad` | `path` | The directory is the filesystem root, the home directory or one containing it |
| `trust_path_not_absolute` | `path` | The project's path is not absolute |
| `trust_home_unknown` | — | The home directory cannot be determined |
| `trust_directory_unreachable` | `path` | The project at `path`, or the folder above it, did not respond in time, so that folder could not be checked against the home directory |
| `trust_not_waiting` | — | The trust confirmation is no longer waiting for an answer |
| `trust_answer_failed` | `agent`, `reason`, `reason_code`, `detail?` | Octoboard accepted the go-ahead but could not press the agent's trust confirmation; the user answers it in the terminal. See the reason codes below |
| `trust_not_carried_over` | `agent`, `path`, `reason`, `reason_code`, `detail?` | Grok Build's confirmation was pressed, but no trust entry for the folder reached the user's own store at `path`; `reason_code` says why (see the carry codes below). Nothing was written there and no permission was recorded, so Grok asks again on its next launch |
| `console_session_trust_not_asked` | — | A go-ahead for a console session's trust confirmation, which Octoboard presses without asking |
| `session_request_not_waiting` | `session` | An answer to a `session_request` whose session is no longer waiting for it (withdrawn, timed out, or its process ended before it could be carried out), so its request was not carried out |
| `session_request_answered` | — | The request has already been answered, from this client or another, or is unknown; nothing changed |
| `binary_not_found` | `binary` | The agent's binary (or `git`) is not on the shell's `PATH` |
| `shell_environment_timeout` | `shell`, `command`, `timeout` | The login shell did not finish printing its environment in time |
| `agent_not_available` | `agent` | `open_session`'s resolved agent has been determined unavailable (its binary does not resolve on the login shell's `PATH`); never raised while that determination is still pending. The console session's own `new_session` tool is refused the same way, as a tool error carrying this same text |
| `agent_not_added` | `agent` | `open_session`'s resolved agent is not on the added agents list (never raised while the list is undecided); the message points to Settings → Agent accounts. `hand_off_session` and the console session's `new_session` and `request_session` tools are refused the same way |
| `account_disabled` | `agent`, `account` | The account a session would open on, be handed off to, or be switched to is turned off (`set_account_disabled`); `account` is its name. Raised for every origin, the user's own request included, wherever `account_quota_exhausted` is, and checked before it. Sessions already on the account, and resuming, restarting or reopening one, are never refused for this |
| `account_quota_exhausted` | `agent`, `account`, `window`, `window_kind`, `window_minutes?`, `model_weekly?`, `resets_at?` | The account a session would open on, or be switched to, has its plan quota used up, going by the reading the daemon already holds: a session, weekly or credit-period window at 100% or more whose reset time has not passed or is unknown. `account` is the account's name, `window` its English name, the same for every agent (`5-hour`, `Weekly`, `Monthly`, `Credits`, or a window's own label when it carries one), `window_kind` its kind (`session`, `weekly` or `credit_period`), `window_minutes` the window's length when known, `model_weekly` `true` when the same reading also has a model's own weekly window (absent otherwise), and `resets_at` the time it resets in milliseconds since the epoch, when known; with several windows used up, the one that resets last. A client words the window from `window_kind`, `window_minutes` and `model_weekly` as the agent's own interface names it (Claude Code's session window "session", Codex's "5h", Claude Code's weekly window "weekly for all models" beside a model's own), and from `window` alone when they are absent; the `message` names it the same way in English. A model's own weekly window never counts, and an account with no reading, or one taken under another directory, is never refused. Raised for every origin, the user's own request included, wherever `open_session` is refused for an agent or account (`hand_off_session`, the console session's `new_session` and `request_session` tools), and for `switch_session_account` |
| `account_not_signed_in` | `agent`, `account` | The account a session would open on, be handed off to, or be switched to is not logged in, going by the quota reading the daemon already holds: its last reading says `quota_not_signed_in` or `quota_config_dir_missing`. `account` is its name. A reading that failed for another reason (a network error, a timeout) never refuses, nor does an account not read yet. Raised for every origin, the user's own request included, wherever `account_quota_exhausted` is. Sessions already on the account, and resuming, restarting or reopening one, are never refused for this |
| `daemon_shutting_down` | none | The daemon has begun to shut down, so no session is launched, resumed, switched or handed off, and no claim on one is granted. Nothing started is left running: a launch past its claim is ended as it is registered |
| `invalid_path` | `path` | A browse path is not a canonical wire path relative to the project (see "Wire paths"), or is too long for the operating system |
| `source_unavailable` | `path`, `cause`, `detail` | The directory a browse request is scoped to — the project's own, or its place in another worktree — does not exist, is not a directory, or (in another worktree) is reached through a symbolic link; `path` is that directory's absolute path. `cause` says which, for a client to word its remedy by: `missing`, `not_a_directory`, `link`, or `other` for any other failure to read it, which only `detail` names; a client treats a cause it does not know as `other` |
| `file_not_found` | `path`, `root?` | Nothing is at the path in that source: no file on disk (a link leading nowhere or round in a loop included), no entry in the index or the commit; for `open_path_link`, nothing is at the path as given (or a file is where a folder on the way should be) inside a root `probe_path_links` reads, and `root` says which kind: `project` (the project's directory or one of its worktrees, even where that lies under a temp root) or `temporary` |
| `permission_denied` | `path`, `detail` | The operating system refused to read the path or a directory on the way to it. `path` is absolute when the directory a request is scoped to cannot be read, and otherwise the request's own |
| `outside_scope` | `path`, `action?` | The path, with its symbolic links followed, leads outside the project's directory (or the worktree), and is not read; or, for `open_path_link`, it is outside every root of the rules of `probe_path_links` (spelled outside them, or resolving outside them; one missing inside a root is `file_not_found`, one refused for another reason is `path_link_refused`), and then `action` is `open` |
| `path_link_refused` | `path`, `cause`, `detail?` (only with `unreadable`) | `open_path_link` will not open `path` (as given) from a link, though it is not outside every root nor gone. `cause` is `ambiguous` (a relative path found in more than one of the project's roots), `launchable` (a file the system would run, as `viewer_only` says), `not_private` (in a temp root, but not solely the daemon user's or writable by group or others), `not_a_file_or_folder` (a socket, a device or the like), `unreadable` (an I/O failure looking at it other than a denied read, with `detail` holding the system's text) or `other` (a path that cannot be looked up: empty, containing NUL, or `~/` with no known home). A client treats a `cause` it does not know as `other`. A read the system denied is `permission_denied` instead |
| `unsupported_file_type` | `path`, `file_type` | The path is not a regular file: `file_type` is `directory`, `file` (listing a file), `symlink` and `submodule` (in the index or a commit), `unmerged` (an index entry with conflicting stages, read from the index or as a change), `fifo`, `socket`, `device` or `other`. A FIFO, socket or device is never opened |
| `source_changed` | `path` | What was being read changed while it was read — the file, the directory being listed, or a change's index entry or file on disk while its patch was made — or a side whose version is no longer the one a body request named, or a symbolic link appeared on its resolved path; nothing that was read is returned |
| `limit_exceeded` | `limit`, `max`, `size?` | A browse budget was reached: `limit` is `file_bytes` (`size` the file's size, when known), `git_output`, `reply_bytes`, `pending_requests`, `patch_bytes` or `probed_paths` (more than 64 paths in one `probe_path_links`); `max` is the budget. See "Browse budgets" |
| `restore_not_on_branch` | — | Restoring changes was refused because HEAD is detached |
| `no_commits_to_restore` | — | Restoring changes was refused because the branch has no commit yet |
| `git_operation_in_progress` | — | Finish or abort the merge, rebase, cherry-pick, revert or bisect before restoring changes |
| `git_restore_failed` | `detail` | Git could not inspect or restore the project's tracked changes; `detail` is Git's message or the invocation failure |
| `session_branches_not_removed` | `count`, `items` | Broadcast as an `error` with no `id` after a client's `archive_session`, `delete_session`, `delete_archived_sessions` or `delete_project` with `trash_directory`, once the removal that follows it has run: `items` is a JSON list with one `{kind, name, branch, state, uncommitted, detail, cause}` per branch or worktree that was to go and is still there, as `archive_session` says which: `name` is its path or branch name as text, `state` a `SessionBranchItem` state saying why, `uncommitted` whether a worktree has uncommitted changes, `detail` the reason given for a `failed` one, and `cause` the `SessionBranchItem` `cause` of a `failed` one, null otherwise. The archive or deletion stands. `message` carries the same in English |
| `project_branches_not_determined` | `name`, `detail` | Broadcast as an `error` with no `id` after a `delete_project` with `trash_directory`, when what goes along with the folder could not be determined before it went: the worktrees outside the folder were left, and the branches too where the repository outlives the folder (where it held the repository, the branches went with it). The removal stands. `name` is the removed project's name, as it is no longer on record; `detail` is the reason |
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
| `handoff_messages_dropped` | — | A message held back during a `hand_off_session`, or forwarded to the session that took the original's place, could not be delivered and was dropped: the session it was queued for has no process to receive it, or the forward failed. `session` is that session (the original, or the new one). A queue dropped while held back or with no process to receive it was never part-written into a terminal, so this notice says nothing of an input line; a forward that fails because the queue was lost raises `queued_messages_dropped` only |
| `sharing_messages_dropped` | — | A message held back for a session while a change of its account's sharing restarted it was dropped because the session was not relaunched (so it has no process to receive it). `session` is that session. It was never part-written into a terminal, so this notice says nothing of an input line; a session whose relaunch failed is told by `sharing_resume_failed` instead |
| `trust_answer_failed` | `agent`, `reason`, `reason_code?`, `detail?` | Octoboard could not press the agent's trust confirmation, so the user answers it in the terminal; `agent` names the agent, `reason` is the daemon's English account of why and `reason_code`, when the failure has one, names it (the `error` of the same code carries the same params) |
| `trust_not_carried_over` | `agent`, `path`, `reason`, `reason_code`, `detail?` | As the `error` of the same code: Grok Build's trust entry did not reach the user's own store, after Octoboard's press or after the person answered Grok's confirmation in the terminal (an answer that writes no entry, a decline, is not reported) |
| `message_not_sent` | `agent` | A message Octoboard wrote into the session while its agent was at its prompt was not reported sent within a few seconds, so it is taken to be waiting in the session's input box; nothing more is written into the session until its agent next reports a prompt sent. Checked for Claude Code only, and only for a message written while the session is idle; `agent` names the agent |
| `message_discarded_by_dialog` | `agent` | A message Octoboard wrote into the session was thrown away by a dialog its agent raised as the message went in, such as Claude Code's "Resume this conversation?" at a launch, "Resume from summary", or a notice that waits for Enter; checked only for a message written while the session is idle, as for `message_not_sent`. Nothing of it is in the input box: it is back at the front of the session's queue and is written again once the dialog has been answered. `agent` names the agent |
| `message_diverted_by_view` | `agent` | A message Octoboard wrote into the session reached a view of its agent's background agents (Claude Code's view of a subagent, or its agents view) rather than the conversation: it went to the subagent, or started another background session, so it is not written again and the session has not read it. The user is to check the session. `agent` names the agent |
| `account_switched_automatically` | `agent`, `from`, `account`, `session`, `from_id?`, `account_id?` | The daemon switched the session to another account of its agent by itself, because its turn stopped on the usage limit of the account it ran on and `auto_switch_on_usage_limit` is on. `agent` is the agent's label, `from` the display name of the account it was on and `account` the display name of the account it is on now (a default account's display name is the one the user gave it, else `Default`; the account's id, or `Default` for the default account, stands in for a name the daemon could not read); `session` is the id of the switched session, which a client names in its message, and `from_id` and `account_id` are the ids of those two accounts, each left out for the agent's default account, which has none |
| `account_switch_automatically_failed` | `agent`, `from`, `account`, `session`, `from_id?`, `account_id?`, `reason`, `reason_code`, and the failure's own params | The daemon tried to switch the session to another account by itself (as for `account_switched_automatically`), the switch failed after the session's process had been ended, and bringing the session back up on the account it had failed too, so it is left `interrupted` there, to be resumed by the user. Nothing was left to retry: it is sent only when both failed. `agent`, `from`, `account`, `session` and the ids are as in `account_switched_automatically`, `account` being the account it was to be switched to; `reason` is the daemon's English account of why the session could not be brought back up and `reason_code` the `code` of the `error` that failed with, whose params arrive beside these as in `launch_failed`; a failure param with the same name as one of these is overridden by it, `from_id` and `account_id` included: they are left out, not taken from the failure, when the account is the default one |
| `account_sharing_repair_failed` | `account`, `name`, `cause`, `items` | Before the session launched, some of its sharing account's links could not be repaired (`cause` and `items` as in the `error` `account_sharing_failed`, `items` empty when the directory could not be checked at all, or the default directory has moved, a `default_moved`); the session launched all the same |
| `sharing_resume_failed` | `account`, `name`, `detail`, `still_running` | The session was idle, so a change of its account's sharing ended its process, and it was not resumed afterwards. With `still_running` `false`, resuming it failed (`detail` says why): it is left `interrupted`, to be resumed by hand. With `still_running` `true`, its process was never seen to end (the change did not start, answered `sharing_session_did_not_stop`): it may still be running, and was not relaunched. Either way the change's own outcome is unaffected |
| `unshared_state_database` | `account`, `name`, `path` | The session's sharing Codex account holds a state database (`path`) that the shared configuration does not cover — a newer Codex's schema generation — so that account's threads are kept apart from the other accounts' from then on |

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
Account { id, agent, name, config_dir,
          share_default_config?, share_default_config_incomplete?, disabled? }
Project { id, console_id, host_id, name, path, default_agent?, source, remote_url?,
          claude_account_id, codex_account_id, grok_account_id,   // each a ProjectAccount
          claude_session_model?, claude_session_effort?,
          codex_session_model?, codex_session_effort?,
          grok_session_model?, grok_session_effort?,
          trust_consent, pinned, tags: string[], description?,
          git_workflow: GitWorkflowOverride }   // absent in an older record: every item follows the application-wide value
GitWorkflowSettings { development: "in_place"|"branch"|"worktree", rebase_before_merge: bool,
                      delivery: "ask"|"merge"|"pull_request", revert_gate: bool }
GitWorkflowOverride { development: "in_place"|"branch"|"worktree"|null, rebase_before_merge: bool|null, delivery: "ask"|"merge"|"pull_request"|null,
                      revert_gate: bool|null }
                                // each item null: follows the application-wide value
Session { id, agent, agent_session_id?, console_id, project_id?, host_id,
          role: "console"|"project", orchestration_role?: "coordinator"|"worker", origin: "console"|"user", title, terminal_title?, context?: SessionContext,
          observed_model?: string, observed_effort?: string, default_effort?: string,
          status: "starting"|"working"|"waiting_user"|"idle"|"interrupted"|"archived",
          background_tasks?: BackgroundTask[],   // only with status "idle"; absent when none
          switching_account?: true,   // while an account switch holds the session; absent otherwise
          has_conversation, bound_to?, colour?: "<family>", ordinal?,
          account_id?, config_dir?, model?: string|null, effort?: string|null, pinned, locked, started_at, ended_at? }
BackgroundTask { id, type?, description? }
SessionContext { tokens, window?, used_percent? }
Page    { id, console_session_id, html, anchor_message_id?, created_at }  // console_session_id: the session that pushed it, a console session or a project session
Settings  { auto_pull_repositories, auto_delete_unresumable_archives, auto_approve_session_requests, auto_approve_resume_requests,
            auto_approve_account_switches, auto_approve_handoffs, ask_questions_as_choices,
            interface_language: string|null, auto_select_account, auto_switch_on_usage_limit, sidebar_order_by_status, auto_start_quota_windows, default_clone_dir, accounts: Account[],
            codex_approval_reviewer, claude_approval_mode,
            account_order: { "<agent>": (string|null)[] },
            claude_compatibility: { "<agent>": boolean } /* codex and grok: the value in effect (see below) */,
            default_account_names: { "<agent>": string }, disabled_default_accounts: string[] /* agents' wire names */,
            git_workflow: GitWorkflowSettings /* defaults: "worktree", true, "ask", false */ }
AccountLogin { agent, account: string|null, status: "starting"|"waiting"|"succeeded"|"failed"|"cancelled"|"timed_out",
               authorization_url: string|null, device_code: string|null, opened_by_agent: bool,
               failure: { code, params, message }|null }
GitStatus { project, repository, branch?, detached, upstream?, ahead, behind,
            activity: "idle"|"checking"|"pulling"|"pushing", error? }
AgentAvailability { agent, availability: "not_determined"|"available"|"unavailable", default_account_dir? }
ShellTerminal { id, project_id, worktree, cwd, title, renamed, state: "running"|"exited", busy, created_at }
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

`Session.orchestration_role` describes capability independently of the reporting relationship in `bound_to`:
`coordinator` can manage child sessions, and `worker` cannot. The daemon supplies it on every session record sent to a
client; it is optional for compatibility with older records. Console sessions are coordinators. A project session
without a parent is a coordinator; a project session with a parent is a worker unless its stored `lead` flag preserves
coordinator capability. A coordinator remains one even when it has no child sessions. The field is derived, never
stored, and is not an authorization input. MCP session records carry the same `orchestration_role`; their existing
`owner` and `owner_kind` fields describe the parent, independently of the role. Existing `bound_to`, `lead`, `role`,
and tool names retain their meanings.

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

`Session.context` is the session's current context size, read from the agent's native records: `tokens` is how much the
main conversation's latest request carried, a gauge of whether continuing the session is getting expensive. It is not
a cumulative usage count or a billing figure, and subagents and side calls (title generation, summaries, classifiers)
never count. It is persisted independently of other session changes and retained when archived or restarted, and it
falls again after a compaction, a revert or a rewind: there is no "never lower" guard. Missing `context` means unknown,
not zero. `window` is the context-window limit, present only once the agent has given one; and `used_percent`, in
whole percent from 0 to 100, is how much of it is in use by that agent's own formula, present only with `window`. The
daemon computes it so that clients stay agent-agnostic.

- Claude Code reads the main transcript only, never the `subagents` directory. The value is the input of the last
  `assistant` line that is not a sidechain, `<synthetic>`, `isUnmetered` or `isApiErrorMessage` line and carries input:
  `input_tokens` plus `cache_creation_input_tokens` plus `cache_read_input_tokens`, with output left out. When
  `message.usage.iterations` exists, its last `message` or `fallback_message` entry stands in for the top-level
  usage. A `compact_boundary` line makes the context unknown until the next qualifying `assistant` line. The
  transcript records no window: `window` is the `context_window.context_window_size` Claude Code last gave the
  status line Octoboard injects (see `POST /context-window/:session`), kept across a compaction, and failing a report
  since the daemon started, the one stored with the session. `used_percent` is
  `round(tokens / window * 100)`, capped at 100, as Claude Code's own status line computes it. A session whose Claude
  Code runs no status line of Octoboard's (a version not known to be 2.0.0 or later, or a managed `statusLine` that
  outranks it) has no `window`.
- Codex reads the last `event_msg` `token_count` record whose `info` is not null: `tokens` is
  `info.last_token_usage.total_tokens` and `window` is `info.model_context_window`. `used_percent` is 100 less the
  percentage left that Codex's own `/status` shows, which counts a fixed 12,000-token baseline as neither used nor
  available: left = round((window - 12000 - max(0, tokens - 12000)) / (window - 12000) * 100), so 18,999 tokens of a
  475,000 window is 98% left and 2% used. A thread's head rollout after a revert has no `token_count` until its next
  turn; until then the last one in the file it continues, before the point it continues from, is used, or failing that the nearest earlier file of the thread that has one. A forked thread
  stays unknown until its first turn.
- Grok Build reads the session directory's own `updates.jsonl`, not the `usage.json` ledger (whose totals include
  subagent calls; the file only marks the session directory): `tokens` is the last `params._meta.totalTokens`. An
  `auto_compact_completed` update gives the size after the compaction (`tokens_after`), and a later line that repeats
  `tokens_before` is ignored. `window` is `contextWindowTokens` of the directory's `signals.json`, and `used_percent`
  is `floor(tokens / window * 100)`, as Grok's own `contextWindowUsage`. A forked session copies its parent's files and
  so starts with its parent's figure.

A change of native conversation id clears the old context; switching accounts with the same conversation preserves it.

`Session.observed_model` and `Session.observed_effort` are the model and reasoning effort the agent's own records show
the session running with, in the agent's spelling; they are not `model` and `effort`, which are the launch overrides.
The last record that names one wins, and a value the records do not show stays unknown rather than being cleared, except
that a Codex `turn_context` naming a model and no effort clears `observed_effort` (see `Session.default_effort` below).
Claude reads the `message.model` and the top-level `effort` of the main transcript's assistant lines, skipping sidechain
lines and the `<synthetic>` model, and never the subagent transcripts, which run other models. Codex reads
`payload.model` and `payload.effort` of `turn_context` records. Grok reads the summary file's `current_model_id` and
`reasoning_effort`, the configured ids rather than the ones a request was served by. They are stored and collected like
`context`, retained when archived, and cleared with it when the native conversation id changes.

`Session.default_effort` is the reasoning effort the agent runs `observed_model` at by itself, present only where the
records name that model but no effort and the agent's own catalogue gives the model a default. Only Codex produces it: a
`turn_context` with no `effort` means no layer of Codex's configuration set one, so the model runs at the
`default_reasoning_level` its entry in the account's `models_cache.json` gives; such a `turn_context` also clears an
`observed_effort` an earlier turn named. It is never stored: the collector works it out again on each pass, from the
`observed_model` and `observed_effort` that pass records and the catalogue file as it is then (a catalogue or account
directory that cannot be read keeps the value worked out before), and a change is announced with `session_upserted`,
together with the observed values that make it apply. A client shows it in place of `observed_effort` where that is
absent, ahead of the session's launch override and its inherited default, since the records show the session running
with no effort set.

`Session.background_tasks` lists the work a Claude Code session has left running in the background, including its
named subagents (`type` `"subagent"`); it is `BackgroundTask { id, type?, description? }` as the agent's hooks name each
entry. It is Claude Code only, and held in the daemon's memory only: it is lost at daemon restart and dropped when the
session's process exits. Every `Stop` replaces the list with the one it carries, so a `Stop` without tasks empties it; a
`StopFailure` clears it; and a completion notification, which Claude Code delivers to the session as a prompt naming the
finished tasks, removes just those. Between turns, a subagent's own events keep it current, because a subagent woken by
its background work runs without the session having a `Stop`: a `SubagentStop` replaces the list with the session-wide
registry it carries, less the stopping subagent's own entry, and a `SubagentStart` adds that subagent. Only a subagent
with an `agent_type` counts (the internal helper agents have none), the two count only while the session is `idle`
(inside a turn its own `Stop` lists everything), and neither changes the session's status. A subagent that started
within the last two seconds stays in the list through a `SubagentStop`, whatever the registry says, because hooks run
as commands can arrive out of order. A `SubagentStop` that would leave the list empty takes effect two seconds later
ininstead, because one subagent stopping and the next starting arrive in the same instant; a `Stop`, a `StopFailure` or a
`SubagentStop` that leaves something running in that time takes it back, a `SubagentStart` applies it at once before
adding itself, and a completion notification does neither. The list is present on the record, and published, only
while the session is `idle`: for any other status it is absent, though the daemon keeps what it was told for the next
time the session is idle. An idle session with tasks takes messages like any idle session; the tasks only
say that its work is not over.

`Session.switching_account` is `true` while an account switch holds the session, whether `switch_session_account` was
asked by a client or by a parent's tool, and is absent otherwise. It is true from the moment the switch takes its claim
on the session (so before the process is ended) until it releases it, which is after the relaunched process has been
watched for its come-up and after any revert to the old account, so it spans a failed switch too. It is held in the
daemon's memory only, never stored, and does not change `status`, which reads `interrupted` while the process is down.
Taking and releasing the claim are each announced with a `session_upserted`, and a `snapshot` carries it for a switch
that is under way. The other operations that claim a session (a handoff, a handover of branches, a binding change) do
not set it.

One daemon collector checks live sources at a one-second interval, reading only newly completed JSONL lines and skipping
unchanged files. Updates appear when the agent writes its own record, which may be after a model call or turn completes;
streaming tokens are not estimated. Existing archived records are read once at daemon start if still available, and a live
session gets a final scan after its process exits. Missing, malformed or unsupported data leaves the last valid context intact.
Filesystem work runs on one blocking worker, source paths are indexed once per account in a pass, line parsing is capped at
8 MiB per record, and changed contexts commit in one transaction before `session_upserted` events. Source identity is checked
again under the store lock so a stale scan cannot write a previous conversation's context after a resume or account switch.

`Session.origin` is `console` for a session started by a session's `new_session` tool (a console session, or a project
coordinator) and `user` for one the user opened, or that the user approved through `request_session`.

`agent` is one of `claude`, `codex`, `grok`. An `Account` is a named config directory of one agent, kept
application-wide and referred to by id wherever a config directory is referred to — a console's per-agent field, a
session's own copy of it. `Account.name` is required and unique within `agent`, compared trimmed and case-insensitively
(not compared across agents); the default account's own name takes part in that comparison, even though it is not a row
here — see below. `Account.config_dir` is an absolute, lexically normalised path; existence is not checked when it is
set (Claude Code creates a missing directory on first run, and the daemon creates a missing Codex directory before
launching it; Grok Build is checked at launch instead). The whole account list travels as `Settings.accounts`, and a
client is told about any change to it through `settings_updated` — there is no separate account event.

`Settings.account_order` stores each agent's account display order by agent wire name; `null` names its default
account. Clients ignore deleted IDs and append accounts not yet present in the order, keeping their original list
order. An agent with no saved order uses the default account first, then its owned accounts in their original list
order. The order is machine-local, changed only by the user's client, and omitted from agents' settings tools. A
configuration export that includes the accounts carries it as the order of each agent's `accounts`, with the default
account's place beside them. It does not change which account automatic quota selection chooses.

`Settings.added_agents` is the ordered list of agents the user has added, persisted with the other settings and sent
with them; only these are offered for a new choice. `add_agent` appends an installed agent and `remove_agent` takes one
off, keeping its accounts, which reappear when it is added again, and `reorder_agents` sets the order, which clients use
wherever agents are listed. The list is decided once: a fresh database starts
undecided and are filled as soon as every agent's availability is determined (`agent_availability_updated`) with the
agents that are `available` plus those that have accounts, in the order `claude`, `codex`, `grok`, announced by
`settings_updated`. An undecided list is sent as `null`, which a client must not
read as the empty list of a user who removed every agent. The fill waits rather than reading `not_determined` as not
installed, so an installed agent is never left off it; a run whose login-shell snapshot does not complete leaves the
list `null` until the next start, and `add_agent` is refused with `agent_availability_pending` meanwhile. Once decided,
a list the user emptied stays empty. The list is not part of a configuration export.

`Settings.codex_approval_reviewer` is `null` (the default), `user`, or `auto_review`, one choice for every Codex
account. A null value inherits the effective Codex home's `config.toml`. Non-null values become per-launch
`approvals_reviewer` CLI overrides on fresh launches and resumes, without editing that file, sandbox settings or the
approval policy. The value is read at launch time, so a change reaches the next launch of a session of any Codex
account.

`Settings.claude_approval_mode` is `null` (the default), `auto`, `manual` or `bypass`, one choice for every Claude Code
account. A null value passes no flag, so the account's own `settings.json` decides. The others become
`--permission-mode auto`, `--permission-mode default` (not `manual`, which only Claude Code 2.1.200 and later accepts)
and `--permission-mode bypassPermissions` on fresh launches and resumes alike, without editing any configuration file;
`bypass` also adds `"skipDangerousModePermissionPrompt": true` to the launch's `--settings` JSON, so Claude Code's
own first-use warning never appears. Neither value is exported.

`Settings.claude_compatibility` is whether each agent other than Claude Code (`codex`, `grok`) loads Claude Code's
instructions (CLAUDE.md and rules), skills, agents, commands, MCP servers, hooks and permissions in its sessions: one
value per agent, whichever account a session uses. The daemon stores only the user's explicit choices; until the user
chooses for an agent, Grok Build is on (Grok's own default) and Codex is on while Claude Code is among the added agents
and off otherwise, evaluated live, so adding or removing Claude Code moves an unset Codex's value. Once
`update_settings` has set an agent, that choice holds whatever agents are added. The value in effect is read at every
launch, resume and account switch, so a running process keeps what it started with. With it on, a Codex launch appends a
"Claude Code compatibility" section (`src/adapter/claude_compatibility.md`) to the `developer_instructions` it already
passes, telling Codex to read the user-level Claude Code configuration, in the directory Claude Code's default account
resolves to (`CLAUDE_CONFIG_DIR` of the launch's shell snapshot, else `~/.claude`), and the project's `.claude/` before
working.

It also passes the MCP servers Claude Code would load for the session's working directory (user, local and approved
project scope, stdio and streamable HTTP; `src/adapter/claude_mcp.rs`) as `-c mcp_servers.<name>.…` overrides, and never
over a server of the same name in Codex's own `config.toml`, in the project's `.codex/config.toml` or the reserved
`octoboard`. A stdio server's expanded `command`, `args` and `cwd` are on Codex's own argv
(`-c mcp_servers.<name>.command`, `.args`, `.cwd`) for the session: `command` and `args` reach the server's own argv
anyway, as under Claude Code, and `cwd` is a path, rarely a secret. Header values and `env` values stay off argv: they
are carried in the Codex process's environment (a header under `OCTOBOARD_CLAUDE_MCP_<n>_<m>_TOKEN`, an `env` entry
under its own name, which the server receives through `env_vars`). A server is skipped when it uses a transport other
than stdio and streamable HTTP (SSE, for one), has a `headersHelper`, or has a field that is not a string where one is
expected or is otherwise malformed; when a `${VAR}` in it has no default and `VAR` is unset; when its `env` or a header
value holds a NUL byte or its `env` has an invalid name; when its `env` would give a name a value other than the one the
shell or another server gives it; when its `env` names `TERM`, `GIT_CONFIG_*` or `OCTOBOARD_*`, which the launch sets
after the shell; when its `env` names a variable that steers Codex itself (`CODEX_*`, `OPENAI_*`, `RUST_LOG`,
`RUST_BACKTRACE`, `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` in either case, `SSL_CERT_FILE`,
`SSL_CERT_DIR`, `NODE_OPTIONS`, `DYLD_*`, `LD_*`) at any value other than the one it already holds; or when its `url`
takes a value from a variable or carries credentials or a query string.

A Grok Build launch sets `GROK_CLAUDE_SKILLS_ENABLED`, `GROK_CLAUDE_RULES_ENABLED`, `GROK_CLAUDE_AGENTS_ENABLED`,
`GROK_CLAUDE_MCPS_ENABLED` and `GROK_CLAUDE_HOOKS_ENABLED` to `true`. Off, neither agent is given anything and each
follows its own configuration. Claude Code has no such setting.

`Account.share_default_config` is whether the account shares the agent's default configuration, and
`Account.share_default_config_incomplete` whether the last change toward that choice did not finish: it is set when a
change starts and cleared when one completes, so a change that failed part-way, or one the daemon stopped in, leaves
it set; only asking again for the recorded value or the opposite change clears it. This daemon always sends both; a client reads an absent one as false. Both start
false, are set by `set_account_sharing` (`update_account` repointing the account and `delete_account` turn it off
first), survive a daemon restart and stay local like the overrides above.

**Sharing the default configuration.**

- **What is shared.** A fixed list of items per agent (the daemon's `shared_items.rs`), between the account's directory
  and the default directory, the one the agent's default account resolves to (`AgentAvailability.default_account_dir`).
  Each item is linked to the default entry's fully resolved target.
- **Turning it on.**
  - A configuration item the account holds is renamed to `<name>.bak-<stamp>`, then linked.
  - Conversation history is moved into the default directory, then linked. Where both hold a record of the same
    name, the copy written last stays and the other is set aside under `<name>.bak-<stamp>/` in the account's
    directory.
  - `history.jsonl` and Codex's `session_index.jsonl` are merged into the default's copy.
  - Codex's `state_<n>.sqlite` is set aside with its `-wal` and `-shm`, and its thread rows are carried into the
    default's database, their rollout paths pointed at the default directory.
  - A directory item the default directory lacks is created there, empty; a configuration file it lacks is skipped.
  - The stamp and the default directory are recorded before anything is touched; a retry reuses them, and, until
    turning sharing on has completed once, redoes the database and history merges an earlier attempt left undone.
    A retry when the agent's default directory is no longer the recorded one is refused: the links already made
    lead into the recorded one.
- **Turning it off.**
  - First, rows of the default's Codex database and entries of the plugin registries (Claude Code's
    `plugins/installed_plugins.json` and `known_marketplaces.json`, Grok Build's `installed-plugins/registry.json`)
    that name a path through the account's directory are pointed at the default directory's own, so threads and
    plugins reached through the account stay usable for the accounts still sharing. An item whose records could not
    be rewritten stays linked and is reported.
  - The links are removed, recognised as leading into the default directory recorded when sharing was turned on
    (even when the default has moved since or is gone) or into the current one, and the backups made then are
    restored; an item with none is left absent.
  - The account gets empty conversation directories, and the conversation of every session on record in the
    account's directory, archived ones included, is taken back into them: every file the agent keeps of it (Claude
    Code's transcript with its sidecar directory and rewind checkpoints, every rollout of a Codex thread, those in
    `archived_sessions/` too, Grok Build's session directory) goes to the same path in the account's directory.
    Where the account already holds a file of it, the copy written last stays and the other is set aside under
    `<directory>.bak-<stamp>/`. A conversation another session outside the account records too is copied, and stays
    in the default directory for it. The default's Codex database forgets a thread taken out of it, even when its
    files left on an earlier attempt, and the thread's names are copied into the account's `session_index.jsonl`.
  - Every other conversation stays in the default directory.
  - A change never carried out, or one undone already, has nothing to undo and completes at once.
- **A change runs at once, or is refused.** It must not move files under anything running under the account's
  directory, so `set_account_sharing` is refused with `account_in_use` while one of the account's sessions (a session
  of the account, or one holding its directory) works there, as `in_use` counts it, or the account is being signed
  in. Octoboard's short agent CLI reads there (its plan quota, its models) do not refuse it: the change holds new ones
  back and waits for those running to end. The check and the account's lock are one step, so no launch can slip in
  between.
- **Idle sessions are restarted around it.** Once the change no longer waits for another account of the same agent
  (until then they stay usable), the account's sessions are judged again and its idle ones claimed, in one step: one
  that turned working meanwhile refuses the change with `account_in_use`, with nothing stopped or recorded. Otherwise
  their processes are ended, with what they started carried over to the relaunch as for `switch_session_account` (see
  "Process trees"), the change runs, and they are resumed on their conversations as `resume_session` resumes one,
  whatever the change's outcome, onto the directory as it then stands. A message Octoboard is sent for such a session
  from its claim on is held, and written into the resumed process when its launch would write one (its first hook, its
  trust screen released), as for any resume. When the session is not resumed it is dropped: told by
  `sharing_messages_dropped` when the session was not relaunched, covered by `sharing_resume_failed` when its relaunch
  failed. A prompt typed straight into its terminal in the moment before its process ends has its turn ended, and a
  draft typed there and not sent is lost with the process. While its process is down a client is sent such a session as
  `starting`, not `interrupted`. One archived, deleted or ended by itself meanwhile is not resumed; one whose resume
  fails, or that was never seen to end, is told as a `sharing_resume_failed` notice, and the change stands. A process
  that does not end in time stops the change before anything is recorded (`sharing_session_did_not_stop`), the others
  being resumed. Refused or stopped before the choice is recorded, a change leaves the account's last `failure` as it
  was; it is cleared once the choice is recorded.
- **The account is locked while its change runs**, from the check through the restart of its sessions. Creating a
  session of it, launching or resuming one (other than the change's own resumes), switching a session from or to it,
  handing one off from it, a second `set_account_sharing` for it, and removing or repointing it are refused with
  `account_sharing_in_progress`. `account_sharing` tells the progress, phase by phase and entry by entry.
- **Nothing is carried out later.** A change that fails is left as it is, the account marked
  `share_default_config_incomplete` and the failure kept in `account_sharing`; no launch, resume or switch retries
  it, and a session of an incomplete account still launches. The outcome is recorded only for the directory it was
  carried out on.
- **Before every launch** of a session of an account whose directory shares, each file item that is no longer a link
  to the default entry's resolved target is backed up (merged, for a history file) and linked again, and a directory
  item that is missing or links elsewhere is linked; nothing is repaired while the account is incomplete, either
  way (the session is given the directory its links lead into only while its `projects/` still links there, for
  auto memory below), or when the agent's default directory is no longer the
  one the links lead into (told as an `account_sharing_repair_failed` notice; the session is still given the
  directory the links lead into). What cannot be repaired is told as an `account_sharing_repair_failed` notice, and
  a Codex `state_<n>.sqlite` the list does not name as an `unshared_state_database` notice.
- **Auto memory.** A Claude Code session of an account whose directory shares is launched with
  `CLAUDE_CODE_REMOTE_MEMORY_DIR` set to the resolved default directory; no other session gets it, and nothing is
  written into the account's settings.

**Every agent also has a *default* account, which is not a row in `Settings.accounts` at all**: it is the state of
pinning nothing, named by `Console.*_account_id` and `Session.account_id` being unset. It cannot be created or removed.
The user may name it (`set_default_account_name`, kept in `Settings.default_account_names` by agent); without a name a
client shows its own word for it, and the daemon compares account names against `Default`.

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
from it; Grok itself still runs against that private home. `Console.*_console_session_model` and
`Console.*_console_session_effort` are the model and reasoning effort a console session of the console is launched with,
one pair per agent in that agent's own spelling, read from the console when the session is opened or resumed; unset
means the agent's own default. Only a console session reads them. `Project.*_session_model` and
`Project.*_session_effort` are the same for a session of one project, and are read by that project's sessions alone.
`Console.*_project_session_model` and `Console.*_project_session_effort` are the pair a project session of the console
falls back to for each value its project leaves unset; unset means the agent's own default. A project session therefore
resolves each value as: the session's override, else its project's, else its console's project session value, else the
agent's own default; a console session never reads the project session pair. `Session.model` and `Session.effort` are
independent launch overrides, set when the session is opened and changeable through `set_session_model`,
`reopen_session` or an approved account switch. An absent field inherits the project's current setting
(else the console's project session setting), or the console's console session setting for a console session, read at
every launch; an explicit `null` uses the agent's own default even when the parent sets a value. A string pins that
value. Every launch, resume and account switch uses these overrides before the inherited values. An override survives
daemon restarts. `Console.icon` is the console's custom avatar, a `data:image/...` URL of at most 256 KiB (the UI sends
a 128x128 WebP or PNG); unset means the default glyph. `Console.console_session_prompt` is the user's own text, stored
trimmed, added after the built-in instructions in the console's instruction file, inside user tags; unset means nothing
is added. A configuration export carries it (see "Configuration export"). The MCP tools neither read nor set it.
`Session.bound_to` is the id of the parent session this session reports to — a console session, or a project coordinator
in the same project — or unset for an independent session. It is set when the session is created and changed afterwards
by `transfer_session` (a parent hands a child session to another parent), `bind_to_session` (an independent project
coordinator sets its parent), or `release_session` (the parent or user makes the session independent). Console sessions
have no parent. A project coordinator can report to a console session while retaining its child sessions and report
panel: its `bound_to` names the console session and its children's `bound_to` name it. Calling `bind_to_session` with a
console parent preserves coordinator capability, as does taking that coordinator's place in a `hand_off_session` or
transferring it to another console parent. A transfer to a project parent, allowed only without any child sessions,
changes it to a worker. Release restores independent coordinator status and keeps its children. A project worker stays a
worker when transferred to either kind of parent. Creating a project session with a parent creates a worker. The
wire-only `orchestration_role` distinguishes coordinator from worker independently of `bound_to`; the persisted `lead`
flag preserves coordinator capability under a console parent. `report` and the synthesised report deliver to the parent;
see `docs/product/reporting-to-owners.md`, "Coordinators with a parent session" in `docs/product/hub-orchestration.md`,
and `docs/product/changing-session-owners.md`.

`Session.colour` is a console session's badge colour, one entry of a
fixed palette of sixteen, each written as its family name: `red`, `orange`, `amber`, `yellow`, `lime`, `green`, `teal`,
`cyan`, `sky`, `blue`, `indigo`, `violet`, `fuchsia`, `pink`, `brown` and `gray`. The ids are never shown to the user;
what each one looks like is the client's. A colour is assigned on creation (picked at random among the entries no other
non-archived console session of its console holds, or among all of them when every entry is held) and changed afterwards
only by `set_session_colour`; unset for a project session. `Session.ordinal` is a console session's place in its
console's history — one past the highest ever used there, never reused after archiving or deletion; unset for a project
session. `Session.account_id` is the account this session's own agent reads, by id, resolved as above when the session
is opened (unset means the default account), and changed afterwards only by a successful `switch_session_account`.
`Session.config_dir` is that account's directory at the same moment, written with it and never alone: each agent keeps a
conversation's transcript inside it, so `resume_session` relaunches with this value and not the account's current one.
It is unset for sessions started on the default account, which resume under whatever the shell exports at that moment. A
launch whose pinned directory no longer exists is refused — for every agent — only when the session has a conversation
to resume (`config_dir_unreachable`); a new session, or one with no conversation yet, launches into it instead. Grok
Build additionally refuses a pinned source home that is not an initialized Grok home (`grok_home_not_initialized`),
whether or not there is a conversation to resume, since it cannot safely create one the way the other two agents can.
`trusted_directories` is a list of absolute, lexically normalised directory paths, stored as given (no symlink is
resolved in what is stored). A project is trusted by one when its directory, as it really is on the file system, is the
directory that trusted path leads to or lies below it: the project's path is resolved with every symbolic link followed,
and the directory it leads to and each one above it are compared with the trusted directory by device and inode, never
by text. So `/a/Project` does not cover `/a/Project2`; a project reached through a link inside a trusted directory that
leads out of it is not covered, wherever its path is spelled; and a project whose path is spelled through a link into a
trusted directory, or in another letter case on a file system that ignores case, is covered, as is one under a trusted
path that is itself a link to a directory. Where either side cannot be resolved — nothing at the path, a link that leads
nowhere, a directory that cannot be read, a file system that reports no inode, or a look that takes more than a few
seconds, as on a mount that does not answer — the project is not covered by that directory; nothing falls back to
comparing the text. The decision is made each time it is needed (a confirmation recognised, the prompts repeated after a
`snapshot`, a directory added), so it follows where a link points then. They are added by `confirm_trust` with
`trust_parent_dir` and removed by `remove_trusted_directory`, which matches the stored text. The comparison looks at a
project's path only, not at its `host_id`: there is one local host today, and a second would need its own set.
`Project.trust_consent` is true once the user has agreed, in the dialog a `trust_prompt` opens, that Octoboard may press
any agent's trust confirmation for that project's directory without asking, and that press succeeded: it is the project
form of the one permission every agent shares, a trusted directory being the other, and it is not any agent's own trust
record. It is only ever set by `confirm_trust` with `remember`; `update_project` neither sets nor clears it, and
removing the project removes it. `pinned` (on a project and on a session) is the user's pin, false until set: by
`update_project` for a project and `set_session_pinned` for a session. It is only a flag for the client to order by;
archiving and resuming leave it alone. `Session.locked` is false until the user sets it with `set_session_locked`;
locking an archived session is refused with `session_archived`, while unlocking one stays allowed. It survives reopening
and account switching and does not change ordering. Archive, delete and bulk delete refuse the operation with
`session_locked` (`session`: the locked session id) if the selected session or any session included by its cascade is
locked. Removing a console or project follows the same rule. A project session's completed report is still delivered
while it or its archive group is locked, but it is not automatically archived. The MCP tools cannot change the lock.
Changing a lock while any archive or delete operation is in progress is refused with `session_lock_busy`; retry after
that operation finishes. `Project.tags` are the user's free-form labels, used by the client to filter the project list;
there is no tag registry, so the tags in use are the distinct ones across projects. `add_project` and `update_project`
normalise what they are given: each tag is trimmed, the empty ones are dropped, and a tag matching an earlier one
ignoring case is dropped (the first spelling wins). The order is kept, and a tags list never fails validation.
`Project.description` is the user's own free text on what the project is for, read by agent sessions through the MCP
tools. New or changed text is trimmed, with blank as none, and limited to 2,000 Unicode scalar values. A new explicit
`Project.name` or a changed name is trimmed and limited to 100 Unicode scalar values. Neither field is truncated. An
unchanged legacy value keeps its stored text, including surrounding whitespace and overlong content; equality is
compared after trimming. Directory-derived names and configuration imports preserve text beyond these editing limits.
`Page.anchor_message_id` is stored and never read (see "Data model" in `docs/architecture.md`). Timestamps are epoch
milliseconds (the UI formats them). `Settings` is the app-wide user settings the daemon stores, plus the account list —
already a record that grows without a new request shape or a new event for every addition, which is where `accounts`
belongs too. `auto_pull_repositories` (default `false`) governs step 5 below: off, the periodic check still fetches the
remote and reports how far ahead or behind the branch is, but never moves it; on, a branch that is behind and can
fast-forward is fast-forwarded. It never pushes, and it never merges non-fast-forward. Turning it on also fast-forwards
straight away, without waiting for the next check — see "Fast-forwarding when the setting is turned on" below.
`auto_delete_unresumable_archives` (default `true`) governs the daemon's automatic deletion of archived sessions whose
conversation record is proven gone: while it is `false` that scan deletes nothing. The scan reads it when it starts and
again before each delete, so a change needs no restart and takes hold mid-scan. It does not affect `resume_session`
refusing a session whose conversation is gone (`conversation_unresumable`). `auto_approve_session_requests` (default
`false`) makes a `request_session` call that passed every check made before the user is asked be approved at once,
through the same carrying out as an approval from the dialog: no `session_request` is broadcast and the session raises
no hand, so no window shows a dialog. The session started, the handover of branches, the result and the outcome message
are as for an approval, except that the opening prompt of the new session and the result's note say Octoboard approved
the request automatically under this setting rather than the user. A request already waiting when the setting is turned
on stays in its dialog. While an earlier approval of the same session is being carried out, a further call is refused as
before, and an automatic request takes the place of a request the session has waiting. Only a client changes it: the
settings tools of the MCP server neither read nor write it, so an agent cannot turn it on. A full configuration export
carries it and an import that applies the settings sets it, like the other portable settings.
`auto_approve_resume_requests` (default `false`) is the same for a `request_resume_session` call, independently of
`auto_approve_session_requests`: either can be on while the other is off. An automatic approval under it is worded in
the outcome as the user's choice to have resume requests approved without asking, and it is as out of an agent's reach
as the other one, and a full configuration export carries it as that one does. `auto_approve_account_switches` (default
`false`) is the same for a parent's `switch_session_account` call, and `auto_approve_handoffs` (likewise) for its
`handoff_session` call, each independent of the others. While its setting is off such a call is a request like
`request_session`'s: it waits for the user's answer and is shown to the user as a `session_request` (`switch` or
`handoff`). While it is on, the call runs the operation at once: no `session_request` is broadcast, the session raises
no hand, and the operation finishes on a task of its own, so a call cancelled meanwhile does not interrupt it (when the
call is gone by the end, the outcome is written into the caller's session as a message). A waiting request of the same
kind for the same session is withdrawn first. Each is read once when the call is made, is as out of an agent's reach as
the other approvals, and is carried by a full configuration export as they are. `auto_select_account` (default `false`)
makes a session started by `new_session`, `request_session` or `handoff_session` whose account was left out run on the
account of that session's agent whose weekly quota resets soonest. An account is a candidate only when it can still open
a session: a 5-hour, account-wide weekly, or credit-period window that is used up drops the whole account, and a reading
taken under another directory does not count; nor does a disabled account (`set_account_disabled`), an account found not
logged in (see `account_not_signed_in`) or an account whose sharing change is running. Among those accounts, the window
that ranks is an account-wide weekly window, or a credit period of about a week (`window_minutes` from 6 to 8 days). A
model's own weekly window, a 5-hour window and a monthly credit period do not rank. `used_percent` must be finite and
below 100, and the reset must be strictly ahead; a reset that has passed or is missing does not rank. The default
account is a candidate, ahead of that agent's stored accounts in the order they are kept. Soonest reset wins, so quota
about to reset is spent first; a larger remainder breaks a tie, and an earlier candidate wins one that is still tied.
The inherited account is resolved as follows: in the caller's own console, the caller's own
account when it is the same agent and that account can open the session, otherwise the project's for the agent, else the
console's; in another console, the project's, else the console's, with no fall-back to the caller; a handoff's session
is always in the caller's own console and uses this inherited account when no weekly window ranks. Agents may read and
change the setting. A full configuration export carries it; a
document exported before it existed omits it, and importing that document leaves the stored value unchanged.

For a new `open_session`, `new_session` or `request_session` with automatic selection on and no explicit account, the
preferred agent is tried first: its weekly pick, then the inherited account above, then its first usable account (saved
account order, default before stored accounts when unset). Accounts with no quota reading remain usable. If no account
works and no agent was explicitly named, other installed agents on the added list are tried in that list's order, using
each agent's weekly, inherited and first-usable account order. An undecided added list uses the built-in agent order. An
explicit agent restricts selection to that agent; an explicit account preserves both agent and account. A resume,
account switch or handoff never falls back to another agent. Unspecified model and effort inherit the final agent's
settings. Quick create fails only after this search finds no usable choice. These rules also apply to `request_session`
in another console.

`ask_questions_as_choices` (default `true`) is whether the instructions
every session is launched with ask it to put each question that needs the user's decision or answer as options to pick
from (through the agent's own tool for it: Claude Code's `AskUserQuestion` and Grok Build's `ask_user_question` are
named, Codex is told generically), and otherwise as numbered questions with lettered options at the end of its reply; a
question that only lacks information, with no options to offer, goes to that numbered list rather than into the tool. A
project session is given this in its role text, a console session in its instruction file. Both are fixed when the session is launched, as the development rule
is: a change reaches only sessions begun afterwards, and a console's file, rewritten at every launch, follows at the
console session's next launch. It is as out of an agent's reach as the approvals (neither `get_settings` shows it nor
`update_settings` changes it), and a full configuration export carries it as it does them.
`interface_language` is the tag of the language the user last picked for the interface, which a client reports in
`update_settings` when the user picks one in Settings, and also on connecting when the daemon holds none yet (never
otherwise, so clients on different languages do not overwrite each other at every start). It is the user's language
rather than a client's, so there is one value: the one reported most recently, `null` until one is. While a language is
held and it is not English (any `en` tag), the instructions every session is launched with name it and ask the session
to talk to the user in it: replies, questions, reports and pages meant for the user, and the titles it gives sessions,
unless the user writes in another language or asks for one, or the project's or the user's own instructions say which to
use; what it writes into a project follows that project's conventions instead. Nothing is added otherwise: sessions
already follow the language the user writes in. A project session is given the section in its role text, a console
session in its instruction file after the guidance on asking and before the user's own section; both are fixed when the
session is launched like the settings above, so a change of the language reaches only sessions begun afterwards. Every
`title` argument (`new_session`, `handoff_session`, `rename_session`, `request_session` and `request_resume_session`)
keeps the same static description: the language of a name is the one the section names where the session has one, else
the one the user writes in. The field is not visible to agents (`get_settings` omits it, `update_settings` ignores it)
and a configuration export does not carry it: a client reports it. A document exported by a build that carried a
`reply_in_interface_language` switch still imports: the field is ignored.
`sidebar_order_by_status` (default `true`) is whether the client's sidebar
orders sessions and projects by status. The daemon only stores and serves it; the ordering is the client's. Off, the
sidebar ranks no session or project by status: sessions go pinned first, then most recently started, and projects keep
active first, then pinned, then name. Agents may read and change the setting. A full configuration export carries it; a
document exported before it existed omits it, and importing that document leaves the stored value unchanged.
`auto_start_quota_windows` (default `false`) is whether the daemon starts a
usage window that has reset but not begun counting; see "Account quotas" below. Agents cannot read or change it, as it
spends the user's quota. A full configuration export carries it; a document exported before it existed omits it, and
importing that document leaves the stored value unchanged.
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

### Switching a session whose turn hit the usage limit

`auto_switch_on_usage_limit` (default `false`) has the daemon switch a
session to another account of its agent by itself when the session's turn stopped because its account reached the plan's
usage limit, with the switch `switch_session_account` makes (the conversation comes along, and the turn the limit
stopped is continued). It is off by default because it ends the session's process and spends another account's quota
without asking. It governs only this: `auto_select_account` decides how a new session picks its account, and
`auto_approve_account_switches` whether an agent's request for a switch asks the user. An agent cannot read or change
it, as it cannot the approvals; a full configuration export carries it, and a document exported before it existed
omits it, which leaves the stored value unchanged.

- **Trigger.** The end of a turn that failed (Claude Code's and Grok Build's `StopFailure`, a Codex turn found failed in
  its rollout), and only when the session's conversation record then says the turn stopped on the usage limit; a quota
  reading never triggers it, and a session is never switched because its account reads as used up, `working` or `idle`.
  The record is read again for a few seconds when it does not say so yet, since an agent may report the failed turn
  before it writes why; that wait is made before the switch takes its claim, so a failed turn that did not stop on the
  limit (an overload, a network error) never shows the session as switching. The record is read once more under the
  claim, before the process is ended. Each such turn is its own trigger: no wait is kept between switches.
- **Which sessions.** One with a process that is `working` or `idle`, with no launch, resume or switch claim on it, not
  shown as `waiting_user` by any route (a request's hand, a prompt on the terminal, a dialog of the agent), still on the
  account that hit the limit, and with no sharing change running on either account. An agent's usage-limit dialog (Grok
  Build's, which is up when its turn stops on the limit) is not a wait that counts: ending it is the point of the
  switch. What is found otherwise under the claim is refused with the session as it was, and tells nobody.
- **Which account.** The one a new session would be given while `auto_select_account` is on (the weekly pick described
  above) but never the account the session is on, nor one it was switched off for a limit in the last 30 minutes. An
  account is skipped when it is clearly unusable: its quota is used up, it is turned off, it is not logged in, a
  sharing change is running on it, it is not one of the agent's, or its quota cannot be read now or has not been read
  yet (the reading's last attempt failed, or none is held). No minimum of quota left is asked. When none qualifies,
  nothing is switched.
- **Success.** The session is relaunched with one prompt that tells it of the switch and carries on with the stopped
  turn, so it opens no turn of its own to be reported on; its parent, if it has one, is written a message (kept for its
  resume while it is interrupted), and the user is sent a `session_notice` with code `account_switched_automatically`.
  The session's `switching_account` is set meanwhile, as for any switch.
- **Failure.** A failure before the session's process was ended, or while a process that did not stop in time may still
  run (`session_did_not_stop`), leaves the session as it was and is only logged. A failure after the daemon had stopped
  the process (a sharing change begun meanwhile, a conversation copy that fails, a relaunch that fails or does not come
  up) is undone by the daemon: it relaunches the session on the account it had, without a prompt, and tells nobody. If
  another claim holds the session (a handover of branches, a binding change), it waits for that claim to end, for up to
  15 seconds, before giving up. The process either relaunch starts is watched for the first seconds: one that ends
  within them is the outcome of that operation, and its parent is not also told that it "ended on its own". A switch
  someone asked for does not watch its process this way: if its relaunch ends on its own, the parent is told. Only when
  the restore fails too is the user sent a `session_notice` with code `account_switch_automatically_failed` and the
  parent written a message, kept like the one for a success; the session is then `interrupted` on its own account, and
  there is nothing to retry. A process that had exited by itself before the stop is not the switch's: it is neither
  restored nor reported. A failure for a session the user archived or deleted meanwhile is neither told nor logged.

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
usual default (see "Agent config directories" in `docs/product/consoles.md`) under the snapshot's own
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
covers a binary that turns out not to be there. The console session's own `new_session` tool is refused through
the same path, as a tool error carrying the same reason.

`open_session` also refuses with `agent_not_added` when the session's resolved agent is not on
`Settings.added_agents`, and refuses nothing while that list is undecided (`null`). It is checked in the same place, so
`hand_off_session`, `new_session` and `request_session` (before the user is asked and again on approval) are refused the same way. Only a
session being opened is checked: a session of an agent removed since keeps running, and resuming, restarting or
reopening it is never refused for this.

`open_session` also refuses with `account_disabled` when the account it resolves to, named or inherited, is turned off
(`set_account_disabled`). It is checked in the same place, before the quota check and whatever the quota says, so
`hand_off_session`, `new_session` and `request_session` are refused the same way, and `switch_session_account` checks its
target account the same way. Only a session being opened or moved is checked: a session already on the account keeps
running, and resuming, restarting or reopening it is never refused for this.

`open_session` also refuses with `account_quota_exhausted` when the account it resolves to has its plan quota used up,
by the reading held at that moment: no fresh reading is asked for or waited on, and an account not read yet is not
refused. It is checked in the same place as the refusals above, so every path that opens a session meets it, and
`switch_session_account` checks its target account the same way. The same place refuses with `account_not_signed_in` an
account the held reading found not logged in (`quota_not_signed_in`, `quota_config_dir_missing`). Resuming, restarting
or reopening a session is not refused for this.

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

Per agent (the measured CLI facts are in the "Model and reasoning effort" note of each agent's section, "Claude Code",
"Codex" and "Grok Build", in `docs/agent-cli-reference.md`):

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
never uses a credential or calls a server of its own; the only thing it takes from the CLI's sign-in record is the
account's email (see `email` below). It runs the CLI the way a session of the account would be run:
the login-shell snapshot, the agent's configuration-directory variable set to the account's directory (left as the
shell has it for the default account), and the home directory as working directory. The exception is that MCP servers,
plugins and hooks are left out where the CLI allows it without changing the sign-in (see "Plan quota" in
`docs/agent-cli-reference.md`).
The commands and the replies they give are in that section too. The readings are runtime state: held in
memory, never stored, never part of `get_settings` or an exported configuration.

```
AccountQuota { agent, account, fetched_at, plan, email, windows: [QuotaWindow], unavailable }
QuotaWindow  { kind, label, used_percent, resets_at, window_minutes }
```

- `account` is the account's id, `null` for the agent's default account. There is one entry per account of every
  available agent, the default account included, ordered by agent and then account with the default account first.
- `fetched_at` and `resets_at` are milliseconds since the epoch. `plan` is the plan as the agent names it (`max`,
  `plus`, `SuperGrok`), null when it does not say.
- `email` is the address the account is signed in with, read from the CLI's own sign-in record with each reading
  (Claude Code's `.claude.json`, the ID token in Codex's `auth.json`, Grok Build's `auth.json`), whether or not the
  quota itself could be read. It is null when the account reads as signed out, its configuration directory is missing,
  the record cannot be read or names no single address, or, for the default account, the login-shell snapshot could
  not be taken. A failed attempt that keeps the last good `windows` carries its own `email`, not the earlier one. The
  email is never logged or put in an error, and `get_account_quotas` does not pass it to sessions.
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
| `quota_timed_out` | `agent` | The command gave no reply within 60 seconds |
| `quota_command_failed` | `agent`, `detail` | The command could not be run, ended without replying, printed more than 8 MiB, or answered the request with an error; `detail` is the exit status and the last line of the first 4 KiB it wrote to stderr, or the error's message, followed by `: ` and its `data` when that is a different string |
| `quota_unreadable_reply` | `agent` | The command replied, but not in a shape that holds a quota window |
| `quota_no_windows` | `agent` | The command replied and the account has no quota windows to report (an account without a subscription, say) |
| `quota_config_dir_missing` | `agent`, `dir` | The account's config directory does not exist: found before anything is run for an account with a config directory (Claude Code and Grok Build would create it, empty, and call it not signed in), and for the default Codex account from Codex's own refusal of a `CODEX_HOME` that does not exist. A state, not a failure; logging in creates the directory |
| `quota_not_signed_in` | `agent` | The directory exists and the agent is not signed in there: Claude Code's `initialize` reply says `account.tokenSource` is `none` (its usage reply alone cannot tell, an account without a subscription answers the same), Codex's and Grok Build's quota request is refused with an "authentication required" error, or Grok Build's with an error whose `data` says "Invalid or expired credentials" (a stored sign-in that has expired). A state, not a failure |

The last two are states of the account: they replace any windows the account held, do not stretch the wait before the
next attempt, and are read again soon after the account's login ends: any end marks the reading due, so the poller takes it on its next look (within about 15 seconds), and a success also starts a read at once. A reply that matches none of the
signals stays one of the failures above.

Readings are kept per agent, account and configuration directory, so an account pointed at another directory never
inherits the old one's. `delete_account`, and `update_account` when it changes the directory, drop the account's reading
at once (a rename keeps it), and a reading still running for it is discarded when it lands. A reading is also dropped
when its account is no longer one of those the poller covers. A background poller started with the daemon reads every
account of every available agent about every five minutes, four CLIs at a time across the whole daemon. An account
whose reading fails is tried again after twice, four times and then eight times that interval (an account that is not
signed in, or that the CLI has no quota to report for, is not backed off), and a reading
still running is never started twice. A newly created account, or an agent found available after the daemon's start, is
read within the poller's next look, which comes every fifteen seconds. `refresh_account_quotas` runs the same path on
request for a client that wants fresher data when it opens a view: it reads what is due, and an account that is not
signed in or has no quota to report once its last attempt is 30 seconds old, so asking on every open costs next to
nothing. Every change to the list is broadcast whole as `account_quotas_updated`.

While `auto_start_quota_windows` is on, a successful reading that shows a window with no `resets_at` (one that has
reset but not begun counting), other than a model's own weekly window, is followed by a kick, unless the account is
disabled (`set_account_disabled`; the default account included): one one-word request on that account, with the agent's cheapest model at its lowest effort (see "Starting a quota window" in
`docs/agent-cli-reference.md`), and then a reading of the account again. The kick and that reading run in the
background, so the refresh that found the window returns without waiting for them. A kick holds the account's claim, so
a refresh that finds the account claimed leaves it alone, and takes one of the four CLI slots; a login that ends drops
the claim all the same, so the account can then be read while a kick still runs. A window is kicked once per episode,
which ends only when a reading shows that window with a `resets_at` again: a window still without one after a kick, or
whose kick failed (logged, not retried), is not kicked again. The episodes are runtime state like the readings, dropped
with them, and a daemon restart starts them afresh. Turning the setting on kicks the accounts whose held readings call
for it at once, and so does turning a disabled account back on.

### Account login

`start_account_login` runs the agent's login flow in the background and `account_login_updated` carries its state,
whole, each time it changes. One login runs per account (`login_already_running`), and Codex allows one at a time
across accounts (`login_codex_busy`). The login of the default account (`account` null) runs with the agent's directory
variable left as the shell has it. `AccountLogin.status`:

- `starting`: the CLI has started and has not said where to authorize.
- `waiting`: `authorization_url` is the page to authorize on, and for a device-code flow `device_code` the code to
  confirm there. `opened_by_agent` is true when the CLI opens the page in the browser itself and cannot be stopped from
  it (Grok Build): the client must not open it again, only offer to reopen it. Otherwise the client opens the page.
- `succeeded`: the agent says it is signed in (confirmed, not only the CLI's exit); the account's quota is read at once.
- `failed`: `failure` is a coded message: `login_failed` (`agent`, `detail`: the CLI's last words or the error) or
  `login_not_confirmed` (`agent`: the CLI reported success but the agent still says it is not signed in).
- `cancelled`: ended by `cancel_account_login`.
- `timed_out`: ended by the daemon after ten minutes of waiting (Claude Code has no timeout of its own; Codex's own
  timeout is reported as this too).

Only `starting` and `waiting` logins are replayed in a `snapshot`'s `account_logins`; an ended one is announced once.
A login that is ended while its CLI still runs (`cancel_account_login`, the ten minutes, the daemon's exit) kills the
CLI's whole process group, and `cancelled` is announced once the CLI has gone; a CLI that exits by itself is not
followed into what it forked. Deleting an account, or repointing its config directory, ends its running login. The
flows (measured in `docs/agent-cli-reference.md`, "Signing in a config directory without a terminal"):

| Agent | Command | Authorization | Done |
|---|---|---|---|
| Claude Code | `claude auth login`, `CLAUDE_CONFIG_DIR` set, `BROWSER` a script the daemon writes that appends its argument to a file it watches | the URL handed to `BROWSER` (its loopback callback finishes the login) | exit 0, then `claude auth status` says `loggedIn` |
| Codex | `codex app-server`, `account/login/start` `{"type":"chatgpt"}`; the config directory is created first (for the default account, the one the agent resolved) | `authUrl` of the reply | `account/login/completed` with `success`, then `account/read` holds an account; cancelled with `account/login/cancel` |
| Grok Build | `grok login --device-auth`, `GROK_HOME` set | the device page and code printed on stderr; the CLI opens the page itself | exit 0 with `auth.json` in the home |

### Configuration export

`export_configuration` produces the document a user carries to another machine. The reply, `configuration_exported`,
holds it under `document`; a client saves it as a file and does not build or alter it. The document is JSON and is the
input of the import as much as a file the user keeps, so its shape is part of this contract.

```
Document { format: "octoboard-configuration", version: 2,
           exported_by?,                                  // the Octoboard version that exported it; informational
           settings?: { auto_pull_repositories,       // present exactly when the application settings were
                                                      // exported, whichever consoles; an older document
                                                      // may carry it as `auto_sync_repositories`, read alike
                        auto_delete_unresumable_archives?,   // absent or null in an older document: left as it is
                        auto_select_account?,                // absent or null in an older document: left as it is
                        auto_switch_on_usage_limit?,         // absent or null in an older document: left as it is
                        sidebar_order_by_status?,            // absent or null in an older document: left as it is
                        auto_start_quota_windows?,           // absent or null in an older document: left as it is
                        auto_approve_session_requests?,      // absent or null in an older document: left as it is
                        auto_approve_resume_requests?,       // absent or null in an older document: left as it is
                        auto_approve_account_switches?,      // absent or null in an older document: left as it is
                        auto_approve_handoffs?,              // absent or null in an older document: left as it is
                        ask_questions_as_choices?,           // absent or null in an older document: left as it is
                        git_workflow?: GitWorkflowSettings,  // absent or null in an older document: left as they are
                        claude_compatibility?: { "<agent>": boolean },  // absent or null in an older document: left as it is
                        keyboard_shortcuts? },               // the exporting client's changed shortcuts by command id
                                                             // (`{}`: all at their defaults), added by the client, never
                                                             // the daemon (see below)
           accounts?: DocumentAccounts,                      // present exactly when the accounts were exported
           consoles: DocumentConsole[] }                     // empty when no console was exported
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
DocumentProject { name, path,                                       // absolute; only the last segment without paths
                  source: "local"|"parent"|"git", remote_url?, default_agent?, pinned, tags: string[],
                  description?,                                     // absent in an older document: none
                  claude_account, codex_account, grok_account,       // each a ProjectAccountRef
                  claude_session_model?, claude_session_effort?,
                  codex_session_model?, codex_session_effort?,
                  grok_session_model?, grok_session_effort?,
                  git_workflow?: GitWorkflowOverride }               // both export scopes; absent in an older document:
                                                                    // every item follows the application-wide value
AccountName       { name }
ProjectAccountRef { kind: "inherit" } | { kind: "default" } | { kind: "account", name }
DocumentAccounts  { added_agents?: Agent[],                 // the added agents in their order; absent when the exporting
                                                            // machine had not decided the list yet (an import adds none)
                    agents: DocumentAgentAccounts[] }       // an entry for each agent with an account or a named default
                                                            // account, in the order claude, codex, grok
DocumentAgentAccounts { agent,                               // "claude" | "codex" | "grok"
                        default_account: { name?, position },   // name: what the user called it (absent or null when
                                                            // unnamed); position: how many accounts come before it in
                                                            // the display order
                        accounts: DocumentAccount[] }        // the accounts the user owns, in the display order
DocumentAccount   { name, directory, share_default_config: bool, disabled: bool }
```

- **Scope.** The regions are chosen one by one (see the `export_configuration` request): the consoles and projects
  (`consoles`: one, all or none; `consoles` is empty for none), the application settings (`settings`) and the accounts
  (`accounts`). `settings` is present exactly when the settings were exported and `accounts` exactly when the accounts
  were: nothing may be inferred from the presence of one about the others (earlier daemons exported `settings` with
  every console and with no other, and a reader must not rely on that). `settings` holds the portable application
  settings (`auto_pull_repositories`, `auto_delete_unresumable_archives`, `auto_select_account`,
  `auto_switch_on_usage_limit`, `sidebar_order_by_status`, `auto_start_quota_windows`, `auto_approve_session_requests`,
  `auto_approve_resume_requests`, `auto_approve_account_switches`, `auto_approve_handoffs`, `ask_questions_as_choices`,
  `git_workflow` and `claude_compatibility`), and the client's `keyboard_shortcuts`, an opaque JSON value the daemon
  never builds, stores or reads: the client adds it to the document it saves, applies it itself on import, and the
  daemon passes it through (`settings_applied` carries it back as it came). Consoles are ordered by creation time and a
  console's `projects` by name, with the stored id breaking a tie, so the order is the same on every export; the id
  itself is not in the document. `projects` is all of a console's projects, whether or not a project is a git
  repository.
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
- **The accounts region.** `accounts` is the account structure of this machine and never a credential. It holds the
  added agents (`added_agents`, in their order) and, for each agent that has an account of its own or a named default
  account, `default_account` (the name the user gave it, if any, and its `position` in the agent's display order, which
  is the number of accounts listed before it) and the `accounts` the user owns, in the display order (the saved order,
  then the accounts added since, as a client shows it). An account is its `name`, its `directory`, whether it shares the
  agent's default configuration (`share_default_config`) and whether the user turned it off for new work (`disabled`).
  It is all of the accounts whatever consoles are exported. It carries no account id, no credential or login state, no
  quota reading, no `share_default_config_incomplete`, and not the default account's directory (the agent resolves it
  on the importing machine) or its disabled flag. A `directory` is written `~/<relative path>` when it lies under the
  exporting machine's home directory (`~` for the home directory itself), so no user name leaks. Elsewhere it is the
  absolute path. `include_paths` does not touch it: an account cannot exist without its directory, so the directories
  are always written.
- **Not exported.** An account's `config_dir` in a console's or project's reference: those are an `AccountName`, and the
  importing machine's accounts (those of the `accounts` region, once imported) are what a reference is rebound to. A
  reference whose account no longer exists, or belongs to another agent than its field, is dropped as deleting the
  account would have left it: `null` on a console, `inherit` on a project. Sessions, pages, transcripts, trust state (a
  project's `trust_consent`, the trusted directories) and `Settings.default_clone_dir` are not configuration and are not
  in the document; the last is the importing machine's own to choose, and an import places projects under it.
  `codex_approval_reviewer` and `claude_approval_mode` of `Settings` are not in it either: the two approval choices stay
  local.
- **Machine-local paths.** `Project.path` is an absolute path of the exporting machine, as stored, and is only a
  starting point for an import to resolve against its own filesystem; an account `directory` outside the home
  directory is the other (see the accounts region). With `include_paths` false the export writes only the path's last
  segment instead (`""` for a path with none, such as `/`), so the document names no directory of the exporting
  machine; the format `version` is unchanged, and an import reads such a name as a directory name, not a path (see
  "Configuration import").
- **`remote_url`** is what is recorded for the project (see "Records"); the export reads it from the stored record
  and runs no `git`. It is absent for a directory that is not a repository and for one with no remote. With
  `include_paths` false it is kept only when it may be cloned from (`git_remote::is_clonable_url`); a local path or a
  `file:` URL is dropped, as it would point into the exporting machine.
- **Models and efforts** are exported as stored. They are free text read only when a session is launched, so a value
  the importing machine's agent does not offer is not an error here.
- **Version.** `format` tells a configuration document from any other JSON; `version` is an integer, 2 today. It is
  raised when a change would make a reader written for the older version misread a document: a field removed, or given
  another meaning. A field added is not such a change, and a reader ignores one it does not know. An import refuses a
  `format` it does not know, or a `version` newer than it supports, before creating anything. An older `version` is
  upgraded one version at a time to the current one before the document is read, so an export kept from before a change
  stays importable. `exported_by` records the Octoboard version that produced the document, for diagnosing one that
  misbehaves; an import never reads it, it is absent in a document exported before it existed, and it takes no part in
  `version`. Version 2 renamed the worktree settings to the git workflow settings: `settings.worktrees` and each
  project's `worktrees` became `git_workflow`, and their boolean `use_worktrees` became `development`, where `true` is
  `worktree`, `false` is `in_place` and a project's `null` stays `null`. A version 1 document is upgraded to that shape
  before it is read. The `accounts` region was added without raising `version`: a reader that does not know it ignores
  it.

### Configuration import

An import is two requests. `inspect_configuration` reads a document and answers what an import of it would meet on this
machine, changing nothing; `import_configuration` takes the document again, together with the choice made for each
project, and creates. A client sends the same `document` to both and does not alter it between them. The document is
untrusted input: its paths and URLs are data this machine checks, and nothing in it is acted on as it stands.

```
configuration_inspected { settings?: { auto_pull_repositories, current_auto_pull_repositories,
                                      auto_delete_unresumable_archives?,   // absent or null: the document predates it
                                      current_auto_delete_unresumable_archives,
                                      auto_select_account?,                // absent or null: the document predates it
                                      current_auto_select_account,
                                      auto_switch_on_usage_limit?,         // absent or null: the document predates it
                                      current_auto_switch_on_usage_limit,
                                      sidebar_order_by_status?,            // absent or null: the document predates it
                                      current_sidebar_order_by_status,
                                      auto_start_quota_windows?,           // absent or null: the document predates it
                                      current_auto_start_quota_windows,
                                      auto_approve_session_requests?,      // absent or null: the document predates it
                                      current_auto_approve_session_requests,
                                      auto_approve_resume_requests?,       // absent or null: the document predates it
                                      current_auto_approve_resume_requests,
                                      auto_approve_account_switches?,      // absent or null: the document predates it
                                      current_auto_approve_account_switches,
                                      auto_approve_handoffs?,              // absent or null: the document predates it
                                      current_auto_approve_handoffs,
                                      ask_questions_as_choices?,           // absent or null: the document predates it
                                      current_ask_questions_as_choices,
                                      git_workflow?: GitWorkflowSettings,  // absent or null: the document predates it
                                      claude_compatibility?: { "<agent>": boolean },  // absent or null: the document predates it
                                      current_claude_compatibility: { "<agent>": boolean },
                                      current_git_workflow: GitWorkflowSettings },
                          accounts?: InspectedAccounts,       // present when the document carries accounts
                          consoles: InspectedConsole[] }
InspectedAccounts { agents_to_add: Agent[],                  // document agents not yet added here, in the document's order
                    agents: InspectedAgentAccounts[] }       // in the document's order, one entry for each agent
InspectedAgentAccounts { agent,
                         accounts: InspectedAccount[],        // in the document's order
                         default_name?,                       // the name the document gives the default account
                         default_name_applies: bool }         // whether an import would apply it
InspectedAccount  { name, directory,                         // as the document records them
                    proposed_directory?,                      // as it resolves here (`~/` expanded); absent when it
                                                              // cannot be resolved
                    status: "new" | "existing" | "directory_in_use" | "invalid",
                    in_use_by?,                               // for "directory_in_use": the name of the account that uses it
                    error?: Failure,                          // for "invalid": why
                    share_default_config, disabled }
InspectedConsole  { name, account_fallbacks: AccountFallback[], projects: InspectedProject[] }
InspectedProject  { name, path, remote_url?,            // as the document records them
                    offers: { exported_path?, default_clone_dir?, clone? },   // each the directory it would use
                    default?: "exported_path" | "default_clone_dir" | "clone",
                    account_fallbacks: AccountFallback[] }
AccountFallback   { agent, name }                          // a reference that matches no account on this machine

import_configuration { document, resolutions?: ProjectResolution[], apply_settings?: bool,
                       apply_accounts?: bool, account_resolutions?: AccountResolution[],
                       apply_consoles?: bool }               // absent is true; false creates no console or project
AccountResolution { agent, account,                     // `account`: the index among the document's accounts of `agent`
                    directory?,                         // another directory, absolute or starting with `~/`; or
                    leave_out?: bool }                  // true, to leave the account out; exactly one of the two
ProjectResolution { console, project,                   // indexes into the document
                    choice: { kind: "exported_path" } | { kind: "default_clone_dir" } | { kind: "clone" }
                          | { kind: "directory", path } }

configuration_imported { settings_applied?: { auto_pull_repositories, auto_delete_unresumable_archives?, auto_select_account?,
                                           auto_switch_on_usage_limit?, sidebar_order_by_status?, auto_start_quota_windows?,
                                           auto_approve_session_requests?, auto_approve_resume_requests?,
                                           auto_approve_account_switches?, auto_approve_handoffs?,
                                           ask_questions_as_choices?,
                                           git_workflow?, claude_compatibility?, keyboard_shortcuts? },   // the document's, as it came; the client applies it
                         settings_error?: Failure,   // the settings asked for could not be applied
                         accounts?: ImportedAccounts,        // present only when `apply_accounts` was sent
                         consoles: ImportedConsole[] }
ImportedAccounts  { agents_added: Agent[],                 // appended to the added agents, in the order they were
                    agents: ImportedAgentAccounts[] }      // in the document's order, one entry for each agent
ImportedAgentAccounts { agent,
                        accounts: ImportedAccount[],        // in the document's order
                        default_name_applied: bool }
ImportedAccount   { name, status: "created" | "existing" | "left_out" | "failed",
                    account?,                           // the id of the account created, or of the one that existed
                    error?: Failure,                    // of a failed one
                    sharing_warning?: Failure }         // sharing the default configuration was asked for and failed;
                                                        // the account was created all the same
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
  impossible resolution, of a project or of an account (`import_resolution_invalid`), a request for settings or accounts
  the document does not carry (`import_settings_missing`, `import_accounts_missing`), and whatever `create_console`
  would refuse of any console of the document (`icon_not_an_image`, `icon_too_large`, with the console's name in
  `console_name`). Past that point nothing is refused and nothing is undone: what goes wrong is reported in the result
  (below), because by then earlier consoles and projects exist and have been broadcast, and a rollback would contradict
  that an import always creates and goes through the ordinary association.
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
  resolved. A bare name, a `path` of one plain segment (what an export without paths records), is never a recorded path
  and so never offers `exported_path`; it is taken only as the directory name of the derived choice. `""`, `.`, `..`
  and a relative path of several segments yield no derived path either.
- **Account references.** A reference is rebound to the account of the agent whose field holds it with the same name,
  compared without regard to case. One that matches no account is bound to that agent's default account (`null` on a
  console, `default` on a project, pinned rather than inheriting) and is listed in `account_fallbacks`: on the console
  for its own references, on the project for the project's. A reference that matches an account of this machine is not
  listed. Inspecting lists what would fall back, which includes a name the document's accounts would create (an account
  with status `new`), since it matches no account on this machine yet: the client compares the list with the document's
  accounts and the user's choices to tell a reference that will resolve from one that will not. The result lists what
  did fall back, and a project's list is empty unless the project was created. The accounts are created before the
  consoles, so with `apply_accounts` a reference binds to an account the import created, and falls back only when that
  account was left out or failed.
- **Importing accounts.** Inspecting, for each account of the document, of each agent in the document's order, gives a
  `status`. `existing`: an account of the same agent has the name (trimmed, compared without regard to case); an import
  uses it as it is and never modifies it. Otherwise the account is judged as the Add account form would: `invalid` when
  the name is blank or is one the form refuses (the word `Default`, a name the default account has been given, or one an
  earlier account of the document takes: `error` is the form's refusal, such as `account_name_taken`) or the directory
  is not absolute once resolved (`config_dir_not_absolute`, `field_required`); `directory_in_use` when the proposed
  directory is already another account's of that agent, the default account included, or an earlier new account's of the
  document, by the form's own comparison (`in_use_by` names it); otherwise `new`. The `proposed_directory` is the
  document's directory as it resolves here: `~` and `~/...` expanded against the home directory, an absolute path as it
  is (a bare name is no directory: `config_dir_not_absolute`); a directory that does not exist is fine. `agents_to_add`
  lists the document's added agents missing from this machine's list (none while this machine's list is undecided).
  `default_name` is the name the document gives the agent's default account, and `default_name_applies` is true only
  when this machine's default account of that agent has no name of its own and the name is neither blank nor `Default`
  and collides with no account of the agent, the ones the import would create included. Applying, with `apply_accounts`,
  does this before any console is created, and nothing in it is undone: (1) every agent of `added_agents` not on this
  machine's list is appended, in the document's order (a list not yet decided is filled first, as `add_agent` does, and
  where it stays undecided nothing is appended, `agents_added` empty); (2) each account in the document's order:
  `existing` (judged before this step, so the document's own duplicates are not) is used and untouched (`existing`, with
  the id), one with an `account_resolutions` entry `leave_out` is skipped (`left_out`), and any other is created through
  the Add account form's validation, with the entry's `directory` in place of the document's when it has one
  (`created`); a failure (a directory now in use, a refused name or directory) is that account's `failed` with its
  `error`, and the import goes on; (3) the new accounts come after the agent's other accounts in its display order, in
  the document's order (the order is saved, so it does not depend on creation times); (4) a created account takes the
  document's `disabled` flag and, where the document says it shares the default configuration, sharing is then turned on
  as `set_account_sharing` does, a failure of which leaves the account created and is `sharing_warning` on it; (5) the
  default account's name is applied under the rule above (`default_name_applied`), and its disabled flag, being an
  account of this machine, never is. Every change is broadcast as the same change made by hand is (`settings_updated`).
  Nothing is logged in: the accounts are created without credentials, and the user logs each one in afterwards.
- **Applying.** The order is the accounts (with `apply_accounts`), then the consoles and projects (unless
  `apply_consoles` is false, which creates none and reads no `resolutions`), then the application settings (with
  `apply_settings`). Every console of the document is created new, with its own working directory, whether or not one of
  its name exists: console names are not unique. The projects of a console go through the ordinary association
  (`add_project`), so each is checked and broadcast exactly as one added by hand: the directory must exist, the tags and
  launch settings are normalised, and a directory already associated with that console is refused
  (`all_projects_already_added`, reported on the project). A project's pin, tags, description, default agent (none stays
  none and is not detected from the directory), account references, models, efforts and worktree override come from the
  document; its `source` is `git` for a clone and `local` otherwise, and its `remote_url` is the one it was cloned from
  or the one read from its directory. Folder trust is not in the document: an imported project is untrusted like any
  other and goes through the trust flow when its first session opens.
- **Application settings.** A document carries `settings` when they were exported; `settings` of the inspection
  is present when the document has them, with the document's value and the value this machine has now, so a client can
  show what applying would change. They are applied only when `import_configuration` is sent `apply_settings: true`,
  because `auto_pull_repositories` makes the daemon fast-forward the user's repositories on its own and
  `auto_delete_unresumable_archives` makes it delete archived sessions on its own, which changes how this machine
  behaves. A document without `auto_delete_unresumable_archives` leaves that setting as it is. `auto_select_account` is
  carried and applied the same way; a document that omits it, as one exported before the setting existed does, leaves
  the stored value unchanged. `auto_switch_on_usage_limit`, `sidebar_order_by_status`, `auto_start_quota_windows`,
  `auto_approve_session_requests`, `auto_approve_resume_requests`, `auto_approve_account_switches`,
  `auto_approve_handoffs` and `ask_questions_as_choices` are carried and applied the same way; the four approvals make
  the daemon approve agents' session and resume requests, account switches and handoffs without asking the user, which
  is why the review states it when the document turns one on. `ask_questions_as_choices` is on by default, so a document
  usually turns it off; the review shows either change as it does for the other switches. `claude_compatibility` is
  carried and applied the same way, agent by agent, as the user's explicit choices only (an agent never chosen for is
  not exported, so the importing machine keeps following its own default for it; an agent the document lists that has no
  such setting is skipped, in the review as in the apply); `git_workflow` is carried and applied the same way, item by
  item; a document without it leaves the git workflow settings as they are. A project's own `git_workflow` override is
  not a setting of this kind: it travels with the project in both export scopes and a created project takes it, every
  item following the application-wide value when the document omits it. They are applied last, through the same path as
  `update_settings` (so they are normalised and `settings_updated` is broadcast when the value changes), and
  `settings_applied` in the result reports what was applied. Asking for them of a document that has none is refused
  (`import_settings_missing`) rather than ignored, so a request never silently does less than it was told to. Settings
  that could not be applied are `settings_error`, and the import is reported all the same. `keyboard_shortcuts` is the
  one setting the daemon does not apply: it is the client's changed shortcuts by command id, `{}` meaning every shortcut
  at its default (which is what a client with none changed writes), and the daemon echoes it in `settings_applied` when
  the settings were applied, for the client to apply to itself.
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

### Branches and worktrees bound to a session

Sessions store no branch or worktree. A local branch is **bound** to a session when its responsible-session record
names that session (see `BranchOriginInfo`) and it is still the branch the record was made for; a linked worktree is
bound when it has such a branch checked out. The sessions' projects say which repositories are looked in; a console
session has none.

A `SessionBranchItem` is `kind` (`worktree` or `branch`), `repository` (the repository's common directory), `branch`
(for a worktree, the branch it has checked out, null for a detached `HEAD`), `path` (a worktree's root; null for a
branch) — all three wire paths — `project` (the project of the item's session, or the removed project's for a
worktree bound to none), `session` (the branch's responsible session, null for a worktree a project's removal takes
although no session is bound to it), `state`, `uncommitted` (whether a worktree has modified or untracked files,
whatever its state; a status that cannot be read counts as changes; false where it was not read), `detail` and
`cause` (absent but for a `failed` item whose reason a client words itself: `project_folder_unknown` where whether the
worktree is or holds a project's folder could not be told; `detail` then says so in English). `state` is one of:

| `state` | Meaning |
|---|---|
| `removable` | Goes without asking: a worktree with no modified or untracked files, or one whose directory is already gone, or a branch merged into its upstream (into the main worktree's `HEAD` when it has none) and checked out nowhere that stays |
| `uncommitted_changes` | A worktree with modified or untracked files |
| `submodules` | A worktree with initialized submodules |
| `unmerged` | A branch not merged as above |
| `checked_out` | A branch checked out in a worktree that stays: the main worktree, a worktree not bound, or a bound one that is kept |
| `locked` | A worktree locked with `git worktree lock` |
| `project_folder` | A worktree that is a project's folder, holds one, or holds a project's Git repository (see below) |
| `failed` | It could not be removed, or git could not say where it stands, or whether it is or holds a project's folder could not be told (`cause` `project_folder_unknown`, see below); `detail` is git's message, why the move to the Trash failed, or that it could not be told |

Removal runs in the repository's common directory once none of the sessions that are still archived, or deleted, has a
process any more (it gives up waiting after 30 seconds), and only for those sessions; the removals of one repository
run one at a time. A `removable` worktree goes through `git worktree remove` and a branch no worktree left holds through
`git branch -d`, git itself making the final check. Only `uncommitted_changes`, `submodules`, `unmerged` and `locked`
items named in a request's `force_remove` go although git would refuse (`git worktree remove --force`, `git branch -D`,
`git worktree remove` forced twice for a lock), and only while they still stand as named: a `force_remove` entry is
`{kind, repository, branch, path, state, uncommitted}`, the item as the user was shown it, and a worktree in another
state by then, or with uncommitted changes `uncommitted` did not show, is left. Every other state is left whatever is
asked. An agent's own archive forces nothing. `keep` takes entries of the same shape: the refused items the
confirmation listed and the user chose to keep, so that one still standing so is not reported. The main worktree is
never removed. A failure leaves the item where it is, unless it turns out to be gone all the same, and never fails the
archive or deletion.

Whether a worktree is or holds a project's folder is told by device and inode, never by the paths' text, so a symbolic
link, a case-only respelling, a bind mount or a macOS firmlink naming the folder makes no difference. A worktree holding
the common directory of a project's Git repository, as a main worktree does that a linked worktree elsewhere works from,
counts as holding that project's folder. A project's path
with nothing at it, or that cannot be read for want of permission, is compared by its text instead, as written and,
where it resolves, as resolved, with the worktree's root; so is a root with nothing at it, which holds nothing else.
Any other path that cannot be resolved — a project's after an I/O error, on a file system that reports no inode, or
that gives no answer within a few seconds, and the worktree's root when it cannot be read or resolved — leaves the
worktree as if it held one: `failed`, with `cause` `project_folder_unknown` and `detail` saying that whether it is or
holds another project's folder could not be told, and reported as left. The same holds for the worktrees a project's removal takes along
(`inspect_project_removal`), which `delete_project` looks at again just before it moves them to the Trash.

### Process trees

Agent CLIs run each shell command in a terminal session of their own, so what a command leaves running (a dev server, a
preview, a watcher) is outside the agent's own session. The daemon follows these processes itself, from the process
table, for every agent launch (`apps/daemon/src/process_ledger/`), and stops everything attributed to a launch when its
agent process ends, before the session reads as having no process (so before an archive removes a worktree). These
requests put the ledger's view on the wire.

A project's **processes** are its trees no session owns: those left behind in it and those kept under it, listed by
`list_processes` with its `project_id` after its sessions' trees, with no `session_id`. Removing the project
(`delete_project`, or `delete_console` for each of its projects) stops them; `inspect_project_removal` lists them for
the confirmation.

A handoff, an account switch, a change of a session's model (`set_session_model`) and a change of its account's
sharing (`set_account_sharing`) carry a launch's processes over instead of stopping them: when the handoff's archive
ends the original's process, its trees become those of the receiving session's launch, and when a switch, a model
change or a sharing change ends the session's process, they are held for its relaunch and become that launch's once it
is registered. They are then listed with the receiving session's `session_id` and stopped when its process ends. What
nothing receives is stopped as before: a receiving session that has ended already, a daemon shutting down or
restarting, and a stop or wait that fails before the process went down. The one exception is a switch, a model change
or a sharing change that fails, for whatever reason, once it has ended the session's process (its relaunch does not
register or ends before it came up, among others; for a sharing change, the session is not resumed): what it carried,
and what a relaunch started meanwhile, is kept, as `keep_processes` keeps a tree, under the session's project, unless
the session is a console session or was archived meanwhile; a failed restart whose agent had already exited on its own
keeps nothing. An owner's `handover_processes` tool moves chosen trees between two running sessions the same way, and
is refused (`session_already_starting`) while the other session is under a switch, a model change, a change of its
account's sharing or a handoff; a project's processes, which no session holds, cannot be handed to one.

A `ProcessTree` is a top process (one whose parent is not attributed to the same launch) and its attributed descendants:
`id` (`<pid>:<start>` of the top, what `stop_processes` and `keep_processes` name the tree by), `session_id` (absent for
a tree no session owns: a leftover or a kept tree), `kept` (the user kept the tree), `command` (the top's arguments
joined by spaces, at most 1024 bytes; empty when they could not be read), `cwd` (null when unknown), `cwd_missing` (the
top's working directory was removed), `started_at` (milliseconds since the Unix epoch), `process_count`, `memory_bytes`
(resident memory of the tree, summed) and `ports` (the TCP ports the tree listens on, sorted). Trees are listed oldest
first.

A **leftover** is a process of the daemon's user that no followed launch has and that is not kept, working in a removed
directory under a local project's directory (its `.worktrees/` included). The daemon's own direct children, Octoboard's
shell terminals among them, and whatever runs in the terminal sessions they lead are the user's and never leftovers.
Leftovers are only listed, never stopped automatically; `stop_processes` stops one on request, and `keep_processes`
keeps one, after which it is listed as kept. Git workflow settings configure no worktree location, so a project's
directory is the only one searched.

A tree id stays good after its top exits between a listing and the request that names it (a wrapper shell ended, and its
child, reparented, is now a top of its own): for every tree id a listing showed, the daemon remembers the members that
tree has had and their descendants observed by subsequent scans, for as long as they run, and the id then reaches those
of them that still qualify, with what descends from them. An id in `keep_process_ids` that reaches nothing is logged and
otherwise ignored.

What the ledger cannot follow: a helper that a kept process starts and that leaves both the kept process's terminal
session and its parent (a double fork into a session of its own) between two reads of the process table, a couple of
seconds apart, still carries the launch's tag and is stopped with the launch. A process whose environment cannot be read
(an Apple platform binary on macOS, such as `/bin/sleep`) that a shell left running in the background is never
attributed when that shell exits before a read of the table has seen it: neither listed nor stopped with the launch. A
process that outlives `SIGKILL` (one in an uninterruptible sleep) is tried again only after a few minutes or when another
launch ends.

The archive confirmation lists the trees of every session the archive would end (the session alone, or the parent's
group) that have run for at least 30 seconds, asked as `inspect_session_branches` with `processes`, and lets the user
keep any of them (`keep_process_ids` of `archive_session`). Archives by a parent or an automatic report ask nobody:
every attributed process is stopped. A handoff's archive of the original asks nobody either, and its processes are
carried over.

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
4. Read the local state in one call, `git status --porcelain=v2 --branch --untracked-files=no --ignore-submodules=all`,
   taking `branch`, `detached`, `upstream`, `ahead` and `behind` from its `# branch.*` header lines;
   `--untracked-files=no` skips enumerating the worktree's files and `--ignore-submodules=all` the status of every
   checked-out submodule, neither of which this step ever reads.
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

### Shell terminals

A shell terminal is a plain shell the daemon runs for a project, beside its agent sessions, with no agent behind it
and nothing of Octoboard's injected into it. It is live state: held in memory, never in the store, and ending with
the daemon.

- **The shell** is the user's login shell (`$SHELL`, `/bin/zsh` when it is unset), started as one (`-l`) on a PTY of
  its own in `cwd`. It starts in the environment an agent launch takes: the snapshot of the user's login shell, under
  the same rules for reusing one. A snapshot that cannot be taken does not refuse it: the shell then starts in the
  daemon's own environment, less the variables that mark an agent session (as the snapshot is filtered), and reads
  the user's profile itself. `TERM` is `xterm-256color` either way. None of the variables Octoboard sets for an
  agent's session (its id, the daemon's port, the hook, MCP and branch reporting) is set.
- **Where it starts** is checked when it is created: the project's own directory, or with `worktree` the project's
  place in that worktree, which must be one `get_project_source` lists for the project's repository and is checked
  against the repository again (see "Project sources"); with `path`, the folder below that place which the path names,
  a symbolic link to a folder inside the scope followed. `cwd` is that directory with its symbolic links resolved.
  Afterwards the shell goes wherever the user takes it, and `cwd` does not follow.
- **`title`** is the last title the shell set with OSC 0 or OSC 2, read from its output by the same rules as a
  session's terminal title: surrounding whitespace trimmed, the leading activity glyphs dropped, and a title holding
  a control character ignored. It is the shell's name, such as `zsh`, before the shell sets one or once what it sets
  is left blank by those rules; and the user's name for it once a client renames it, from then on fixed, which
  `renamed` says.
- **`busy`** is judged twice a second while the shell runs: the shell is busy when the terminal's foreground process
  group is not the shell's own, or when a descendant of the shell still has the terminal open as its stdin, stdout or
  stderr, which covers a job left in the background (a server, say) but not a helper the prompt keeps with all three
  streams redirected away from the terminal (a prompt's git status daemon, say). A descendant whose descriptors the
  daemon may not inspect, or more descendants than one check will look at, make the shell busy too, because what
  cannot be ruled out is asked about. A shell at
  its prompt with nothing started is not busy. A client asks before closing a busy terminal and closes an idle one
  without asking; one that asks before quitting while sessions run counts busy terminals too, never idle or exited
  ones. A record is broadcast only when `busy` changes.
- **A shell that exits on its own** (`exit`, a crash) leaves its terminal with `state` `exited` and `busy` false,
  broadcast as `shell_terminal_upserted`. Its output stays replayable on the terminal socket until a client closes
  the terminal.
- **A terminal ends**, broadcast as `shell_terminal_deleted`, when a client closes it, when its project is removed
  (`delete_project`, or `delete_console` for each of its projects; unlike a running session, a running shell does not
  hold a removal up; a removal that moves the folder to the Trash ends them before the move), and when the daemon
  exits on `shutdown` or a termination signal. A shell still running is sent `SIGHUP`, as a terminal closing sends,
  which an interactive shell passes on to its jobs; one still there three seconds later is killed with its process
  group. Then the terminal is hung up: the daemon closes its side of the PTY, so a job still on it (one left running
  by a shell that exited, or sent off with `nohup` or `disown`) loses its terminal, and nothing of the terminal,
  its output included, is kept. If the daemon goes down on an internal failure, the shells' process groups are
  killed along with the agents'.
- A project may have any number of terminals. Each is attached to on `GET /ws/term/:session` by its `id`.

A shell terminal runs whatever a client types, as the daemon's own user, one request away for any client. That is no
new kind of exposure on a daemon that already lets anything local type into an agent's terminal, but it makes the
server form's want of credentials sharper: it must not be offered beyond loopback before that form has them (see "Why
the daemon is split out" in `docs/architecture.md`).

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
WorktreeInfo      { id, root, main, head?, branch?, scope_present, scope_cause? }
BrowseEntry       { name, kind: "file"|"directory"|"symlink"|"other", size?, version?,
                    target?: "file"|"directory"|"other"|"missing"|"outside",
                    ignored?: true }  // present only when Git ignores the entry
ContentSource     = { kind: "live", root_id, version }
                  | { kind: "index", worktree, blob }
                  | { kind: "commit", commit, branch?, blob }
FileContent       { size, kind: "text"|"binary", media_type?, text?, data? }
ChangeEntry       = { group: "staged"|"unstaged"|"untracked"|"committed", old: ChangeSide, new: ChangeSide, stats?: LineStats }
                  | { group: "conflicted", path, conflict }
LineStats         { added, removed }
ChangeSide        = { state: "present", path, kind: "file"|"symlink"|"submodule", source: ContentSource }
                  | { state: "absent" }
                  | { state: "out_of_scope", repository_path }
SideRead          = { state: "present", path, kind, source: ContentSource, file: FileContent? }
                  | { state: "absent" }
                  | { state: "out_of_scope" }
BranchInfo        { name, commit, upstream?, origin? }
UpstreamInfo      { name, commit }
BranchOriginInfo  { responsible_session, responsible_title, responsible_since }
ComparisonEndpoint { branch, commit, remote?, empty? }
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
- `worktrees` lists every worktree of the repository that is there now, the one holding the project included: each one's
  `id`, its `root`, whether it is the `main` worktree, the commit at its `HEAD` (null before the first commit), the
  branch checked out (a wire path, null when `HEAD` is detached; an unborn branch is named, with a null commit),
  `scope_present`, whether the project's scope is a directory in it reached through no symbolic link (see below), and,
  when it is not, `scope_cause`: why, in the vocabulary of `source_unavailable`'s `cause` (`missing`, `link`,
  `not_a_directory`, or `other` for any other failure to read it), or `permission_denied` when the system denied reading
  it. A bare main repository, and a worktree `git` reports as prunable, are not listed.

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

#### Ignored entries

An entry is ignored when Git would ignore it for the repository the entry is in: every `.gitignore` level,
`.git/info/exclude` and the user's global excludes, as `git check-ignore` and `git ls-files --ignored` apply them,
so a tracked file is never ignored and everything inside an ignored folder is. A `project_dir` entry carries
`ignored: true` when it is, and a project outside a repository has none. A project below the repository's root is
judged by the whole repository, and one that is itself ignored (or inside an ignored folder) has every untracked
entry ignored (a tracked file in it is not marked).
A folder that is a repository of its own (a submodule, a clone inside the project) is judged by that repository's
rules, in a listing and in a search alike.

A listing asks `git check-ignore` about its entries, once (more only for over a thousand names, or about 96 KiB of
them), within 5 s for all of its runs. A folder's answer is remembered (32 folders, at most 30 s) and reused while
its entries' names and kinds, the index, `info/exclude` and the `.gitignore` files from the repository's root down to
the folder are as they were; a change to the global excludes file is therefore seen within 30 s. A search asks
`git ls-files --others --ignored --directory` for the project's repository, and once more for each repository
nested in it that its traversal enters, together within 2 s counted from the start of the search (inside its 5 s),
at most 4 MiB of output and 100,000 paths.

Both are advisory: a `git` that cannot be run, fails, or runs past its deadline or budget leaves everything not
ignored instead of failing the read, and a search that way does not say `complete: false`, since it holds more
paths than it should rather than fewer. Only a cancel ends the read. Known limitations: `git` is run with
`core.precomposeUnicode` off so that its names can be compared with the directory's own, so a pattern with
non-ASCII text matches only a name in the same Unicode form as the pattern; and a search that finds a nested
repository only after its 2 s are spent judges it by the repository around it.

#### Searching file names

`search_project_files` searches the project's own directory and all real subdirectories, including hidden entries,
and ignored ones when `include_ignored` is true (otherwise ignored files, and ignored folders with all beneath them,
are not matched or entered; see "Ignored entries"), excluding every entry, with all beneath it, whose name is one of
those `docs/product/project-pane.md`'s "What is listed" section leaves out (`.git` and the files an operating system
leaves in folders, such as `.DS_Store`). It matches only the basename, read as UTF-8 with replacement characters,
against the trimmed query using Unicode lowercase on both. An empty or whitespace-only query matches all files, with
the same traversal and result budgets. Returned paths retain their canonical wire encoding; their byte order is the
result order.

Directory symbolic links are not traversed, preventing cycles and duplicate subtrees. A symbolic link to a regular
file is a result only when its target stays inside the project. Files are named without reading their bodies.
The traversal uses the same no-follow directory opens as ordinary live reads. An unreadable entry or subtree makes
`complete` false while other subtrees continue; failure to open the project root fails the request.

The scan stops with `complete: false` at the traversal, depth, result, path-byte or elapsed-time budgets below.
Results are the matches reached before stopping, sorted afterwards, rather than the first matches of a complete
sorted traversal. Cancellation is checked between entries. Elapsed time is also checked between entries, so an
individual filesystem operation on a stalled volume can outlast the time budget. A search reserves the reply
memory of one directory listing and that of its ignored paths (see "Browse budgets").

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

A listed change carries `stats`, the lines it adds and removes (`added`, `removed`), for a change of files inside the
project; it is absent where the daemon does not count, and a client shows nothing then. The counts of a staged and an
unstaged change are `git diff --numstat` (`--cached` for the staged ones, submodules ignored), a comparison's are
`git diff-tree --numstat` and a commit's `git log --numstat`, each run beside the listing's own read, limited to the
project's directory and with rename detection on, so a rename counts only what changed on top of it and an unstaged
change counts against the index. The listing detects renames over the whole tree and a count run only within the
project's directory, so a count is kept only for a change of the same shape: a rename's for a change with the same source
and destination, and any other for a change that is no rename. Counts are advisory: a run that fails, prints past its
output budget or runs past its own deadline (5 s, not the `git` deadline, so that a slow run is dropped and the listing
returns at once) leaves the listing without counts and never fails it. An untracked file has no diff, so it is counted
from the disk, every line an addition, by the same rule `lines` counts (an unterminated last line counts); one over the
file body budget, one whose content looks binary the way `git` judges it (a NUL in its first 8000 bytes; its
`.gitattributes` are not consulted, so a file marked binary or `-diff` still gets a count) and every file past the
untracked-count budget are left uncounted. Never counted: a binary file (`git` prints `-`), a link, a submodule, a type
change, and a change with a side outside the project (a rename across its boundary), whose count would draw on content
outside it. A change of nothing but a mode, or a pure rename, counts 0 and 0.

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
which the file's own lines need not match. The listing carries no `lines`, since it reads no content; its entries'
`stats` are counted apart, as above. A reply that carries a side's body counts the body's bytes; for a file side that is
an object of the index or a commit, whose patch shows its
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
sent (each branch's name and its upstream's), are listed, the first in byte order; a list cut there carries
`complete: false`.

The branch the main working copy has checked out — the one `git symbolic-ref HEAD` names in the repository's common
directory, whatever format the references are kept in; none when it is detached — carries `upstream`, an
`UpstreamInfo`, when the repository has a remote-tracking branch to compare it with; no other branch carries one. (In a
bare repository `HEAD` names the default branch, which is listed as checked out all the same.) Its `name` is that branch
below `refs/remotes/` as a wire path (such as `origin/main`) and `commit` the object id the reference names, as of the
last fetch — nothing here contacts the remote. It is the remote branch the local one is set to track; when it is set to
track none, or that is not a remote branch (its upstream is another local branch), or the reference it names is gone, it
is the branch of the same name on a configured remote (`refs/remotes/<remote>/<branch>` for a `<remote>` in the
repository's configuration, so `refs/remotes/origin/backup/main` is not `main` unless a remote `origin/backup` exists),
`origin`'s when there is one, otherwise the first remote's in byte order of the names. The lookup asks `git` for those
references by their exact names, whatever the number of remote-tracking references. Besides the listing it runs
`git symbolic-ref` when some branch is checked out in a worktree (none otherwise), then one `git for-each-ref` when the
branch tracks a remote branch and, when that finds nothing, `git remote` and one more `git for-each-ref` for the name on
each remote; the same run asks for the `HEAD` of each remote too (see below). A branch for which all of that finds
nothing leaves the reply with a `default_branch` instead, an `UpstreamInfo` of the same kind: the remote branch the repository's remote default branch points at, which is what
`refs/remotes/<remote>/HEAD` names (as `git clone` or `git remote set-head` leaves it) for a `<remote>` in the
repository's configuration, `origin`'s when there is one, otherwise the first such remote's in byte order of the names,
with the object id that branch is at. Only a `HEAD` that is a symbolic reference to a branch below `refs/remotes/`
that the repository has counts: a remote whose `HEAD` is anything else is passed over for the next one, `origin`'s
included. The reply has none when no remote has such a `HEAD`. The lookup adds no run: it is the same `git remote` and
the same `git for-each-ref` as the same-named branch's, which asks for the `HEAD` of each remote beside it. That is
one to four runs in all, and usually two. A lookup that is past one of its budgets (see "Browse budgets")
gives up with what it has found, which is nothing unless the branch's upstream was found first, and `HEAD` that `git`
cannot answer for is such a failure; the reply says `upstreams_complete: false` (absent otherwise), so that a client does
not take the branch for one that has none.

A branch the daemon can name a responsible session for carries `origin`, a `BranchOriginInfo`: `responsible_session` is
the session working on the branch now (it may be gone), `responsible_title` is the title it had when it became
responsible, and `responsible_since` is when it did, in milliseconds. The session whose own git reported creating the
branch is responsible for it until the record is moved to another; nothing else about who created it is kept. A project
session's agent is started with `core.hooksPath` pointed, through `GIT_CONFIG_COUNT`, at hook dispatchers of the
daemon's, and when a reference transaction of any git in that agent's process tree commits a branch that did not exist
while the transaction was prepared, the `reference-transaction` dispatcher reports it to `POST /git-ref/:session`
(below) — a forced update of an existing branch is not a creation; every other hook, and that one too, runs the hook the
user's own configuration names. The `reference-transaction` and `pre-push` dispatchers also run the revert gate, after
the user's hook has passed, as `POST /revert-gate/:session` (below) describes. The report is recorded only for a
repository the session's own project is in, and, where the repository keeps reflog files, only while the branch's
reflog still begins with its creation at the reported commit.
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
base's full id as `base`. Where there are several best common ancestors, the one `git merge-base` prints is used. Two
branches with no commit in common have no merge base, and are refused with `no_merge_base` (`no_merge_base_shallow` in a
shallow clone) rather than compared tip to tip, which would present the whole of one branch as a change of the other.
Every change in the reply is between `base` and the right commit, in the `committed` group, its present sides' `source`
naming the commit and the blob (with no `branch`, which the endpoints carry). Two branches at the same commit have no
changes, and the reply says so by its two equal commits and a `base` equal to them; a right branch that is an ancestor
of the left one forks at its own tip and has none either.

Only the left side can be something other than a local branch. With `left_remote` it is a remote-tracking branch,
checked and looked up as exactly `refs/remotes/<left>` in the same way, and the reply's `left` carries `remote: true`; a
local branch of the same name is not the one compared. With `left_empty` it is the empty tree: `left` is ignored, the
reply's `left` is `{ branch: "", commit, empty: true }` with `commit` the empty tree's id in the repository's object
format, `base` is that id too, and no merge base is looked for, so every file of the right branch's tip is an addition.
A read of one of the comparison's changes names the endpoints back as the reply reported them; an endpoint's `remote`
makes its name checked as a remote-tracking one, and `empty` requires `base` to be the empty tree's id (`invalid_commit`
otherwise). A `right` that says `remote` or `empty` is refused with `invalid_branch_name`.

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
up, so like a comparison's changes, a commit list never moves with a branch. A `tip` equal to `base` has no commits. A
`base` that is the empty tree's id (what a comparison from the empty tree reports) is accepted in place of a commit, and
lists every commit reachable from `tip`, the root commits included; the commit budget applies as ever.
Each `CommitChanges` names the commit, its first parent (`null` for a commit with no parent: the first commit of a
history, which a branch has when it is listed from the empty tree or has merged in an unrelated history), its subject
line — read as UTF-8 with replacement characters and cut to 200 characters — and its changes against that parent that
touch the project, in the `committed` group, its present sides' `source` naming the parent's or the commit's blob. A
merge commit's changes are those against its first parent, what it took in; a commit with no parent is compared with the
empty tree. A commit that changed nothing in the project is listed with no changes. The changes are found as a
comparison's are (see "Comparing two branches" above): the whole trees are compared with rename detection on, a side
outside the scope is `out_of_scope`, and when that output is past its 8 MiB budget, a project below the repository's
root is listed within its own directory instead — and then only the commits that touch it are listed — while a project
at the root is refused (`git_output`). At most 100 commits are listed, the newest, and `commits_complete` is false when
there were more; their changes together are held to one change list's budgets, at most 10,000 entries or 2 MiB of paths,
and past that the last commit listed holds only the changes read before the budget ran out, the older commits are left
out (as is the commit the budget ran out in when none of its changes fit, so no commit is listed as having changed
nothing that did), and `complete` is false. Nothing is read from any worktree's disk or index.

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
| Ignored check, per listing | 5 s | The `git check-ignore` runs behind a listing, all together: which entries are ignored is an addition to a listing already read, and a folder in view is relisted every few seconds. Past it nothing is marked |
| Ignored folders remembered | 32 folders, 30 s | A folder's ignored entries are reused while its names and the files that decide them are unchanged; the least recently stored goes first |
| Ignored paths, per search | 2 s, 4 MiB, 100,000 paths | The `git ls-files` runs of one search together, counted inside the search's 5 s. A run past any of them is dropped and the search goes on without those exclusions |
| Change entries | 10,000 | The largest measured commit changed 704 files and 99 % changed under 170; a worktree's pending changes are smaller still. A change list past it is cut |
| Change paths | 2 MiB, as sent | Bounds a change list whose entries are few but whose paths are long, counted as JSON writes them, the repository paths of sides outside the project included. A change list past it is cut |
| Untracked files counted, per change list | 32 MiB | The bytes of untracked files a change list reads to count their lines, together; a file over the file body budget is never counted. Reached only by something like an untracked dependency directory nobody ignored, where reading every file would be the listing's slowest part. The files past it are listed without a count |
| Line counts output, per listing | 2 MiB | The `git diff --numstat` or `git log --numstat` behind a listing's `stats`, sized for what a listing carries: two counts and a path (two for a rename) a record, within the path budget. A run that prints more, as one beside a cut listing can, leaves the listing without counts |
| Line counts deadline | 5 s | The counts are an addition to a listing already read, so a slow run is dropped quickly rather than waiting out the `git` deadline; the listing goes without counts |
| Branches | 10,000 | A repository usually has a handful to tens of local branches, and one kept for years with every branch left behind a few thousand. A branch list past it is cut |
| Branch names | 2 MiB, as sent | Bounds a branch list whose branches are few but whose names are long, counted as JSON writes them, the name of an upstream with its branch's. A branch list past it is cut |
| Branch origins, per repository | 1,000 | Which session is working on a branch, kept in the daemon's database (see "Comparing two branches"). In a repository with more branches than this the ones a listing reaches first keep theirs and the rest carry none; a record past the cap is dropped oldest first, by the time its branch was created. A branch list reserves 768 bytes for each one it attaches, as JSON writes it |
| Origin title | 80 characters | The responsible session's title as the record snapshots it when the session becomes responsible, so the badge can name it once it is gone; a longer title is cut to this |
| `git for-each-ref` output for a branch list | 3.5 MiB | One branch past the budget is asked for, each a line of about 90 bytes beside its name, so this is room for names averaging some 270 bytes. Past it the branch list is refused (`git_output`) |
| Upstream lookup, reference names | 64 KiB | The exact names one `git for-each-ref` is asked for: the main working copy's branch and the `HEAD` of each remote, so room for some one thousand remotes. Past it the lookup is not made, and the list says `upstreams_complete: false` |
| Upstream lookup, `git remote` output | 32 KiB | The names of the repository's remotes. Past it neither the same-named branch nor the default branch is looked up, and the list says so |
| Upstream lookup, `git for-each-ref` output | 128 KiB | A line for each name asked for that exists, and for whatever lies below one (`refs/remotes/origin/a/b` below `refs/remotes/origin/a`). Past it the list says so |
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
| Reserved per request | about 12 MiB for a file or a project source, about 7.8 MiB for a listing, about 21.9 MiB for a search, about 11.8 MiB for a branch list, about 35 MiB for a change list or a comparison, about 35.3 MiB for a commit list, about 24.3 MiB for a change of either, or for the bodies of one | Each request reserves its worst case before it starts. For a file that is three times the file body (a text body held as read and serialized, JSON escaping at most doubling it; a binary body held base64-encoded and serialized again) and room for the rest of the frame; for a listing, its names and the rest of its entries, each held as read and serialized, and for a search the same with its ignored paths (the `git ls-files` output while it is read and the paths as a set holds them, at 64 bytes each beside their bytes); for a branch list, the `git for-each-ref` output while it is read, its branches at 128 bytes each beside their names and the origins attached to at most 1,000 of them at 768 bytes each, held as built and serialized, and the upstream lookup's names and output; for a change list, the `git status` output while it is read and its entries, at 768 bytes each beside their paths, held as built and serialized, and for a comparison the same with its `git diff-tree` output, and for a commit list the same with its `git log` output and its commits at 1,536 bytes each beside their changes, each with the line counts' output (2 MiB) and the paths and records they are matched with, at 128 bytes each beside their paths; for a change of either kind, two bodies as a file reserves them beside a patch of at most 64 KiB, a body coming with a patch only when the patch carries no content (a text patch alone, at most 4 MiB, needs no more than one body). The reservation shrinks to the serialized reply and is given back once the reply is written to the socket. A reply that would be larger than its reservation is refused (`reply_bytes`) |
| Bytes held for replies, per connection | about 48 MiB | Four file reads at their worst, so one window gets every read the daemon runs at a time |
| Bytes held for replies, MCP tools together | about 48 MiB | The `list_project_changes` and `read_project_diff` reads of every session share one connection's share, taken before the daemon-wide bytes and a read slot, so a few agents' reads cannot hold the budget a window's reads need; a read keeps its place until the agent's answer is built, and gives it back as soon as its `git` stops if the call is dropped |
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

`:session` is a session's id or a shell terminal's. A shell terminal is attached to with the same frames as a
session, and its screen is never frozen; what is said below of a dormant session's preview is a session's alone.

- Daemon to client: **binary** frames carrying raw PTY output. On attach the session's ring buffer is replayed first,
  then live output follows. Once the ring has dropped its oldest output, the replay opens with escape sequences that
  restore the terminal's current modes (alternate screen, mouse reporting, focus reporting, bracketed paste,
  application cursor and keypad, a hidden cursor, disabled line wrap), since programs announce those only once.
- Attaching to a live process, the daemon first sends one **text** frame, before the replay:
  `{"type":"replay","length":N,"offset":E,"answered":K}`. `length` is the replay's size in bytes; when it is 0 no
  replay frame follows. `offset` is the count of bytes of output the process had written when the replay was taken,
  so the end of every later output frame is `offset` plus the bytes received since. `answered` is how many of the
  replay's leading bytes hold only settled output (see "Answers to the program's queries" below), whose queries the
  client does not answer. A saved, frozen or dormant preview and an exited shell's replay get no mark.
- Client to daemon: **binary** frames are raw PTY input, written through verbatim. **Text** frames are JSON control:
  `{"type":"resize","cols":N,"rows":N}`, and `{"type":"reply","data":"…","offset":O}` (below). Supported dimensions
  are 2–1,000 columns and 1–500 rows; other sizes are ignored and logged. Input, replies and resizing are refused once
  the session's preview has been frozen.
- **Answers to the program's queries.** A program asks its terminal things through its output — the cursor position
  (`CSI 6n`), a colour (`OSC 11 ?`), the device attributes — and reads the answer as input. Every attached client's
  terminal answers them, so the client sends its terminal's answers, as opposed to the user's input, as `reply`
  frames: `data` is the answer, `offset` the end of the output frame whose parsing produced it. Of the answers to a
  point in the output, the first client's are written (that client may send several for the same point); another
  client's copies are dropped, and so is an answer to a point before the last one answered, however late it comes,
  one to output older than 5 seconds (a program waits that long for an answer at most, and one that has given up would
  read it as input), and one to a point past the output. Output older than 5 seconds or up to the last point answered
  is settled. A client sends no answers without the mark, for the settled part of its replay, or for output of an
  earlier connection. Limits: a query nobody answered and whose program gave up in under 5 seconds is still answered
  by a client attaching within those 5 seconds; the unsettled rest of a replay is answered as of the replay's end, so
  an answer another client gives to output inside it in the instant after the mark was taken is written too. Focus
  reports (`CSI I`, `CSI O`) are each client's own and go as input.
- A newly opened `starting` session has no process or saved preview yet, so attaching closes the socket straight
  away. A client waits for the session to leave `starting` (its `session_upserted`) before attaching. During a model
  change, `starting` also covers a controlled stop and the relaunched process's initial observation period:
  attaching returns the frozen preview while the old process stops, or live replay once the new process is registered.
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
- A shell terminal whose shell has exited answers an attach with its ring buffer replayed, as for a running one, and
  then closes the socket, accepting no input. A client attached when the shell exits has the socket closed once the
  shell's last output has been sent to it, and one attached when the terminal is closed has it closed then.
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
{ cwd, git_dir?, head?, refs: [{ name, oid }], unix_time }
```

`cwd` is the directory git ran the hook in and `git_dir` the `GIT_DIR` it set, if any (relative to `cwd` when
relative), both wire paths; `head` is the branch (`refs/heads/...`, a wire path) the git had checked out while the
transaction was prepared, absent for a detached `HEAD`; `refs` holds each created reference's full name (a wire path)
and the object id it was created at; `unix_time` is when the report was made, in seconds. Answers `200` with an empty
body, or `400` for a body that does not parse, and records the report afterwards as "Comparing two branches" describes;
the reporter ignores the answer. Beside the responsible session, each recorded branch keeps its target branch, the local
branch it was created from (read from the first line of its reflog, `branch: Created from <start>`: a local branch, the
local branch behind a remote-tracking one, or for `HEAD` the reported `head`; a reftable repository has no reflog and
takes `head` whatever the start point was), and its fork point, the reported commit.

## `POST /revert-gate/:session` — a session's git asking whether the revert gate applies

Not part of the application's interface. This is how `octoboardd gate-hook`, which a project session's
`reference-transaction` and `pre-push` hook dispatchers start in the foreground when git moves an existing branch or
pushes one, gets the inputs it runs the revert gate with, in its own process:

```
{ cwd, git_dir? }
```

`cwd` and `git_dir` are as in `POST /git-ref/:session`. Answers `200` with `null` for a hook that is to skip the gate —
a session that is gone or is no project's, a repository other than the one the session's project is in, the project's
effective `revert_gate` setting off (read at every request, so turning it off reaches running sessions at once;
`development` plays no part), or any failure — and otherwise with
`{ guard: [branch], targets: [[branch, target_branch]], fork_points: [[branch, commit]] }`:
`guard` is the repository's default branch (what a `refs/remotes/<remote>/HEAD` file names, `origin` preferred and else
the first remote in byte order; empty for a repository with none; only a reftable repository, which has no such files,
asks `git`), and the other two are the repository's recorded target branches and fork points, leaving out, where the
repository keeps reflog files, a record whose branch has been deleted and made again since. Answers `400` for a body
that does not parse. The hook lets the git command through whenever it gets anything else.

## `POST /context-window/:session` — a Claude Code session reporting its context window

Not part of the application's interface. This is how `octoboardd status-line`, the status line command the Claude
Code adapter injects through `--settings`, reports the context window Claude Code gave it on stdin:

```
{ window }
```

`window` is `context_window.context_window_size`, a positive number of tokens; a status line input without one sends
nothing. Answers `200` with an empty body, or `400` for a body that does not parse or a `window` of 0. The window is
held in memory for the session and stored with its context (`Session.context`) once the session's records show a
size; the reporter ignores the answer. The command then runs the status line the user's and the project's own
settings files configure, read again on every run, with the same stdin, and passes its output and exit status on;
with none configured it prints nothing.

## `POST /mcp/:token` — Octoboard's MCP tools

Not part of the application's interface. This is how a session's own MCP server — the
`octoboardd mcp` child process each adapter registers — runs a tool call against the daemon; the
desktop application never calls it.

The token in the path is what says which session is calling. It is issued per launch, dropped when
that process goes away, and never sent by the caller as an argument, so a child that rewrote its
arguments still cannot act on another session. An unrecognised token is refused without detail.

Request body is `{"tool": "<name>", "arguments": {…}}`. The answer is always `200` with `{"ok": true, "result": {…}}` or
`{"ok": false, "error": "<prose>"}`: a refused call is an outcome the model is meant to read and act on, not a transport
failure. Which tools a session may call follows from its kind, coordinator or worker capability, and current parent
relationship, and is checked here as well as in the child. A coordinator with a console parent keeps the coordinator
tool catalogue. Capability can change while a session runs, so every answer also carries `bound`, the legacy flag
meaning that the session is a worker after the call, not merely that it has a parent. The `octoboardd mcp` child,
started with `--bound` for a worker, switches the catalogue it announces to match and sends
`notifications/tools/list_changed` when it changes. A call to a tool in the other capability catalogue, but not in the
one the child last announced, is forwarded rather than refused in the child, so that the daemon decides and its answer
brings the child's catalogue up to date; a name in neither catalogue is a protocol error in the child. An agent that
checks tool names against the list it last received can still refuse such a name before the call leaves it, so the
notice that makes a session independent tells it to call `get_session` once with no arguments first, which brings its
list up to date.

Every argument named `title` names a session (`new_session`, `request_session`, `request_resume_session`,
`handoff_session`, `rename_session`), and its description tells the agent to write it in the user's language: the one
its instructions name under "The user's language" where they have that section, else the one the user converses with it
in, not that of the project, its docs and code, or a brief; where its instructions name none, a session that takes work
from a parent session uses the user's language as the parent passes it on, else that of the parent's messages. The
brief's `context` description asks the parent to say which language that is.

A `request_session` or `request_resume_session` call, and a `switch_session_account` or `handoff_session` call that the
user is asked to approve, waits for the user's answer, up to 8 minutes, below the 10-minute
timeout the `octoboardd mcp` child gives every call it forwards. A third limit is the agent's own on a tool call, and
the three are kept in this order, 8 minutes < 10 minutes < the agent's as Octoboard sets it:

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

When the agent cancels one of those calls (`notifications/cancelled`; a `switch_session_account` or
`handoff_session` that runs without asking is not one), the child closes that call's connection to the daemon, which
withdraws the request, and the agent gets a tool error saying the call was cancelled. A cancelled call of any other tool
is left to finish in the daemon. The waiting, the endings and which waiting requests a later one replaces are described
in `docs/product/session-requests.md` ("Waiting for the answer", "How a request ends").

### The tools of each role

| Tool | Console session | Project coordinator | Project worker |
|---|---|---|---|
| `add_project`, `update_project`, `get_console`, `update_console`, `get_settings`, `update_settings` | yes | — | — |
| `show_page`, `list_pages`, `delete_pages` | yes | yes, with or without a parent | — |
| `new_session`, `instruct_session`, `interrupt_session`, `archive_session`, `reopen_session`, `rename_session`, `set_session_pinned` | yes | yes | — |
| `get_session`, `list_sessions`, `read_session_history`, `get_session_branches`, `list_project_changes`, `list_project_branches`, `read_project_diff`, `list_agent_models`, `list_consoles`, `list_projects`, `list_archived`, `send_message`, `get_account_quotas` | yes | yes | yes |
| `archive_self` | yes | offered; accepted only without a parent | — |
| `handoff_session`, `handover_branches`, `handover_processes`, `switch_session_account`, `set_session_model`, `transfer_session`, `release_session` | yes | yes | — |
| `bind_to_session` | — | offered; accepted only without a parent | — |
| `request_session`, `request_resume_session` | yes | yes | yes |
| `report` | — | offered; accepted only with a parent | yes |
| `get_project` | — | yes | yes |

The role determines the catalogue; the parent relationship separately controls `report`, `archive_self` and
`bind_to_session`. A coordinator with a parent finishes through `report`, like a worker.

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
refused, with nothing changed, while the caller, or any session under it (bound to it, or bound to a project coordinator
bound to it), is locked, and while any session under it is not archived, running or interrupted alike. The refusal names
those sessions: a parent archives its sessions once their tasks are done, so an unarchived one means its task is not
finished. For one bound to a project coordinator, which the caller cannot archive itself, it advises waiting for the
coordinator to finish it or archiving the project coordinator. The answer is sent before the process is ended.

An agent's own archive — `archive_session`, `archive_self`, and the archive a `done` report with no open items brings —
removes the branches and worktrees bound to the sessions it archived as "Branches and worktrees bound to a session"
says, never forcing anything. `archive_session` waits for that removal and answers with `branches_left`, each item left
as `{kind, branch, path?, reason}` in plain text, and a `note` saying how to clean them up; it is absent when nothing is
left. `archive_self` answers before the process ends, so its `branches_left` is what the removal will leave as judged
before the archive; `report`'s answer names the same in its text. `archive_session` also takes an optional boolean
`keep`: with it nothing is removed and nothing is waited for, the branches and worktrees staying bound to the archived
sessions, and the answer carries `branches_kept: true` and a `note`. The kept branches stay bound to their sessions and
come back with them when those are reopened (a session under a project coordinator is reopened by its coordinator). The
note names the archived session's id and, since archiving a project coordinator takes the sessions under it too, the
branches each of those keeps; the archived session's own are not named, `get_session_branches` lists them. For a caller
that needs the branches now it says that `handover_branches` with `from` the archived session moves its own, and that
the branches of the sessions archived with it are not the caller's to name as `from`: it reopens the archived session
with a `text` asking it to `handover_branches` them from each of those to itself, then hands them on with `from` that
session once it is idle. This `keep` is not the control request's, which lists refused items the user chose to keep so
that they are not reported, and it never forces: there is no `force_remove`.

Scope: a console session changes only its own console and its projects (it reads other consoles' projects and sessions
like every role, see below), and reads the settings of its own only; `get_console` never returns the avatar
(`has_icon` says whether there is one) and `update_console` cannot set it, nor read or set the console's
`console_session_prompt`. A project session's `get_project` takes no `project`, only the optional `refresh` below, and
returns its own project only.
`rename_session` and `set_session_pinned` follow the rules of `instruct_session`: only a session bound to the caller,
never the caller's own and never a console session. Nothing here deletes a project or console, creates or deletes
another console, changes a project's path, source, host, remote or `trust_consent`, or writes an account or a trusted
directory.

- `get_session`, `list_sessions`, `read_session_history`, `get_session_branches`, `list_project_changes`,
  `list_project_branches`, `read_project_diff`, `list_agent_models`, `list_consoles`, `list_projects`, `list_archived`
  and `get_account_quotas` (`list_project_changes`, `list_project_branches` and `read_project_diff` read the caller's
  own console's projects only, below; the third row of the table above, without `send_message`, which writes) have one
  definition and one scope for every role: any session reads any session, and any console's project list and archive.
  What stays narrower is `get_project` (a project session's own project), `get_console` and `get_settings` (the console
  session's own console and the application's settings), and the writes. `list_consoles` answers `{ consoles: [{
  console, name, yours }] }`, every console with the id and name that `console` takes, `yours` marking the caller's own.
- `list_projects` takes an optional `console` (an id, or a name that names one console), defaulting to the caller's own,
  so for a project session the console it is in, and an optional `project` (an id, or a name that names one project in
  that console; anything else is refused) that narrows either answer to that project, which is how a console session
  reads one project, `get_project` being the project sessions' own. Both answers carry `console`, the console's id. For
  another console it answers `{ console, projects }`, each project only `{ project, name, description, tags }` — what
  `request_session` and `request_resume_session` need to target it, with no sessions or settings; reads reach any
  console, writes stay in the caller's own. For the caller's own console it answers the same shape a console session
  gets, `yours` and `owner` worked out against the caller, each project with `project`, `name`, `host`, `path`,
  `source`, `remote_url`, `default_agent`, `pinned`, `tags`, `description`, `trust_consent`, `accounts`, `launch`,
  `effective`, `git_workflow`, `git_status` and `sessions`. `get_project`, `update_project` and `add_project` answer
  with the same entries without `sessions`. `default_agent`, `accounts` and `launch` are the project's own settings as
  stored (`null`, `inherit`); `effective` is what they resolve to under the project's console, by the code a launch
  uses: `{ default_agent, accounts, launch }`, the agent a session opens on when nothing names one (a session a project
  session starts runs on the starting session's own agent instead), per agent the account id (`default` for the agent's
  default account) and `{ model, effort }` (`null` is the agent's own default). A session's own choice, and the account
  `auto_select_account` or the starting session's account takes instead, come before it at a launch. A `remote_url` is
  always given without the credentials it carries, as `https://example.com/r.git` for
  `https://user:token@example.com/r.git`. A project's `git_workflow` is `{ "override": GitWorkflowOverride, "effective":
  GitWorkflowSettings }`: the project's own choices (`null` for an item that follows the application-wide value) beside
  the resolved values, each item taking the project's own choice else the current application-wide value.
  `rebase_before_merge` and `delivery` apply only while the resolved `development` is not `in_place`. A project's
  `git_status` is its `GitStatus` as the daemon holds it in memory (see `GitStatus` above), read without going to the
  remote or running `git`: `null` while the daemon holds none for the project (nothing has checked it since the daemon
  started, or its console has not been the current one), else `{ repository, branch, detached, upstream, ahead, behind,
  activity, error, checked_seconds_ago }`, `activity` including `pushing` while a push runs. `error` has any credential
  in a URL it names removed, as `remote_url` has. `checked_seconds_ago` is how long ago the last check completed (a
  project with no repository or no remote has checks too), counted on the wall clock so a sleep of the machine is part
  of it, `null` when none has completed since the daemon started: the branch and counts are then only what the local
  re-reads gave, and in any case they may be stale by that age. A call starts a check only with `refresh`, an optional
  boolean of `list_projects` and `get_project` (default false) that runs what `refresh_git_status` runs for the projects
  the answer lists — the same one-minute floor and in-flight claim, so a project checked within the last minute, or
  being checked, is not started again, and with `auto_pull_repositories` on a branch that is behind and has not diverged
  is fast-forwarded as in any check — and waits up to 30 seconds for each of them to complete (one the floor skipped
  counts as complete) before reading the statuses. The answer then carries `git_refresh`, at the top for `list_projects`
  and in the project's entry for `get_project`: `{ finished: true }`, or `{ finished: false, unfinished, note }` with
  the ids of the projects whose check had not completed, which go on running. `refresh` is refused for another console's
  projects, which carry no status.
- `add_project` takes `tags`, `description`, `accounts`, `launch`, `pinned` and `git_workflow` besides its location
  (`git_workflow` as in `update_project`, except that an item absent or `null` follows the application-wide value, there
  being nothing to leave alone; every project a `parent` source adds takes the same values). A `git`
  `remote_url` must be an http(s), ssh or git URL (the shapes a configuration import clones from) without credentials in
  it, else `invalid_remote_url` and nothing is cloned.
- `update_project` takes `project` (id, or a name that names one project) and any of `name`, `description` (a string
  sets it; `null` or blank clears it), `tags` (replaces the list), `add_tags` and `remove_tags` (matched ignoring case;
  `tags` beside either is `conflicting_fields`), `pinned`, `default_agent` (`null` follows the console's
  `project_session_agent`, else its `default_agent`), `accounts`, `launch` and `git_workflow` (an object of the four
  git workflow items, each absent to leave alone, `null` to follow the application-wide value again, or a value; a bad
  `development` or `delivery` or an unknown key is refused by the tool, where the control request ignores an unknown key).
- `get_console` answers `console` (its id), `name`, `default_agent` (console sessions), `project_session_agent` (`null`
  follows `default_agent`), `accounts`, `launch` (the console session model and effort of each agent),
  `project_session_launch` (the pair a project session falls back to), `has_icon` and `workdir`. `update_console`
  takes `name`, `default_agent`, `project_session_agent` (`null` follows `default_agent` again), `accounts`, `launch`
  and `project_session_launch`, and a changed account reaches only console sessions opened afterwards, while a
  changed model or effort reaches the calling session on its next launch unless it was opened with its own. A changed
  `project_session_launch` reaches project sessions at their next launch, unless their project or the session sets
  its own.
- `get_settings` answers `auto_pull_repositories`, `auto_delete_unresumable_archives`, `auto_select_account`,
  `sidebar_order_by_status`, `default_clone_dir`, `git_workflow` (the four application-wide values),
  `claude_compatibility` (per agent other than Claude Code), `accounts` (`id`, `agent`, `name`, `default`, `disabled`;
  never a config directory; a default account's `name` is the one the user gave it, else `Default`, and its id stays
  `default`) and `trusted_directories`. `update_settings` takes `auto_pull_repositories`,
  `auto_delete_unresumable_archives`, `auto_select_account`, `sidebar_order_by_status`, `default_clone_dir` (blank
  resets it), `claude_compatibility` (an object of `codex` and `grok` booleans, the rest left alone) and `git_workflow`
  (an object of any of the four items, the rest left alone; a bad `development` or `delivery` or an unknown key is
  refused by the tool) through the function `update_settings` runs, so enabling the automatic pull fast-forwards the
  projects already behind at once.
- `new_session` takes `title`, `agent`, `account` (id or name, `default`), `model` and `effort` besides the brief; the
  account is read among the accounts of the agent the session will be of. A `model` or `effort` left out is the
  project's, else the console's project session value. The result carries `session`, `agent`, `account` (the id, or
  `default`) and `account_name` of the account the session opened on, the `model` and `effort` it was launched with
  (`null` is the agent's own default), and with `branches` the `branches` moved and a `note`. `title` is the session's
  title, trimmed; left out or blank, the title is the first line of `brief.goal` cut to 48 characters. Left out, `agent`
  is the caller's own agent — unless that agent has been determined unavailable or is not on the added agents list, when
  the project's and console's default applies as for `open_session` while automatic selection is off. While
  `auto_select_account` is on, a left-out `account` is the account of the session's agent whose weekly quota resets
  soonest, among accounts that can still open a session. A 5-hour, account-wide weekly, or credit-period window that is
  used up drops the whole account. The window that ranks is an account-wide weekly window, or a credit period of about a
  week (`window_minutes` from 6 to 8 days), whose `used_percent` is finite and below 100 and whose reset is strictly
  ahead. A model's own weekly window, a 5-hour window, a monthly credit period, a reset that has passed or is missing,
  and a reading taken under another directory do not rank. When the setting is off, or no such account ranks, `account`
  for a session of the caller's own agent is the caller's own account (`Session.account_id`, the default account
  included) unless that account no longer exists or it is disabled (see `account_disabled`), its quota is used up by the
  reading held (see `account_quota_exhausted`) or the reading found it not logged in (see `account_not_signed_in`); a
  session of another agent, or one whose caller's account was passed over, takes the project's account for the agent,
  else the console's, as for `open_session`. With automatic selection on, the account and agent fallback rules in the
  settings description above apply before a request is refused.
- `request_session` takes `kind` (`project` or `console`), `project`, `console`, `brief` and an optional `branches` —
  and the overrides of `new_session` — and waits for the user's answer. `branches` is a list of local branch names in
  the project's repository to hand to the new session; each must be one the caller is responsible for (see
  `BranchOriginInfo`), and a call that names one it is not responsible for, names any for a console session, or names
  any for a project that is not in a git repository, is refused before the user is asked. On approval, once the session
  has started, their responsible session becomes the new session, a branch whose responsible session changed meanwhile
  being reported in the outcome instead, and a move that cannot be recorded being reported there too, with the caller
  still responsible for the branches; nothing moves on any other ending. A project session is in the named project,
  which defaults to the caller's own, and may belong to any console; a console session is in the named console,
  defaulting to the caller's own. The new session has no parent and the caller carries on unchanged. `title` is read as
  for `new_session`, and so is a left-out `agent` when the session is in the caller's own console; in another console
  the agent is that console's and its project's, as for `open_session`. A left-out `account` follows the weekly pick
  while `auto_select_account` is on, in either console. When the setting is off, or no such account ranks, a session in
  the caller's own console takes the account `new_session` would while the setting is off, and a session in another
  console takes that console's and its project's. See `docs/product/session-requests.md`.
- `request_resume_session` asks the user to resume a conversation by its id, with what the new-session dialog's Resume
  conversation tab gives the user and the same limits, and waits for the user's answer. It takes `conversation` (the
  agent's own conversation id, a UUID in any spelling; an Octoboard session id is not accepted), and optionally
  `project` (an id, or a name that is unambiguous; a project session defaults to its own project, a console session has
  to give one), `title`, `model` and `effort`. There is no argument for an agent, account, branches, a parent or a
  message to deliver: afterwards `send_message` reaches the session. The conversation is located as
  `locate_conversation_to_resume` does it for `project`, and what the tab could not do is refused in prose before the
  user is asked, changing nothing: a `conversation` that is not a UUID (`resume_id_invalid`), a project that does not
  exist or is ambiguous, a conversation held by a session of another project (naming it and the project to ask in) or by
  a console session, one that no session holds and no agent record holds, one held by the caller itself, by a session of
  the caller's (which `reopen_session` reopens without approval), or by a session that is starting, working or otherwise
  not archived or interrupted, one whose session is being launched, a `title` for an interrupted session, a `model` or
  `effort` for a conversation a session holds, and what `resume_session` would refuse for a conversation its agent no
  longer has (`conversation_unresumable`). For an archived or interrupted session that holds the conversation, an
  approval reopens or resumes it as `resume_session` does, with no instruction, its parent staying as it is and its
  archived parents coming back with it; `title` renames an archived session after the reopen when it differs from its
  name. When no session holds the conversation and an agent's record of it is on the machine, the approval opens a
  session in the project, independent and with the user as its origin, on the agent and account that hold the record (as
  the tab does), named `title`, else for its project, with `model` and `effort` checked as for `new_session` and the
  opening checks of `open_session` run before the user is asked. The result carries `action` (`reopened`, `resumed`,
  `opened`, or `already_running` when the session was running by the time of the approval and nothing was done),
  `session`, `role`, `agent`, `title`, `console`, `project`, `owner`, `reopened_owners` (the ids of the parents the
  approval brought back), `renamed` (only when a name was given: `false` when it could not be) and a `note`. An approval
  that cannot be carried out, such as a conversation a session came to hold meanwhile, comes back as an error saying it
  was approved but not carried out. The caller carries on unchanged, is never made a parent, and the outcome is also
  written into its session as a message as for `request_session`. It shares the queue, the hand, the withdrawal and the
  answer with `request_session`, but not its setting: it is waived by `auto_approve_resume_requests`, read once when the
  call is made, and `auto_approve_session_requests` has no effect on it.
- `interrupt_session` takes `session`, one of the caller's, and runs the same function as the control request of that
  name: it answers with `interrupted`, `status` and `note` as `session_interrupted` carries them, and its refusals
  reach the model as their prose. A session that is not the caller's is refused in prose, as for `instruct_session`.
- `transfer_session` takes `session` (one of the caller's), `to` (the new parent) and `note`, and needs no approval:
  `Session.bound_to` changes, the session and the new parent are told in messages, and the note reaches the new parent.
  `bind_to_session` takes `owner` and `summary`, which reaches the parent as the caller's first report. Both refuse, in
  prose and changing nothing, a parent that breaks the rules a binding keeps; see "Who can own a session" in
  `docs/product/changing-session-owners.md`.
- `release_session` takes `session` (one of the caller's) and optionally `note`, and needs no approval: it runs the same
  function as the control request of that name, so `Session.bound_to` is cleared and the session is told in a message
  that it has no parent. A non-blank `note` of at most 4000 characters is appended to that message, quoted and marked as
  its former parent's parting note; a blank one is ignored, and a longer one is refused in prose, changing nothing. It
  is allowed whatever the session's state, refused in prose for one that is not the caller's, and answers with `session`
  and a `note` saying whether the session was told. The caller is not messaged: it knows. For a session with no process
  the `note` is stored with the owed notice (below), in the same write as the binding change, and told at the next
  launch while the release is still the latest change of parent. A message that could not be written to a running
  session keeps the note owed the same way, as long as the session is still independent. A later change of parent drops
  the note, and the launch that delivers the notice clears it with the count it read. A change of parent (a release, a
  `transfer_session`, a handoff that moves the sessions of the original) is told to a session that is not running when
  it next launches or resumes. The daemon keeps a count of owed notices for the session, outside the record clients
  read, and marks it while it holds the claim that keeps a launch out, deciding there whether the session has a process.
  The launch reads the count before it writes anything, puts a notice built then from the binding as it stands ahead of
  that launch's own instruction (Claude Code and Grok replay what they were first told of their parent on every resume)
  and, once the process has started, clears the count it read, so a change marked since stays owed. A launch that fails
  leaves it owed, and so does a message that could not be written to a session that was running. A later change replaces
  an earlier pending one, since the notice states the parent as it stands. A `transfer_session` takes a session with no
  process for the same reason; only the new parent must be running.
- `handoff_session` takes `session` (one of the caller's), `agent`, and optionally `account`, `title`, `model`, `effort`
  and `note`: it runs `hand_off_session` on the session, with `note` quoted in the receiving session's opening prompt
  and `title` (trimmed; left out or blank, the original's displayed name, task title included) as its created name. The
  receiving agent's own task title is followed separately as for any new session. A left-out `account` follows
  `auto_select_account` while it is on. When it is off, or no account ranks, it is the caller's own when `agent` is the
  caller's own agent and that account still exists, is not disabled, is logged in and has its quota not used up;
  otherwise it is `hand_off_session`'s. Unless `auto_approve_handoffs` is on, the user is asked first, in a form that
  starts on what the call asked for (the account settled as described; neither the title nor the note the agent wrote is
  shown) and lets them change the agent, account, model and effort; the call waits as `request_session` does (the
  8-minute limit, the hand, a request withdrawn when the call's connection drops or the agent cancels it), and returns
  the result of what was done, or an error saying the user refused or did not answer and that nothing was changed. It is
  refused before the user is asked for what could never be done: a session not the caller's, a session in the middle of
  a turn, the session's own agent, and what opening the receiving session refuses, except an account whose quota is used
  up or that is not logged in, which the user can replace in the form and which is refused at approval. The receiving
  session takes the original's place, so it reports to the caller as the original did, and the caller is not sent the
  message that tells a parent of a handoff. The result carries `session` (the receiving session), `agent`, `account`,
  `account_name`, `model` and `effort` (where set), `archived` (the original), `branches` and `owned_sessions` (what
  moved; `branches` is `[]`, with the reason in `note`, when the branches could not be moved), and a `note`. A session
  that is not the caller's, and anything `hand_off_session` refuses, is refused in prose and changes nothing.
- `handover_branches` takes `branches` (a non-empty list of local branch names), and optionally `from` and `to`, each
  the caller itself when left out; it needs no approval. `from` is the caller or a session it owns, archived or not,
  `to` the caller or a non-archived session it owns, and they differ. Each branch must have `from` as its responsible
  session. While it is another session that is working, starting or waiting for the user (the stored status or the
  agent's, which can show a prompt, such as Codex's "Action Required" terminal title, before the record has caught up),
  `from` is refused with `session_working`; an interrupted or archived one is not. A session other than the caller is
  claimed (`session_already_starting` while a launch, resume or switch of it is under way) and `from`'s messages are
  held while everything is checked again and the branches move. It moves the branch records only and messages `from` and
  `to`, each unless it is the caller, archived or interrupted; the message to `to`, like the opening prompt of a session
  `new_session` or `request_session` hands branches to and the message of a `handoff_session` to the session taking its
  place, names each branch's recorded target branch (`target_branch` in `get_session_branches`) where it has one. The
  result carries `from`, `to`, `branches` (per branch moved: `branch`; `worktree`, the path when a worktree has it, else
  `null`; `worktree_status`, one of `checked_out`, `gone` (git's record of the worktree is left, the directory is not),
  `none` (no worktree has it) and `unknown` (the worktrees could not be read); and `uncommitted_changes`, `null` unless
  the worktree is checked out and its changes could be read), `not_handed_over` and a `note`, which names any session
  that could not be told. `new_session` takes the same `branches` as `request_session` does, from a project session
  only, moved to the new session once it is running and named in its opening prompt, with `branches` (names of those
  moved) and a `note` added to its result. See "Handing branches between sessions of an owner" in
  `docs/product/branch-responsible-session.md`.
- `handover_processes` takes `processes` (a non-empty list of tree ids, as `processes` in `get_session` gives them), and
  optionally `from` and `to`, each the caller itself when left out; it needs no approval. `from` and `to` are each the
  caller or a session it owns, they differ, neither may be archived, and both must have a process running. Each tree
  must be one `from`'s launch holds now; a project's process, which no session holds, cannot be handed over. A `from`
  other than the caller that is working, starting or waiting for the user is refused with `session_working`; a session
  other than the caller is claimed (`session_already_starting` while a launch, resume, switch, model change, change of
  its account's sharing or handoff of it is under way), and `from`'s messages are held while everything is checked again
  and the trees move. All or nothing: a refusal moves no tree. The trees keep running and become `to`'s, stopped when
  its process ends instead of `from`'s; `from` and `to` are messaged, each unless it is the caller. The result carries
  `from`, `to`, `processes` (each tree moved, a `ProcessTree` as `to` now holds it) and a `note`, which names any
  session that could
  not be told. See "Moving processes between sessions" in `docs/product/background-processes.md`.
- `switch_session_account` takes `session` (one of the caller's) and `account` (an account's id or name, or `default`),
  read for the session's own agent: it runs `switch_session_account` on the session, which keeps its conversation.
  Unless `auto_approve_account_switches` is on, the user is asked first, in a form that starts on the account asked for
  and the session's model and effort, and lets them change each (the model and effort become the session's own
  overrides); the call waits and ends as the `handoff_session` call does. An archived session, an account the user
  turned off (`account_disabled`) and the account the session is on already are refused before the user is asked; an
  account whose quota is used up or that is not logged in is not, as the user can pick another in the form. The session
  is sent no message for it. The result carries `session`, `agent`, `account` (the id, `default` for the default
  account), `account_name`, `model` and `effort` (where the session has an override) and a `note`; the session comes
  back awaiting instructions, so the note tells the caller to send it the next step. A session that is not the caller's,
  an account of another agent, and anything `switch_session_account` refuses is refused in prose and changes nothing;
  the session must still be the caller's once the switch holds its claim.
- `set_session_model` takes `session` (one of the caller's) and an optional `model` and `effort`, at least one of them,
  given as the `model` and `effort` of `reopen_session` are: it runs `set_session_model` on the session, without asking
  the user, since the session stays on its agent and account. On top of what that request refuses, it refuses in prose
  an archived session (pointing to `reopen_session`), and with `session_waiting_for_user` a session shown
  `waiting_user`, whose prompt the restart would end; the session must still be the caller's once the change holds its
  claim. The session is sent no message for it. The result carries `session`, `agent`, `restarted` (whether its process
  was restarted), `model` and `effort` (where the session has an override) and a `note` saying that nothing changed,
  that an interrupted session takes the values at its next resume, or that it was restarted and either sent the prompt
  to continue (which the caller must not repeat) or is back awaiting its next step.
- `get_session` takes an optional `session`; left out, it reads the caller itself, which is how a session learns its own
  id. `get_session_branches` and `read_session_history` default to the caller the same way. An interrupted or
  archived session read with `get_session` also carries `resumable`: whether the agent still holds its conversation,
  judged from the agent's own records in the session's own account in the way `reopen_session` does (see
  `conversation_unresumable`), though from the daemon's own environment rather than a launch's, so it can differ at
  the margin — `true` when it does and for a session with no conversation, `false` when it is proven
  gone, so `reopen_session` would refuse it, `null` when that cannot be told, which `reopen_session` lets through. A
  running session has no `resumable`. `get_session` also carries `processes`: for a session with a process running,
  the process trees its launch holds (`ProcessTree`s, see "Process trees"), the ids `handover_processes` names; `[]`
  for any other session.
- `list_sessions` answers every running session but the caller's, in every console and project, console sessions
  included, with `owner`, `owner_kind`, `yours` and `your_owner`, narrowed by the optional `console`, `project`,
  `owner`, `yours`, `kind` (`console` or `project`, as `list_archived`'s) and `branch` (the name of a local branch: the
  sessions that are responsible for it, by the `branches` each carries) filters, all of which must match. The optional
  `status` picks which sessions are listed: `live` (the default) the ones with a process, `interrupted` the ones whose
  process ended without their being archived (the status each shows is `interrupted`; a session being restarted for a
  change of sharing counts as live), and `any` both; archived sessions are never listed, they are `list_archived`'s, and
  any other value is refused; `get_session` (whose optional `output_bytes`, 1 to 65536 and 8192 by default, sets the
  length of the terminal output tail) and `send_message` reach any session, whichever console or project it is in, while
  the tools that act on a session (`instruct_session`, `rename_session` and so on) stay with the session's parent.
  `send_message` to an interrupted session keeps the information in the store until the session is resumed, and is
  refused for an archived one. The tool is not the control request of that name: it delivers information, marked as such
  and copied to the session's parent when it has one, while the control request writes the user's own message into the
  session. See `docs/product/information-between-sessions.md`.
- `report` to a parent that is interrupted keeps the report in the same store, written into the parent once it is
  resumed, in the order it came among everything else kept for it, shared information included; its answer says it was
  kept, and a `done` report with no open items archives the reporter all the same. A report that waited for a parent
  that was handed off and archived meanwhile goes to the session that took its place. The daemon's notices follow the
  replacement the same way. A report to an archived parent is refused and nothing is kept, and so is one to a parent
  with no process that is not interrupted. A parent under a claim (a resume, an account switch, a handoff, a handover of
  branches or a change of parent) is waited for, up to 15 seconds, before the report is judged; one still without a
  process then is refused as having none, and the refusal names the parent. What the daemon sends a parent of its own
  accord — a synthesised report, and the notices that a session ended on its own, waits for the user, or had its account
  switched automatically, or that the switch failed, and the notice of a handoff — is kept for an interrupted parent the
  same way and waits out a claim alike. The notice that the user released a session from its parent is the exception: a
  parent with no process is not told, and is not owed it. Each kept report and notice carries a line dating it and
  telling the parent to check the session before acting on it. A kept notice that a session waits for the user is
  dropped once any later report or notice about the same session is kept for that parent (shared information does not
  drop it), and at the resume it is dropped if the session no longer waits for the user. See
  `docs/product/reporting-to-owners.md`.
- `get_account_quotas` takes an optional `agent` (`claude`, `codex` or `grok`), listing only that agent's accounts
  (every agent's when it is left out), and an optional `refresh`: the accounts it lists are read again first where
  their last attempt is at least 60 seconds old, as are those of an agent that is installed but not added, which the
  daemon reads as well, without ever starting a quota window. An account the user turned off is neither listed nor
  read again, the caller's own included (`get_session` still names a session's account, which is how a session finds
  its own). Each account carries `can_open` and, when it is false, `cannot_open_reason`: the account is used up, or
  its last reading found it not logged in, or a change of its sharing is running, the reasons besides being turned off
  that automatic account selection passes an account over for. The result also carries `auto_select_account`, the
  setting of that name, read-only, so a session that cannot read `get_settings` knows whether an account it leaves out
  is chosen for it. See "Reading quotas from a session" in `docs/product/account-quotas.md`.
- `show_page`, `list_pages` and `delete_pages` are the report panel's tools, offered to a console session and to a
  project coordinator, with or without a parent; a project worker has none. Pages are kept per session. `list_pages`
  lists the report pages the caller pushed, or with `page` answers one of them with its HTML. `delete_pages` withdraws
  pages the caller pushed, for good: `pages` names their ids or `all: true` takes every one, exactly one of the two. A
  named id that is not one of the caller's pages (another session's included) is refused as an id that names no page,
  and then nothing is deleted. It answers `deleted`, the ids removed (empty when `all` found none; checked and removed
  in one step, so a page two calls delete at once is reported by only one), broadcasts `pages_deleted` for them, and
  deleting the newest page makes the one before it the newest, the one `submit_page` accepts. A session that becomes a
  worker — `bind_to_session` to a project coordinator, or `transfer_session` of a coordinator to a project parent —
  loses the three tools, and every page it pushed is withdrawn at once as `delete_pages` with `all` would
  (`pages_deleted` is broadcast), so the panel stops offering their forms; a coordinator with a console parent keeps its
  pages. See `docs/product/report-panel.md`.
- Every session a tool describes (`list_sessions`, `list_projects`, `get_session` and the rest) carries the facts the
  write tools depend on: `account` (the account's id, `default` for the agent's default account, as `get_account_quotas`
  and `switch_session_account` name it) and `account_name`, never the account's directory; `observed_model` and
  `observed_effort` (what the agent's records show, `null` until reported) and `default_effort` (the effort the agent
  runs the observed model at by itself, `null` when the records name an effort or no model, or the catalogue gives
  none); `pinned`; `locked`; and `branches`, the plain names of the git branches the session is responsible for (empty
  for none, and for a console session). It also carries `display_title` (the name the session list shows: the agent's
  current task title, a fullwidth bar `｜`, then `title`; `title` alone when the agent has no task title or it equals
  `title`), `agent_session_id` (the agent's conversation id, `null` until known; `request_resume_session` takes it as
  its `conversation`, and only when the user asked for that conversation), `origin` (`console` for a session started
  with `new_session`; `user` for the user's own and for one opened through `request_session` or
  `request_resume_session`; a session that replaces another in a handoff copies the original's), `has_conversation` (a
  turn was taken; whether the agent still holds the record is not read, `reopen_session` refusing as
  `conversation_unresumable` when it does not), `model` and `effort` (the session's own launch choices as
  `Session.model` and `Session.effort` carry them: a key absent when it follows its project's or console's setting,
  `null` for the agent's own default), `orchestration_role` (`coordinator` or `worker`), the compatibility flag `lead`,
  and `owned_sessions` (its child sessions in any status, archived ones included, as the rules of `transfer_session`
  count them). Reading them runs no process: `branches` comes from the store and the reflog files, once per repository
  for a listing.
- Every session a tool describes also carries `queued_messages`, the number of messages queued for it and not yet
  written (the messages kept for it while it is interrupted included: shared information, and the reports and notices
  for a parent), and `writes_held`: `null`, or for a running session whose writes are held whatever its `status` reads,
  `{ reason, explanation }` with `reason` one of `starting` (its agent has not sent the signal that it is past its
  startup screens), `trust_confirmation`, `agent_dialog` (its agent is showing a dialog only the user can answer, such
  as Claude Code's "Resume this conversation?"; the object then also carries `dialog`, the dialog's code),
  `agent_action`, `agent_error` (its agent stopped on an error and may be showing a prompt only the user can answer;
  held for a couple of seconds or until a prompt is sent there, whichever is first), `handoff`, `sharing_change` (it is
  being restarted for a change of its account's sharing of the default configuration, and is resumed afterwards) and
  `unsent_message`, and `explanation` in prose. See "Messages held until a session can take them" in
  `docs/product/message-delivery.md`.
- `list_agent_models` takes `agent` and an optional `account` (an account's id or name, or `default`; absent is the
  default account) and answers `{ agent, account, models, unavailable }`, `account` being the id or `default`, each
  model `{ id, name, efforts }` and `unavailable` `null` or `{ code, message }` with `models` empty. It runs what
  the `list_agent_models` control request runs, cache included, and an account that does not exist or belongs to another
  agent is `unknown_account`. The `model` and `effort` of every tool that takes them, `launch` included, must be among
  what it lists.
- `read_session_history` takes an optional `session` (any session, defaulting to the caller) and optional `limit` (1 to
  100, default 20) and `before` (an entry's `index`, 0 or more, not past the history's length), and answers `{ session,
  unavailable, total, older, entries }`: the `limit` entries before `before` (the end when absent), oldest first, each
  `{ index, speaker, text, turn, at }` with `index` the entry's position in the whole history from 0, `speaker` `user`,
  `octoboard` or `agent`, and `at` `null` when the record gives no time; `older` counts the entries before the page.
  Indices hold only while entries are appended: a compacted Grok session shows only the turns since the compaction and a
  Claude Code rewind can replace entries, so a `total` smaller than an earlier one means the history was rewritten and
  the reader starts again from the latest page. In a running session the last `agent` entry of the turn still open can
  be a progress note, not a final reply. `unavailable: true` (with `total` and `older` 0 and no entries) says the record
  could not be found or read; a session with no conversation answers `unavailable: false` and no entries. It reads what
  the `read_session_history` control request reads, and no tool calls.
- `list_archived`, for every role, answers `{ sessions, total, older, next }`, the archived sessions of every console,
  console sessions included, whichever session owns them, newest archived first, narrowed by the optional `console` (an
  id, or a name that names one console), `project` (an id in any console, or a name that names one project in the
  caller's console, else in all consoles; more than one is refused; with `console` given, the id or name must be one of
  that console's projects), `owner` (a session id: the sessions that were bound to it when archived) and `kind`
  (`console` or `project`) filters, all of which must match. The optional `limit` (1 to 200, 50 by default) caps how
  many sessions are described, each in full; `total` counts all that match, and `older` how many of them come after the
  page. `next` is a cursor for the page after this one, `null` when `older` is 0. The optional `before` takes the `next`
  of the previous page, unchanged, and returns only the sessions after that position in the order (`ended_at`
  descending, then id), so pages follow until `older` is 0. The cursor is a position, not a session, so it holds after
  the session it was taken from is reopened, archived again or deleted; a `before` that is not a cursor the daemon made
  (a session id, say) is refused, with nothing listed. The optional `yours` (boolean) keeps the sessions that were bound
  to the caller when archived (`true`) or those that were not (`false`), as `list_sessions` does for running ones. A
  `console`, `project` or `owner` that names nothing, a `kind` of any other value, a `limit` out of range, and `kind:
  console` together with a `project` (a console session is in no project) are refused. `list_sessions` takes `console`,
  `project`, `owner`, `kind` and `branch` the same way. The optional `branch` keeps the sessions responsible for the
  local branch of that name, and `total` counts what it keeps. The optional boolean `check_resumable` adds `resumable`
  (as `get_session`'s) to each session listed; it is off by default because it looks through the agents' record
  directories once for each session listed.
- `reopen_session` takes `session` (one of the caller's, archived or interrupted), an optional `text` to deliver once it
  runs, and an optional `model` and `effort` to reopen it with; they are given as the `model` and `effort` of
  `new_session` are, and become the session's own overrides. They are checked against what the session's agent offers
  under the session's own account, as `list_agent_models` lists it (`invalid_launch_setting` otherwise, with nothing
  reopened or recorded), and put back if the relaunch fails; a value left out keeps what the session has, and a blank
  one is the agent's own default. The session stays on its agent and account (`switch_session_account` moves it).
  Archived parents that come back with it keep their settings. It answers `{ session, account, account_name, model,
  effort }`: the account the session is on, as `new_session` names it, and the model and effort the relaunch used (the
  session's own, else its project's or console's; `null` is the agent's own default).
- `list_project_changes` and `read_project_diff` are the browse requests of the Git mode (`list_project_changes`,
  `compare_project_branches`, `list_project_commits`, `read_project_change`, `read_project_commit_change`; see "Browsing
  a project"), served by `browse::serve` for a session instead of a window: the path checks, the `git` invocation and
  the rule that nothing is written to the repository are theirs, and each read waits for a read slot and its reservation
  from the same daemon-wide gate as a window's, all of them together held to one connection's share of the retained
  bytes ("Browse budgets"), not a connection's own bounds. They are offered to every role, and read only a project of
  the caller's own console (a project of another console is refused, since `list_projects` lists those without their git
  status); `project` defaults to a project session's own project and is required of a console session.
  `list_project_changes` takes `worktree` (a worktree's path or the branch checked out in it) for the uncommitted
  changes of another worktree, or `branch` with `base` (and `base_remote` for a remote-tracking base) for what the
  branch changed since it forked (with `base` left out it is the target branch recorded for `branch`, as
  `get_session_branches` gives it as `target_branch`; a `branch` with none recorded, or whose recorded target no longer
  exists, and `base_remote` without a `base`, are refused), with `commit` to narrow that to one of its commits, and
  pages its `changes` by `limit` (200, at most 1000) and `offset`. It answers `{ project, view, changes, total, offset,
  next_offset, complete }` plus, for `uncommitted`, `worktree: { path, branch, head }` (`path` and `branch` plain text,
  as `get_session_branches` gives them); for `branch`, `branch`, `base`, `branch_commit`, `base_commit`, `merge_base`,
  `commits: [{ commit, parent, subject, files }]` (at most 100, newest first) and `commits_complete`; for `commit`,
  `commit`, `parent` and `subject`. A change is `{ group?, status, path, old_path?, kind?, outside_project?: { side,
  repository_path }, lines_added, lines_removed }`, `group` only in the uncommitted views. `read_project_diff` takes
  `path`, `old_path`, then `group` (with `worktree`) or `commit` with `parent`, and `offset` and `max_bytes` (32768, at
  most 131072) for the window of the diff it returns; it answers `{ project, path, old_path?, format, text, size_bytes,
  offset, next_offset, truncated, note }`, `format` being `patch`, `file` (an untracked file's whole content), `binary`
  or `none`. A window ends on a line where one lies in it; a patch or file over the browse layer's 4 MiB is refused, not
  cut. `base`, the branch compared with, is the caller's to name or else the recorded target branch, and `branch`,
  `base` and a `worktree` given by branch are plain names (the wire spelling is accepted for a worktree path or branch
  too). `path` and `old_path` keep the wire spelling, so that what a change lists is what the diff takes.
- `list_project_branches` is served with the browse requests `get_project_source` and `list_project_branches` (see
  "Browsing a project"), for a session the way `list_project_changes` is: offered to every role, the caller's own
  console's projects only, `project` defaulting to a project session's own. The two requests run one after the other,
  each admitted on its own by the same gate with its own read slot and reservation. It takes optional `limit` (1 to
  1000, 200 by default) and `offset`, and answers `{ project, branches, total, offset, next_offset, complete,
  upstreams_complete, default_branch }`, the local branches in byte order of their names, each `{ branch, tip, upstream,
  worktree, origin }` (names as plain text): `upstream` is `{ branch, commit }` for the branch the main working copy has
  checked out when it has one, else `null`; `worktree` is `null`, or `{ path, main, holds_project_directory }` for the
  worktree that has the branch checked out; `origin` is `null`, or the record the Git mode's `origin` is made from — the
  branch's responsible session, even one that no longer exists — as `{ session, title, session_state, since,
  target_branch, fork_point }`: `title` is the session's title when it became responsible, `session_state` is `live`,
  `interrupted`, `archived` or `gone` (no record of the session is left), `since` the milliseconds since the epoch of
  its creation or handover, `target_branch` the local branch it was created from (`null` when from something that is
  none) and `fork_point` the commit it was created at. `default_branch` is the reply's `default_branch` as `{ branch,
  commit }`, given only when the main working copy's branch has no `upstream` and the daemon found the remote's default
  branch, `null` otherwise. The records are those the listing has just reconciled. It does not extend
  `handover_branches`, which still moves only the branches of sessions that exist.
- `get_session_branches` takes an optional `session` (any session, defaulting to the caller) and answers `{ session,
  branches }`, each branch `{ branch, project, worktree, worktree_status, uncommitted_changes, target_branch,
  fork_point, responsible_since, archive }`: `worktree_status` is `checked_out`, `gone`, `none` or `unknown` (`git`
  could not be run or the worktrees could not be listed), `worktree` the path when there is one, and
  `uncommitted_changes` a boolean for a checked-out worktree and `null` otherwise, also when that worktree's changes
  could not be read. It runs `git` for the worktrees only when the session is responsible for a branch, and answers an
  empty list for a console session. The branches it lists are the ones `handover_branches` and the `branches` of
  `new_session` and `request_session` need the caller to be responsible for, and the ones archiving the session would
  try to remove along with their worktrees. `target_branch` is the local branch the branch was created from, the one to
  merge it into, `null` when it was created from something that is none; `fork_point` the commit it was created at;
  `responsible_since` the milliseconds since the epoch at which the session became responsible, by creating it or by a
  handover. `archive` is what an agent's own archive of the session would do with the branch, from the same judgement as
  `inspect_session_branches` but for what forces nothing: `{ removed, branch, worktree }`, `removed` whether the
  branch and its worktree both go, and `branch` and `worktree` (`worktree` is `null` when no linked worktree has the
  branch: the main working copy is
  never removed, so a branch checked out there has `worktree_status` `checked_out` and a `null` `archive.worktree`) each
  `{ state, reason }`, `state` one of `removable`, `uncommitted_changes`, `submodules`, `unmerged`, `checked_out`,
  `locked`, `project_folder` and `failed` and `reason` in prose; everything but `removable` stays, since an agent's
  archive never forces. An unmerged branch that a worktree `git worktree remove` keeps unless forced has checked out
  stays `checked_out` with it, where `inspect_session_branches` lists it as `unmerged`, to be forced along with that
  worktree. `archive` is `null` when it could not be judged. The judgement is a second look at the
  repository's worktrees, made only when the session is responsible for a branch. `handover_branches` reports each
  branch it moved in the worktree part of that shape, without `project`, the origin fields and `archive`.
