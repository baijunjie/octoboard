# Smaller conveniences

> Goal: the remaining small gaps in existing tools are closed.
> Completion criteria: each item below works for the roles stated and is covered by tests; tool descriptions and
> product docs are updated.

## Technical design

- [ ] Refreshing account quotas on request, rate-limited (limit settled under "Key design decisions" in the overview).
- [ ] A console session can list the report pages it pushed, limited to its own pages.
- [ ] `list_sessions` filters: console, project, owner, and only the caller's own sessions.
- [ ] `get_session` takes a length for the terminal output tail, bounded.
- [ ] `share_info` to a session that is not running is held and delivered later, with the semantics settled under
      "Key design decisions" in the overview.

## Implementation plan

- [ ] Each item extends an existing tool or wraps an existing protocol request; they are independent of each other.

## Notes for the developer

- **Reusable capabilities**: the protocol's `refresh_account_quotas` and `list_pages`; the queueing `send_message`
  already does for sessions that are not ready.
- **Development notes**: a refresh must not auto-open a usage window.
- **Reference docs**: `docs/product/account-quotas.md`, `docs/product/report-panel.md`,
  `docs/product/information-between-sessions.md`, `docs/product/message-delivery.md`.
