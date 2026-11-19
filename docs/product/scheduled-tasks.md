# Scheduled tasks

A **scheduled task** has Octoboard start a session at a set time, once or on a repeating schedule, handing it a prompt
to work on. The daemon owns the tasks: it stores them, works out when each next runs, starts the session when one comes
due, and keeps a record of each task's recent runs. Every window of a daemon lists and edits that daemon's tasks on its
Scheduled tasks page (see `docs/product/scheduled-tasks-page.md`); the wire contract is "Scheduled tasks" in
`apps/daemon/PROTOCOL.md`.

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
  is.
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
- the prompt is not blank (it is saved without the blanks around it, and so is the name);
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
