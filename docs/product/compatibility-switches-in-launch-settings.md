# Compatibility switches in launch settings

An agent's compatibility switches are the provider settings that make its sessions load another agent's configuration
as well as its own: Claude Code's **Codex compatibility** (`codex_compatibility`), Codex's **Claude compatibility**
(`claude_compatibility`), and Grok Build's **Claude compatibility** and **Codex compatibility** (`claude_compatibility`,
`codex_compatibility`). Each agent's provider declares which of its settings are such switches, and only those can be
held at a launch level; no other provider setting, Permission approval included, can. The row in the agent's settings
dialog is the agent-level value (see "Claude compatibility" and "Codex compatibility" in
`docs/product/per-agent-settings.md`); what a launch is given while a switch is on is in
`docs/product/compatibility-between-agents.md`.

Wherever a model and reasoning effort are set, each switch can also be held:

- a console holds a value per agent for its console sessions and another for its project sessions, beside its two
  sets of model and effort (see "Editing a console" in `docs/product/consoles.md`);
- a project holds a value per agent, beside its model and effort (see "The project's account, model and reasoning
  effort" in `docs/product/projects.md`);
- a session holds its own value, beside its own model and effort (see "A session's own switches" below).

## Following, On and Off

At every one of those levels each switch is in one of three states:

- **following** — the level holds nothing for it, and the level behind it decides. This is where every level starts,
  and the only state stored as nothing;
- **On** or **Off** — the level holds that value, which wins over every level behind it.

The switches are kept per agent, as a model and effort are, since each agent has its own; a session reads only the
switches of its own agent. A level holds a value only for a switch its agent declares: one naming any other setting is
refused when it is sent, and a value read from a configuration document or a stored record that this build does not
know as such a switch is dropped.

## Which value a launch uses

Each switch is resolved on its own, taking the first level that holds a value:

| Session | Levels, first to last |
|---|---|
| A project session | the session's own, its project's, its console's project session value, the agent-level value |
| A console session | the session's own, its console's console session value, the agent-level value |

The console session value never reaches a project session, and the project session value never reaches a console
session. The agent-level value is the switch's value in effect, its default included while the user has made no choice.

The value is resolved afresh at every launch — a start, a resume or reopening, an account switch, the restart of a
model change, a change of sharing — from the levels as they are then. A running process keeps what it was started
with: a changed console, project or agent-level value reaches a session that follows it at its next launch.

## A session's own switches

A session's switches behave as its model and reasoning effort do (see `docs/product/model-and-reasoning-effort.md`),
each switch independently:

- They are set when the session is opened — from the session dialog, an owner's `open_session`, a `request_session` or
  a `request_resume_session` that opens a new session on an agent's record, or a handoff for its receiving session —
  and changed through the Change model dialog or an owner's `set_session_model` (see "Changing a session's model and
  effort" in `docs/product/model-and-reasoning-effort.md`), an owner's `resume_session`, and the user's approval of an
  owner's switch of its account or of its handoff. A switch left out keeps what the session has; following clears the
  session's own value.
- The values a session holds are recorded on it and survive a daemon restart, resuming, reopening and switching
  accounts. A session opened without any holds none, so every switch follows its parent.
- A change of switches alone restarts a running session exactly as a change of its model does, and records the values
  on an interrupted session for its next resume. A restart, resume or switch that fails after recording them puts back
  every one of the session's earlier choices — model, effort and switches together.
- A handoff's receiving session holds only the switches the handoff names for it; the original's own switches belong
  to its agent and are not carried over, as its model and effort are not.
- Resuming an existing Octoboard session from the session dialog's Resume conversation tab offers no switches, as it
  offers no model or effort: the session keeps its own.

## Where the switches are set in the window

The switches have no tab or section of their own. Below the Model and Reasoning effort pickers, for the agent those
pickers are currently showing, there is one picker per switch of that agent, in the order its provider declares them,
named for the switch and carrying the same "?" help tip as its row in the agent's settings dialog. A switch the window
has no wording for is not shown. Values set for an agent not currently shown are kept and saved, as its model and
effort are.

Each picker offers three options: a follow option, **On** and **Off**. The follow option is labelled for the level
behind and carries, as a badge, **On** or **Off**: the value the session or level would get by following, as the
levels behind resolve it now.

| Where | Follow option | Its badge shows |
|---|---|---|
| The console dialog's Advanced section, both kinds of session | **Same as agent setting** | the agent-level value |
| The project dialog's **Launch settings** tab | **Same as console** | the console's project session value, else the agent-level value |
| A project session: the session dialog, the Change model dialog, the handoff dialog, the approval of an owner's switch or handoff, the Resume conversation tab's fields for a record | **Same as project** | the project's value, else its console's project session value, else the agent-level value |
| A console session: the same dialogs | **Same as console** | the console's console session value, else the agent-level value |

In the console dialog each switch is a row of the agent tab's table, below the Reasoning effort row, its name and help
tip once in the leading column and a picker under each kind of session; where the cells stack, the switch's name and
help tip sit above each picker (see "The Advanced section" in `docs/product/model-and-reasoning-effort.md`). For a
handoff, the receiving agent's switches are shown, following what a new session of that agent would get; in the
approval of an owner's handoff, the switches the call asked for apply only while the agent it asked for is chosen.

A session-level dialog sends only the switches the user moved off what the session has: picking the follow option for a
switch the session holds clears it, and choosing again what the session holds sends nothing for it. The copy of the
Change model dialog, the switch and handoff approvals and the Switch account confirmation names the compatibility
settings beside the model and reasoning effort.

## Sessions' tools

Every tool that takes a model and a reasoning effort takes the switches beside them, and every tool that reports them
reports the switches. This lifts the rule that an agent's provider settings are the user's alone for the compatibility
switches at these levels only: the agent-level value and the approval modes stay the user's, and `get_settings` and
`update_settings` are unchanged (see "Who reads and changes what" in `docs/product/agent-settings-tools.md`). The shape
the tools share is in "Accounts and launch settings in arguments and results" in the same document.

## In a configuration document

A console's and a project's launch settings in an exported configuration carry the switches they hold, among the
switches whose agent-level choice an export carries (today all of them); an import drops a switch it does not know (see
"Models and efforts travel as stored" in `docs/product/configuration-export-and-import.md`).
