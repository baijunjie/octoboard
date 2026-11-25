# Scheduled tasks

A **scheduled task** has Octoboard start a session at a set time, once or on a repeating schedule, handing it a prompt
to work on. The daemon owns the tasks: it stores them, works out when each next runs, starts the session when one comes
due, and keeps a record of each task's recent runs. Every window of a daemon lists and edits that daemon's tasks on its
Scheduled tasks page (see `docs/product/scheduled-tasks-page.md`), and console sessions and project coordinators read
and, with the user's approval, change them through Octoboard's tools (see "The scheduled task tools" below); the wire
contract is "Scheduled tasks" in `apps/daemon/PROTOCOL.md`.

## What a task holds

- **What it starts**: a console session in a console, or a project session in a project of that console.
- **The launch settings**, those of the session dialog (see "The session dialog" in `docs/product/opening-sessions.md`):
  the session's name, agent, account, model, reasoning effort and compatibility switches. A setting the task leaves
  open is decided at each run as it is for a session opened with defaults: no name gives the session its default name,
  and an agent, account, model or effort the task does not name comes from the project, else the console, at the time
  of the run (see "Which agent a session uses", "Which account a session uses" and "Which model and reasoning effort a
  session uses" in `docs/product/opening-sessions.md`).
- **Report to**, for a project task only: a console session of the task's console that each session it starts reports
  to, as a session opened with a console session as its Report to choice does (see "Coordinators with a parent session"
  in `docs/product/hub-orchestration.md`). None by default.
- **The prompt**, required: what each session it starts is handed as its opening task, so the session comes up working
  on it.
- **The schedule** (below).
- **Whether it is enabled**. A new task is enabled unless it is created turned off.

A task is called by its name, else by the first line of its prompt.

## Schedules

| Schedule | Runs |
|---|---|
| Once | at one date and time |
| Every hour | every hour at a chosen minute past it (0 to 59), in every real hour across a daylight saving change |
| Every day | every day at a chosen time |
| Weekdays | Monday to Friday at a chosen time |
| Every week | on each of the chosen days of the week (at least one) at a chosen time |
| Every month | on one day of the month (1 to 31) at a chosen time; a month without that day runs on its last day, so day 31 is the last day of every month |
| Manual only | never on its own, only by Run now |

There are no cron expressions.

**Schedules are read in the local time zone of the machine the daemon runs on**, not the window's: a remote daemon's
tasks run by that machine's clock. The page shows and enters times in that zone and names it where the window's own
zone differs (see "What the page shows" in `docs/product/scheduled-tasks-page.md`). A wall-clock time a
daylight saving change skips runs as far past the change as it was past the hour before it (02:30 on a night that jumps
from 02:00 to 03:00 runs at 03:30); one the change repeats runs at its first occurrence.

**Known limit**: when the machine's time zone changes, the next run already worked out for each task is not worked out
again, so the first run after the change comes at the old zone's time; runs after it follow the new zone.

## Running a task

**A run starts the task's session as the session dialog would open it**, in the task's console or project with its
launch settings, and goes through the same launch rules, refusals and failures (see `docs/product/opening-sessions.md`).
The session is the user's own, as one the user opens is. A run's refusal or launch failure is recorded on the task as a
failed run (below) and is never an error alert in a window.

- **Due runs.** The daemon starts each enabled task's session when it comes due, and works out the task's next run at
  once; a one-off has none after it has run.
- **Missed runs.** At the daemon's start, each enabled task that came due while the daemon was not running is caught up
  once: its most recent missed occurrence runs, as a catch-up, if it is no more than **24 hours** old, and nothing runs
  for it otherwise. Several missed occurrences never make more than one catch-up run. An occurrence the running daemon
  reaches more than two minutes late, as after the machine slept, is caught up the same way. A scheduled or catch-up run
  that the daemon's own stop refuses is not recorded, and the next daemon catches it up.
- **Run now** runs a task once at once, whatever its schedule and whether it is enabled, and leaves its schedule as it
  is. A session's `run_scheduled_task` is a Run now too (see "Changing tasks from a session" below).
- **A disabled task** does not run on its schedule and is not caught up; it has no next run. Turning it on works out
  its next run from that moment, so occurrences passed while it was off are not run.
- **A one-off that has run** stays listed, with no next run, until it is deleted; it can still be run with Run now, and
  edited.
- **Overlapping runs are not skipped**: a run that comes due while the session an earlier run started is still at work
  starts another session.
- **The account at run time.** An account that is turned off, has used up its quota or is not logged in when the task
  runs — whether the task names it or inherits it — gives way, while **Automatically select an account** is on, to the
  account a new session that names none would get (see "Automatically select an account" in
  `docs/product/opening-sessions.md`). With the switch off, or when that account is not usable either, the run fails
  with the account's refusal rather than starting a session that cannot work.
- **A task stored with a prompt that saving now refuses** — nothing but whitespace once its control and invisible
  characters are ignored, saved before that was checked — starts no session: the run fails for the missing prompt, as
  saving such a prompt is refused (see "Saving a task" below).
- **A Report to session that cannot take a session at that moment** — archived, deleted or still starting — does not
  fail the run: the session starts independent, reporting to no one, and the run's record says so.
- **While the daemon is stopping**, nothing is started (see "Quitting" in `docs/product/application-lifecycle.md`): a
  Run now asked for then is refused, and one the stop overtakes once it was accepted is recorded as a failed run.

## Run records

Each run is recorded with when it ran, what started it — its **schedule**, a **catch-up** or **Run now** — and what came
of it:

- **started**, with the session it started (which may since have been archived or deleted), and whether it started
  independent because its Report to session could not take it;
- **failed**, with the reason, as an error alert would word the same refusal or failure.

**The last 50 runs of each task are kept**; older ones are dropped, and deleting the task deletes its runs. A task is
**running** while a session one of its kept runs started is starting or working; the rail's Scheduled tasks button counts
such tasks (see "Opening and leaving the page" in `docs/product/scheduled-tasks-page.md`).

## The notification of a failed run

**A run that fails raises a system notification**, titled that a scheduled task could not start a session, naming the
task and the reason in the application's words. A run that starts its session raises none: the session shows in the
sidebar as any other. Each open window of the daemon raises its own, under the system's
notification settings and, in a browser, only once the browser allows notifications (see
`docs/product/notification-settings.md`); a notification that cannot be shown is not reported.

**Choosing it brings forward the window that raised it and opens its Scheduled tasks page on that task**, asking first
where a page form holds changes not saved (see `docs/product/leaving-a-form-with-unsaved-changes.md`). Where that window
has closed since, the main window comes forward and nothing is opened. When and where a click on a notification is seen
is the same as for the raised hand (see "The raised hand" in `docs/product/session-statuses.md`).

## Saving a task

Creating or changing a task checks it as a whole, and a task that fails a check is not saved:

- the console exists, and the project, where there is one, belongs to it;
- the prompt is not blank: one that is nothing but whitespace once its control and invisible characters are ignored is
  refused as a missing prompt, since a launch would hand the session no task (see "How each agent is given the
  injection" in `docs/product/per-agent-launch-specifics.md`); it is saved without the blanks around it, and so is the
  name;
- an Every week schedule has at least one day; hours, minutes and days of the month are in range;
- a one-off's time is in the future — when the task is created, and when an edit changes its schedule, so a one-off
  that has run can still be edited otherwise;
- Report to, where given, names a console session of the task's console that the daemon holds, whatever its status:
  whether it can take a session is decided when the task runs;
- the launch settings are checked as opening a session with them would check them, except that an account that has used
  up its quota or is not logged in is accepted (decided when the task runs); a turned-off or missing account is
  refused.

An edit changes only what it names, so two windows editing different parts of one task both land, and it never turns
the task on or off, which is a change of its own. An edit that leaves the schedule as it is keeps the next run already
worked out; one that changes it works the next run out again from that moment.

## When what a task names goes away

- **Deleting a console** deletes its tasks and their runs (see "Deleting a console" in `docs/product/consoles.md`), and
  **removing a project** deletes the project's tasks and their runs (see `docs/product/removing-projects.md`).
- **Removing an account** clears it from every task that named it; those tasks inherit their target's account again
  (see `docs/product/removing-accounts.md`).
- **Deleting a task** stops it running; the sessions it already started stay as they are.

Tasks and their runs are kept in the daemon's database (see `docs/product/files-octoboard-owns.md`) and are not
configuration: a configuration document does not carry them to another machine (see "What is not configuration and
never travels" in `docs/product/configuration-export-and-import.md`).

## The scheduled task tools

Seven of Octoboard's tools (see `docs/product/hub-orchestration.md`) read and change scheduled tasks, named as the
control requests they run: the reads `list_scheduled_tasks` and `list_scheduled_task_runs`, and the writes
`create_scheduled_task`, `update_scheduled_task`, `set_scheduled_task_enabled`, `delete_scheduled_task` and
`run_scheduled_task`. A write is checked and refused exactly as the same change made on the Scheduled tasks page is
(see "Saving a task" above), and reaches every open window the same way.

**Who is offered them, and which tasks each reaches:**

- **A console session** reaches the tasks of its own console: console tasks, and the tasks of that console's projects.
- **A project coordinator**, independent or reporting to a console session, reaches only the tasks whose target is its
  own project. It cannot create a console task, name another project, or move a task out of its project.
- **A worker** is offered none of them.

A task outside the caller's reach is refused as one that is not among its tasks (this console's, or this project's),
whether or not it exists, pointing to `list_scheduled_tasks`. A `project` argument names a project of the caller's
console by its id, or by its name where that is unambiguous in the console.

**Tasks belong to the user.** Each tool's description, a coordinator's role description and the console session's
instruction file tell the agent so: it creates, changes, turns on or off, deletes or runs a task only when the user
asked it to. They also tell it that the sessions a task starts are the user's own — they report to the user, and the
caller can only share information with them (see `docs/product/information-between-sessions.md`) — unless the task's
`launch.bound_to` names a console session, which then owns them; and that the writes ask the user first, unless the
user has chosen to have scheduled task changes approved without asking. A worker's role description names none of the
tools.

## Reading tasks from a session

`list_scheduled_tasks` takes no arguments. It answers with `tasks`, the tasks within the caller's reach, oldest first,
and `time_zone`, the IANA name of the zone schedules are read in (the daemon machine's, see "Schedules" above), or
`null` where that zone has no IANA name. Each task is shown as:

| Field | Contents |
|---|---|
| `id` | The task's id, which the other tools take as `task`. |
| `console` | Its console. |
| `project` | Its project, or `null` for a console task. |
| `launch` | Its launch settings as stored, `{title?, agent?, account?, model?, effort?, compatibility, bound_to?}`: `title` is the task's name, `account` an account's id, and `compatibility` the switches it holds, by setting id. A setting left out follows the project's, else the console's, at each run (see "What a task holds" above); `null` for `account`, `model` or `effort` is the agent's default account, its default model and its own reasoning effort. |
| `prompt` | The prompt each session it starts is handed. |
| `schedule` | Its schedule, in the shape `create_scheduled_task` takes (below). |
| `enabled` | Whether it is on. |
| `next_run_at` | When it next runs on its schedule, in epoch milliseconds; `null` for a manual-only task, a one-off whose time has passed, or a task that is off. |
| `last_run_at` | When it last ran, `null` until it first runs. |
| `created_at`, `updated_at` | When it was created and last changed, in epoch milliseconds. |
| `run_sessions` | The sessions its kept runs started, most recent run first, each once, leaving out archived sessions and those the daemon no longer holds. |

`list_scheduled_task_runs` takes `task` and answers with `task` and `runs`, the task's kept runs (see "Run records"
above), most recent first, each with its `id`, `task_id`, `ran_at` (epoch milliseconds), `due_at` (the occurrence of
the schedule it stands for; `null` for a Run now), `trigger` (`scheduled`, `catch_up` or `run_now`) and `outcome`:
`{kind: "started", session, started_independent}` for a run that started the session `session`, where
`started_independent` is true when the session started reporting to no one because its Report to session could not
take it, or `{kind: "failed", code, params, message}` for a run that started none.

Neither read asks the user.

## Changing tasks from a session

| Tool | Arguments | What it does |
|---|---|---|
| `create_scheduled_task` | `prompt`, `schedule`, `launch?`, `enabled?`, `project?` | Creates a task. `prompt` is required and not blank; `enabled` is true when left out, and a task created off has no next run until it is turned on. For a console session, `project` names one of its console's projects for a project task; left out or `null`, the task is a console task. A coordinator's tool has no `project`: the task is always of its own project, and a `project` naming another, or `null`, is refused. |
| `update_scheduled_task` | `task`, `project?`, `launch?`, `prompt?`, `schedule?` | Changes a task. At least one of the four is required, and one left out leaves that part as it is. `launch` replaces the task's launch settings whole, so a setting it leaves out goes back to following the project or console; `prompt` and `schedule` replace theirs. `project`, for a console session only: left out, the task stays where it is; `null` makes it a console task; a project moves it to that project of the console. A coordinator's tool has no `project`: its task stays of its own project, and a `project` naming another, or `null`, is refused. The task as changed is checked whole, not only its changed parts. It never turns the task on or off. |
| `set_scheduled_task_enabled` | `task`, `enabled` | Turns a task on or off (see "Running a task" above). Turning on a task that is already on leaves its next run as it is. |
| `delete_scheduled_task` | `task` | Deletes a task and its runs; the sessions it already started stay as they are. |
| `run_scheduled_task` | `task` | Runs a task once now, as Run now does, whatever its schedule and whether it is on, leaving its schedule as it is. The call returns once the run has been started; what came of it is the run's record, in `list_scheduled_task_runs`, once the launch has settled. |

`launch` takes the launch settings of the sessions the task starts, as `open_session` takes them (see "The console
session's tools" in `docs/product/hub-orchestration.md`), each checked as saving a task checks it:

| Field | Contents |
|---|---|
| `title` | The task's name, which is also the name of each session it starts (see "What a task holds" above). Left out or blank, the task has none. The description tells the agent to write it in the user's language (see "Naming the sessions a session starts" in `docs/product/hub-orchestration.md`). |
| `agent` | The agent of the sessions it starts; left out or `null`, the project's, else the console's, at each run. |
| `account` | An account's id or name among the accounts of `agent`; with no `agent`, among every account, where a name that fits accounts of several agents is refused. `default` or `null` is the agent's default account. Left out, the project's, else the console's, at each run. |
| `model`, `effort` | A model or reasoning effort the agent offers (`list_agent_models` lists them); blank or `null` is the default model, or the agent's own effort. Left out, the project's, else the console's, at each run. |
| `compatibility` | The compatibility switches the task holds, by setting id, each `true` or `false`; a switch left out follows the project, else the console, else the application-wide value. |
| `bound_to` | For a project task, the id of a console session of the task's console, its Report to; left out, the sessions report to the user. Not kept for a console task. |

`schedule` is an object whose `kind` says which schedule it is (see "Schedules" above), with the fields that kind
needs: `once` with `run_at`, an instant in epoch milliseconds; `hourly` with `minute`, 0 to 59; `daily` and `weekdays`
with `time`; `weekly` with `days`, a non-empty list of `monday` … `sunday`, and `time`; `monthly` with `day`, 1 to 31,
and `time`; `manual` with nothing else. `time` is `{hour, minute}`, an hour of 0 to 23 and a minute of 0 to 59. For
example:

```json
{
  "prompt": "Check last night's failed builds and summarize what broke.",
  "schedule": { "kind": "weekdays", "time": { "hour": 9, "minute": 0 } },
  "launch": { "title": "Morning build check", "agent": "codex" }
}
```

`create_scheduled_task`, `update_scheduled_task` and `set_scheduled_task_enabled` answer with `task`, the task as
stored, in the shape `list_scheduled_tasks` gives, and a `note` saying what was done and whether the task runs on its
schedule: off, so not until it is turned on; on, next running at `next_run_at`; or on with no next run, its schedule
being manual or its one-off time past, so only when run by hand. `delete_scheduled_task` answers with `deleted`, the
task's id, and a `note`; `run_scheduled_task` with `task`, the task's id, and a `note` saying the run was started in
the background and where to find what came of it.

## The user's approval of a session's task change

**Every write is a session request** (see `docs/product/session-requests.md`), unless the user has turned on
**Automatically approve scheduled task changes** (see that section in `docs/product/agent-behavior-settings.md`): the
change is made only once the user approves it. The call waits for the user's answer for up to 8 minutes, with the
caller's hand raised meanwhile, and ends as any request ends — approved, refused, timed out, or withdrawn, the agent
cancelling the call included — with an approval's outcome also written into the caller's session (see "Waiting for the
answer", "How a request ends" and "The outcome message" in `docs/product/session-requests.md`).

**What could never be carried out is refused before the user is asked**, with the reason in prose and no dialog shown:
a task or project that is unknown or outside the caller's reach, a blank prompt, a schedule that is not one or is out of
range, a one-off time that is not in the future, launch settings that saving the task would refuse, a Report to that is
not a console session of the task's console, an update that names nothing to change, and a run asked for while the
daemon is stopping.

**The change is checked again when the approval is carried out**, the caller's reach included, as the task, its target
and the caller then stand: a task deleted or moved out of the caller's reach meanwhile, a project removed, a one-off time
gone by or an account turned off since the request was made fails the approval, and nothing changes (see "When an
approval cannot be carried out" in `docs/product/session-requests.md`). Once approved, the change is carried out
exactly as the call asked; the user cannot change it in the dialog.

**One request per tool and task.** An update, enable or disable, delete or run request takes the place only of the
caller's waiting request of the same tool for the same task, which is withdrawn; requests about other tasks, or of
another tool, wait beside it. A create names no task, so it takes the place of none: several creates wait side by side
and are answered one by one. A task request never takes the place of the caller's request for a session, nor the other
way round.

**Approving selects nothing.** The task is created, changed, turned on or off, deleted or run — a run starting its
session as Run now does — and no window's view or focus changes.

**With Automatically approve scheduled task changes on, a write is not a request at all**: the change is made at once,
with no dialog, no raised hand and no wait, what it refuses is refused at that moment, and the result is the tool's own.
The setting is read once, when the call is made, and the caller's waiting request of the same tool for the same task is
withdrawn first (see "Approving requests automatically" in `docs/product/session-requests.md`).

## The confirmation of a task change

A plain confirmation, with **Refuse** and a confirm button for what an approval does, and nothing to change:

| The call | Title | Confirm button |
|---|---|---|
| `create_scheduled_task` | "Create a scheduled task?" | **Create task** |
| `update_scheduled_task` | "Change a scheduled task?" | **Change task** |
| `set_scheduled_task_enabled`, turning it on | "Enable a scheduled task?" | **Enable task** |
| `set_scheduled_task_enabled`, turning it off | "Disable a scheduled task?" | **Disable task** |
| `delete_scheduled_task` | "Delete a scheduled task?" | **Delete task**, styled as a destructive action |
| `run_scheduled_task` | "Run a scheduled task?" | **Run now** |

- **One sentence says who asks and for what.** It names the requesting session as a request for a session names it
  (see "The confirmation dialog" in `docs/product/session-requests.md`), what it asks to do, the task, and where the
  task starts sessions — a session in a project of a console, or a console session in a console — after the change. The
  task is named as the Scheduled tasks page names it, by its name, else by the first line of its prompt: a task to
  create by what it would be saved with, any other by the window's list of tasks, and by its id, as code, where the
  window does not hold that task. A project or console the window does not know is named by its id.
- **A delete** adds that the task will not run again and that the sessions it already started stay as they are.
- **An update** adds which parts of the task it changes: the prompt, the schedule, the launch settings, and where it
  starts sessions — the last only where the window holds the task to compare with.
- **A list of the new values** follows for a create, and for an update for what it changes: **Name**, for an update
  whose launch settings give the task a name; **Runs**, the schedule as the Scheduled tasks page words it, in the zone
  schedules are read in; **Enabled**, On or Off, for a create only; and the launch settings the task names — **Agent**,
  **Account** (the default account by the name the account pickers give it where the task names the agent, else
  "Default"), **Model**, **Reasoning effort** (an explicit default shown as the default), **Report to** (a project task
  only, by the session's displayed name), and each compatibility switch the task holds, named as the session dialog
  names it and valued On or Off, in the order the agent declares its switches, then any it does not declare, by id. A
  setting the task leaves open is not listed. The launch settings are shown as the task would be saved with them,
  after the daemon's checks. Every value is plain text; one the window cannot name is shown by its id, as code.
- **The prompt the session wrote**, for a create and for an update that changes it, is in a disclosure folded away
  under the list, titled "Prompt written by the session" (see "The confirmation dialog" in
  `docs/product/session-requests.md`). A request to turn a task on or off, delete it or run it carries no text of the
  agent's and has no disclosure.
- The text wraps anywhere, so a long name or id does not widen the dialog.

**A task request the window cannot describe is not offered** — one lacking the task it acts on or what it creates or
changes, or carrying a schedule of a kind this version of the application does not know — as any request the window
cannot describe (see "The confirmation dialog" in `docs/product/session-requests.md`).
