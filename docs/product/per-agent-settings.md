# Per-agent settings

Settings that apply to every account of one agent, the default account included, rather than to a single account:
how Claude Code and Codex handle permission requests, and whether Codex and Grok Build load Claude Code's configuration.
Each is set in the agent's tab of Settings' **Agent accounts** section, in a row above the agent's accounts table (see
"The accounts table" in `docs/product/agent-accounts.md`).

## Approval modes

How Claude Code and Codex handle permission requests is **one setting per agent**, applied to every account of that
agent, the default account included. It is set in the agent's tab, in the **Permission approval** row above the
accounts table: the row's name with a "?" help tip, a line saying that it applies to every account of the agent, and a
selector at the row's end. The help tip explains how the agent handles permission requests, that **Follow account
configuration** leaves it to each account's own configuration and, for Codex, that automatic review may use
additional tokens. Grok Build has no such row.

Codex's choices:

- **Follow account configuration** leaves the reviewer to each account's own Codex configuration. This is the initial
  choice; automatic review is never enabled just by adding an account.
- **Automatic review** asks Codex to review approval requests itself and may use additional tokens.
- **Ask me** asks Codex to send approval requests to the user.

Claude Code's choices:

- **Follow account configuration** passes nothing, so each account's own `settings.json` decides. This is the initial
  choice.
- **Automatic** launches Claude Code in its auto mode, where a classifier approves what it judges safe and blocks the
  rest. Whether auto mode is available to an account, which depends on its plan among other things, is Claude Code's
  own decision; Octoboard only asks for it.
- **Ask me** launches Claude Code so that every tool call not already allowed asks the user.
- **Approve everything** launches Claude Code so that every tool runs without asking.

The choice is saved when selected; the choices are unavailable while saving, and a failure is shown in an error alert
titled "Could not save the permission approval". It applies when a session of any account of the agent next starts or
resumes, including an account switch, and a legacy pinned directory with no account reference included; a running
process keeps what it was started with. An explicit choice overrides the agent's own setting for that launch without
writing its configuration files, and for Codex without changing its sandbox or approval policy. Whatever the choice,
Follow account configuration included, Octoboard's own tools run without asking in both agents, while every other tool
stays under the choice (see "Claude Code" and "Codex" in `docs/product/per-agent-launch-specifics.md`).

**Approve everything** is saved only once the user accepts a destructive confirmation that names the agent and warns
that every tool will run without asking in every account of that agent, commands that change or delete files included;
its confirm button carries the choice's own name. Cancelling, Escape included, leaves the choice as it was. If the save
fails, the error alert above is shown over the confirmation, which stays open, and the choice stays unchanged. Because
the user has agreed there, Claude Code's own blocking warning for that mode never appears (see "Claude Code" in
`docs/product/per-agent-launch-specifics.md`).

The choices are stored by Octoboard and stay on this machine (see "What is not configuration and never travels" in
`docs/product/configuration-export-and-import.md`). Versions of Octoboard that kept a choice per account are moved over
once: each agent's setting takes the choice its default account had, and the other accounts' choices are dropped.

## Claude compatibility

Whether an agent other than Claude Code — Codex and Grok Build — has its sessions load Claude Code's configuration as
well as its own: Claude Code's instructions (`CLAUDE.md` and rules), skills, agents, commands and MCP servers, and
following Claude Code's hooks and permissions. It is **one setting per agent**, applied to every account of that agent,
the default account included. Claude Code has no such setting.

It is set in the agent's tab, in the **Claude compatibility** row: under the Permission approval row for Codex, directly
above the accounts table for Grok Build, and never in Claude Code's tab. The row is there whether or not Claude Code is
an added agent. It holds the row's name with a "?" help tip ("About Claude compatibility"), a line saying that it
applies to every account of the agent, and a switch at the row's end, named "Claude compatibility for" and the agent.
The help tip says what the setting does whatever the switch's position: when on, the agent's sessions load Claude
Code's instructions, skills, agents, commands and MCP servers and follow its hooks and permissions; when off, they
follow only their own configuration. For Codex it adds that Octoboard passes these in when a session starts or resumes,
so a change takes effect then; for Grok Build, that Octoboard turns on the Claude support built into Grok Build, and
that a change takes effect when a session next starts or resumes.

**Until the user chooses** for an agent, its value follows a default: **Grok Build is on**, which is Grok's own default;
**Codex is on while Claude Code is an added agent** and off otherwise (see "Added agents" in
`docs/product/agent-accounts.md`). Codex's default is worked out afresh whenever it is read, so adding or removing
Claude Code moves it; while the list of added agents is still undecided, Claude Code counts as not added. Once the user has turned an agent's switch, that choice holds,
whatever agents are added or removed later. The switch always shows the value in effect.

The choice is saved when the switch is turned. The switch is unavailable while the save is unanswered — it looks
disabled but stays focusable — and a failure is shown in an error alert titled "Could not save Claude compatibility",
the switch keeping its position. The value is read when a session of any account of the agent starts or resumes,
including an account switch; a running process keeps what it was started with. What a launch is given for it is in
"Claude compatibility for Codex" and "Claude compatibility for Grok Build" in
`docs/product/per-agent-launch-specifics.md`. Octoboard writes none of Claude Code's, Codex's or Grok Build's
configuration files for it.

The choices are kept by the daemon, so every window shows the same. Unlike the approval modes, a console session may
read and change them (see "Application settings: `get_settings` and `update_settings`" in
`docs/product/agent-settings-tools.md`), and a configuration export that includes the application settings carries
the explicit choices (see "The application settings an export carries" in
`docs/product/configuration-export-and-import.md`).
