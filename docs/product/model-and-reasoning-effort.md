# Model and reasoning effort

A console carries a model and a reasoning effort for its console sessions, and a project carries a pair for the
sessions opened under it — in both cases one pair per agent, since model names and effort levels are each agent's own.
A session is launched with the pair of its own agent and no other: a console session its console's, a project session
its project's. Neither falls back to the other. A project that sets nothing therefore launches its sessions exactly as
the agent would run them unasked, whatever its console sets for its console sessions, and the console's pair reaches
console sessions alone.

**Unset means the agent picks**: Octoboard passes nothing, and the agent chooses as it would outside Octoboard — which
is whatever the agent's own configuration pins, where it pins a model. Unset is not a promise of the agent's newest
model. Both values are read from the console or the project each time a session is opened or resumed, rather than
recorded on the session, so a running session keeps what it was started with and a change reaches it at its next
resume.

Where the pair is set is in "Editing a console" and "Editing a project" in `docs/product/consoles-and-projects.md`;
which pair a session takes is in "Which model and reasoning effort a session uses" in `docs/product/sessions.md`; how
each value reaches the agent, and what each agent does with one it cannot honour, is in "The model and reasoning
effort" in `docs/product/launching-agents.md`.

## The Advanced section

Both are set in the **Advanced** section of the console dialog and of the project dialog, collapsed when the dialog
opens, which holds a **Model** picker and a **Reasoning effort** picker for the agent the dialog's agent field
currently settles on, and a note saying whose
sessions they apply to. An unset picker is empty, showing a placeholder standing for the agent's own setting instead of
a value. A picked value is put back to unset with the picker's clear button, or with Backspace or Delete while the
picker has keyboard focus, which the section says under its note.

Neither value is checked against any list, in the dialog or at launch. What is stored is whatever was given, trimmed,
with a blank value read as unset. A stored value the current list does not hold is kept, offered last and marked as
not in the list, so the dialog never silently changes what a console or a project is set to — which matters the more
now that the list is the agent's own rather than Octoboard's.

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
account for that agent — the agent's default account included — else the one its console resolves to (see "The
project's account, model and reasoning effort" in `docs/product/consoles-and-projects.md`).

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
