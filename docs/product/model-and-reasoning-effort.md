# Model and reasoning effort

A console carries two pairs of a model and a reasoning effort, one for its console sessions and one for its project
sessions, and a project carries a pair for the sessions opened under it — each of them one pair per agent, since model
names and effort levels are each agent's own. These are the defaults for a session of that agent:

- a **console session** follows its console's console session pair;
- a **project session** takes each value separately from its project, else from its console's project session pair,
  else leaves it to the agent. A project that sets a model but no effort, say, takes the effort from that pair.

The console session pair never reaches a project session, and the project session pair never reaches a console
session.

Each session can override **each value independently** when it is opened or through "Changing a session's model and
effort" below. An owner's switch of its account that the user approves can change either too (see "The user's approval
of an owner's switch" in `docs/product/switching-session-accounts.md`):

- **Inherited** — no session choice is recorded, and the value the defaults above give at that moment is read on
  every launch, including a resume. Sessions with no recorded override follow this rule.
- **Agent's own default** — the session explicitly clears the value, so Octoboard passes nothing for it even when the
  console or project sets one.
- **A chosen value** — the session keeps that value and passes it on every launch.

The last two choices are recorded on the session and survive a daemon restart, resuming, reopening and switching
accounts. A changed console or project default reaches only values the session still inherits, on its next launch;
a running session keeps what it started with.

**The agent's own default means the agent picks**: Octoboard passes nothing, and the agent chooses as it would outside
Octoboard — including whatever the agent's own configuration pins. It is not a promise of the agent's newest model.
A value none of the defaults above sets has this meaning too.

Where defaults are set is in "Editing a console" in `docs/product/consoles.md` and "Editing a project" in
`docs/product/projects.md`; opening a session is in `docs/product/opening-sessions.md`; how each value reaches the
agent, and what each agent does with one it cannot honour, is in "The model and reasoning effort" in
`docs/product/launching-agents.md`.

## The Advanced section

The console dialog, project dialog and dialog that opens a console or project session hold an **Advanced** section,
collapsed when the dialog opens. The handoff dialog holds one too, which works as the session dialog's does for the
session that takes over (see "Handing off from the window" in `docs/product/handing-off-sessions.md`).

In the project dialog and the session dialog it holds a **Model** picker and a **Reasoning effort** picker for the agent
the dialog's agent field currently settles on. In the project dialog the section holds a row of tabs, as wide as their
content and starting at the row's start, which sits with the shown tab's content inside a bordered frame: **Launch
settings**, holding the Model and Reasoning effort pickers, and **Git workflow**, holding the project's own git workflow
settings (see "Reading and changing them" in `docs/product/git-workflow-settings.md`). Launch settings is the tab shown
when the dialog opens. While no agent is settled the frame holds the Git workflow tab alone, shown (see "The project's
account, model and reasoning effort" in `docs/product/projects.md`). The tab shown stays shown while the section is
folded and opened again and while the agent field moves from one agent to another; each time the field moves between
settling on an agent and settling on none, the first tab is shown again. In the session dialog, a note explains that
unchanged values follow the project settings, or the console settings for a console session; the handoff dialog carries
the same inheritance note for its receiving session. The session dialog's **Resume conversation** tab never lets the
agent be picked: it shows the section only once the conversation is found in an agent's own records, for the agent and
account that hold it (see `docs/product/resuming-a-conversation-by-its-session-id.md`). In the session dialog, untouched
fields show the values the session would inherit — for a project session each from its project, else its console's
project session pair; for a console session the console's console session pair — and keep following them; changing a
field applies only to that session, including its later resumes. Switching agents shows the other agent's values and
retains edits separately, but only the selected agent's edits apply to the opened session. Changing accounts keeps those
edits.

In the console dialog it holds a row of tabs, one per agent that can be picked, in the order of the added agents (see
"Which agents the agent pickers list" in `docs/product/consoles.md`), each with the agent's icon and name, as wide as
its content, the tabs starting at the row's start; the row of tabs and the shown tab's content sit together inside a
bordered frame. An agent that cannot be picked has a tab, with the Not installed or Not added badge after its name, only
while the console stores a console session or project session value for it, until every one of them is cleared. Until
the user picks a tab, the one shown follows the agent the Default agent group's Console sessions field holds; a picked
tab stays shown until that tab goes away, which forgets the pick. From a window 768 px wide up, each tab holds a table:
a column for each kind of session, headed **Console sessions** and **Project sessions**, and a **Model** row and a
**Reasoning effort** row, named in a leading column, with a picker in each cell. Below the whole table, a note explains
that a project session's project or session settings take precedence. In a narrower window the cells stack: each kind
under its own heading, then its Model and Reasoning effort pickers, each with the setting's name above it. Either way,
Tab moves through one kind's pickers before the other's: Console sessions' Model and Reasoning effort, then Project
sessions'. Edits on one tab are kept while another is shown, and each value is saved only when changed. Below the frame
and outside it, since it applies whichever tab is shown, is the console session instructions field (see "Console session
instructions" in `docs/product/consoles.md`).

An unset picker is empty, showing a placeholder instead of a value. The placeholder stands for the agent's own setting,
except in the project dialog, where it stands for what the project inherits: "Same as console", with a badge naming the
console's project session value for that agent — a model by its name in the list where the list holds it — or saying
it is the agent's own setting when the console sets none. A picked value is cleared, back to that placeholder, by
clicking the picker's × button, or by pressing Backspace or Delete while its list is closed and its trigger has
keyboard focus. These keys do not clear the choice while focus is on an option in the open list. Clearing a value in
the session dialog explicitly chooses the agent's own default for that value.

What is stored is what was given, trimmed, with a blank value treated as the agent's own default. A value is checked
when it is saved for a console or a project, or given to a session as it is opened or as its owner reopens it with
`reopen_session`, or changed with `set_session_model` (see "Changing a session's model and effort" below): a model must
be one the list in "The models offered" holds, and an effort one of the levels "The reasoning effort offered" gives for
the model the console, project or session ends up with, both read for the account that agent's sessions would run under
there. Where a project or a session sets an effort but no model of its own, the model it ends up with is the one it
inherits — for a project, its console's project session model for that agent; for a session, as in the defaults above —
and the effort is checked against that. Only the given values are checked: in a pair with one value given and the other
inherited, the inherited one is not. A value the agent does not offer is refused, with a message naming it, whether it
came from a dialog or from a session's tool (see `docs/product/agent-settings-tools.md`). Only a value being changed is
checked: one left as it is is never checked again, so a stored value the current list does not hold is kept, offered
last and marked as not in the list once the list is known, and the dialog never silently changes a setting. The mark is
absent while the list is being fetched or could not be read. A list that could not be read (see "While the list is being
fetched, and when there is none") checks nothing. Nothing is checked at launch, and a configuration import stores its
document's values unchecked.

## The models offered

The list is the agent's own, asked of the agent while the section is open rather than kept by Octoboard, so it cannot
lag behind the agent's releases:

| Agent | What is offered |
|---|---|
| Claude Code | Its four version-free aliases — "Best available", Opus, Sonnet and Haiku — which Claude Code itself resolves to the newest model of each family. A pick therefore never names a version and never goes stale. Nothing is run to list them. |
| Codex | Codex's own catalogue, by the name each entry carries, in the order Codex itself ranks them; entries it marks as not for listing are left out. |
| Grok Build | The models Grok offers, in the order Grok itself lists them, by the display names Grok records in its own home and by their ids where it records none; models Grok marks as hidden are left out. |

What is stored, and passed to the agent, is the identifier behind the entry — Claude Code's alias, or the Codex or Grok
Build catalogue's own id — not the name shown.

A catalogue belongs to an account, so the list is asked for the account that agent's sessions would run under: in the
console dialog the account the dialog is currently set to for the agent whose tab is shown, and in the project dialog
the project's own account for that agent — the agent's default account included — else the one its console resolves to;
in the session dialog the account currently selected for that session, and in the Change model dialog the session's own
account (see "The project's account, model and reasoning effort" in `docs/product/projects.md` and "Which account a
session uses" in `docs/product/opening-sessions.md`).

The Change model dialog asks as soon as it opens; elsewhere nothing is asked until the Advanced section is opened for
the first time. It is asked again whenever the agent or that account changes; an answer to a question the dialog has
since moved off is dropped. An answer is reused for ten minutes, and a reason there is none for thirty seconds, so
opening the section again does not run the agent's command each time. Sessions read the same list, from the same
answers, with `list_agent_models` (see "Reading the models an agent offers" in
`docs/product/what-every-session-reads.md`).

## The reasoning effort offered

The levels follow the chosen model, and are each agent's own business rather than a list Octoboard keeps:

- a model whose entry states the levels it takes offers exactly those;
- with no model chosen — where an unset model inherits one, as in the project dialog, the levels of that model
  instead — or for one whose entry does not say — a stored model the list does not hold included — the
  levels of every listed model, each once, in the order they were listed;
- a model that takes no reasoning effort at all offers nothing, and the field says so. The picker is then locked,
  except that a value already stored stays clearable, so the setting can always be undone.

## While the list is being fetched, and when there is none

While the answer for the agent and the account the dialog shows is outstanding, its pickers are locked and show what is
stored, and the Model field says the list is being loaded — in the console dialog, both Model fields of the tab, Console
sessions and Project sessions, each carry this note. Asking the agent is bounded at fifteen seconds.

The agent cannot always be asked: its binary does not resolve on the user's login shell `PATH`, the shell environment
could not be captured (see "The launch environment" in `docs/product/launching-agents.md`), the command failed or did
not finish in time, or what it printed held no model that could be read. For a Codex or Grok Build account the user owns
whose config directory does not exist the command is not run at all, so the directory is never created by it, and the reason is that the command failed. None of those stops the dialog, and it is about
the whole field rather than the value it holds, so the reason is said by an amber warning mark right after the Model
field's label, named that the models could not be loaded. In the console dialog's table of console-session and
project-session settings, the mark follows the Model row's name, once; where the table is stacked, with the setting's
name over each picker, it follows each Model name. Its popover says which of them it was, what to do about it, and any
raw diagnostic output, as "Names and diagnostic details" in `docs/product/error-alerts.md` describes; it has no **Try
again**. It opens as any mark does and opens no error alert (see "Failures, warnings and notes marked at their point" in
`docs/product/error-alerts.md`). The field offers the stored value alone, without marking it as not in the list, or
nothing at all when there is none; the effort picker has no list to offer but stays operable, so a stored level can
still be cleared. A refusal of the model-list request is shown the same way, with its actual reason, including named
records or technical values supplied with it. When the mark goes while it or its popover holds keyboard focus, focus
goes to the Model field. For assistive technology, the Advanced section announces the popover's first sentence once,
alone (the generic sentence for a failure Octoboard has no words for) — never what to do about it, and never the raw
diagnostic output. Neither a missing list nor one still being fetched stops the dialog from being saved.

## Changing a session's model and effort

**Change model** in a session's action menu opens a dialog for that session's model and reasoning effort. It precedes
Switch account, is absent for archived sessions, and is disabled while the session is starting or switching accounts.
The dialog shows the Model and Reasoning effort pickers directly, with the same choices, loading and unavailable-list
rules described above. They start on the session's launch choices, taking inherited values where the session has no
choice of its own; they do not start on the model last observed in the agent's records. While the dialog is open, its
session name, account, restart explanation and untouched fields follow the session's current state and settings. An
edited field keeps its chosen value; choosing the session's current value again makes it follow the session again.
Clearing either chooses the agent's own default for it. Confirm requires at least one value to differ from the
session's current launch values. Only changed values are sent, so the other value keeps its choice or inheritance.

A session that becomes starting or switches accounts while the dialog is open temporarily disables Confirm, with a
warning explaining that the user must wait for it to finish. A session deleted or archived while it is open instead
turns the dialog into a failure modal, naming the session in an inline name chip; a deleted session keeps its last
known name, including a rename made while the dialog was open. Confirm closes the dialog immediately and the change
runs in the background. A refusal or failure opens an error alert identifying the session and the failed model change.
See "Recovery and unexpected errors" in `docs/product/error-alerts.md` for its recovery action.

The agent and account stay the same. A running session is restarted, keeping its conversation; one with no conversation
yet starts fresh. It shows as starting in every window and in session reads from when the change claims the running
session, including time spent checking the settings, until it ends. A refusal before the restart restores its prior
presentation. A turn the restart cuts off, or one stopped by a usage limit, receives a prompt to continue its task, the
same one a switch of account sends (see "Continuing after a switch" in `docs/product/switching-session-accounts.md`);
otherwise the session returns awaiting instructions. The processes the session's agent left running in the background
are carried over to the restarted session rather than stopped, as in an account switch, and kept as project processes
when the restart fails after it ended the process (see "Automatic stopping and its boundaries" in
`docs/product/background-processes.md`). An interrupted session is not started: the choices are recorded for its next
resume. The changed values become the session's own choices, persisting as described at the top of this document. The
user can change a session waiting for the user too; the dialog explains that restarting ends the prompt it was showing.
Where no conversation has started, it explains that the restart starts a new conversation instead.

A console session or project coordinator changes a session bound to it with **`set_session_model`**, without asking for
approval. Workers have no such tool, and an owner cannot change an independent session or another owner's session. The
call takes `session` and at least one of `model` and `effort`. Each omitted value keeps the session's current choice or
inheritance; a blank string chooses the agent's own default, and another string is trimmed and becomes the session's own
value. Only values actually being changed are checked against what the agent offers under the session's account, under
"The Advanced section" above. Giving the choices it already has succeeds without restarting or checking them again, even
when the current list no longer offers them. This no-change answer does not bypass another model change already in
progress: a repeated request is refused while that change holds the session. Giving an inherited value explicitly makes
it the session's own choice, even if it resolves to the same value.

Once started, the change runs to completion even if the calling session exits and drops the call. If the call is no
longer waiting when it ends, the result or failure is delivered as a message to the calling session instead.

The call waits for the restart to come up and answers with `session`, `agent`, `restarted` and a `note`, plus the
session's own `model` and `effort`: each absent when inherited, `null` for the agent's own default, or its chosen
string. `restarted` is false for an interrupted session or unchanged choices. The note says whether it was sent a prompt
to continue its task, which the owner must not repeat; otherwise the owner sends its next step with `instruct_session`.
No separate message about the change is sent to the session.

A change is refused for an archived session (reopen it with the desired values instead), a session starting or already
being changed, or while its account's sharing of the default configuration is being changed. An owner's call is also
refused while the session is waiting for the user, so it cannot end a prompt only the user can answer. A usage-limit
dialog alone does not count as such a wait and can be ended by the restart. A changed value the agent does not offer is
refused before the process is stopped or any choices are recorded. If the process cannot be stopped in time, the choices
stay as they were (`model_change_did_not_stop`). If the restart is refused, fails to launch or ends immediately
(`model_change_did_not_come_up`), the session is left interrupted with its earlier choices.

The window's `session_model_set` reply distinguishes `outcome: unchanged`, `recorded` and `restarted`; only a restarted
result carries `continued`, saying whether a continuation prompt was sent. The window announces a recorded change as
settings saved for the next resume and a restarted change as a completed restart; unchanged choices produce no success
announcement. The session name is read again when the reply arrives. These announcements, and successful account-switch
announcements, wait until the window is visible and focused and no modal covers their status region, including an error
alert, Settings or the file viewer. The initial announcement delay and its expiry wait while those conditions are not
met; a newer result replaces one still waiting to be announced.

## The model and effort a session is shown running with

Where a session's model and reasoning effort are shown — the model line of focus mode's session cards (see "What both
views show" in `docs/product/focus-mode.md`) and of the archive view's rows (see "The archive view" in
`docs/product/archive-view.md`), the agent icon's tooltip and the accessible name of those cards and rows, of focus
mode's archived rows and of a session row in the sidebar (see "Session rows" in
`docs/product/sidebar-session-rows.md`) — each of the two is decided
on its own, taking the first of these that gives a value:

1. what the agent's own records show the session running with (below);
2. for the effort only, where the records show the model the session runs but no effort, the level the agent runs that
   model at by itself, where Octoboard can tell it (below, under "The agent's own default level");
3. a value the session chose when it was opened or subsequently changed;
4. for a value the session inherits, what the defaults at the top of this doc give at that moment — a project
   session's project, else its console's project session pair; a console session's console session pair.

Step 2 comes before the session's own choice and its inherited default because the records show the session running with
no effort set: a console or project default set after the session was opened does not reach it, so only the agent's own
level says what it runs at. A level from step 2 is shown as a plain level, like one the records show — "gpt-6.1-sol ·
medium" — with nothing marking it as a default. A value the session cleared to the agent's own default gives nothing at
steps 3 and 4.

An item none of these gives — the agent's own default where Octoboard cannot tell its level, included — reads
**Default**; a session knowing neither reads **Default model** instead of the pair.

**Names.** A Claude Code model id is shortened to its family and version, its release date dropped: `claude-opus-5-5`
reads "Opus 5.5". Claude Code's aliases are capitalised — Opus, Sonnet, Haiku — and the `best` alias is named "Best
available", as the launch pickers name it. Every other id, a Claude Code one of an unfamiliar shape included, and every
effort is shown as the agent spells it.

**What the records show.** The daemon reads, as each agent records them:

| Agent | Read from |
|---|---|
| Claude Code | The model and effort of each reply in the session's main conversation; its own native subagents, which run other models, and Claude Code's own notices are skipped. |
| Codex | The model and effort of each turn's context record. |
| Grok Build | The model and effort the session's summary records as configured, not the model a request was served by. |

The last record that names a model, or an effort, wins; a record that names neither leaves the last one known in place.
Codex's turn context is the exception for the effort: it carries the effort its whole configuration resolved to and
leaves it out when none sets one, so a turn context that names a model and no effort unsets an effort an earlier turn
named. They are collected alongside the context size and as often (see "When the context size refreshes" in
`docs/product/session-context.md`), kept through archiving and restarting Octoboard, and cleared when the session moves
to a different native conversation, until the new one shows its own. They are reported on the session as
`observed_model` and `observed_effort`, separate from the session's launch choices `model` and `effort`. A model-change
restart clears the earlier observations once the old process has stopped. Until the restarted agent records its new
model and effort, the display falls back to its launch choices as above: older records, including an unread tail of the
previous process's conversation, cannot restore the earlier model or effort. This remains true across a daemon restart,
a copy of the same conversation to another account, and Codex unpacking its conversation records before the agent
records the new values. Moving or recreating the same records does not make their earlier model or effort a new
observation.

**The agent's own default level.** Where the records name the model a session runs but no effort, the level that model
runs at by itself is worked out per agent:

| Agent | Where the level comes from |
|---|---|
| Claude Code | Not worked out, and not needed: every reply in the main conversation records its effort, so the records give it from the session's first reply on. |
| Codex | The default level that the session's account's catalogue cache — `models_cache.json` in the account's Codex home — gives the model the records name. Codex leaves the effort out of a turn's context record exactly when no layer of its configuration sets one, and then runs the model at that catalogue default. The catalogue is the account's, so the same model can default to different levels on two accounts. A model the catalogue does not list, or a catalogue that is missing, gives no level; a catalogue or an account directory that cannot be read leaves the level last worked out. |
| Grok Build | Not worked out: the session's summary records the configured effort, and whether Grok's own default effort setting applies to a model chosen at launch is not established, so a session whose summary has not been written yet reads Default. |

Before a Codex session's first turn there is no context record, so no level is worked out, and an effort steps 3 and 4
leave unset reads Default; Codex's configuration files are not read in its place. The level is not stored: it is worked
out again on each pass that collects the records, from the model and effort that pass records and the catalogue as it
then stands, so it reaches clients together with the record that makes it apply, and a changed catalogue reaches
archived sessions too. It is reported on the session as `default_effort`, separate from `observed_effort`, which keeps
meaning what the records show.
