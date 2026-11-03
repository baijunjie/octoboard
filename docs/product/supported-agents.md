# Supported agents

Which agents an Octoboard build supports, how the window names and marks each one, and what becomes of stored data
that names an agent the running build does not support. Which of the supported agents the user has added, and so which
ones the window offers, is in "Added agents" in `docs/product/agent-accounts.md`; how each one is launched is in
`docs/product/launching-agents.md`.

## The agents and their order

Octoboard supports three agents, each known by an id that names it wherever an agent is named in data — the daemon's
protocol, a configuration document, a session's tools:

| Agent | Id |
|---|---|
| Claude Code | `claude` |
| Codex | `codex` |
| Grok Build | `grok` |

The daemon decides which agents are supported and serves them to every window in a fixed order, the order above. The
window lists them in the order the daemon serves them wherever it lists the supported agents in their usual order —
the order Claude Code, Codex, Grok Build that other docs name — and a value given per agent, such as a console's
account for each agent, holds one entry for each of them. The first agent served, Claude Code, is the one a control
opens on when nothing else chooses one. A request from a window or a program naming any other agent id is refused as
an unreadable request (`unreadable_request`).

## How an agent is named and marked

Each agent is shown by its name and its mark: Claude Code, Codex and Grok Build each by their own. An agent the daemon
serves that the window has no presentation of its own for is still listed and usable: it is named by the name the
daemon gives it, its shorter name where the window uses one, such as the menu bar icon's menu, and shown with a
generic, neutral mark in the text colour in place of a mark of its own.

## Data of an agent this build does not support

The database may name an agent the running build does not support — data written by a build that supported another
agent. Opening it never fails on that account, and nothing of that agent is deleted:

- **Its accounts and sessions** stay stored and are left out of everything the window shows and every session's tools
  read: they are not listed, counted or offered anywhere.
- **A console's or project's values for that agent** — the console's account and the project's account for it
  (including a project's choice of the agent's default account against its console), and the model and reasoning
  effort for its console and project sessions — stay stored, are left out the same way, and survive later edits of the
  console or project.
- **A console's or project's default agent** naming that agent reads as Claude Code, the first agent served, and the
  console or project is shown as usual, not hidden. A console's default agent for project sessions naming that agent
  reads as unset, following the default agent for console sessions. Either stays stored through a later edit of the
  console or project that leaves it reading so — Claude Code, or unset — and is replaced once another agent is chosen
  for it.
- **The application settings' entries for that agent** — its place on the list of added agents, its account order, its
  default account's name, whether its default account is turned off, its tags and its provider settings — are ignored,
  and kept through later changes to those settings.

A configuration export carries none of what is left out (see "The configuration document" in
`docs/product/configuration-export-and-import.md`). What is left out comes back, unchanged, when a build that supports
the agent opens the database again.
