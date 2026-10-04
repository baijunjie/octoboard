# octoboardd protocol

The only interface between the desktop application and the daemon. There is no Tauri IPC channel and no shared state
between them (see "Why the daemon is split out in the MVP" in `docs/mvp.md`), so everything the UI can do is in here.

The daemon binds to `127.0.0.1` on an ephemeral port and prints `octoboardd listening on 127.0.0.1:<port>` on stdout as
its first line; it also writes the port to `$TMPDIR/octoboardd.port`. The application reads the stdout line.

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
| `create_console` | `name`, `hub_agent`, `default_agent` | The daemon creates the console's working directory under `~/.octoboard/consoles/<id>/` |
| `update_console` | `console`, `name?`, `hub_agent?`, `default_agent?` | — |
| `delete_console` | `console` | Takes its projects and all their session records with it. Refused while any of them is still running |
| `add_project` | `console_id`, `source` (`local`\|`parent`\|`github`), `path?`, `remote_url?`, `name?`, `default_agent?` | `local` associates one directory; `parent` associates every git repository directly beneath `path`; `github` clones `remote_url` into `path` (used as the parent directory) and associates the clone |
| `update_project` | `project`, `name?`, `default_agent?` | An absent `default_agent` leaves it alone; an explicit `null` clears it, so the project inherits the console's default again |
| `delete_project` | `project` | Removes the association and the project's session records, archived ones included; never touches the directory. Refused while the project has live sessions |
| `list_dir` | `path` | Answered with `dir_listing` on the asking socket |
| `open_session` | `console_id`, `project_id?`, `agent?`, `task?`, `title?`, `include_in_hub?` | Omit `project_id` for the console's hub session; a console has at most one that is not archived, so a second is refused with `session_already_running`. `agent` follows the priority in the "Which agent gets used" section of `docs/mvp.md` when omitted. `include_in_hub` defaults to false: a session the user opens by hand stays outside the hub's orchestration and sends it no reports unless this is set |
| `resume_session` | `session` | Relaunches an `interrupted` or `archived` session through the agent's own resume mechanism, re-injecting everything. Reopening an archived hub session while the console's hub is already running is refused with `session_already_running` |
| `archive_session` | `session` | Ends the process and archives the session |
| `send_message` | `session`, `text` | Writes a message into a running session. Refused while the session is `waiting_user`: the message would be discarded and its trailing Enter would answer whatever dialog is up. The hub's own `send_message` tool holds such a message instead of refusing it — the user can be told to answer the prompt first, the hub cannot |
| `rename_session` | `session`, `title` | — |
| `shutdown` | — | Terminates every session process (leaving them `interrupted`) and exits the daemon |

### Daemon to client

| `type` | Fields |
|---|---|
| `snapshot` | `hosts`, `consoles`, `projects`, `sessions` — sent once, unprompted, when a control socket connects |
| `console_upserted` / `project_upserted` / `session_upserted` | `console` / `project` / `session` — the whole record, under that key |
| `console_deleted` / `project_deleted` | `console` / `project` |
| `session_notice` | `session`, `message` — something about a session the user has to be told that no status field carries: an injected capability that will not apply, a setting of theirs Octoboard had to work around, a message Octoboard accepted and could not deliver. Broadcast when it is found, which may be at launch or at any point in the session's life; nothing stores it, so a client that connects later does not see it |
| `session_opened` | `id`, `session` — the reply to `open_session`, naming the session it started |
| `dir_listing` | `id`, `path`, `entries`: `[{name, path, is_git_repo}]` — only directories are listed |
| `ack` | `id` |
| `error` | `message`, `id?`, `code?` — a code is present only for failures a client has to act on rather than just show. Today the one code is `session_already_running`. A client's own double click produces it and is not worth showing; a refused second hub session produces it too, and that one has to be shown, so a client branches on what it asked for rather than on the code alone |

### Records

```
Host    { id, name, kind: "local"|"ssh", ssh_config? }
Console { id, name, workdir, hub_agent, default_agent, created_at }
Project { id, console_id, host_id, name, path, default_agent?, source, remote_url? }
Session { id, agent, agent_session_id?, console_id, project_id?, host_id,
          role: "hub"|"worker", origin: "hub"|"user", title,
          status: "working"|"waiting_user"|"idle"|"interrupted"|"archived",
          has_conversation, include_in_hub, started_at, ended_at? }
```

`agent` is one of `claude`, `codex`, `grok`. Timestamps are epoch milliseconds (the UI formats them).

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
