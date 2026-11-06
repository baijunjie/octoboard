# Per-agent settings

Settings that apply to every account of one agent, the default account included, rather than to a single account:
the agent's tags, and the settings each agent's provider declares for itself — how Claude Code and Codex handle
permission requests, and whether Codex and Grok Build load Claude Code's configuration. Each is set in the agent's tab
of Settings' **Agent accounts** section, in a row above the agent's accounts table (see "The accounts table" in
`docs/product/agent-accounts.md`).

## Agent tags

The user can put tags on each agent — Claude Code, Codex and Grok Build: short free-form labels saying what the agent
is good for, such as "review" or "ui". They are **one list per agent**, applied to every account of that agent, the
default account included, not to an account or a model. They are information for agents; Octoboard itself selects
nothing by them. A tag follows the rules of a project's tag (see "Project tags" in `docs/product/project-tags.md`):
there is no list of tags to manage, a tag is typed the first time it is used, and each tag is trimmed, an empty one is
dropped and one matching another of the agent's tags ignoring case is dropped, the first spelling staying.

They are set in the agent's tab, in the **Tags** row, the tab's top row in every agent's tab, above the rows of the
agent's provider settings (see "Provider settings" below). The row holds its name, a line saying that it
applies to every account of the agent and, under them across the row's full width, the same Tags field as a project's
(see "Editing a project" in `docs/product/projects.md`), its picked tags looking and behaving as there (see "Picked
tags" in `docs/product/project-tags.md`). The field has no visible label of its own; its accessible name is "Tags for"
and the agent. Its empty text input says the agent has no tags while none is picked, and invites adding a tag once one
is. The tags offered are the distinct tags the added agents carry (every agent's while the list of added agents is
undecided; see "Added agents" in `docs/product/agent-accounts.md`), tags differing only in case counting as one, so a
tag used on one agent is offered for the others; a typed tag matching an offered one takes its spelling.

Each change is saved as it is made, as the agent's whole list; the field stays usable meanwhile. While a save is
unanswered the field shows the list last asked for, so changes made in quick succession build on one another. Once the
saves are answered it shows the stored tags, and a list another window set meanwhile is shown then and built on by the
next change. A failed save is shown in an error alert titled "Could not save the tags", and once no
later change is still being saved the field goes back to the stored tags.

The tags are kept by the daemon with the other application settings, so every window shows the same ones, and a change
reaches them at once; they survive a restart. **Removing an agent keeps its tags**, which come back when it is added
again (see "Reordering and removing an agent" in `docs/product/agent-accounts.md`); a removed agent's tags are not
offered for the other agents. Every session reads the tags of the agents a session could be opened on, with their
accounts, through `list_agents`, which does not list a removed agent (see
`docs/product/listing-agents-from-a-session.md`). Like Claude compatibility, a console session may read them but not
change them (see "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`), and a
configuration export that includes the accounts carries them (see "The agent account settings an export carries" in
`docs/product/settings-in-a-configuration.md`).

## Provider settings

Each agent's provider declares the settings the user can make for it. Each is **one setting per agent**, applied to
every account of that agent, the default account included. Today they are:

| Agent | Row | Setting id | Kind | Carried by a configuration export | Readable by sessions |
|---|---|---|---|---|---|
| Claude Code | **Permission approval** (see "Approval modes" below) | `approval_mode` | Choice of `auto`, `manual` or `bypass` | Yes | No |
| Codex | **Permission approval** (see "Approval modes" below) | `approval_reviewer` | Choice of `auto_review` or `user` | Yes | No |
| Codex | **Claude compatibility** (see "Claude compatibility" below) | `claude_compatibility` | Switch, on while Claude Code is an added agent until chosen | Yes | Yes |
| Grok Build | **Claude compatibility** (see "Claude compatibility" below) | `claude_compatibility` | Switch, on until chosen | Yes | Yes |

A setting is one of two kinds:

- **A choice** takes one of the values its provider declares, or none. With none, which is where every choice setting
  starts, the agent follows each account's own configuration; the row offers this as **Follow account configuration**,
  before the declared choices. A declared choice may need the user's confirmation before it is saved (see "Approval
  modes" below).
- **A switch** is on or off. Until the user turns it, it follows a default rule: on, off, or on while a named agent is
  among the added agents (see "Added agents" in `docs/product/agent-accounts.md`). Such a rule is worked out afresh
  whenever the value is read, so adding or removing that agent moves it; while the list of added agents is still
  undecided, the named agent counts as not added. Once the user has turned the switch, that choice holds, whatever
  agents are added or removed later.

Octoboard keeps only the user's explicit choices, in the daemon, so every window shows the same and they survive a
restart; every window is given the **value in effect** of every setting of every agent: a switch's choice or else its
default, a choice setting's choice or else none. A stored choice the setting no longer takes is read as no choice.

The agent's tab has one row per setting, in the order the provider declares them, under the Tags row: Claude Code's
tab shows Permission approval, Codex's Permission approval then Claude compatibility, and Grok Build's Claude
compatibility. A setting the window has no wording for is not shown; a choice the window has no name for is offered by
its value in words (a value `read_only` as "Read only"). Each row holds the setting's name with a "?" help tip saying
what it does, a line saying that it applies to every account of the agent, and, at the row's end, a selector
for a choice or a switch for a switch, named for the setting and the agent ("Permission approval for Codex"). A choice
is saved when selected, its choices unavailable while the save is unanswered; a switch is saved when turned, unavailable
while the save is unanswered — it looks disabled but stays focusable — and keeps its position when the save fails. The
switch always shows the value in effect. A failed save is shown in an error alert titled for the setting ("Could not
save the permission approval", "Could not save Claude compatibility").

A value is read when a session of any account of the agent starts or resumes, including an account switch, and a legacy
pinned directory with no account reference included; a running process keeps what it was started with. Octoboard
writes none of the agents' configuration files for any of them.

Whether a configuration export carries a setting's explicit choice is the provider's to declare (see "The agent account
settings an export carries" in `docs/product/settings-in-a-configuration.md`); one it does not carry stays on this
machine. A setting whose turning on itself changes files outside Octoboard's own directory is not carried (see "What is
not configuration and never travels" in `docs/product/configuration-export-and-import.md`); none of today's does, since
Octoboard writes none of the agents' configuration files for them. Whether sessions may read a setting's value in effect is declared the same way; no session changes any of them
(see "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`).

A program on the daemon's host sets a choice with the control request `set_provider_setting`, naming the agent, the
setting id and the value — `true` or `false` for a switch, the choice's text for a choice setting, or `null` to clear
the choice, a switch going back to its default rule and a choice setting to following the account configuration. A
setting the agent's provider does not declare, a value of the other kind, or a choice the setting does not offer is
refused as an unreadable request (`unreadable_request`). The confirmation a choice needs is asked for by the window, not
by the daemon, so this request saves such a choice at once.

## Approval modes

How Claude Code and Codex handle permission requests: the **Permission approval** row of each one's tab, a provider
setting of the choice kind (see "Provider settings" above). The help tip explains how the agent handles permission
requests, that **Follow account configuration** leaves it to each account's own configuration and, for Codex, that
automatic review may use additional tokens. Grok Build has no such row.

Codex's choices:

- **Follow account configuration** leaves the reviewer to each account's own Codex configuration. This is the initial
  choice; automatic review is never enabled just by adding an account.
- **Automatic review** (`auto_review`) asks Codex to review approval requests itself and may use additional tokens.
- **Ask me** (`user`) asks Codex to send approval requests to the user.

Claude Code's choices:

- **Follow account configuration** passes nothing, so each account's own `settings.json` decides. This is the initial
  choice.
- **Automatic** (`auto`) launches Claude Code in its auto mode, where a classifier approves what it judges safe and
  blocks the rest. Whether auto mode is available to an account, which depends on its plan among other things, is Claude
  Code's own decision; Octoboard only asks for it.
- **Ask me** (`manual`) launches Claude Code so that every tool call not already allowed asks the user.
- **Approve everything** (`bypass`) launches Claude Code so that every tool runs without asking.

An explicit choice overrides the agent's own setting for that launch without writing its configuration files, and for
Codex without changing its sandbox or approval policy. Whatever the choice, Follow account configuration included,
Octoboard's own tools run without asking in both agents, while every other tool stays under the choice (see "Claude
Code" and "Codex" in `docs/product/per-agent-launch-specifics.md`).

**Approve everything** is saved only once the user accepts a destructive confirmation that names the agent and warns
that every tool will run without asking in every account of that agent, commands that change or delete files included;
its confirm button carries the choice's own name. Cancelling, Escape included, leaves the choice as it was. If the save
fails, the error alert is shown over the confirmation, which stays open, and the choice stays unchanged. Because the
user has agreed there, Claude Code's own blocking warning for that mode never appears (see "Claude Code" in
`docs/product/per-agent-launch-specifics.md`).

A configuration export that includes the accounts carries the explicit choices, and an import of the accounts applies
them — **Approve everything** included, without this confirmation; the import's review names the choice beforehand (see
"The agent account settings are applied with the accounts" in `docs/product/settings-in-a-configuration.md`). No
session can read or change them.

## Claude compatibility

Whether an agent other than Claude Code — Codex and Grok Build — has its sessions load Claude Code's configuration as
well as its own: Claude Code's instructions (`CLAUDE.md` and rules), skills, agents, commands and MCP servers, and
following Claude Code's hooks and permissions. It is the **Claude compatibility** row of each one's tab, a provider
setting of the switch kind (see "Provider settings" above): under the Permission approval row for Codex, directly above
the accounts table for Grok Build, and never in Claude Code's tab, which has no such setting. The row is there whether
or not Claude Code is an added agent. Its help tip ("About Claude compatibility") says what the setting does whatever
the switch's position: when on, the agent's sessions load Claude Code's instructions, skills, agents, commands and MCP
servers and follow its hooks and permissions; when off, they follow only their own configuration. For Codex it adds
that Octoboard passes these in when a session starts or resumes, so a change takes effect then; for Grok Build, that
Octoboard turns on the Claude support built into Grok Build, and that a change takes effect when a session next starts
or resumes.

**Until the user chooses** for an agent, its value follows a default: **Grok Build is on**, which is Grok's own default;
**Codex is on while Claude Code is an added agent** and off otherwise.

What a launch is given for it is in "Claude compatibility for Codex" and "Claude compatibility for Grok Build" in
`docs/product/per-agent-launch-specifics.md`.

A console session may read the value in effect, unlike the approval modes, but not change it (see "Application
settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`), and a configuration export
that includes the accounts carries the explicit choices (see "The agent account settings an export carries" in
`docs/product/settings-in-a-configuration.md`).
