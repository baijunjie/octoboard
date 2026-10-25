# Read tools for models, history, archives and projects

> Goal: an agent can look up valid models, review a session's conversation, find its archived sessions and find
> projects in other consoles.
> Completion criteria: each tool below is available to the roles stated, refuses what it should, and is covered by
> tests; tool descriptions and product docs are updated.

## Technical design

- [ ] Listing an agent's models and effort levels, available wherever `model` / `effort` can be passed; the `model`
      parameter descriptions point to it instead of listing only Claude's aliases.
- [ ] Reading a session's history (user messages and final replies), readable for any session, the same scope as
      `get_session`; shape as settled under "Key design decisions" in the overview.
- [ ] Listing archived sessions for project sessions that can own sessions, limited to their own project.
- [ ] Listing projects across consoles, returning only id, name, description and tags, so `request_session` and
      `request_resume_session` can be targeted.

## Implementation plan

- [ ] Each tool calls the existing protocol request behind it; no new data source.
- [ ] Add the tools to the role catalogues and dispatch, and to the product docs listing each role's tools.

## Notes for the developer

- **Reusable capabilities**: `list_agent_models` with the UI's ten-minute cache; `read_session_history`; the console
  session's `list_archived`; the project listing behind `list_projects`.
- **Development notes**: listing models runs agent CLIs, so reuse the cache rather than probing per call. Writes stay
  limited to the caller's console; this milestone adds reads only.
- **Reference docs**: `docs/product/model-and-reasoning-effort.md`, `docs/product/session-history.md`,
  `docs/product/archive-view.md`, `docs/product/session-requests.md`, `docs/product/hub-orchestration.md`,
  `docs/memory/probing-agent-clis.md`.
