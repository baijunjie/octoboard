# Requests to resume a conversation

A session's `request_resume_session` call asks the user to resume a conversation by its id, with exactly what the
session dialog's **Resume conversation** tab gives the user and the same limits (see
`docs/product/resuming-a-conversation-by-its-session-id.md`): the session of the project that holds the conversation is
reopened or resumed, or, when no session holds it, a new session is opened on the agent's own record of it. Unlike
`reopen_session`, which brings back only the caller's own sessions and needs no approval, it can bring back any session
of the project, whoever owns it.

It is a session request (see `docs/product/session-requests.md`): nothing is resumed on the call alone, and it shares
with `request_session` the user's approval, automatic approval, the wait and its limit, the raised hand, the ways a
request ends, the rule of one request per session, and the outcome message written into the caller's session (see
"Approving requests automatically", "Waiting for the answer", "How a request ends" and "The outcome message" in
`docs/product/session-requests.md`). It is approved automatically under a setting of its own, **Automatically approve
resume requests**, independent of the one for `request_session` (see "Automatically approve resume requests" in
`docs/product/settings.md`). Where those sections say a request refused, timed out or withdrawn started nothing, for
this request it resumed nothing. What follows is what differs.

## Who is offered the tool

Every session that is offered `request_session` is offered `request_resume_session`: a console session, an unbound
project session, a lead session, and a project session bound to an owner. The console session's instruction file and
every project session's role description describe it: it resumes a conversation by its agent conversation id, bringing
back the session that holds it or opening a session that is nobody's on the agent's record of it; it is to be used only
when the user asked for that conversation to be resumed; it can resume any session, not only the caller's (where the
caller has `reopen_session`, that one is for its own and needs no approval); what it resumes is not the caller's to
drive; and the call waits for the user's answer. The tool's description adds that it may also be used when the agent
was told to carry on work whose conversation the user named, and that the agent asks the user first when their intent
is unclear. These are instructions for the agent; the user's approval is what Octoboard enforces.

## The call

| Argument | Required | Contents |
|---|---|---|
| `conversation` | yes | The agent's own conversation id — the session ID the agent shows, which its resume command takes — as a UUID in any spelling. An Octoboard session id is not one. |
| `project` | no | The project the conversation is resumed in, playing the part of the project whose session dialog the tab is in: by id in any console, or by a name that is unambiguous among the caller's own console's projects, else among every console's. Defaults to a project session's own project; a console session has to give it. |
| `title` | no | For a new session opened on an agent's record: its name, as the tab's **Name** field; left out, it is named after its project, as a session the user opens without a name. For an archived session that holds the conversation: the name it is given once it is reopened. Refused for an interrupted session, which keeps its name. Its description tells the agent to write it in the user's language (see "Naming the sessions a session starts" in `docs/product/hub-orchestration.md`). |
| `model`, `effort` | no | Only for a new session opened on an agent's record: its model and reasoning effort, as the tab's Advanced section sets them; blank is the agent's own default, and any other must be one the agent offers. A session that holds the conversation keeps its own. |

There is no argument for an agent or an account, which are those that hold the conversation, nor for an owner, branches
or a message to deliver: a session the call resumes is given no instruction, and the caller tells it what it needs
afterwards with `share_info` (see `docs/product/information-between-sessions.md`).

The conversation is located exactly as the tab locates it (see "Finding the conversation" in
`docs/product/resuming-a-conversation-by-its-session-id.md`): a session that records the id as its agent's conversation
decides on its own, whatever its project; only when no session records it are the records of every available and added
agent searched, in its default account and every stored account of it.

## What is refused before the user is asked

**A request that could never be carried out is refused before the user is asked**, with the reason in prose, no
dialog shown and nothing changed:

- a project that does not exist, a name that names more than one, or a console session that gives no project;
- a `conversation` left out, or one that is not a UUID;
- a conversation held by a session of another project, which the refusal names with where it is, telling the agent to
  ask again with `project` set to that session's project;
- a conversation held by a console session, which cannot be resumed this way;
- a conversation no session holds and no searched account has a record of;
- a conversation held by the caller itself;
- a conversation held by a session bound to the caller, which needs no approval: the refusal points to
  `reopen_session` while it is archived or interrupted, and to `send_message` while it runs;
- a conversation held by a session that is starting or whose process is running, since there is nothing to resume:
  the refusal names who the session reports to and says `share_info` tells it something;
- a holder that is being launched or resumed right now, or that is being switched to another account, handed off,
  rebound or having its branches handed over right now; each refusal says which, and to ask again in a moment;
- a `model` or `effort` that is not blank for a conversation a session holds;
- a `title` for an interrupted session;
- a held conversation its agent no longer has (`conversation_unresumable`), judged as a reopen judges it before
  relaunching anything (see "Archiving, interruption and resuming" in
  `docs/product/archiving-and-resuming-sessions.md`);
- for a conversation only an agent's record holds, whatever opening the session would meet in the session dialog: an
  agent determined unavailable on this machine or not added, an account that is turned off, whose quota is used up,
  that is not logged in or whose sharing is being changed, a model or effort the agent does not offer (see "When the agent or the account cannot
  open a session" in `docs/product/opening-sessions.md`).

A `title` that is an archived session's current name asks for no rename.

## On approval

Once the user approves — or the request is approved automatically under the user's setting to approve resume requests
without asking (see "Approving requests automatically" in `docs/product/session-requests.md`) — what is done depends on
where the conversation is held, judged again at that moment:

- **An archived session holds it: it is reopened** (`reopened`), exactly as the user's own **Reopen** does (see
  "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`), with its own agent,
  account, model and reasoning effort, and no instruction, so it comes up at its agent's prompt. Its archived owners
  are reopened first, topmost down. It **keeps its owner**, and everything bound to it stays as it is. Where the request
  carried a `title` that differs from the session's name by then, the session is renamed once it is back, as
  `rename_session` renames (see "Renaming a session" in `docs/product/sessions.md`); a rename that fails does not undo
  the reopen, and the result and the outcome say it could not be renamed and why.
- **An interrupted session holds it: it is resumed** (`resumed`) the same way, keeping its name and its owner. Only a
  session the approval reopens from the archive is renamed: a `title` carried for a session that is no longer archived
  by the approval is not given, which the result reports as `renamed: false`.
- **Only an agent's record holds it: a new session is opened on it** (`opened`), as the tab's **Open** opens one (see
  "Opening a new session on a found conversation" in `docs/product/resuming-a-conversation-by-its-session-id.md`): a
  plain project session in the project, on the agent and account that hold the record, named by `title` or after its
  project, with the model and reasoning effort asked for. It is bound to nobody and owned by nobody but the user, as a
  session the user opened from the tab, and it is given no prompt: it comes up at the agent's prompt, continuing the
  conversation.
- **The session that holds it is already running by then** (`already_running`): nothing is done. This holds only where
  the session is no longer dormant, or is being launched or resumed itself; one that is held instead for an account
  switch, a handoff, a handover of branches or a binding change while still dormant fails the approval (see "When an
  approval cannot be carried out" below).

**The caller gains nothing over what it resumed.** It is never made an owner and keeps running unchanged; what was
reopened or resumed reports to whom it reported before, and a session opened on a record reports to the user. As with
`request_session`, no window follows the session: approving selects nothing and changes no window's view or focus
mode.

The call returns:

| Field | Contents |
|---|---|
| `action` | `reopened`, `resumed`, `opened` or `already_running`, as above. |
| `session` | The session reopened, resumed, opened or found running. |
| `role`, `agent`, `title`, `console`, `project` | That session's, as they stand after the approval. |
| `owner` | The session it reports to, or null when it reports to the user. |
| `reopened_owners` | The ids of the archived owners the approval reopened with it; empty for none. |
| `renamed` | Present only when the request asked for a rename that was still needed: true when it was given, false when it could not be. |
| `note` | Prose telling the caller who approved — the user, or Octoboard under the user's setting to approve resume requests without asking — and what was done to which session (its kind, id, title, agent and where it runs); the archived owners reopened with it, named; a rename that failed, with the reason; whom it reports to, not the caller; that it was given no instruction and waits at its prompt, except where it was already running; that it is not the caller's to drive, and the caller should follow the user's direction on continuing its own work or ending; that `share_info` tells it something, and, where the caller has `get_session`, that it reads it; and, where the caller could bind itself to it, that `bind_to_session` would do so, as for `request_session` (see "On approval" in `docs/product/session-requests.md`). |

The outcome message written into the caller's session carries the same note and names `request_resume_session` as the
call it is about.

## When an approval cannot be carried out

As for `request_session`, an approval that cannot be carried out returns a tool error saying the request was approved
but could not be carried out, with the reason (see "When an approval cannot be carried out" in
`docs/product/session-requests.md`). For this request it is for one of these reasons:

- the caller's process ended before the approval was carried out;
- the reopen or resume was refused at that moment — the conversation found gone, an archived owner that could not be
  reopened, the session held for an account switch, a handoff, a handover of branches or a binding change, or any other
  refusal the user's own reopen would meet; what a reopen that fails partway leaves behind is as for any reopen (see
  "Archiving, interruption and resuming" in `docs/product/archiving-and-resuming-sessions.md`);
- the session could not be opened on the record — such as a conversation a session came to hold meanwhile, or any
  refusal opening it meets at that moment.

The window that approved shows the reason in an error alert about the session the error concerns: for a reopen or a
resume, the session the error names — the target, or an archived owner of it — and otherwise the target; for a request
that was no longer waiting, and for a session opened on a record, the requesting session. An automatic approval shows
no alert.

## The confirmation dialog

A plain confirmation with no editable field, which shows and behaves as every session request's dialog does (see "The
confirmation dialog" in `docs/product/session-requests.md`), with a title and a confirm button for what an approval
would do, and **Refuse** beside it:

| The conversation is held by | Title | Confirm button |
|---|---|---|
| An archived session | "Reopen a session?" | **Reopen session** |
| An interrupted session | "Resume a session?" | **Resume session** |
| Only an agent's record | "Resume a conversation?" | **Resume conversation** |

**One sentence says who asks and for what.** The requesting session is named as in a request to start a session, its
place given only where it is not the project of the session in question.

- **To reopen or resume a session**, the sentence names that session by its displayed name (see "Displayed names" in
  `docs/product/session-titles.md`), its agent, its account, its project and that project's console, and says whom it
  reports to: the user, or its owner, named. Where archived owners would be reopened with it, a following sentence names
  them, as they stood when the request was made.
- **To resume a conversation from an agent's record**, the sentence names the conversation by its id, the agent and the
  account that hold the record (the default account by the name the account pickers give it), and the project and its
  console, and says the new session would report to the user.

A session, project or console the window does not know is named by its id, and the account of a session the window
does not know is shown as a dash. **Nothing the agent wrote is shown**: a name the request would give an archived
session is not shown either.

**A warning stands in for the second confirmation.** Where the session would run on an account whose last change of
sharing is incomplete and no change of it is running, an amber warning mark follows the account in the sentence, named
that sharing may be incomplete; its popover says that the account's last sharing change did not finish, so
its configuration and conversations may be incomplete (see "Failures, warnings and notes marked at their point" in
`docs/product/error-alerts.md`). Approving is not asked about again (see "Starting a session on an account whose last
change is incomplete" in `docs/product/sharing-the-default-configuration.md`). A session the window does not know has
no account to check, so it has no mark. When the mark goes while it or its popover holds keyboard focus, focus moves to
the dialog. An automatic approval warns nothing.
