# Resuming a conversation by its session ID

A project session can be opened on a conversation that already exists, given its session ID — the one the agent's own
resume command takes — instead of starting a new one: a conversation the user had with the agent outside Octoboard
(an agent run in a terminal), or one a session of the same project already holds, which is then
reopened, resumed or gone to rather than continued a second time, since one conversation is never shared by two
sessions. The session dialog's **Resume conversation** tab, beside the default **New conversation** tab at the head of
the dialog, does this. The tabs are offered for a project session only, from the project list, the terminal's **New
session** offer and a project's focus mode alike: the dialog for a console session has none, and neither has the dialog
opened from a console session's focus mode, whose session always reports to that console session. A session started with
the `open_session` tool never resumes one. A session can ask the user to do what the tab does, with the same limits,
through `request_resume_session` (see `docs/product/requests-to-resume-a-conversation.md`).

The tab first shows the required **Session ID** field alone. The fields after it appear only once the lookup of a valid
ID has answered, and depend on what holds the conversation (see "Finding the conversation" below). **The agent and
account are picked only for an ID found nowhere**: otherwise they are found from the ID and shown as a locked Agent
field. It has no **Report to** field
either, since a resumed session is never bound; the **New conversation** tab still shows the choice made there before.
The ID is acted on only when the dialog is submitted from the **Resume conversation** tab; one typed there and then left
for the **New conversation** tab is not.

**The ID must be a UUID.** The field takes the hyphenated form the agents show (such as
`123e4567-e89b-12d3-a456-426614174000`), in either letter case. An empty field, or anything else, stops the submit and
is reported under the field — asking for the session ID of the conversation to resume when it is empty, and giving an
example ID otherwise — and focus moves to the field. From then on the message follows what is typed.

## Finding the conversation

Once the field holds a valid ID and typing pauses, the daemon is asked where the conversation is held, and the answer
settles which fields follow the ID and what the submit button is and does. Until it answers, a spinner shows at the end
of the ID field and the submit button is disabled. Changing the ID asks again; only the answer for the ID in the field
counts. What was found is announced to assistive technology; it is not shown as a line of text.

A session that records the ID as its agent's conversation is looked for first, whatever its agent, and decides the
answer on its own; only when no session records it are the agents' own records searched. The session found is judged by
its state at that moment in the window, and the tab follows it while the dialog is open: one archived meanwhile is
reopened, one interrupted meanwhile is resumed, and one whose process starts meanwhile is gone to.

| Where the conversation is | Fields after the Session ID | Submit button |
|---|---|---|
| No session; an agent's own record | **Name**, editable, as on the **New conversation** tab; the agent and account that hold the record, locked; the Advanced section for that agent and account | **Open** — opens a new session resuming it (see "Opening a new session on a found conversation" below) |
| An archived session of this project | **Name**, filled in with the session's name and editable; its agent and account, locked | **Reopen** — reopens that session (see "Reopening an archived session from the tab" below) |
| An interrupted session of this project | **Name**, showing the session's name, and its agent and account, all locked | **Resume** — resumes that session (see "Resuming or going to a session of this project from the tab" below) |
| A session of this project that is starting or whose process is running | **Name**, showing the session's name, and its agent and account, all locked | **Go to session** — selects that session (see "Resuming or going to a session of this project from the tab" below) |
| A session of another project, or a console session, in any state | none; an alert directly under the field names that session and where it is (see "Naming a session outside the project" in this section) | disabled |
| Nowhere | a warning directly under the field; then **Name**, the **Agent** field with its agent and account pickers, and the Advanced section, as on the **New conversation** tab | **Open** — opens a new session resuming it on the agent and account picked (see "Opening a new session on a found conversation" below) |

A conversation found nowhere is no verdict on the ID: an agent release may keep its records where Octoboard does not
look, and the agent's own resume is what says whether it has the conversation. So it is a warning, not the field's
error: titled that the conversation was not found, it says that no account has a record of this ID, and to choose the
agent and account the conversation was started in, whose agent the session then asks to resume it. The field is not
marked invalid; it is described by the warning, which shows as the answer arrives without waiting for a submit. A
problem with the ID's own text, once a submit has tried it, takes its place. A lookup that fails — it could not be made,
so it says nothing about the ID — shows its failure in an alert directly under the field instead, with no fields after
it, and the submit button stays disabled. Each of these alerts — a session held elsewhere, a holder no longer offered
(below), a failed lookup — is titled that this conversation could not be resumed, with what was found or the failure
under the title. They are the lookup's outcome, not a verdict on what was typed: the field is not marked invalid, and
each is announced to assistive technology as it appears (see "Failures shown on the screen" in
`docs/product/error-alerts.md`). The locked Agent field holds that agent alone; its account half shows the account that
holds the conversation whenever it has a name to show, including when it is the agent's sole default account.

**Which records are searched.** Every agent that is available and added, in its default account and in every stored
account of it, each in the config directory a launch of that account would use: the account's own directory, or for the
default account the directory the agent defaults to under the user's login shell environment (see "Agent config
directories" in `docs/product/consoles.md`). When several accounts hold a record of the conversation, the one whose
record was written to last wins; for Grok Build that is judged by when its session directory last changed, so only
approximately. When the login shell environment cannot be captured, the default accounts are searched in the
directories the agents default to under the fallback environment a launch would use (see "The launch environment" in
`docs/product/launching-agents.md`), so the search and the launch that resumes the conversation agree.

If the agent or account holding the record is not offered, or stops being offered while the dialog is open — the agent
is uninstalled or no longer added, or the account is removed — the fields after the ID go, an alert under the field says
that the agent or account holding the conversation is no longer available, and the submit button is disabled; no other
agent or account is used in its place. An account holding the record that is offered but cannot open a session — turned
off, or having its sharing changed — keeps the fields, and its locked Agent field says why under it, marked invalid, as
the **Agent** field does (see "The session dialog" in `docs/product/opening-sessions.md`); the submit button is disabled
meanwhile. An account holding the record that was last read as not logged in, or whose quota is used up, is not
refused: the conversation opens on it, as in the **Agent** field, which for one read as not logged in shows the same
warning under the locked field.

**Naming a session outside the project.** The error for a conversation held outside the project names the session that
holds it by its displayed name (see "Displayed names" in `docs/product/session-titles.md`), and where it is: the project
it belongs to for another project's session, or the console for a console session. When the window does not know that
session, or its project or console, the error says only that the conversation belongs to a session outside this project.

## Reopening an archived session from the tab

**Reopen** reopens the archived session exactly as its **Reopen** action does (see "Archiving, interruption and
resuming" in `docs/product/archiving-and-resuming-sessions.md`): with its own agent, account, model and reasoning
effort, its terminal saying it is being resumed. The conversation is never opened in a second session. The session is
then selected and the dialog closes. There is no Advanced section, since the session keeps its own launch settings. The
**Name** field opens on the session's current name; the session is renamed once it is reopened only when the field
holds, after surrounding whitespace is removed, a name that is not empty and differs from it (see "Renaming a session"
in `docs/product/sessions.md`), and an emptied field keeps the session's name. A refusal of the reopen, such as a
conversation that can no longer be resumed, is shown in an error alert and the dialog stays open; a reopen that finds
the session already running counts as done. A rename that fails after the reopen is reported in an error alert, and the
session stays reopened.

## Resuming or going to a session of this project from the tab

A session of this project that is not archived keeps its own name, agent, account and launch settings: the **Name**
field and the agent and account are locked, there is no Advanced section, and no new session is opened.

- **An interrupted session** — **Resume** resumes that same session exactly as its terminal's "Resume" does (see
  "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`), then selects it and
  closes the dialog. A refusal of the resume is shown in an error alert and the dialog stays open; a resume that finds
  the session already running counts as done.
- **A session starting or whose process is running** — **Go to session** selects that session and closes the dialog.

## Opening a new session on a found conversation

Open starts a new project session that resumes the conversation, on the agent and account that hold its record, or,
for an ID found nowhere, on the agent and account picked, with the name typed and the model, reasoning effort and compatibility switches of the Advanced section, which are applied to the
launch that resumes the conversation (see "Where the switches are set in the window" in
`docs/product/compatibility-switches-in-launch-settings.md`). In the daemon protocol, `open_session` takes the ID as `resume_agent_session_id`, trimmed, blank read as
absent, together with the session's `agent` and `account`; the dialog sends the account it found or the one picked, `null` for
the default account.

- **A resumed session is a plain, independent project session.** `open_session` refuses a request that names both a
  conversation to resume and a session to report to (`conflicting_fields`).
- **The ID must be a UUID.** `open_session` takes any spelling of a UUID, trimmed, and keeps it lowercase and
  hyphenated; anything else is refused (`resume_id_invalid`).
- **A conversation already in Octoboard is refused.** No two sessions share one conversation: an ID that a session
  already records as its agent's conversation, live or archived, is refused. A session of the same project is named, and
  the user is told to resume or reopen that session instead (`conversation_in_session`); a session outside the project —
  of another project, or a console session — is refused with that session's id (`conversation_outside_project`, with
  `session`); the error alert says only that the conversation belongs to a session outside the project, and the lookup
  the dialog then asks again names that session (see "Finding the conversation" above). The check is made again once the
  new session is recorded, so two requests for the same conversation cannot both succeed.
- **A conversation not found in the session's account directory still opens.** It is looked for in the config
  directory of the account the session will use (see "Which account a session uses" in
  `docs/product/opening-sessions.md`), as in the search above. With no record of it there, the session opens all the
  same and is launched with the agent's own resume of that ID, and its terminal's banner carries the standing notice
  `conversation_record_not_found` (see `docs/product/standing-session-notices.md`), saying that the record was not found
  and the agent was asked to resume the conversation anyway, what the agent finds being shown in the terminal. For the
  default account the directory is resolved from the launch environment, a fallback one where the login shell
  environment could not be captured (see "The launch environment" in `docs/product/launching-agents.md`).
  `conversation_to_resume_not_found`, which refused such an open, is no longer sent; a window still words it for an
  older daemon.
- Every refusal comes before anything is recorded or launched: no session appears, and the reason is shown in an error
  alert, the dialog staying open with what was entered. `open_session` also refuses an ID for a console session
  (`resume_for_console_session`).
- **The refusals about the conversation** — an ID that is not a conversation ID (`resume_id_invalid`) and a
  conversation already in a session (`conversation_in_session`, `conversation_outside_project`) — are shown in an error
  alert like any other failure of the open; once it is acknowledged, keyboard focus is back on the Session ID field.
  Either means the answer the tab holds may be stale, so the lookup is asked again: while it runs the fields after the ID go and the spinner shows, and the new answer
  decides the fields and the submit button as in "Finding the conversation" above.

A session opened this way starts by resuming that conversation, and comes up at the agent's prompt like any session
opened from the dialog. It **has a conversation from the moment it is opened**, not from its first turn: it reads as *done* once up (see
"Done, awaiting instructions and child sessions working" in `docs/product/session-statuses.md`); interrupted
before the user has typed anything, it resumes that conversation rather than starting a fresh one; it is never unused,
so it is archived rather than deleted (see "Deleting an unused session" in `docs/product/deleting-sessions.md`);
switching its account copies that conversation; and its relaunch is refused when its pinned config directory has gone
(see "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`). Octoboard's status
hooks and tools are injected as on any resume, so its status is tracked and its tools are there; whether the role
description takes effect in it is in "What is injected on every launch" in `docs/product/launching-agents.md`.

## Locating a conversation in the daemon protocol

`locate_conversation_to_resume` takes `project` (a project id) and `agent_session_id`, and is answered with
`conversation_located`, whose `location` is tagged by `kind`:

| `kind` | Fields | Meaning |
|---|---|---|
| `archived_session` | `session` | An archived session of `project` records the conversation. |
| `live_session` | `session` | A session of `project` that is not archived records it. |
| `outside_project` | `session` | A session of another project, or a console session, records it, in any state; `session` is its id. |
| `record` | `agent`, `account` | No session records it; that agent's own records hold it in that account (an account id, or `null` for the agent's default account). |
| `not_found` | — | No session records it and no searched account holds it. |

The search is the one described in "Finding the conversation" above. An `agent_session_id` that is not a UUID is refused
with `resume_id_invalid` (any spelling of a UUID is taken, as `open_session` takes it), a `project` that does not exist
with `unknown_project`; a login shell environment that cannot be captured is no error, the fallback environment
standing in for it. The request only reads what is stored and on disk.
