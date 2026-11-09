# Per-agent settings

Settings that apply to every account of one agent, the default account included, rather than to a single account: the
agent's tags, and the settings each agent's provider declares for itself — how Claude Code, Codex and Grok Build handle
permission requests, whether Codex and Grok Build load Claude Code's configuration, and whether Claude Code and Grok
Build load Codex's. Each is set in a row of the
agent's **Agent settings** dialog, opened from the Settings entry of the agent's menu in Settings' **Agent accounts**
section (see "The agent settings dialog" in `docs/product/agent-accounts.md`).

## Agent tags

The user can put tags on each agent — Claude Code, Codex and Grok Build: short free-form labels saying what the agent
is good for, such as "review" or "ui". They are **one list per agent**, applied to every account of that agent, the
default account included, not to an account or a model. They are information for agents; Octoboard itself selects
nothing by them. A tag follows the rules of a project's tag (see "Project tags" in `docs/product/project-tags.md`):
there is no list of tags to manage, a tag is typed the first time it is used, and each tag is trimmed, an empty one is
dropped and one matching another of the agent's tags ignoring case is dropped, the first spelling staying.

They are set in the **Tags** row, the first row of every agent's settings dialog, above the rows of the agent's
provider settings (see "Provider settings" below). A "?" help tip beside the row's name explains that tags
describe the tasks the agent is suited for and give other sessions a reference when choosing an agent. The row keeps
a line saying that it applies to every account of the agent and, under them across the row's full width, the same
Tags field as a project's
(see "Editing a project" in `docs/product/projects.md`), its picked tags looking and behaving as there (see "Picked
tags" in `docs/product/project-tags.md`). The field has no visible label of its own; its accessible name is "Tags for"
and the agent. Its empty text input says the agent has no tags while none is picked, and invites adding a tag once one
is. The tags offered are the distinct tags the added agents carry (every agent's while the list of added agents is
undecided; see "Added agents" in `docs/product/agent-accounts.md`), tags differing only in case counting as one, so a
tag used on one agent is offered for the others; a typed tag matching an offered one takes its spelling.

Each change is saved as it is made, as the agent's whole list; the field stays usable meanwhile. One save is out at a
time: a change made while one is unanswered waits, and the save sent after it carries every change since. While saves
are outstanding the field shows the list last asked for, so changes made in quick succession build on one another and
never conflict with one another. Once the saves are answered it shows the stored tags, and a list another window set
meanwhile is shown then and built on by the next change. Each save is checked against the tags' version (see
`docs/product/concurrent-edits.md`): a save refused because another window changed the agent's tags just before it,
and a waiting change that would overwrite a list another window set meanwhile, are dropped, the field shows the stored
tags, and an error alert titled "Could not save the tags" says that a change was made elsewhere and nothing was saved.
Any other failed save is shown in the same alert with its own reason; a change that waited is still sent, unless a list
another window set is stored by then, when it is dropped too; and once no later change is still being saved the field
goes back to the stored tags.

The tags are kept by the daemon with the other application settings, so every window shows the same ones, and a change
reaches them at once; they survive a restart. **Removing an agent keeps its tags**, which come back when it is added
again (see "Reordering and removing an agent" in `docs/product/agent-accounts.md`); a removed agent's tags are not
offered for the other agents. Every session reads the tags of the agents a session could be opened on, with their
accounts, through `list_agents`, which does not list a removed agent (see
`docs/product/listing-agents-from-a-session.md`). Like the compatibility switches, a console session may read them but not
change them (see "Application settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`), and a
configuration export that includes the accounts carries them (see "The agent account settings an export carries" in
`docs/product/settings-in-a-configuration.md`).

## Provider settings

Each agent's provider declares the settings the user can make for it. Each is **one setting per agent**, applied to
every account of that agent, the default account included. Today they are:

| Agent | Row | Setting id | Kind | Carried by a configuration export | Readable by sessions |
|---|---|---|---|---|---|
| Claude Code | **Permission approval** (see "Approval modes" below) | `approval_mode` | Choice of `auto`, `manual`, `accept_edits`, `plan` or `bypass` | Yes | No |
| Claude Code | **Codex compatibility** (see "Codex compatibility" below) | `codex_compatibility` | Switch, off until chosen | Yes | Yes |
| Codex | **Permission approval** (see "Approval modes" below) | `approval_reviewer` | Choice of `user`, `auto_review` or `full_access` | Yes | No |
| Codex | **Claude compatibility** (see "Claude compatibility" below) | `claude_compatibility` | Switch, off until chosen | Yes | Yes |
| Grok Build | **Permission approval** (see "Approval modes" below) | `approval_mode` | Choice of `ask`, `auto` or `always_approve` | Yes | No |
| Grok Build | **Claude compatibility** (see "Claude compatibility" below) | `claude_compatibility` | Switch, off until chosen | Yes | Yes |
| Grok Build | **Codex compatibility** (see "Codex compatibility" below) | `codex_compatibility` | Switch, off until chosen | Yes | Yes |

A setting is one of two kinds:

- **A choice** takes one of the values its provider declares, or none. With none, which is where every choice setting
  starts, Octoboard passes no override and the agent applies its native configuration and resume behavior. The first
  option is the provider's own inheritance label where it has one, otherwise **Follow account configuration**. Codex's
  permission setting uses **Custom (config.toml)** (see "Approval modes" below). A declared choice may need the user's
  confirmation before it is saved.
- **A switch** is on or off. Until the user turns it, it follows the default its provider declares, on or off,
  whatever agents are added; every switch today is off until chosen. Once the user has turned the switch, that choice
  holds.

Octoboard keeps only the user's explicit choices, in the daemon, so every window shows the same and they survive a
restart; every window is given the **value in effect** of every setting of every agent: a switch's choice or else its
default, a choice setting's choice or else none. A stored choice the setting no longer takes is read as no choice.

The agent's settings dialog has one row per setting, in the order the provider declares them, under the Tags row:
Claude Code's shows Permission approval then Codex compatibility; Codex's shows Permission approval then Claude
compatibility; Grok Build's shows Permission approval, Claude compatibility, then Codex compatibility. A setting the
window has no wording for is not shown; a choice the window has no name for is offered by its value in words (a value
`read_only` as "Read only"). Each row holds the setting's name with a "?" help tip saying what it does, a line saying
that it applies to every account of the agent, and, at the row's end, a selector for a choice or a switch for a switch,
named for the setting and the agent ("Permission approval for Codex"). A choice is saved when selected, its choices
unavailable while the save is unanswered; a switch is saved when turned, unavailable while the save is unanswered — it
looks disabled but stays focusable — and keeps its position when the save fails. The switch always shows the value in
effect. A failed save is shown in an error alert titled for the setting ("Could not save the permission approval",
"Could not save Claude compatibility").

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
the choice, a switch going back to its default and a choice setting to the agent's native configuration and resume
behavior. A setting the agent's provider does not declare, a value of the other kind, or a choice the setting does not
offer is refused as an unreadable request (`unreadable_request`). The confirmation a choice needs is asked for by the
window, not by the daemon, so this request saves such a choice at once.

## Approval modes

How Claude Code, Codex and Grok Build handle permission requests and access: the **Permission approval** row of each
agent's settings dialog, a provider setting of the choice kind (see "Provider settings" above).

All three permission menus show a short secondary explanation under every option, including the inheritance option.
Native summaries retain the original English wording for Codex and Grok Build and the original English and Simplified
Chinese wording for Claude Code; other locales translate the English source. Claude Code's and Grok Build's Follow
account configuration are Octoboard choices with their own explanation. The notes wrap within the expanded list; the
closed selector shows only the selected label. Assistive technology receives the explanation as a separate description,
keeping the option's name to its label and reading the explanation once.

The native summaries contain no added qualifiers. Each agent's separate help has two spaced paragraphs. The first
explains that changes take effect on the next session start or resume and permissions apply on the computer running the
session. The second explains the agent's native configuration and resume behavior: Codex's Custom distinction between
new and resumed sessions, Claude Code's native rules and explicit-mode precedence, and Grok Build's native rules and
fallback from Auto to Ask when its native feature is unavailable. Permission grants and safeguards are explained in the
risk confirmations.

Codex's choices, in menu order:

- **Custom (config.toml)** passes no permission override. This is the initial choice. New sessions use the account's own
  native configuration; resumed sessions retain their saved native permissions. Selecting Custom does not reset a
  resumed conversation's saved permissions to the account's current configuration. Select an explicit preset to replace
  saved permissions on the next launch or resume.

- **Ask for approval** (`user`) uses Codex's built-in workspace permission profile, its `on-request` approval policy and
  the user as reviewer. It permits work inside the workspace and asks before actions beyond that boundary.

- **Approve for me** (`auto_review`) uses the same workspace profile and approval policy, with automatic review of
  eligible approval requests. Automatic review can still lead to a request for manual approval.

- **Full access** (`full_access`) uses Codex's built-in unrestricted permission profile and `never` approval policy. It
  removes Codex's sandbox restrictions and permission approval for file and internet access.

Claude Code's choices, in menu order:

- **Follow account configuration** passes no permission-mode override. This is the initial choice. Claude Code decides
  the mode using its native configuration and resume rules. A resume may restore a saved mode under those rules, with
  exceptions such as bypass not being restored solely from the saved mode and auto depending on account eligibility.
  Selecting an explicit mode takes precedence over the saved mode.

- **Auto** (`auto`) uses Claude Code's auto mode, where a classifier reviews actions. Whether auto mode is available to
  an account is Claude Code's own decision; Octoboard only asks for it.

- **Manual** (`manual`) asks for actions that Claude Code's permission rules have not already allowed.

- **Accept edits** (`accept_edits`) automatically accepts file edits and the filesystem operations Claude Code permits
  in this mode; other actions remain subject to its permission rules.

- **Plan** (`plan`) starts Claude Code in its planning mode, for preparing a plan before making changes.

- **Bypass permissions** (`bypass`) skips routine permission prompts, including for tools that change or delete files.
  Explicit permission rules and Claude Code's built-in safeguards still apply.

Grok Build's choices, in menu order:

- **Follow account configuration** passes no permission-mode override. This is the initial choice. Grok Build uses its
  native configuration and resume rules.

- **Ask** (`ask`) requests permission for tool calls that are not already allowed.

- **Auto** (`auto`) uses Grok Build's classifier to approve safe tool calls; dangerous calls can still ask. The native
  feature decides its availability and falls back to Ask when unavailable.

- **Always-approve** (`always_approve`) approves tool calls without routine prompts, including calls that change or
  delete files. Deny rules and hooks still apply. Permission mode is separate from sandboxing; this setting does not
  disable a sandbox or make a sandboxed launch compatible with Octoboard.

An explicit preset or mode is passed on every launch and resume without writing the agent's configuration files. The
concrete launch parameters are in "How each agent is given the injection" in
`docs/product/per-agent-launch-specifics.md`. Claude Code and Codex pre-approve Octoboard's own tools under every
choice; Grok Build receives no such grant. The agents' explicit rules and each tool's own refusals still apply (see
"Claude Code", "Codex" and "Grok Build" in that document).

**Full access**, **Bypass permissions** and **Always-approve** are saved only once the user accepts a destructive
confirmation that names the agent and explains the scope of the permission grant across all of its accounts. The confirm
button carries the choice's own name. Cancelling, Escape included, leaves the choice as it was. If saving fails, an
error alert appears over the confirmation, which stays open, and the stored choice stays unchanged. Accepting Bypass
permissions also accepts Claude Code's own blocking startup warning for that mode, for the launch alone.

A configuration export that includes the accounts carries every explicit choice, and an import of the accounts applies
them, including Full access, Bypass permissions and Always-approve, without this separate confirmation. The import
review names the choice beforehand (see "The agent account settings are applied with the accounts" in
`docs/product/settings-in-a-configuration.md`). No session can read or change these settings.

## Claude compatibility

Whether an agent other than Claude Code — Codex and Grok Build — has its sessions load Claude Code's configuration as
well as its own: Claude Code's instructions (`CLAUDE.md` and rules), skills, agents, commands and MCP servers, and
following Claude Code's hooks and permissions. It is the **Claude compatibility** row of each one's settings dialog, a
provider setting of the switch kind (see "Provider settings" above): under the Permission approval row for Codex and
Grok Build, and never in Claude Code's dialog, which has no such setting. The row is there whether
or not Claude Code is an added agent. Its help tip ("About Claude compatibility") says what the setting does whatever
the switch's position: when on, the agent's sessions load Claude Code's instructions, skills, agents, commands and MCP
servers and follow its hooks and permissions; when off, they follow only their own configuration. For Codex it adds
that Octoboard passes these in when a session starts or resumes, so a change takes effect then; for Grok Build, that
Octoboard turns on the Claude support built into Grok Build, and that a change takes effect when a session next starts
or resumes.

**Until the user chooses** for an agent, it is **off**, for Codex and Grok Build alike, whichever agents are added.
Off, Octoboard passes Codex nothing for it. For Grok Build, off means Octoboard sets nothing either way and Grok follows
its own configuration, whose own Claude Code support is on unless the user turns it off there; so a Grok Build session
may load Claude Code's configuration with the switch off, and turning the switch on makes sure it does.

What a launch is given for it is in "Claude compatibility for Codex" and "Claude compatibility for Grok Build" in
`docs/product/compatibility-between-agents.md`.

A console session may read the value in effect, unlike the approval modes, but not change it (see "Application
settings: `get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`), and a configuration export
that includes the accounts carries the explicit choices (see "The agent account settings an export carries" in
`docs/product/settings-in-a-configuration.md`).

## Codex compatibility

Whether Claude Code and Grok Build have their sessions load Codex's configuration as well as their own: Codex's
instructions (`AGENTS.md` and rules), skills, custom prompts and MCP servers, and following Codex's hooks. It is the
**Codex compatibility** row of each one's settings dialog, a provider setting of the switch kind (see "Provider
settings" above): under the Permission approval row for Claude Code, under the Claude compatibility row for Grok Build,
and never in Codex's dialog, which has no such setting. The row is there whether or not Codex is an added agent.

Its help tip ("About Codex compatibility") says what the setting does whatever the switch's position. For Claude Code:
when on, its sessions load Codex's instructions, skills, custom prompts and MCP servers and follow Codex's hooks; when
off, they follow only their own configuration; Octoboard passes these in when a session starts or resumes, so a change
takes effect then. For Grok Build: when on, its sessions load Codex's instructions, skills and MCP servers, use Codex's
custom prompts when asked for one by name and follow Codex's hooks; when off, they follow only their own configuration;
Octoboard passes these in when a session starts, and a resumed session picks up a change to the MCP servers and skills
only.

**Until the user chooses** for an agent, it is **off**, for Claude Code and Grok Build alike, whichever agents are
added. Off, Octoboard passes nothing for it.

What a launch is given for it is in "Codex compatibility for Claude Code" and "Codex compatibility for Grok Build" in
`docs/product/compatibility-between-agents.md`.

Like Claude compatibility, a console session may read the value in effect but not change it (see "Application settings:
`get_settings` and `update_settings`" in `docs/product/agent-settings-tools.md`), and a configuration export that
includes the accounts carries the explicit choices (see "The agent account settings an export carries" in
`docs/product/settings-in-a-configuration.md`).
