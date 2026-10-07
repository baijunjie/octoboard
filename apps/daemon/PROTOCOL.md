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
| `create_console` | `name`, `hub_agent`, `default_agent`, `claude_config_dir?`, `codex_config_dir?`, `grok_config_dir?`, `icon?` | The daemon creates the console's working directory under `~/.octoboard/consoles/<id>/`. `icon` is validated first, as in the "Records" notes below: a failure is answered with `icon_not_an_image` or `icon_too_large`, and a blank value means unset. Each config directory is validated first, by the same rules: it is trimmed, a leading `~` is expanded, the result is normalised lexically and must be an absolute path to an existing directory, and a blank value means unset; a failure is answered with `error` naming the agent, and nothing is created |
| `update_console` | `console`, `name?`, `hub_agent?`, `default_agent?`, `claude_config_dir?`, `codex_config_dir?`, `grok_config_dir?`, `icon?` | For each config directory independently: absent leaves it alone; an explicit `null` or a blank string clears it. A value is validated as in `create_console`. `icon` follows the same absent / `null` / blank rule, clearing back to the default glyph. Only sessions opened afterwards take a changed value (see `Session.config_dir`) |
| `delete_console` | `console` | Takes its projects and all their session records with it. Refused while any of them is still running |
| `add_project` | `console_id`, `source` (`local`\|`parent`\|`github`), `path?`, `remote_url?`, `name?`, `default_agent?` | `local` associates one directory; `parent` associates every git repository directly beneath `path`; `github` clones `remote_url` into `path` (used as the parent directory) and associates the clone. A `path` must be absolute or start with `~/`, and is stored lexically normalised (`.` and `..` folded, no trailing slash); a relative one is refused, because the trusted directories compare project paths and a relative path means nothing to them |
| `update_project` | `project`, `name?`, `default_agent?`, `pinned?` | An absent `default_agent` leaves it alone; an explicit `null` clears it, so the project inherits the console's default again. An absent `pinned` leaves the pin alone |
| `delete_project` | `project`, `stop_sessions?` | Removes the association and the project's session records, archived ones included; never touches the directory. Refused with `project_has_running_sessions` while the project has live sessions, unless `stop_sessions` (absent means false) is true: the running ones are then ended first, exactly as `archive_session` ends one, and the project goes with them. A session being launched or resumed at that moment is not stoppable, so it refuses either way |
| `list_dir` | `path` | Answered with `dir_listing` on the asking socket |
| `open_session` | `console_id`, `project_id?`, `agent?`, `task?`, `title?`, `include_in_hub?` | Omit `project_id` for the console's hub session; a console has at most one that is not archived, so a second is refused with `hub_already_running` (`hub_already_starting` while one is still being launched). `agent` follows the priority in the "Which agent a session uses" section of `docs/product/sessions.md` when omitted. `include_in_hub` defaults to false: a session the user opens by hand stays outside the hub's orchestration and sends it no reports unless this is set |
| `resume_session` | `session` | Relaunches an `interrupted` or `archived` session through the agent's own resume mechanism, re-injecting everything. Reopening an archived hub session while the console's hub is already running is refused with `hub_reopen_blocked` (`hub_already_starting` while a hub is being launched) |
| `archive_session` | `session` | Ends the process and archives the session |
| `delete_session` | `session` | Removes Octoboard's record of one archived session and broadcasts `session_deleted`. Refused with `session_not_archived` unless the session is archived. An archived session whose process is still ending can be deleted; one being resumed right now cannot. Only Octoboard's own record goes: the agent's transcript and the project's directory are never touched |
| `delete_archived_sessions` | `console`, `project?` | With `project` (which must belong to `console`), deletes every archived session of that project; without it, every archived hub session of the console. Broadcasts `session_deleted` for each. A session that stopped being archived meanwhile (a resume got there first) is skipped, not an error |
| `set_session_pinned` | `session`, `pinned` | Pins or unpins a session; broadcasts `session_upserted` |
| `send_message` | `session`, `text` | Writes a message into a running session. Refused while the session is `waiting_user`: the message would be discarded and its trailing Enter would answer whatever dialog is up. The hub's own `send_message` tool holds such a message instead of refusing it — the user can be told to answer the prompt first, the hub cannot |
| `rename_session` | `session`, `title` | — |
| `list_pages` | `console` | The console's report panel pages, oldest first. Answered with `page_list` on the asking socket |
| `submit_page` | `page`, `data` | A report panel form submission. Written into the console's hub session as a user message naming the page it came from; held rather than refused while the hub is `waiting_user`, since the hub is not the one who has to answer that prompt. Refused when `page` is not the console's newest page — history pages are read-only |
| `confirm_claude_trust` | `session`, `remember`, `trust_parent_dir?` | The user's go-ahead to a `claude_trust_prompt`: Octoboard may answer that session's Claude Code trust screen, which it does by typing at the session's terminal (a Down and an Enter) after checking that the screen is still up and where its cursor is. `remember` also records the project's consent (`Project.claude_trust_consent`) once the screen has been answered, so later sessions of that project are answered without a prompt, and broadcasts the updated `project_upserted`. `trust_parent_dir` (absent means false) records the project's parent directory as trusted instead, once the screen has been answered, and then `remember` adds nothing. Every project whose path lies under that directory is trusted with it — those already there, those added later by any means, repositories the hub clones or adds into it included — and the permissions and hooks in their `.claude/settings.json` then apply without asking. The screens already waiting under it are answered at once. The daemon derives the directory from the session's project (it is the prompt's `trust_dir`); a client never names one. A parent that is the filesystem root, the user's home directory (however it is spelled or linked) or a directory containing it is refused, before anything is answered, with `error` code `trust_directory_too_broad`; so is a project whose path is not absolute (`trust_path_not_absolute`), and so is any case where the home directory cannot be determined to check against (`trust_home_unknown`). Refused, with nothing recorded and nothing sent, unless `session` is a running Claude Code project session that is still waiting at its trust screen and has not been answered (a screen that is no longer waiting is answered with `error` code `claude_trust_not_waiting`, which a client shows nothing for); a failure to answer once it was accepted (the screen is not as expected, or something else typed into the session meanwhile) is an `error` with code `claude_trust_answer_failed` as well, records no consent, and is also broadcast as a `session_notice` so that it is seen even if the requesting dialog has closed. Octoboard never edits Claude Code's config files for this |
| `remove_trusted_directory` | `path` | Stops trusting a directory (compared after lexical normalisation, so a trailing slash does not matter) and broadcasts `trusted_directories_updated`. Projects' own consents and sessions already running are untouched. Removing one that is not trusted does nothing. Directories are added only by `confirm_claude_trust` |
| `shutdown` | — | Terminates every session process (leaving them `interrupted`) and exits the daemon |

### Daemon to client

| `type` | Fields |
|---|---|
| `snapshot` | `hosts`, `consoles`, `projects`, `sessions`, `trusted_directories` — sent once, unprompted, when a control socket connects |
| `trusted_directories_updated` | `trusted_directories` — the whole list of trusted directory paths, sent when it changes |
| `console_upserted` / `project_upserted` / `session_upserted` | `console` / `project` / `session` — the whole record, under that key |
| `console_deleted` / `project_deleted` / `session_deleted` | `console` / `project` / `session` — the id of the record that was removed |
| `session_notice` | `session`, `code`, `params`, `message` — something about a session the user has to be told that no status field carries: an injected capability that will not apply, a setting of theirs Octoboard had to work around, a message Octoboard accepted and could not deliver. Broadcast when it is found, which may be at launch or at any point in the session's life; nothing stores it, so a client that connects later does not see it. See "Coded messages" |
| `claude_trust_prompt` | `session`, `project`, `path`, `trust_dir` — a running Claude Code session of a project is at Claude Code's workspace-trust screen, which is asking whether `path` is trusted, and the project has no consent of its own (`claude_trust_consent`) and does not lie under any of `trusted_directories`. `trust_dir` is the directory `confirm_claude_trust` with `trust_parent_dir` would trust — the project's parent — or null when there is none to offer (it would be the filesystem root, the home directory or one containing it, the home directory cannot be determined, or `path` is not absolute); a client offers the button only when it is not null. Broadcast once per screen, when it is recognised in the session's terminal output. Each client is also sent one for every screen still waiting under the same condition, with the same fields, right after every `snapshot` (on connect and on lag recovery), so a client that missed the broadcast is still asked; a client already holding the prompt ignores the repeat. A client that declines ("Not now") drops the prompt locally, and a later `snapshot` may ask again. With no client connected the screen simply stays for the person to answer in the terminal. The client answers with `confirm_claude_trust`, or leaves the screen alone. A hub session's screen is answered by the daemon without a prompt, because its working directory is the console's own; a project session's is when it meets the condition above the other way round. A client whose queue holds prompts for projects under a directory that has just become trusted drops them |
| `session_opened` | `id`, `session` — the reply to `open_session`, naming the session it started |
| `dir_listing` | `id`, `path`, `entries`: `[{name, path, is_git_repo}]` — only directories are listed |
| `page_list` | `id`, `console_id`, `pages` — oldest first. Pages are not in `snapshot`: one carries a whole HTML document, and only the console whose hub is on screen needs them, so the panel asks. Asking again after every `snapshot` is what keeps it correct across a `page_created` a lagging client never received: such a client is sent a fresh snapshot in place of the events it missed, on the socket it already has |
| `page_created` | `page` — the whole record. The hub pushed a page with `show_page` |
| `ack` | `id` |
| `error` | `code`, `params`, `message`, `id?` — a failure of a request (carrying its `id`) or, with no `id`, one that belongs to no request. See "Coded messages" |

### Coded messages

Everything the daemon sends for the user to read — every `error` and every `session_notice` — carries a stable `code`
and `params`, an object of named string values. A client words the message from them in its own language; the daemon
never translates. `message` is the same text in English: it is what a client shows for a code it does not know (a
daemon newer than the client), so it must always be shown whole, never parsed. A code never changes its meaning or its
params; a new reading gets a new code.

A param named `console`, `project` or `session` is that record's id, and a client shows the record's current name
instead (the raw value only when it does not know the record). Every other param is text to be shown as is: a path, an
agent's name, an operating-system or `git` message that cannot be translated.

A failure that is not meant to be read in detail — an unexpected one with no meaning of its own — is `internal_error`
with the text as its `detail`.

A client also branches on some codes, instead of only showing them:

- `session_already_running`, `session_already_starting`, `hub_already_running`, `hub_reopen_blocked` and
  `hub_already_starting` all mean a launch was refused because the session or the console's hub is already running or
  being started. A client's own double click produces them and is not worth showing; a refused second hub session is,
  so a client branches on what it asked for rather than on the code alone.
- `trust_directory_too_broad`, `trust_path_not_absolute` and `trust_home_unknown` all mean no parent directory can be
  offered to trust: nothing was answered and the dialog stays open.
- `claude_trust_not_waiting` means nothing is wrong: the screen is no longer waiting, and a client shows nothing.

`error` codes:

| `code` | `params` | Meaning |
|---|---|---|
| `unreadable_request` | `detail` | The frame is not a request the daemon can read |
| `internal_error` | `detail` | Any other failure |
| `unknown_console` / `unknown_project` / `unknown_session` / `unknown_page` | `console` / `project` / `session` / `page` | The record is not there |
| `field_required` | `field` | A request lacks a field its kind needs, named as on the wire |
| `console_has_running_sessions` / `project_has_running_sessions` | — | Deleting needs the sessions archived first |
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
| `config_dir_not_absolute` | `agent` | A console's config directory is neither absolute nor `~`-relative; `agent` is the agent's name |
| `config_dir_not_a_directory` | `agent`, `path` | A console's config directory is not a directory |
| `config_dir_unreachable` | `agent`, `path` | A session's pinned config directory has gone, which refuses the launch |
| `icon_not_an_image` | — | A console's icon is not a `data:image/` URL |
| `icon_too_large` | `limit_kib` | A console's icon is longer than the limit, 256 KiB |
| `session_already_running` | `session` | The session is running already, or is not interrupted or archived |
| `session_already_starting` | `session` | The session is being started already |
| `hub_already_running` | `session` | The console has a live hub (`session`) |
| `hub_reopen_blocked` | `session` | The console has another live hub (`session`), which has to be archived first |
| `hub_already_starting` | `console` | The console's hub is being started already |
| `hub_missing` | — | The console has no hub session to submit to |
| `session_not_archived` | `session` | Only an archived session can be deleted (also: one being resumed right now cannot) |
| `session_not_running` | `session` | A message or a go-ahead for a session with no running process |
| `session_waiting_for_user` | `session` | The session waits at a prompt only the user can answer, so a message is refused |
| `queued_messages_lost` | — | A message queued for a session could not be written in full, and it and what was behind it were dropped |
| `page_not_current` | — | A form can be submitted only from the console's newest page |
| `trust_directory_too_broad` | `path` | The directory is the filesystem root, the home directory or one containing it |
| `trust_path_not_absolute` | `path` | The project's path is not absolute |
| `trust_home_unknown` | — | The home directory cannot be determined |
| `claude_trust_not_waiting` | — | The trust screen is no longer waiting for an answer |
| `claude_trust_answer_failed` | `reason`, `reason_code`, `detail?` | Octoboard accepted the go-ahead but could not answer the trust screen; the user answers it in the terminal. See the reason codes below |
| `not_a_claude_session` | — | A go-ahead for the trust screen of a session that is not Claude Code's |
| `hub_trust_not_asked` | — | A go-ahead for a hub session's trust screen, which Octoboard answers without asking |
| `binary_not_found` | `binary` | The agent's binary (or `git`) is not on the shell's `PATH` |
| `shell_environment_timeout` | `shell`, `command`, `timeout` | The login shell did not finish printing its environment in time |

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
Console { id, name, workdir, hub_agent, default_agent, claude_config_dir?, codex_config_dir?,
          grok_config_dir?, icon?, created_at }
Project { id, console_id, host_id, name, path, default_agent?, source, remote_url?,
          claude_trust_consent, pinned }
Session { id, agent, agent_session_id?, console_id, project_id?, host_id,
          role: "hub"|"worker", origin: "hub"|"user", title,
          status: "working"|"waiting_user"|"idle"|"interrupted"|"archived",
          has_conversation, include_in_hub, config_dir?, pinned, started_at, ended_at? }
Page    { id, console_id, html, anchor_message_id?, created_at }
```

`agent` is one of `claude`, `codex`, `grok`. Each `Console.*_config_dir` is an absolute path to that agent's own
configuration directory, and a session reads only its own agent's. Claude Code is launched with `CLAUDE_CONFIG_DIR` set to
it and Codex with `CODEX_HOME`, each over any value in the user's shell environment. For Grok it replaces `~/.grok` (or
the shell's `GROK_HOME`) as the directory the session's private home is built from, so the user's config, login, trust
store and session records come from it; Grok itself still runs against that private home. Unset leaves the environment as
it is (for Grok, the source falls back to the shell's `GROK_HOME`). For Claude Code, pointing it at `~/.claude` is not the same as leaving it unset, because Claude Code reads its
global config from `<dir>/.claude.json` whenever the variable is set and from `~/.claude.json` otherwise.
`Console.icon` is the console's custom avatar, a `data:image/...` URL of at most 256 KiB (the UI sends a 128x128
WebP or PNG); unset means the default glyph.
`Session.config_dir` is the directory of that session's own agent that it was started with, copied from its console
when the session is opened and never changed afterwards: each agent keeps a conversation's transcript inside it, so
`resume_session` relaunches with this value and not the console's current one. It is unset for sessions started with no
directory pinned, which resume under whatever the shell exports at that moment. A launch whose pinned directory no
longer exists is refused, for every agent, with an error naming it.
`trusted_directories` is a list of absolute, lexically normalised directory paths (no symlink is resolved). A project is trusted when its path equals one or lies below one, compared component by component (`/a/Project` does not cover `/a/Project2`). They are added by `confirm_claude_trust` with `trust_parent_dir` and removed by `remove_trusted_directory`. The comparison looks at a project's path only, not at its `host_id`: there is one local host today, and a second would need its own set. A symlink is not followed when comparing, so a project reached through a link inside a trusted directory is trusted wherever the link points, and a project outside the directory is not, however it is linked from inside.
`Project.claude_trust_consent` is true once the user has agreed, in the dialog a `claude_trust_prompt` opens, that Octoboard may answer Claude Code's trust screen for that project's directory. It is only ever set by `confirm_claude_trust` with `remember`; `update_project` neither sets nor clears it.
`pinned` (on a project and on a session) is the user's pin, false until set: by `update_project` for a project and `set_session_pinned` for a session. It is only a flag for the client to order by; archiving and resuming leave it alone.
`Page.anchor_message_id` is stored and never read (see "Data model" in `docs/architecture.md`). Timestamps are
epoch milliseconds (the UI formats them).

## `GET /ws/term/:session` — terminal stream

- Daemon to client: **binary** frames carrying raw PTY output. On attach the session's ring buffer is replayed first,
  then live output follows.
- Client to daemon: **binary** frames are raw PTY input, written through verbatim. **Text** frames are JSON control:
  `{"type":"resize","cols":N,"rows":N}`.
- Attaching to a session whose process is not running closes the socket immediately; the UI shows the stored status
  instead and offers resume.
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
follows from its role, and is checked here as well as in the child.
