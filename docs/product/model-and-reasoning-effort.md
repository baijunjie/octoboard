# Model and reasoning effort

A console carries a model and a reasoning effort for its console sessions, and a project carries a pair for the
sessions opened under it — in both cases one pair per agent, since model names and effort levels are each agent's own.
These are the defaults for a session of that agent: a console session follows its console, a project session its
project. Neither falls back to the other. A project that sets nothing therefore leaves the choices to the agent,
whatever its console sets for its console sessions.

Each session can override **each value independently** when it is opened:

- **Inherited** — no session choice is recorded, and the console's or project's current value is read on every launch,
  including a resume. Sessions with no recorded override follow this rule.
- **Agent's own default** — the session explicitly clears the value, so Octoboard passes nothing for it even when the
  console or project sets one.
- **A chosen value** — the session keeps that value and passes it on every launch.

The last two choices are recorded on the session and survive a daemon restart, resuming, reopening and switching
accounts. A changed console or project default reaches only values the session still inherits, on its next launch;
a running session keeps what it started with.

**The agent's own default means the agent picks**: Octoboard passes nothing, and the agent chooses as it would outside
Octoboard — including whatever the agent's own configuration pins. It is not a promise of the agent's newest model.
Unset console and project values have this meaning too.

Where defaults are set is in "Editing a console" and "Editing a project" in `docs/product/consoles-and-projects.md`;
opening a session is in "Opening a session" in `docs/product/sessions.md`; how each value reaches the agent, and what
each agent does with one it cannot honour, is in "The model and reasoning effort" in
`docs/product/launching-agents.md`.

## The Advanced section

The console dialog, project dialog and dialog that opens a console or project session hold an **Advanced** section,
collapsed when the dialog opens. It holds a **Model** picker and a **Reasoning effort** picker for the agent the
dialog's agent field currently settles on, and a note saying whose sessions they apply to. In the console dialog it
also holds, below the two pickers, the console session instructions field (see "Console session instructions" in
`docs/product/consoles-and-projects.md`). In the session dialog, untouched fields
show that agent's current project defaults, or the console's console-session defaults for a console session, and keep
following them; changing a field applies only to that session, including its later resumes. Switching agents shows the other agent's values and retains edits separately, but only
the selected agent's edits apply to the opened session. Changing accounts keeps those edits.

An unset picker is empty, showing a placeholder standing for the agent's own setting instead of a value. A picked
value is cleared by clicking the picker's × button, or by pressing Backspace or Delete while its list is closed and
its trigger has keyboard focus. These keys do not clear the choice while focus is on an option in the open list.
Clearing a value in the session dialog explicitly chooses the agent's own default for that value.

What is stored is what was given, trimmed, with a blank value treated as the agent's own default. A value is checked
when it is saved for a console or a project, or given to a session as it is opened: a model must be one the list in
"The models offered" holds, and an effort one of the levels "The reasoning effort offered" gives for the model the
console, project or session ends up with, both read for the account that agent's sessions would run under there. A
value the agent does not offer is refused, with a message naming it, whether it came from a dialog or from a console
session's tool (see `docs/product/agent-settings-tools.md`). Only a value being changed is checked: one left as it is
is never checked again, so a stored value the current list does not hold is kept, offered last and marked as not in the
list, and the dialog never silently changes a setting. A list that could not be read (see the last section) checks
nothing. Nothing is checked at launch, and a configuration import stores its document's values unchecked.

## The models offered

The list is the agent's own, asked of the agent while the section is open rather than kept by Octoboard, so it cannot
lag behind the agent's releases:

| Agent | What is offered |
|---|---|
| Claude Code | Its four version-free aliases — "Best available", Opus, Sonnet and Haiku — which Claude Code itself resolves to the newest model of each family. A pick therefore never names a version and never goes stale. Nothing is run to list them. |
| Codex | Codex's own catalogue, by the name each entry carries, in the order Codex itself ranks them; entries it marks as not for listing are left out. |
| Grok Build | The models Grok offers, in the order Grok itself lists them, by the display names Grok records in its own home and by their ids where it records none; models Grok marks as hidden are left out. |

What is stored, and passed to the agent, is the identifier behind the entry — Claude Code's alias, or the Codex or
Grok Build catalogue's own id — not the name shown.

A catalogue belongs to an account, so the list is asked for the account that agent's sessions would run under: in the
console dialog the account the dialog is currently set to for that agent, and in the project dialog the project's own
account for that agent — the agent's default account included — else the one its console resolves to; in the session
dialog the account currently selected for that session (see "The project's account, model and reasoning effort" in
`docs/product/consoles-and-projects.md` and "Which account a session uses" in `docs/product/sessions.md`).

Nothing is asked of the agent until the Advanced section is opened for the first time, and it is asked again whenever
the agent or that account changes; an answer to a question the dialog has since moved off is dropped. An answer is
reused for ten minutes, and a reason there is none for thirty seconds, so opening the section again does not run the
agent's command each time.

## The reasoning effort offered

The levels follow the chosen model, and are each agent's own business rather than a list Octoboard keeps:

- a model whose entry states the levels it takes offers exactly those;
- with no model chosen, or for one whose entry does not say — a stored model the list does not hold included — the
  levels of every listed model, each once, in the order they were listed;
- a model that takes no reasoning effort at all offers nothing, and the field says so. The picker is then locked,
  except that a value already stored stays clearable, so the setting can always be undone.

## While the list is being fetched, and when there is none

While the answer for the agent and the account the dialog shows is outstanding, both pickers are locked and show what
is stored, and the Model field says the list is being loaded. Asking the agent is bounded at fifteen seconds.

The agent cannot always be asked: its binary does not resolve on the user's login shell `PATH`, the shell environment
could not be captured (see "The launch environment" in `docs/product/launching-agents.md`), the command failed or did
not finish in time, or what it printed held no model that could be read. None of those is an error on the dialog — the
Model field says which it was, and offers the stored value alone, marked as not in the list, or nothing at all when
there is none; the effort picker has no list to offer but stays operable, so a stored level can still be cleared.
Neither a missing list nor one still being fetched stops the dialog from being saved.
