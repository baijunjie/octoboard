# octoboardd protocol

The only interface between the desktop application and the daemon. There is no Tauri IPC channel and no shared state
between them (see "Why the daemon is split out" in `docs/architecture.md`), so everything the UI can do is in here.

The daemon binds to `127.0.0.1` on an ephemeral port and prints `octoboardd listening on 127.0.0.1:<port>` on stdout as
its first line; it also writes the port to `$TMPDIR/octoboardd.port`. The application reads the stdout line.

## Who may connect

The daemon has no authentication: it trusts local processes and does not trust web pages, which can reach a loopback
port and, on a WebSocket handshake, are not subject to CORS. Every request on every route, the 404 fallback included, is
checked before any handler or WebSocket upgrade runs, and a request that fails the check gets `403`.

- **`Host`** must be a loopback name: `localhost`, `127.0.0.1` or `[::1]`, with any port, compared case-insensitively
  and exactly (`127.1` and the like are refused). A missing, repeated or malformed `Host` is refused. This is what stops
  DNS rebinding.
- **No `Origin` header** is allowed. Non-browser clients send none: scripts, the hook callback (`octoboardd hook`), the
  MCP child process (`octoboardd mcp`), native applications.
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
| `create_console` | `name`, `console_session_agent`, `default_agent`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `icon?` | The daemon creates the console's working directory under `~/.octoboard/consoles/<id>/`. `icon` is validated first, as in the "Records" notes below: a failure is answered with `icon_not_an_image` or `icon_too_large`, and a blank value means unset. Each account field names the account this console should reference for that agent, by id; absent means the agent's default account. An id that names no account, or an account of another agent, is answered with `unknown_account`, and nothing is created |
| `update_console` | `console`, `name?`, `console_session_agent?`, `default_agent?`, `claude_account_id?`, `codex_account_id?`, `grok_account_id?`, `icon?` | For each account field independently: absent leaves the console's reference alone; an explicit `null` clears it, back to the agent's default account; an id is validated as in `create_console` and replaces the reference. A reference left alone is not checked again. `icon` follows the same absent / `null` / blank rule, clearing back to the default glyph. Only sessions opened afterwards take a changed reference (see `Session.account_id`) |
| `delete_console` | `console` | Takes its projects and all their session records with it. Refused while any of them is still running |
| `add_project` | `console_id`, `source` (`local`\|`parent`\|`git`), `path?`, `remote_url?`, `name?`, `default_agent?`, `tags?` | `local` associates one directory; `parent` associates every git repository directly beneath `path`; `git` clones `remote_url`, any git remote, into `path` (used as the parent directory) and associates the clone; `path` is required for `local` and `parent` only, and a `git` request without one clones into `default_clone_dir` of the settings. `github`, this source's former name, is still accepted in a request and in a stored project, and is always emitted as `git`. A `path` must be absolute or start with `~/`, and is stored lexically normalised (`.` and `..` folded, no trailing slash); a relative one is refused, because the trusted directories compare project paths and a relative path means nothing to them. Absent `tags` means none, and each added project takes the same list, normalised as in the "Records" notes below |
| `update_project` | `project`, `name?`, `default_agent?`, `pinned?`, `tags?` | An absent `default_agent` leaves it alone; an explicit `null` clears it, so the project inherits the console's default again. An absent `pinned` leaves the pin alone. An absent `tags` leaves the tags alone; a present array, even an empty one, replaces them wholesale; an explicit `null` is the same as absent |
| `delete_project` | `project`, `stop_sessions?` | Removes the association and the project's session records, archived ones included; never touches the directory. Refused with `project_has_running_sessions` while the project has live sessions, unless `stop_sessions` (absent means false) is true: the running ones are then ended first, exactly as `archive_session` ends one (sessions bound to another session of the project are ended first, so an owner is never refused over them, and a console session belongs to no project), and the project goes with them. A session being launched or resumed at that moment is not stoppable, so it refuses either way |
| `list_dir` | `path` | Answered with `dir_listing` on the asking socket |
| `get_project_source` | `project`, `slot?` | Where the project's files are read from: its resolved directory and, when it is inside a Git repository, the repository, the worktree holding it, its place in that worktree and every worktree of the repository, that one included. Answered with `project_source`. A browse request: see "Browsing a project" below for the rules every browse request shares (`slot`, budgets, its own outbound queue) |
| `list_project_dir` | `project`, `path`, `worktree?`, `slot?` | Lists one directory of the project's files on disk, `path` being a wire path relative to the project's directory (empty for the directory itself). With `worktree`, the same place in that worktree of the project's repository is listed instead. Answered with `project_dir`. A browse request |
| `read_project_file` | `project`, `path`, `worktree?`, `from?`, `slot?` | Reads one file of the project, `path` being a wire path relative to the project's directory. `from` picks the content: `{"kind":"live"}` (the default) the file on disk, `{"kind":"index"}` the blob staged for it, `{"kind":"branch","branch":…}` the blob at a local branch's tip, `{"kind":"commit","commit":…}` the blob in a commit given by its full id. With `worktree`, the disk and the index read are that worktree's. Answered with `project_file`. A browse request |
| `open_session` | `console_id`, `project_id?`, `agent?`, `account?`, `task?`, `title?`, `bound_to?` | Omit `project_id` for a console session; a console may hold any number of them at once. `agent` follows the priority in the "Which agent a session uses" section of `docs/product/sessions.md` when omitted. `account` is the account the session's agent reads, by id: absent leaves it to the console's reference for the session's agent, else the agent's default account; an explicit `null` chooses the default account outright, whatever the console refers to. An id that names no account of the session's agent is refused with `unknown_account`. `bound_to` names the session this (project) session should report to; absent means none. Ignored for a console session, which is never bound. A `bound_to` that names neither a console session of `console_id` nor an unbound project session of `project_id` is refused with `unknown_session` |
| `resume_session` | `session` | Relaunches an `interrupted` or `archived` session through the agent's own resume mechanism, re-injecting everything. A session bound to an archived owner (a console session, or a project session that started it) relaunches that owner first, and the whole request fails with the owner's own refusal if it cannot, leaving both as they were; an owner that is only `interrupted` is left alone. Resuming an owner relaunches nothing bound to it |
| `archive_session` | `session` | Ends the process and archives the session. Archiving a session that has sessions bound to it (a console session, or a project session that started some) goes by whether a process is running, not by status: refused with `session_has_running_sessions`, and nothing changed, while any session bound to it has a process (or is being launched, resumed or switched), and otherwise it archives every bound session that is not archived yet — the interrupted ones — along with the session itself, so nothing bound to it is left outside the archive. Archiving a session with nothing bound to it reaches no other session |
| `switch_session_account` | `session`, `account` | Moves the session to another account of its own agent: ends its process the way `archive_session` does (and archives nothing — the session reads `interrupted` while the process is down), copies the session's conversation record into the target account's directory, records the account and directory on the session, and relaunches it the way `resume_session` does, with every launch refusal applying unchanged. `account` is required: an id, or an explicit `null` for the default account, whose directory is resolved once from one login-shell snapshot that both the copy and the relaunch use (an absent `account` is `field_required`). The record is found by name under the root where the agent keeps its records and copied to the same path relative to the config directory, creating the directories in between and replacing an earlier copy; the original stays. The copy is built and checked in a transient `.octoboard-switch` directory at the target's root, which is removed again. A session with no conversation (`has_conversation` false) has nothing to copy, and the switch records the account and relaunches into a fresh conversation. Answered once the relaunched process has stayed up for a few seconds; an `error` answer means the session is on the account it had, and the `session_upserted` events that put it back there (recording it interrupted when the relaunch did not come up) were sent before that answer. Refused, with nothing recorded and the process left running: `session_already_starting` / `session_already_running` while a launch, resume or switch of it is under way, `session_archived`, `session_already_on_account`, `unknown_account` for an id that names no account of the session's agent, `grok_home_not_initialized` for a Grok Build target that was never run against (checked before anything is copied into it), and `conversation_not_found`. Refused after the process was ended, with nothing copied or recorded: `session_did_not_stop`. The session stays claimed against a resume or another switch until the answer is sent. Once the process has been ended and the conversation is being moved: `relocation_failed` when the copy does not complete, the launch's own refusals (`config_dir_unreachable`, `directory_unreachable`, `binary_not_found`, …) and `switch_did_not_come_up` when the relaunched process ends at once — each leaves the session `interrupted` on its previous `account_id` and `config_dir`, with the conversation in both directories |
| `delete_session` | `session` | Removes Octoboard's record of one archived session and broadcasts `session_deleted`. Refused with `session_not_archived` unless the session is archived. Deleting an archived owner (a console session, or a project session that started some) deletes the archived sessions bound to it as well, each broadcast as `session_deleted` (a refusal deletes none); deleting an archived bound session leaves its owner alone. An archived session whose process is still ending can be deleted; one being resumed right now cannot. Only Octoboard's own record goes: the agent's transcript and the project's directory are never touched |
| `delete_archived_sessions` | `console`, `project?`, `console_session?` | With `project` (which must belong to `console`), deletes every archived session of that project; with `console_session` (a console session of `console`, else `unknown_session`), every archived session bound to it, leaving the console session itself; with neither, every archived console session of the console together with the archived sessions bound to them. Naming both `project` and `console_session` is `conflicting_fields`. Broadcasts `session_deleted` for each. A session that stopped being archived meanwhile (a resume got there first) is skipped, not an error |
| `set_session_pinned` | `session`, `pinned` | Pins or unpins a session; broadcasts `session_upserted` |
| `send_message` | `session`, `text` | Writes a message into a running session. Refused while the session is `waiting_user`: the message would be discarded and its trailing Enter would answer whatever dialog is up. The console session's own `send_message` tool holds such a message instead of refusing it — the user can be told to answer the prompt first, the console session cannot |
| `rename_session` | `session`, `title` | — |
| `list_pages` | `console_session` | The console session's report panel pages, oldest first. Answered with `page_list` on the asking socket. A `console_session` that does not name a console session (one that names a project session, say) is refused with `unknown_session` |
| `submit_page` | `page`, `data` | A report panel form submission; `data` is the submitted form's fields as an object of strings, field name to value (a repeated name's values joined by `, `), in document order, including the pressed submit button's `name`/`value`. Written into the console session that pushed the page, found from the page itself, as a user message naming the page; held rather than refused while the console session is `waiting_user`, since the console session is not the one who has to answer that prompt. Refused when `page` is not its console session's newest page — history pages are read-only |
| `confirm_claude_trust` | `session`, `remember`, `trust_parent_dir?` | The user's go-ahead to a `claude_trust_prompt`: Octoboard may answer that session's Claude Code trust screen, which it does by typing at the session's terminal (a Down and an Enter) after checking that the screen is still up and where its cursor is. `remember` also records the project's consent (`Project.claude_trust_consent`) once the screen has been answered, so later sessions of that project are answered without a prompt, and broadcasts the updated `project_upserted`. `trust_parent_dir` (absent means false) records the project's parent directory as trusted instead, once the screen has been answered, and then `remember` adds nothing. Every project whose path lies under that directory is trusted with it — those already there, those added later by any means, repositories the console session clones or adds into it included — and the permissions and hooks in their `.claude/settings.json` then apply without asking. The screens already waiting under it are answered at once. The daemon derives the directory from the session's project (it is the prompt's `trust_dir`); a client never names one. A parent that is the filesystem root, the user's home directory (however it is spelled or linked) or a directory containing it is refused, before anything is answered, with `error` code `trust_directory_too_broad`; so is a project whose path is not absolute (`trust_path_not_absolute`), and so is any case where the home directory cannot be determined to check against (`trust_home_unknown`). Refused, with nothing recorded and nothing sent, unless `session` is a running Claude Code project session that is still waiting at its trust screen and has not been answered (a screen that is no longer waiting is answered with `error` code `claude_trust_not_waiting`, which a client shows nothing for); a failure to answer once it was accepted (the screen is not as expected, or something else typed into the session meanwhile) is an `error` with code `claude_trust_answer_failed` as well, records no consent, and is also broadcast as a `session_notice` so that it is seen even if the requesting dialog has closed. Octoboard never edits Claude Code's config files for this |
| `remove_trusted_directory` | `path` | Stops trusting a directory (compared after lexical normalisation, so a trailing slash does not matter) and broadcasts `trusted_directories_updated`. Projects' own consents and sessions already running are untouched. Removing one that is not trusted does nothing. Directories are added only by `confirm_claude_trust` |
| `update_settings` | `auto_sync_repositories?`, `default_clone_dir?` | Each settable field absent means "leave it alone". `default_clone_dir` follows the path rules of `add_project`'s `path` (absolute or `~/`, stored lexically normalised, a relative one refused), and a blank value goes back to the built-in default; a refused value leaves the whole request unapplied. Broadcasts `settings_updated` only when something actually changed, as `remove_trusted_directory` does. A change that turns `auto_sync_repositories` on also starts the immediate fast-forward pass described in "Fast-forwarding when the setting is turned on" below, whose statuses follow as `project_git_status` broadcasts; an update that leaves the value as it was starts nothing. Turning it off starts nothing and undoes nothing |
| `refresh_git_status` | `console` | Checks every project of `console` against its remote, concurrently; answered with `ack` at once, and the statuses follow as `project_git_status` broadcasts, one per project as its own check finishes. A project whose check is already running — this call raced ahead of an earlier one, or another client is watching the same console — is not started again, and nor is one whose last check completed less than a minute ago: several clients can each be polling this console on their own 5-minute interval and phase, and without this floor their sweeps would interleave into several checks per project every 5 minutes instead of one. Either way nothing is broadcast for the project that was skipped — it keeps whatever status it already had. An unknown console is `unknown_console`. See "Daemon behaviour, per project" below for what one project's check does |
| `create_account` | `agent`, `name`, `config_dir` | Both fields are required: an account always has a name and a directory. `name` is trimmed and must be non-empty and unique within `agent` (compared trimmed, case-insensitively, including against the default account's own name); a collision is `account_name_taken`, naming the account it collides with. `config_dir` must be absolute or start with `~/`, and is stored lexically normalised; existence is not checked. Broadcasts `settings_updated` |
| `update_account` | `account`, `name?`, `config_dir?` | Renames the account, repoints it, or both, independently; either field absent leaves it alone. Validated as in `create_account`. Broadcasts `settings_updated` when something actually changed |
| `delete_account` | `account` | Clears the reference of every console that refers to this account — putting each one back on its agent's default account — then removes the account. A session holding it is left alone: it already carries its own copy of the directory it launches with. Broadcasts `settings_updated`, and a `console_upserted` for every console whose reference was cleared |
| `shutdown` | — | Terminates every session process (leaving them `interrupted`) and exits the daemon |

### Daemon to client

| `type` | Fields |
|---|---|
| `snapshot` | `hosts`, `consoles`, `projects`, `sessions`, `trusted_directories`, `settings`, `git_statuses`, `agent_availability`, `home_dir` — sent once, unprompted, when a control socket connects. `git_statuses` is every `GitStatus` the daemon currently holds, empty on a fresh start — carried here, like the trusted directories, so a reconnecting client never has to ask for it separately. `agent_availability` is always three entries, one per agent, each `not_determined` until the daemon's one-time login-shell snapshot for this run lands — see "Agent availability" below. `home_dir` is the daemon host's home directory — not the browser's, the two may be on different machines — so a client can show a path under it as `~/...`; null when the daemon cannot determine one (`HOME` unset, relative or the filesystem root) |
| `agent_availability_updated` | `agent_availability` — the whole three-entry list, sent once, when the daemon's one-time determination of it lands; never again afterwards, since nothing re-determines it during a run |
| `trusted_directories_updated` | `trusted_directories` — the whole list of trusted directory paths, sent when it changes |
| `settings_updated` | `settings` — the whole `Settings` record, sent when `update_settings` actually changes it, and also whenever the account list changes (`create_account`, `update_account`, `delete_account`, or a console dialog's save that repoints or mints one) |
| `console_upserted` / `project_upserted` / `session_upserted` | `console` / `project` / `session` — the whole record, under that key |
| `console_deleted` / `project_deleted` / `session_deleted` | `console` / `project` / `session` — the id of the record that was removed |
| `project_git_status` | `status` — one project's whole `GitStatus`, sent on every change, including every transition of `activity` (so an animated icon has something to follow). Nothing is sent when a project is removed; the client drops its status along with it |
| `session_notice` | `session`, `code`, `params`, `message` — something about a session the user has to be told that no status field carries: an injected capability that will not apply, a setting of theirs Octoboard had to work around, a message Octoboard accepted and could not deliver. Broadcast when it is found, which may be at launch or at any point in the session's life; nothing stores it, so a client that connects later does not see it. See "Coded messages" |
| `claude_trust_prompt` | `session`, `project`, `path`, `trust_dir` — a running Claude Code session of a project is at Claude Code's workspace-trust screen, which is asking whether `path` is trusted, and the project has no consent of its own (`claude_trust_consent`) and does not lie under any of `trusted_directories`. `trust_dir` is the directory `confirm_claude_trust` with `trust_parent_dir` would trust — the project's parent — or null when there is none to offer (it would be the filesystem root, the home directory or one containing it, the home directory cannot be determined, or `path` is not absolute); a client offers the button only when it is not null. Broadcast once per screen, when it is recognised in the session's terminal output. Each client is also sent one for every screen still waiting under the same condition, with the same fields, right after every `snapshot` (on connect and on lag recovery), so a client that missed the broadcast is still asked; a client already holding the prompt ignores the repeat. A client that declines ("Not now") drops the prompt locally, and a later `snapshot` may ask again. With no client connected the screen simply stays for the person to answer in the terminal. The client answers with `confirm_claude_trust`, or leaves the screen alone. A console session's screen is answered by the daemon without a prompt, because its working directory is the console's own; a project session's is when it meets the condition above the other way round. A client whose queue holds prompts for projects under a directory that has just become trusted drops them |
| `session_opened` | `id`, `session` — the reply to `open_session`, naming the session it started |
| `dir_listing` | `id`, `path`, `entries`: `[{name, path, is_git_repo}]` — only directories are listed |
| `project_source` | `id`, `source` — the reply to `get_project_source`; `source` is a `ProjectSourceInfo` (see "Browsing a project") |
| `project_dir` | `id`, `project`, `worktree`, `path`, `root_id`, `entries`, `complete` — the reply to `list_project_dir`. `project`, `worktree` and `path` echo the request; `root_id` is the identity of the directory the listing was scoped to; `entries` are `BrowseEntry` records in byte order of their names' wire forms; `complete` is false when the listing was cut at a budget or an entry could not be read |
| `project_file` | `id`, `project`, `worktree`, `path`, `source`, `file` — the reply to `read_project_file`. `project`, `worktree` and `path` echo the request; `source` is the `ContentSource` the body was read from and `file` the `FileContent` |
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
- `claude_trust_not_waiting` means nothing is wrong: the screen is no longer waiting, and a client shows nothing.
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
| `console_has_running_sessions` / `project_has_running_sessions` | — | Deleting needs the sessions archived first |
| `session_has_running_sessions` | `count`, `sessions` | Archiving a session while sessions bound to it have a process running, whatever their status; `count` is how many and `sessions` their titles, quoted and comma separated |
| `path_not_absolute` | `path` | A project path that is neither absolute nor `~`-relative |
| `path_not_found` | `path` | A directory to list does not exist |
| `path_not_a_directory` | `path` | The path is not a directory |
| `path_already_exists` | `path` | A clone's target directory exists already |
| `directory_unreadable` | `path`, `detail` | A directory exists but cannot be read (on macOS, file access to its volume is not granted) |
| `directory_unreachable` | `path` | A launch's working directory is not reachable by the daemon |
| `no_repositories_found` | `path` | No git repository directly under a parent directory |
| `all_projects_already_added` | — | Every directory found is already a project of the console |
| `repository_name_missing` | `url` | A clone URL has no repository name in it |
| `git_clone_failed` | `detail` | `git clone` failed; `detail` is its own message |
| `config_dir_not_absolute` | `agent` | An account's config directory is neither absolute nor `~`-relative; `agent` is the agent's name |
| `config_dir_unreachable` | `agent`, `path` | A session's pinned config directory has gone, which refuses the launch — only a session that has a conversation on the agent's side (`Session.has_conversation`); a new session, or one that never had a turn, launches into it instead (Claude Code creates the directory itself; Octoboard creates it for Codex, and refuses a new Codex session with this code when it cannot) |
| `grok_home_not_initialized` | `path` | A Grok Build session's pinned source home exists but Grok has never been run against it, so it carries no login and no session history for the per-session home to link; refused at launch on the same path as `config_dir_unreachable` |
| `unknown_account` | `account` | No account has this id, or the account belongs to another agent than the field naming it (a console's per-agent reference, `open_session`'s `account`) |
| `account_name_taken` | `agent`, `name` | A requested account name collides with an existing one of the same agent (trimmed, case-insensitive) — including the default account's own name; `name` is the name of the account it collides with |
| `icon_not_an_image` | — | A console's icon is not a `data:image/` URL |
| `icon_too_large` | `limit_kib` | A console's icon is longer than the limit, 256 KiB |
| `session_already_running` | `session` | The session is running already, or is not interrupted or archived |
| `session_already_starting` | `session` | The session is being started already |
| `session_already_on_account` | `session` | `switch_session_account` named the account the session is on |
| `session_archived` | `session` | `switch_session_account` for an archived session; reopening one is `resume_session`'s job |
| `conversation_not_found` | `agent`, `path` | `switch_session_account` found no conversation record of the session in the directory of the account it is on (`path`) |
| `relocation_failed` | `path`, `detail` | Copying the conversation into the target account's directory (`path`) did not complete |
| `session_did_not_stop` | `session` | `switch_session_account` ended the session's process and still did not see it gone after a generous wait; nothing was copied or recorded, and the process may still be running |
| `switch_did_not_come_up` | `session` | The relaunched process ended within moments, so the switch is reported as failed and the session stays on its previous account |
| `session_not_archived` | `session` | Only an archived session can be deleted (also: one being resumed right now cannot) |
| `session_not_running` | `session` | A message or a go-ahead for a session with no running process |
| `session_waiting_for_user` | `session` | The session waits at a prompt only the user can answer, so a message is refused |
| `queued_messages_lost` | — | A message queued for a session could not be written in full, and it and what was behind it were dropped |
| `page_not_current` | — | A form can be submitted only from its console session's newest page |
| `trust_directory_too_broad` | `path` | The directory is the filesystem root, the home directory or one containing it |
| `trust_path_not_absolute` | `path` | The project's path is not absolute |
| `trust_home_unknown` | — | The home directory cannot be determined |
| `claude_trust_not_waiting` | — | The trust screen is no longer waiting for an answer |
| `claude_trust_answer_failed` | `reason`, `reason_code`, `detail?` | Octoboard accepted the go-ahead but could not answer the trust screen; the user answers it in the terminal. See the reason codes below |
| `not_a_claude_session` | — | A go-ahead for the trust screen of a session that is not Claude Code's |
| `console_session_trust_not_asked` | — | A go-ahead for a console session's trust screen, which Octoboard answers without asking |
| `binary_not_found` | `binary` | The agent's binary (or `git`) is not on the shell's `PATH` |
| `shell_environment_timeout` | `shell`, `command`, `timeout` | The login shell did not finish printing its environment in time |
| `agent_not_available` | `agent` | `open_session`'s resolved agent has been determined unavailable (its binary does not resolve on the login shell's `PATH`); never raised while that determination is still pending. The console session's own `start_session` tool is refused the same way, as a tool error carrying this same text |
| `invalid_path` | `path` | A browse path is not a canonical wire path relative to the project (see "Wire paths"), or is too long for the operating system |
| `source_unavailable` | `path`, `detail` | The directory a browse request is scoped to — the project's own, or its place in another worktree — does not exist, is not a directory, or (in another worktree) is reached through a symbolic link; `path` is that directory's absolute path |
| `file_not_found` | `path` | Nothing is at the path in that source: no file on disk (a link leading nowhere or round in a loop included), no entry in the index or the commit |
| `permission_denied` | `path`, `detail` | The operating system refused to read the path or a directory on the way to it. `path` is absolute when the directory a request is scoped to cannot be read, and otherwise the request's own |
| `outside_scope` | `path` | The path, with its symbolic links followed, leads outside the project's directory (or the worktree), and is not read |
| `unsupported_file_type` | `path`, `file_type` | The path is not a regular file: `file_type` is `directory`, `file` (listing a file), `symlink` and `submodule` (in the index or a commit), `unmerged` (an index entry with conflicting stages), `fifo`, `socket`, `device` or `other`. A FIFO, socket or device is never opened |
| `source_changed` | `path` | What was being read changed while it was read — the file, or the directory being listed — or a symbolic link appeared on its resolved path; nothing that was read is returned |
| `limit_exceeded` | `limit`, `max`, `size?` | A browse budget was reached: `limit` is `file_bytes` (`size` the file's size, when known), `git_output`, `reply_bytes` or `pending_requests` — and, once the requests that use them are served, `patch_bytes` and `change_entries`; `max` is the budget. See "Browse budgets" |
| `not_a_git_repository` | `project` | An index, branch or commit read, or a `worktree`, for a project that is in no Git repository |
| `git_unavailable` | `detail` | The project's directory is inside a Git repository that `git` cannot read; `detail` is its message |
| `worktree_unavailable` | `worktree` | The worktree id names no worktree of the project's repository any more: removed, or its path now holds something else |
| `invalid_branch_name` | `branch` | Not a valid local branch name (a revision expression such as `main~1` or `@{-1}` is not one) |
| `unknown_branch` | `branch` | No local branch has that name |
| `invalid_commit` | `commit` | Not a full, lower-case hexadecimal object id |
| `unknown_commit` | `commit` | No commit has that id in the repository |
| `git_failed` | `detail` | `git` failed on a browse read, or did not finish within its deadline; `detail` is its message |
| `request_superseded` | — | A browse request was given up because a newer one took its slot |

`session_notice` codes:

| `code` | `params` | Meaning |
|---|---|---|
| `claude_workspace_untrusted` | — | Claude Code is not yet trusted with the session's directory, so the project's own `allow` rules are ignored until its trust prompt is answered |
| `queued_messages_dropped` | — | Octoboard dropped what it had queued for the session; its input line may hold part of a message |
| `claude_trust_answer_failed` | `reason`, `reason_code?`, `detail?` | Octoboard could not answer Claude Code's trust screen, so the user answers it in the terminal; `reason` is the daemon's English account of why and `reason_code`, when the failure has one, names it (the `error` of the same code carries the same params) |

`reason_code` values of `claude_trust_answer_failed`, which a client words in place of `reason` and shows `reason` for
one it does not know:

| `reason_code` | `detail` | Meaning |
|---|---|---|
| `screen_gone` | — | The trust screen is no longer on the terminal |
| `cursor_not_on_decline` | — | The cursor is not on the screen's first option, or could not be found |
| `cursor_did_not_move` | — | The cursor did not move to "Yes, I trust this folder", so Enter was not sent |
| `terminal_not_settled` | — | The terminal did not settle after the Down, so Enter was not sent |
| `cursor_moved_away` | — | The cursor is no longer on "Yes, I trust this folder", so Enter was not sent |
| `screen_not_dismissed` | — | The screen did not go away after Enter |
| `screen_redrawn` | — | The screen was drawn again after Enter |
| `input_touched` | — | Something else wrote into the terminal meanwhile, so the keys were not sent |
| `terminal_write_failed` | the system's own message | Writing to the terminal failed |

### Records

```
Host    { id, name, kind: "local"|"ssh", ssh_config? }
Console { id, name, workdir, console_session_agent, default_agent,
          claude_account_id?, codex_account_id?, grok_account_id?, icon?, created_at }
Account { id, agent, name, config_dir }
Project { id, console_id, host_id, name, path, default_agent?, source, remote_url?,
          claude_trust_consent, pinned, tags: string[] }
Session { id, agent, agent_session_id?, console_id, project_id?, host_id,
          role: "console"|"project", origin: "console"|"user", title,
          status: "working"|"waiting_user"|"idle"|"interrupted"|"archived",
          has_conversation, bound_to?, colour?: "olive"|"jade"|"teal"|"azure"|"violet"|"rose", ordinal?,
          account_id?, config_dir?, pinned, started_at, ended_at? }
Page    { id, console_session_id, html, anchor_message_id?, created_at }
Settings  { auto_sync_repositories, default_clone_dir, accounts: Account[] }
GitStatus { project, repository, branch?, detached, upstream?, ahead, behind,
            activity: "idle"|"checking"|"syncing", error? }
AgentAvailability { agent, availability: "not_determined"|"available"|"unavailable", default_account_dir? }
```

`Session.origin` is `console` for a session started by a session's `start_session` tool (a console session, or an
unbound project session) and `user` for one the user opened.

`agent` is one of `claude`, `codex`, `grok`. An `Account` is a named config directory of one agent, kept
application-wide and referred to by id wherever a config directory is referred to — a console's per-agent field, a
session's own copy of it. `Account.name` is required and unique within `agent`, compared trimmed and case-insensitively
(not compared across agents); the default account's own name takes part in that comparison, even though it is not a row
here — see below. `Account.config_dir` is an absolute, lexically normalised path; existence is not checked when it is
set (Claude Code creates a missing directory on first run, and the daemon creates a missing Codex directory before
launching it; Grok Build is checked at launch instead). The whole account list travels as `Settings.accounts`, and a
client is told about any change to it through `settings_updated` — there is no separate account event.

**Every agent also has a *default* account, which is not a row in `Settings.accounts` at all**: it is the state of
pinning nothing, named by `Console.*_account_id` and `Session.account_id` being unset. It cannot be created, renamed
or removed, and what it is shown as on screen is a client-side concern this protocol does not carry.

`Console.*_account_id` is the account each agent's sessions opened in this console read, by id; unset means that agent's
default account. A session reads only its own agent's. The account a session opens under is resolved, in descending
priority, from the one chosen for that session (`open_session`'s `account`), else the console's reference for the
session's agent, else the agent's default account; the project contributes the agent alone. Claude Code is launched with
`CLAUDE_CONFIG_DIR` set to the resolved directory and Codex with `CODEX_HOME`, each over any value in the user's shell
environment; unset leaves that environment as it is. For Claude Code, pointing it at `~/.claude` is not the same as
leaving it unset, because Claude Code reads its global config from `<dir>/.claude.json` whenever the variable is set and
from `~/.claude.json` otherwise — which is why the default account is never offered as a second, pinned account of its
own. For Grok it replaces `~/.grok` (or the shell's `GROK_HOME`) as the *source* directory the session's private home is
built from, so the user's config, login, trust store and session records come from it; Grok itself still runs against
that private home.
`Console.icon` is the console's custom avatar, a `data:image/...` URL of at most 256 KiB (the UI sends a 128x128
WebP or PNG); unset means the default glyph.
`Session.bound_to` is the id of the session this session reports to — a console session, or an unbound project session
of the same project that started it — or unset outside the orchestration. Set when the session is created and never
changed afterwards; always unset for a console session itself, which is never bound, and for a session that owns
another. `report` and the synthesised report both deliver to this session; see "Reporting" in
`docs/product/hub-orchestration.md`.
`Session.colour` is a console session's badge colour, one of a fixed palette — `olive`, `jade`, `teal`, `azure`,
`violet`, `rose` — assigned on creation and fixed afterwards; unset for a project session. `Session.ordinal` is a
console session's place in its console's history —
one past the highest ever used there — which gives it its default title ("Hub" for the first, "Hub `<ordinal>`"
after); unset for a project session.
`Session.account_id` is the account this session's own agent reads, by id, resolved as above when
the session is opened (unset means the default account), and changed afterwards only by a successful
`switch_session_account`. `Session.config_dir` is that account's directory at the same moment, written with it and
never alone: each agent keeps a conversation's transcript inside it, so `resume_session` relaunches with this value and
not the account's current one. It is unset for
sessions started on the default account, which resume under whatever the shell exports at that moment. A launch
whose pinned directory no longer exists is refused — for every agent — only when the session has a conversation to
resume (`config_dir_unreachable`); a new session, or one that never had a turn, launches into it instead. Grok Build
additionally refuses a pinned source home that is not an initialized Grok home (`grok_home_not_initialized`),
whether or not there is a conversation to resume, since it cannot safely create one the way the other two agents can.
`trusted_directories` is a list of absolute, lexically normalised directory paths (no symlink is resolved). A project is trusted when its path equals one or lies below one, compared component by component (`/a/Project` does not cover `/a/Project2`). They are added by `confirm_claude_trust` with `trust_parent_dir` and removed by `remove_trusted_directory`. The comparison looks at a project's path only, not at its `host_id`: there is one local host today, and a second would need its own set. A symlink is not followed when comparing, so a project reached through a link inside a trusted directory is trusted wherever the link points, and a project outside the directory is not, however it is linked from inside.
`Project.claude_trust_consent` is true once the user has agreed, in the dialog a `claude_trust_prompt` opens, that Octoboard may answer Claude Code's trust screen for that project's directory. It is only ever set by `confirm_claude_trust` with `remember`; `update_project` neither sets nor clears it.
`pinned` (on a project and on a session) is the user's pin, false until set: by `update_project` for a project and `set_session_pinned` for a session. It is only a flag for the client to order by; archiving and resuming leave it alone.
`Project.tags` are the user's free-form labels, used by the client to filter the project list; there is no tag registry, so the tags in use are the distinct ones across projects. `add_project` and `update_project` normalise what they are given: each tag is trimmed, the empty ones are dropped, and a tag matching an earlier one ignoring case is dropped (the first spelling wins). The order is kept, and a tags list never fails validation. A project from before tags existed has none.
`Page.anchor_message_id` is stored and never read (see "Data model" in `docs/architecture.md`). Timestamps are
epoch milliseconds (the UI formats them).
`Settings` is the app-wide user settings the daemon stores, plus the account list — already a record that grows
without a new request shape or a new event for every addition, which is where `accounts` belongs too.
`auto_sync_repositories` (default `false`) governs step 5 below:
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
contacted and the status read, `syncing` while the branch is being fast-forwarded, and `idle` otherwise; it is a
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

### Daemon behaviour, per project

What `refresh_git_status` does for each of a console's projects, concurrently; the daemon has no timer of its own
for this, so nothing runs until a client asks, and a project already checked within the last minute is skipped
entirely (see `refresh_git_status` above):

1. If the directory is not a git repository, broadcast `repository: false`, `activity: idle`, and stop.
2. Set `activity` to `checking` and broadcast `project_git_status`. That broadcast carries the branch, upstream,
   counts and error of the project's previous status (none before its first check), so the badge keeps showing them
   while the check runs; the check itself starts from a blank status, which the steps below fill in.
3. If the repository has no remote at all (`git remote` prints nothing), skip the fetch — that is not an error.
   Otherwise run `git fetch --quiet` in the project's directory; a failure (offline, authentication, no upstream
   remote) records `error` and the check continues to the next step regardless, so the branch name and the last
   known numbers still show.
4. Read the local state in one call, `git status --porcelain=v2 --branch --untracked-files=no`, taking `branch`,
   `detached`, `upstream`, `ahead` and `behind` from its `# branch.*` header lines; `--untracked-files=no` skips
   enumerating the worktree's files, which this step never reads anyway.
5. If `auto_sync_repositories` is on, and `upstream` is set, and `behind > 0`, and `ahead == 0`: set `activity` to
   `syncing` and broadcast, run `git merge --ff-only <upstream>`, then redo step 4 to pick up the new numbers. A
   failed merge records `error`; `git` itself refuses rather than clobbering local changes, which is why nothing
   here checks separately whether the worktree is clean.
6. Set `activity` to `idle` and broadcast.

### Fast-forwarding when the setting is turned on

An `update_settings` that actually turns `auto_sync_repositories` on fast-forwards, concurrently and right away, every
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
- Otherwise `activity` goes to `syncing` and is broadcast, `git merge --ff-only <upstream>` runs, step 4 is redone,
  and `activity` goes back to `idle` and is broadcast — as in step 5, including a refused merge recording `error`.
  An `error` the last check left is **not** cleared: it came from a step that contacted the remote, which this pass
  does not do.

Because a status is dropped only when its project or console is deleted, an `error` this pass records for a project
of a console no client is showing stays in the daemon's status for it until that console is refreshed again.

### Browsing a project

The browse requests — `get_project_source`, `list_project_dir` and `read_project_file` — read a project's files and
their Git versions on demand, for a client showing them. Their replies go to the asking socket only: nothing about
them is broadcast, nothing is kept for a later client, and none of it is part of `snapshot`, which carries no file
content at all.

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
  the branch checked out (a wire path, null when `HEAD` is detached or unborn), and `scope_present`, whether the
  project's scope is a directory in it reached through no symbolic link (see below). A bare main repository, and a
  worktree `git` reports as prunable, are not listed.

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

#### Telling a stale reply from a current one

A reply says exactly what it answers: `project`, `worktree` and `path` echo the request, and `source` (or `root_id`)
identifies the directory, the index or the commit the content came from and the version or object read. A client that
has since moved to another project, worktree, mode, file or comparison discards a reply that no longer matches what it
shows, and discards by request `id` as well: a request that finished just before a newer one in its slot arrived is
still answered with its real reply. Two kinds of change stay distinct: a reconnect, after which a client asks again
for everything it shows because the daemon keeps nothing from an earlier connection; and a change of content, which no
`snapshot` reports — an agent staging, committing or overwriting a file changes `version`, `blob` or `commit` in the
next reply, and nothing else.

#### Changes and comparisons

The requests that list a worktree's changes and compare two branches are not served yet. Their replies will identify
content in the terms above, and this is the shape they take:

```
ChangeIdentity { group: "staged"|"unstaged"|"untracked"|"committed", old: ChangeSide, new: ChangeSide }
ChangeSide     = { state: "present", path, kind: "file"|"symlink"|"submodule", source: ContentSource }
               | { state: "absent" }
               | { state: "out_of_scope", repository_path }
```

- A change is identified by its group and both of its sides, not by a path alone: a file both staged and changed
  again since has two changes, one per group.
- A present side's `path` is a wire path relative to the project's directory, as every browse `path` is.
- An absent side — the old side of an added file, the new side of a deleted one — carries no path at all, so it can
  never be resolved to a live file of the same name.
- A side whose path lies outside the project's scope (a rename into the project from elsewhere in the repository) is
  `out_of_scope`, distinct from absent, and carries its `repository_path` — relative to the repository's root, since
  no path relative to the project can name it — for display only: no body is read for it and no patch content from it
  is returned.
- A present side's `kind` says what is there: a regular file, a symbolic link or a submodule. Only a file side's body
  can be read; the patch shows a link's target as text and a submodule as its commit, the way `git` writes them. A
  type change — a file that became a link, say — is one change whose two sides have different kinds.
- A diff reply carries the `ContentSource` of both sides, and the patch and both bodies come from those same versions.
  When a live side changes between reading the patch and reading its body, the reply is `source_changed` rather than a
  patch from one version beside a body from another.
- A patch over its budget is refused with `limit_exceeded` (`patch_bytes`), never cut: an incomplete patch is not
  presented as a diff. A change list over its budget is cut and carries `complete: false`, as a listing does, its
  limit being `change_entries`.
- A branch comparison is the left branch's tip as the old side against the right branch's tip as the new side, each
  resolved to its commit as above. Its reply echoes both and what they resolved to — `left: { branch, commit }`,
  `right: { branch, commit }` — beside `project`, the `changes` and `complete`, so every change in it is identified
  against exactly those two commits.

#### Git invocation

Every `git` a browse request runs gets the same controlled environment, starting from the user's own shell environment
as every other `git` the daemon runs does:

- Every `GIT_*` variable is removed except `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM` and `GIT_CONFIG_NOSYSTEM`, which
  only say where the user's own configuration is. That takes out everything that could point `git` at another
  repository, worktree, index or object store, and configuration passed through the environment.
- `GIT_LITERAL_PATHSPECS=1`, so a file name is never a pattern; `GIT_NO_REPLACE_OBJECTS=1`, so an object id names the
  object stored under it; `GIT_NO_LAZY_FETCH=1`, so a partial clone's missing object fails the read instead of going to
  the network; `GIT_OPTIONAL_LOCKS=0`, so a read never takes the index lock from under an agent; the non-interactive
  settings every daemon `git` has; and discovery capped at the home directory as above.
- Every command runs with `--no-pager -c core.fsmonitor=false`, and every diff-family command with
  `--no-ext-diff --no-textconv --no-color`: a configured external diff, textconv filter or file-system monitor is a
  program that would run during the read and, for the first two, rewrite what is returned. Clean and smudge filters
  stay in effect, since they define what a file's content in the repository is.
- Output is read as bytes, bounded while it is read, under a deadline, and the whole process group is killed on a
  deadline, an output budget or a cancel. A `git` whose output is still held open by something it started after it
  has exited fails rather than handing back output that may stop short.

This needs Git 2.36 or later, for `git worktree list -z`. `GIT_NO_LAZY_FETCH` came with Git 2.45; an older Git ignores
it, and can still fetch a partial clone's missing object during a read.

#### Browse budgets

| Budget | Value | Why |
|---|---|---|
| File body | 4 MiB | Of the tracked files measured across seven real projects (about 3,000 files), 99.9 % were under 2.4 MB; the largest text file was a 2.3 MB string catalog and the largest image 2.6 MB. Only one file was larger than this budget, a 13 MB `.wasm` — not something anyone opens in a viewer |
| Patch, per change | 4 MiB | 99 % of the measured commits' whole patches were under about 1.2 MB, so one file's patch under this is the overwhelming case |
| Directory entries | 10,000 | Counted on disk across the same projects' worktrees, ignored directories included: the largest held 6,418 entries (a Gradle cache) and 4,536 (an icon package in `node_modules`); 99.99 % held under 1,600 |
| Listing names | 2 MiB, as sent | Bounds a listing whose names are few but long, counted as JSON writes them; 10,000 names of typical length are a small fraction of it |
| Change entries | 10,000 | The largest measured commit changed 704 files and 99 % changed under 170; a worktree's pending changes are smaller still |
| `git` metadata output | 1 MiB | A `worktree list` entry is about 200 bytes; one path's index or tree entry is one line |
| `git` stderr kept | 64 KiB | A `git` message is a few hundred bytes; the rest of a longer one is read and dropped |
| `git` deadline | 30 s | Browse reads are local, so this only catches a wedged repository (a dead network volume, a held lock) |
| Reads at once, daemon-wide | 4 | A viewer, its neighbour and a couple of directory expansions in parallel; more waits its turn. A read from disk has no deadline, so a volume that stops answering can hold these |
| Outstanding requests, per connection | 16 | Waiting, being read, or queued until their reply is written. One more is refused with `limit_exceeded` at once, so a flooding client cannot grow the daemon's backlog |
| Reserved per request | about 12 MiB for a file or a project source, about 7.8 MiB for a listing | Each request reserves its worst case before it starts. For a file that is three times the file body (a text body held as read and serialized, JSON escaping at most doubling it; a binary body held base64-encoded and serialized again) and room for the rest of the frame; for a listing, its names and the rest of its entries, each held as read and serialized. The reservation shrinks to the serialized reply and is given back once the reply is written to the socket. A reply that would be larger than its reservation is refused (`reply_bytes`) |
| Bytes held for replies, per connection | about 48 MiB | Four file reads at their worst, so one window gets every read the daemon runs at a time |
| Bytes held for replies, daemon-wide | about 96 MiB | Twice one connection's share, so a client that stops reading cannot hold every other connection's reads up |
| Writing one frame | 30 s | Any frame on the control socket, not only a browse reply. On a loopback connection a client that takes no bytes for this long has stopped reading; its connection is closed, giving back what its queued replies held, and it reconnects to a fresh `snapshot` and asks again |

The patch and change-entry budgets apply to the change and comparison requests once they are served. A diff reply
holds a patch and two bodies, and reserves its own worst case on the same principle.

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
  then live output follows.
- Client to daemon: **binary** frames are raw PTY input, written through verbatim. **Text** frames are JSON control:
  `{"type":"resize","cols":N,"rows":N}`.
- Attaching to a session whose process is not running sends what that session's last process printed, as one binary
  frame, when the daemon kept it, and then closes the socket; with nothing kept it closes straight away. Nothing is
  read from such a socket. The UI shows the stored status and offers resume, and shows this output read-only in the
  meantime.
- What is kept is the ring buffer's contents at the moment the process ended, for whatever reason it ended (archiving,
  the agent exiting or crashing, a switch of the session's account, the daemon shutting down). Each end replaces what an
  earlier one left, starting a new process removes it, and deleting the session's record deletes it. A session whose
  process ended before the daemon kept output, or ended with the daemon killed outright, has none, even if an earlier
  process left some.
- The daemon takes backpressure from each attached client: a client that stops draining for longer than the daemon's
  grace window is dropped (its socket closes) rather than allowed to make the daemon buffer without bound. A dropped
  client reconnects and gets the ring buffer replayed.

## `POST /hook/:session` — hook callback

Body is the hook's stdin payload verbatim, or empty. The event is read from the payload
(`hook_event_name` / `hookEventName`) rather than the URL, because the injected hook script takes no arguments —
Codex derives a hook's trust hash from its handler definition, so a per-event command string would need a hash per
event as well. Answers `200` with an empty body as fast as it can and never
steers the agent: the injected hook script is shaped to fail silently and fast, because every agent surfaces a
failing hook to the user.

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
follows from its role and whether it is bound, and is checked here as well as in the child.
