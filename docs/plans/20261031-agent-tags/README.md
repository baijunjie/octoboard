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
- **Tags are set in the agent's tab** of Settings' Agent accounts section, in a row above the accounts table like the
  per-agent settings, with the same Tags field and tag appearance as a project's.
- **Sessions read the tags when choosing an agent.** The project coordinator's role description tells it to weigh an
  agent's tags when deciding whether another agent fits a piece of work better.

## Milestones

- [01 Storing and editing agent tags](01-storing-and-editing-agent-tags.md)
- [02 Agent tags for sessions choosing an agent](02-agent-tags-for-sessions-choosing-an-agent.md) — needs 01

## Open

- Where sessions read the tags. There are three options:
  - alongside the accounts `get_account_quotas` returns, which has no per-agent entry yet;
  - in `get_settings`, which only console sessions read, so a project coordinator would still need one of the other
    two;
  - in a read tool of their own.
- Whether a console session may read and change agent tags through `get_settings` and `update_settings`. Settings
  that shape what sessions are told are kept from those tools today.
- Whether the tools that today name an agent only when the user asked may point to the tags, letting the caller name
  an agent unasked when its tags fit the work: a console session's `new_session`, and every session's
  `request_session`. Also whether `handoff_session`, which always names its receiving agent, is in scope.
- When a change to an agent's tags is saved: on change like the other per-agent rows, or with a save action. Also
  where the Tags row sits among the per-agent rows.
- Whether configuration export and import carry agent tags.
- Whether removing an agent from the added list drops its tags, or keeps them for when it is added again.
