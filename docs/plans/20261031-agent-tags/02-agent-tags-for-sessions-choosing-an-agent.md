# 02 Agent tags for sessions choosing an agent

> Goal: a session choosing an agent for a piece of work sees each agent's tags and can choose by them.
> Completion criteria:
> - A session can read every added agent's tags.
> - Set up a Claude Code project coordinator, with Codex tagged for reviews and a Codex account that can open a
>   session. Asked to have a change reviewed in a separate session, the coordinator reads the tags and starts the
>   review session on Codex without being told to.

## Technical design

- [ ] Every added agent's tags in what sessions read when choosing an agent. Where is open; see the overview.
- [ ] The project coordinator's role description: a kind of work another agent handles better is judged by the agents'
      tags.
- [ ] The coordinator's `new_session` description points to the tags when it mentions naming another agent.

## Notes for the developer

- **Reusable capabilities**:
  - The shared Octoboard MCP tool catalogue and role descriptions, which serve all three agents.
  - `get_account_quotas`, which shows which accounts, and so which agents, can open a session.
  - `new_session`'s named-agent handling.
- **Reference docs**: `docs/product/hub-orchestration.md` ("The agent and account of a session a session starts",
  "Choosing the session a task goes to"), `docs/product/what-every-session-reads.md`,
  `docs/product/reading-quotas-from-a-session.md`.
