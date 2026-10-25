# Closing the gaps in what agents can do over MCP

## Problem

The MCP server already covers orchestration end to end for all three caller roles (console session, project session
that can own sessions, bound session): starting, messaging, reading, archiving, reopening, handing off, handing over
branches, switching accounts, transferring, releasing, binding, the two approval requests, settings, quotas and the
report panel. Most of what it deliberately leaves out is documented with a reason.

The gaps are on the read side: several write tools need facts no tool returns, so an agent has to guess or rely on
memory.

- `handover_branches`, and the `branches` of `new_session` / `request_session`, require the caller to be responsible
  for each branch, but nothing reads which branches a session is responsible for.
- `switch_session_account` refuses the account a session is already on, but the session record does not show its
  account, so it also cannot be matched to `get_account_quotas`.
- `set_session_pinned` writes `pinned`, which no tool reads back; archiving is refused for a `locked` session, which no
  tool shows.
- `model` must be one the agent offers, but the schema lists only Claude's aliases; Codex and Grok model ids and effort
  levels cannot be looked up.
- `get_session` returns only the last 8 KiB of terminal output, so an owner cannot review a session's conversation.
- A project session can `reopen_session` but cannot list its archived sessions, unlike a console session; no reason is
  documented for the asymmetry.
- `request_session` can target any project of any console, but no tool lists projects beyond the caller's own console.

Smaller gaps: quota readings can lag by up to five minutes with no way to refresh; a console session cannot read back
the report pages it pushed; `list_sessions` has no filter; `get_session`'s output length is fixed; `share_info` reaches
running sessions only.

## Outline

All additions are reads or caller-scoped conveniences; no new write power over other sessions or the user's
configuration is added. Each reuses a capability the app already has behind its control protocol.

## Key design decisions

- **A session's responsible branches are read through a tool of their own**, which returns each branch with its
  worktree state. The `get_session` record carries only the branch names, so `list_sessions` stays light and reading
  a record runs no git.
- **Session history is readable with the same scope as `get_session`**: any session may read any session's history.
  Chosen by the user over limiting it to owners; it follows the existing read-wide, write-narrow rule.
- **Session history holds user messages and final replies only**, no tool calls, and is paged by entry count from the
  most recent entry backwards.
- **Listing projects across consoles is an optional `console` parameter on `list_projects`**, defaulting to the
  caller's console and available to every role that can list projects. For another console it returns only id, name,
  description and tags: reads are not limited to the caller's console (writes are), and these fields are what an agent
  needs to target `request_session`.
- **Refreshing quotas is rate-limited to one refresh per account per 60 seconds**; a request within that window
  returns the cached reading. A refresh runs agent CLIs or makes requests. Refreshing a reading is distinct from
  auto-opening a usage window, which spends quota and stays unexposed.
- **`share_info` to a session that is not running is queued for an interrupted session** and delivered when it
  resumes. It is refused for an archived session, so nothing piles up for sessions that may never be reopened.
- **Out of scope**: interrupting a session's current turn, and opening a file or diff in the app for the user. Both are
  new features, not gaps. Charts in report pages are tracked separately.

## Milestones

1. Document the user-only agent settings (closed)
2. [Complete the session record](02-complete-the-session-record.md)
3. [Read tools for models, history, archives and projects](03-read-tools-for-models-history-archives-and-projects.md)
4. [Smaller conveniences](04-smaller-conveniences.md)
