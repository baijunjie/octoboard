# Added agents

## Problem

Octoboard lists every agent it supports (Claude Code, Codex, Grok Build) everywhere an agent is shown, installed or
not. Settings → Agent accounts keeps a tab for each, and an agent the user never installed is flagged there as not
installed, as if something were wrong. Not having an agent is a normal choice, not a fault.

## Plan outline

1. The daemon keeps a persisted list of **added agents** in its settings, with requests to add, remove and reorder
   them, and fills it once on upgrade.
2. Settings → Agent accounts shows only added agents, gets an **Add agent** button and a way to remove one, and an
   empty state when none is added.
3. Every other place that lists agents — the console, project and session dialogs' agent pickers and the rail's
   quota panel — shows only added agents.

## Key design decisions

- **Only added agents appear, app-wide**: the Settings agent strip, the Add account picker, the console, project and
  session dialogs' agent pickers, and the rail quota panel. The user chose app-wide over Settings-only for
  consistency.
- **Adding requires the agent to be installed.** The Add agent list shows every agent with its installed status;
  one that is not installed is listed but cannot be added.
- **The console, project and session dialogs list only agents that are added and installed** (decided before 03): an
  added agent that is not installed is hidden there and shown, with its warning, only in Settings → Agent accounts. A
  picker whose current value is an agent that is not added or not installed still shows that value, marked, so the
  existing setting stays visible; once changed away it is no longer offered.
- **The not-installed warning is for added agents only.** An added agent that later stops being available (the user
  uninstalled it) keeps its tab with the warning glyph after its name and the explanatory tooltip already built
  (opening on hover and on keyboard focus). An agent that was never added shows nothing, since not having it is not
  an error.
- **An added agent can be removed**, after a confirmation. Its accounts are kept, not deleted, and reappear when the
  agent is added again. Removal is allowed while consoles, projects or live sessions use the agent: the confirmation
  names them, and they keep their setting and go on running; only new choices no longer offer the agent.
- **Migration on the first start after upgrade**: the added list is filled with the agents that are installed plus
  the agents that have accounts the user owns, so no existing account disappears from view; an agent added this way
  that is not installed shows the warning.
- **A fresh install adds the agents that are installed**, the same rule without accounts, so a session can be opened
  straight away and the welcome screen needs no change.
- **The added agents can be reordered** in Settings → Agent accounts, and that order is the order agents appear in
  everywhere: the Settings agent strip, the Add account picker, the console, project and session dialogs' pickers
  and the rail quota panel. A newly added agent goes last.
- **Settings controls** (decided at the start of 02): Remove agent sits in the agent tab's context / more menu;
  Add agent is a dialog listing every agent with its installed status and a link to that agent's official install
  instructions (Octoboard writes no install instructions of its own); reordering is by dragging tabs, with Move earlier /
  Move later in the same tab menu as the keyboard way.
- **Account row actions** are one actions menu per account row, and it gains a **Log in** action that signs the
  account in with its agent's own CLI against the account's config directory: the daemon runs the agent's login flow
  in the background and Octoboard opens the browser authorization page itself (the user chose this over opening an
  external terminal). It is part of milestone 02.
- **The default account can be renamed** (name only; its directory stays Octoboard's own), and the custom name is
  used everywhere the default account is named; clearing it brings back the localized default name.
- **A not-signed-in account is a state, not an error.** When reading quotas the daemon recognises, per agent, an
  account whose config directory does not exist yet and one that is not signed in, and reports them as a distinct
  quota state; the UI shows a friendly line pointing to Log in instead of the CLI's raw error, which stays only for
  real, unexpected failures. The user confirmed the wording and placement at the 02 preview.
- **With no agent added**, the Accounts page shows an empty-state hint and the Add agent button instead of the tab
  strip, and Add account is unavailable.

## Milestones

1. 01 Added agents in the daemon (closed; not merged yet — to be merged together with 02, as the user asked)
2. 02 Settings: add, remove and reorder agents (closed; not merged yet — to be merged together with 01)
3. [Added agents across the app](03-added-agents-across-the-app.md)
4. [Final confirmation](04-final-confirmation.md)

## Open

- None.
