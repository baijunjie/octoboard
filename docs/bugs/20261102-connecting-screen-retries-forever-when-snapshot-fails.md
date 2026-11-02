> Severity: Critical

## Symptom

When the daemon cannot build the state snapshot, the window stays on "Connecting to the daemon…" forever and never
offers Retry.

## Reproduction steps

1. Build a database from the current `SCHEMA` (`apps/daemon/src/store/schema.rs`) with one `settings` column left out
   (for example `agent_tags`), and set `PRAGMA user_version = 1`.
2. Put it at `.octoboard/octoboard.db` under a throwaway `HOME` (or, to use the real app, first move the existing
   `~/.octoboard/octoboard.db` aside).
3. Launch Octoboard, or start `octoboardd` with that `HOME` and connect a WebSocket to
   `ws://127.0.0.1:<port>/ws/control?key=<key>`, taking the port and key from the daemon's startup line
   (`apps/daemon/PROTOCOL.md`, "The key"); a non-browser client sends no `Origin`, which is allowed.
4. The window stays on "Connecting to the daemon…"; a raw WebSocket is closed with code 1006 and receives no message.
   `.octoboard/logs/daemon.log` logs `building the state snapshot failed` about every 0.55 s, without end.

## Expected vs. actual

- Expected: after a couple of automatic retries the attempts are spent and a Retry button is offered; nothing retries
  forever on its own (`docs/product/application-lifecycle.md`, "Losing the daemon connection").
- Actual: the window retries forever and never offers Retry.

## Environment

macOS, desktop app built from main after a0cefef4e (which added `settings.agent_tags` without raising the schema
version). Data precondition: any database on which building the snapshot fails.

## Scope of impact

Until the first release the schema is edited in place at version 1 (`apps/daemon/src/store/schema.rs`), so any
development database created before a later `SCHEMA` column change hits this, as does any other snapshot failure,
before or after release. The app is unusable and the window gives no clue why. Workarounds: move `octoboard.db` aside so a
fresh one is created (the documented route, losing its data), or add the missing column by hand (here,
`ALTER TABLE settings ADD COLUMN agent_tags TEXT`).

## Leads

- Verified: `apps/daemon/src/server/control.rs:58-67` logs the snapshot error and returns, closing the socket without
  sending anything to the client.
- Verified: `packages/ui/src/daemon-client.ts:106-108` resets `reconnectAttempts` to 0 on the socket's `open` event.
- Inferred from the code: `open` fires before the daemon drops the connection (the handler runs after the upgrade), so
  every close starts again from the first attempt and the budget is never spent.
- Verified: daemon log line `ERROR octoboardd::server::control: building the state snapshot failed err=no such column:
  agent_tags in SELECT agent_tags FROM settings WHERE id = ?1`, repeated over 187 times, about every 0.55 s.

## Acceptance criteria

- [ ] With a database on which the snapshot fails, the connecting screen stops retrying on its own after the same
      couple of attempts as a dropped connection, and offers Retry.
- [ ] A daemon that accepts the control socket and closes it before any snapshot, on every attempt, ends in the Retry
      state after that same budget.
- [ ] Normal connection, and recovery after a real drop of a working connection, still behave as
      "Losing the daemon connection" describes.
