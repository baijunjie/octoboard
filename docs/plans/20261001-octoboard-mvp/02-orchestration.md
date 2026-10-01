# 02 Orchestration

> Goal: the hub session can orchestrate project sessions through MCP tools, reports are delivered and archived
> automatically, and project sessions raise their hand to the user.
> Done when: one full "dispatch → execute → report → archive → summarize" loop completes; several projects raising their hand
> at once do not interfere with each other and bubble up correctly; the hub and projects use different agents; the hub can
> override the agent for one session.

## Technical design

**Injected capabilities** (injected by the adapter when each session starts, pointing at the daemon on the session's host)

- [ ] Status hooks: report working / stopped / waiting for the user
- [ ] The Octoboard MCP server: `rmcp`, Streamable HTTP, localhost only, a token per session, different tools exposed per
  role
- [ ] A role description: hub / worker plus the reporting conventions

**Hub tools**

- [ ] `list_projects`
- [ ] `add_project`
- [ ] `start_session(project, brief, agent?)`, returns a session id
- [ ] `send_message(session, text)`: delivered immediately when idle, queued when busy or waiting for the user
- [ ] `get_session(session)`: status and a summary of recent output
- [ ] `archive_session(session)`
- [ ] `list_archived(project)` / `reopen_session(session, text?)`
- [ ] `show_page(html)` (implemented in 03)

**Project session tools**

- [ ] `report(summary, status, open_items[])`, where `status` is one of `done` / `failed` / `needs_decision`

**Communication conventions**

- [ ] `brief` carries `goal` / `context` / `acceptance` / `constraints`; the daemon renders it into an initial prompt using a
  fixed template, omitting the section for each missing field
- [ ] Fields a program must act on travel as tool arguments; content stays natural language

## Implementation

- [ ] The hub working directory `~/.octoboard/consoles/<id>/`, with generated hub-specific instruction files (`CLAUDE.md` or
  `AGENTS.md` depending on the hub's agent) stating that the hub only decomposes / dispatches / follows up / summarizes and
  does not modify project code itself
- [ ] Manual sessions do not report to the hub by default, but "include in hub" can be checked
- [ ] Report delivery: written into the hub session as a user message; queued while the hub is busy and delivered once it
  stops
- [ ] Forced reporting: when a hub-dispatched session stops without having called `report` this turn and without waiting for
  the user, block the stop through the Stop hook and prompt it to call `report` (at most once per turn); if it still does
  not, take its last reply as a fallback report with status `needs_decision`; agents that cannot block a stop go straight to
  the fallback
- [ ] Automatic archiving: with `status = done` and an empty `open_items`, end the process and archive once the report has
  been delivered; otherwise keep the session "awaiting instructions" until the hub sends a `send_message` or archives it
  explicitly
- [ ] Raised hand: at a permission prompt or when asking the user something, the session state becomes `waiting_user`, the
  menu shows a raised-hand icon bubbled up to the project and console nodes, and a system notification and Dock count fire;
  the user answers right in the terminal and the hooks automatically return the state to working
- [ ] The hub neither nags nor re-dispatches a session that is "waiting for the user", and messages bound for it queue until
  the user is done
- [ ] Keep "needs a hub decision" (the agent actively calling `report(needs_decision)`) apart from "needs a user decision" (a
  permission prompt or asking a person directly, which goes through the raised hand)
- [ ] Writing messages into a running session is implemented per the conclusions from 00; write only in the "awaiting
  instructions" state and queue otherwise

## Notes for developers

- **Reusable from earlier**: the adapter interface, session state machine, and daemon protocol from 01.
- **Key points**: hooks must fail fast and exit silently, and must not disturb the agent when the daemon is unreachable; MCP
  tokens are issued per session and the role determines which tools are visible.
- **Reference**: `docs/mvp.md` section 5.
