# 03 Final confirmation

> Goal: confirm with real agents what tests cannot show: that a coordinator chooses an agent by its tags.
> Completion criteria:
> - Set up a Claude Code project coordinator, with Codex tagged for reviews and a Codex account that can open a
>   session. Asked to have a change reviewed in a separate session, the coordinator reads the tags and starts the
>   review session on Codex without being told to.

## Checks

- [ ] In Settings > Agent accounts, give Codex the tag `review`; make sure a Codex account can open a session
      (`list_agents` shows it with `can_open: true`).
- [ ] In a project, ask a Claude Code coordinator to have a change reviewed in a separate session, without naming an
      agent. It calls `list_agents`, sees Codex's `review` tag, and starts the review session on Codex.

## Notes

- Tests already cover that `list_agents` returns each added agent's tags and that the coordinator's role description
  and `open_session` description point to them (`apps/daemon/src/mcp/tools/agents.rs`, and the `session_tools/role.rs`
  and `session_tools/catalogue/` tests). What is left is the live model's choice.
- If the coordinator does not pick Codex, the fix is in the wording of `session_choice_guidance`
  (`apps/daemon/src/session_tools/role.rs`) or the coordinator's `open_session` description
  (`apps/daemon/src/session_tools/catalogue/sessions.rs`).
