# Agent Tags Development Plan

## Problem and approach

A project coordinator may start a session of another agent for a kind of work that agent handles better, and a session
can name its agent in `new_session`. What another agent handles better is left entirely to the calling agent's own
judgement. Octoboard gives the user no place to record, for example, that Codex is the one for reviews or Claude Code
the one for UI work.

The user puts short tags on each added agent. A session choosing an agent sees each agent's tags and can choose by
them. Tags are information for the agent; Octoboard itself does not select anything by them.

## Key design decisions

- **Tags belong to an agent** (Claude Code, Codex, Grok Build), not to an account or a model. Like the per-agent
  settings, they apply to every account of that agent, the default account included.
- **Tags are application-wide**, kept by the daemon with the other application settings, so every window and every
  console sees the same tags.
- **A tag is a short free-form label, with the same rules as project tags**:
  - There is no list of tags to manage. A tag is typed the first time it is used.
  - The tags offered for an agent are the distinct tags the added agents carry.
  - A tag is trimmed, and an empty one is dropped.
  - A tag that matches another of the agent's tags ignoring case is dropped, and the first spelling stays.
- **Tags are set in the top row of the agent's tab** of Settings' Agent accounts section, above Permission approval
  (or above Claude compatibility, or directly above the accounts table, where those rows are absent), so the row sits
  in the same place in every tab, with the same Tags field and tag appearance as a project's. A change is saved as it
  is made, like the other per-agent rows.
- **A console session may read and change agent tags** through `get_settings` and `update_settings`, like Claude
  compatibility: tags are information, they lift no permission and spend no quota.
- **A configuration export that includes the application settings carries agent tags**, and an import applies them.
- **Removing an agent keeps its tags**, which come back when it is added again, as its accounts do. A removed agent's
  tags are neither offered for the others nor shown to sessions.
- **Sessions read the tags when choosing an agent, in one call with the accounts.** `get_account_quotas` is renamed
  `list_agents`: its answer is grouped by agent, each entry giving the agent, its tags and its accounts (with the same
  account fields as before), keeping the `agent` and `refresh` arguments.
- **The tags may lead a session to name an agent unasked.** The project coordinator's role description tells it to
  weigh an agent's tags when deciding whether another agent fits a piece of work better. A console session's
  `new_session` and every session's `request_session`, which today name an agent only when the user asked, may also
  name one whose tags fit the work. `handoff_session` is out of scope.

## Milestones

- 01 Storing and editing agent tags (closed)
- [02 Agent tags for sessions choosing an agent](02-agent-tags-for-sessions-choosing-an-agent.md)
