# Prototype WebSocket protocol

Throwaway, deliberately minimal. Bound to `127.0.0.1` only. The port is printed on stdout at startup as
`obd-proto listening on 127.0.0.1:<port>` and also written to `$TMPDIR/obd-proto.port`.

## `GET /ws/control` — management and status

Text frames, JSON, one object per frame.

Client to daemon:

| `type` | Fields | Meaning |
|---|---|---|
| `start_session` | `agent` (`claude`\|`codex`\|`grok`), `cwd`, `role` (`hub`\|`worker`), `task?` | Launch an agent in a PTY. The daemon allocates the session id |
| `send_message` | `session`, `text` | Write a message into a running session (bracketed paste + Enter) |
| `kill_session` | `session` | Terminate the session process |
| `list_sessions` | — | Replays every known session (including exited ones — the daemon never prunes them) as a `session_started` followed by a `status` frame, in the same shape `start_session` would have produced |

Daemon to client:

| `type` | Fields |
|---|---|
| `session_started` | `session`, `agent`, `pid`, `agent_session_id?` — re-emitted (not just sent once at launch) whenever `list_sessions` replays a session, and the first time the agent's own session id becomes known |
| `status` | `session`, `state` (`working`\|`waiting_user`\|`idle`\|`exited`), `source` (`hook`\|`process`), `raw` (the hook payload) |
| `tool_call` | `session`, `tool`, `args` — an MCP tool the session invoked |
| `message_queued` | `session`, `reason` — `send_message` arrived while the session was not awaiting instructions |
| `error` | `message` |

## `GET /ws/term/:session` — terminal stream

- Daemon to client: **binary** frames carrying raw PTY output verbatim. On attach, the session's ring buffer is replayed
  first, then live output follows (T3).
- Client to daemon: **binary** frames are raw PTY input, written through verbatim. **Text** frames are JSON control:
  `{"type":"resize","cols":N,"rows":N}`.

## `POST /hook/:session/:event` — hook callback

Body is the hook's stdin payload verbatim (or empty). Always answers `200` with an empty body as fast as possible; the
response body is never used to steer the agent in the prototype. The injected hook command is shaped to fail fast and
silently (G4):

```
curl -s -m 2 -o /dev/null -X POST --data-binary @- http://127.0.0.1:<port>/hook/<session>/<event> || true
```

## `POST /mcp/:session/:tool` — MCP tool bridge

The stdio MCP server (`obd-proto mcp --session <id> --role <hub|worker> --daemon <url>`) forwards every `tools/call` here
and returns the daemon's JSON as the tool result. The daemon broadcasts a `tool_call` control event for each.

Tool sets differ by role, which is what proves G5:

| Role | Tools |
|---|---|
| `hub` | `start_session(project, task, agent?)`, `show_page(html)` |
| `worker` | `report(summary, outcome)`, `ask_user(question)` |
