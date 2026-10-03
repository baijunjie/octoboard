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
- [ ] `send_message(session, text)`: delivered immediately when the session is idle *or* mid-turn (every agent queues it
  itself and consumes it at turn end); held by the daemon only while a modal dialog is up or the session's state is
  unknown
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

- [ ] The hub working directory `~/.octoboard/consoles/<id>/`, with generated hub-specific instruction files for a Claude
  Code hub (`CLAUDE.md`) or a Codex hub (`AGENTS.md`) — a Grok hub takes the same content through `--rules` instead, see
  below — stating that the hub only decomposes / dispatches / follows up / summarizes and does not modify project code
  itself
- [ ] Manual sessions do not report to the hub by default, but "include in hub" can be checked
- [ ] Report delivery: written into the hub session as a user message, on the same terms as any other write into a running
  session — delivered when the hub is idle *or* mid-turn, held only while a modal dialog is up in it or its state is
  unknown
- [ ] Report synthesis: when a hub-dispatched session stops without having called `report` this turn, take the `Stop`
  hook's `last_assistant_message` as the report with status `needs_decision`. **Nothing is blocked** — milestone 00
  established that gating the stop through the Stop hook works but is user-visible as an error and makes the model refuse
  often enough to matter. Mind the conditions in `docs/mvp.md` 5.3 — among them: filter Grok's teardown
  `Stop`; treat a Claude Code `Stop` with non-empty `background_tasks` as paused rather than finished; register
  `StopFailure`, which is mutually exclusive with `Stop`, or an API error leaves the session looking busy forever; and
  remember Grok's `idle_prompt` backstop carries no turn id, so it can only be attributed by session and clock
- [ ] Automatic archiving: with `status = done` and an empty `open_items`, end the process and archive once the report has
  been delivered; otherwise keep the session "awaiting instructions" until the hub sends a `send_message` or archives it
  explicitly
- [ ] A Grok hub takes its role description from `--rules`, not from a generated instruction file: Grok locates a project
  by walking up for a `.git` directory, and a console's working directory is not a repository, so an instruction file
  written there is never read
- [ ] Raised hand: at a permission prompt or when asking the user something, the session state becomes `waiting_user`, the
  menu shows a raised-hand icon bubbled up to the project and console nodes, and a system notification and Dock count fire;
  the user answers right in the terminal and the state returns to working. Coverage is not uniform — see "Waiting for the
  user (raised hand)" in `docs/mvp.md`: a permission prompt is detectable on all three agents, a question asked through the
  agent's own tool is not detectable on Codex, a question asked in prose is detectable on none of them, and no agent
  signals "the user answered", so clearing the hand is inferred from the next event for the same turn
- [ ] The hub neither nags nor re-dispatches a session that is "waiting for the user", and messages bound for it queue until
  the user is done
- [ ] Keep "needs a hub decision" (the agent actively calling `report(needs_decision)`) apart from "needs a user decision" (a
  permission prompt or asking a person directly, which goes through the raised hand)
- [ ] Writing messages into a running session follows the four rules settled in 00 and written into the "Writing into a
  running session" bullet of `docs/mvp.md` section 6 — in particular the gate is modal versus non-modal, not busy versus
  idle, and it comes from hook-reported state rather than from terminal text

## Notes for developers

- **Reusable from earlier**: the adapter interface, session state machine, and daemon protocol from 01.
- **Key points**: hooks must fail fast and exit silently, and must not disturb the agent when the daemon is unreachable; MCP
  tokens are issued per session and the role determines which tools are visible.
- **Reference**: `docs/mvp.md` section 5.
