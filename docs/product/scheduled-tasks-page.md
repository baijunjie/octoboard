# The Scheduled tasks page

Every window of a daemon — the main window, a remote window and a browser — has a **Scheduled tasks page**, where that
daemon's scheduled tasks are listed, created, changed, run, turned on and off, and deleted, and where each task's runs
are read. What a task is, how it runs and what saving one checks are in `docs/product/scheduled-tasks.md`. The page is a
page of the window as a console's sessions are, and takes their place while it shows, as the Remote devices page does
(see `docs/product/remote-devices-page.md`): the sidebar lists the tasks instead of a console, and the content area shows
the selected task instead of a session. The page is there only while the window's daemon can be used; while it cannot
(see "Without the local daemon" in `docs/product/remote-devices-page.md`), the window has no Scheduled tasks button and
no page.

## Opening and leaving the page

The page is opened by:

- the rail's **Scheduled tasks** button, a clock, directly above the Remote devices button where the window has one
  (see "The rail" in `docs/product/window-layout.md`). The button only opens the page: pressed while the page shows, it
  does nothing. While the page shows, the button is drawn with a filled circle behind its glyph and tells assistive
  technology it is the current item, and no console's avatar is marked current. **A green badge counts the tasks that
  are running** — those with a session one of their kept runs started that the window holds and that is starting or
  working (see "Run records" in `docs/product/scheduled-tasks.md`) — placed as the waiting count's and shown only while
  at least one is (99+ beyond 99). Its tooltip and name are Scheduled tasks, followed while any is running by how many
  are;
- choosing the notification of a failed run, which opens the page on that task (see "The notification of a failed run"
  in `docs/product/scheduled-tasks.md`).

**The page is left by going to another place of the window**, as the Remote devices page is (see "Opening and leaving
the page" in `docs/product/remote-devices-page.md`): a console's avatar on the rail, the current console's included,
anything that changes the current console or the selected session, Back and Forward to a place without the page, a
run's session chosen on the page, or the Remote devices button. One of the two pages shows at a time. `Escape` does not
leave it. What the window showed before is kept as it was while the page shows, and is there again once it is left. The
page goes away, whatever its form holds, when the daemon stops being usable — it turns out to be of another version, or
refuses the window's key.

**Leaving the page, or choosing another task, while the form holds changes not saved asks first** (see
`docs/product/leaving-a-form-with-unsaved-changes.md`).

**The page is part of the navigation history** (see `docs/product/navigation-history.md`): opening it is a visit, and so
is choosing another task on it. Opening a new task's form is not. A place naming a task that no longer exists is passed
over.

**Which task the page shows when it opens**: the one last chosen on it, remembered for the daemon across launches,
while it still exists, else the first.

## What the page shows

The top bar keeps **Back**, **Forward** and the sidebar toggle, which reads **Show scheduled tasks** or **Hide scheduled
tasks**. Its breadcrumb is **Scheduled tasks** followed by the shown task's name, or Scheduled tasks alone while a new
task is being made or there is none. There are no panel toggles and no right pane on the page.

Nothing but the frame is drawn, in the sidebar and in the content area, until the tasks have been read.

**Times are read in the daemon's time zone.** Every time on the page — schedules, next and last runs, the runs' times —
is shown in the time zone the daemon reads schedules in, and the form's times are entered in it. Where that zone is not
the window's own, the times are followed by the zone's short name, the header says which zone the times are in, and the
form's time fields say so under them; where the daemon names no zone, the window's own is used.

### The list of tasks

The sidebar lists the tasks, oldest first. It is the sessions' sidebar, docked, resized, hidden, floating and opened as
a drawer as that one is (see `docs/product/pane-docking.md` and `docs/product/sidebar-drawer.md`). To assistive
technology it is named Scheduled tasks; the dimmed area behind the drawer is named Close scheduled tasks.

- **Its header** reads **Scheduled tasks**, with a **+** button, New scheduled task. With no console it is unavailable,
  its reason saying to add a console first, since a task starts its session in one (see
  `docs/product/controls-unavailable-for-a-reason.md`).
- **One row for each task**: its name, then under it one quiet line with its schedule in a few words ("Daily 9:00 AM",
  "Mon, Wed, Fri 9:30 AM", "Monthly, day 15 9:00 AM", "Hourly :15", "Manual", or a one-off's short date and time),
  then the name of the console or project it starts a session in, and at the line's end how long until it next runs, in
  one unit ("in 3 days", "in 2 hr", "in < 1 min"), whose tooltip is the exact time. A disabled task's name is drawn
  muted and its line ends with **Off**; a task with no next run has nothing there. A part of the line too long for the
  row fades, with the whole line in its tooltip. To assistive technology the row is named by the task and described by
  its target, its schedule in words and its next run's time, or Off. The row of the task the content area shows is
  marked as the selected one, and choosing a row shows that task.
- **A row's ⋮ menu**, also opened by a right-click on the row, holds **Run now** (unavailable while that task's own Run
  now is being asked for), **Enable** or **Disable**, and, after a separator, **Delete**.
- **With no task**, one compact empty row says there are no scheduled tasks yet; pressing it starts New scheduled task,
  and with no console it is plain text (see "The compact row" in `docs/product/empty-states.md`).
- **When the tasks could not be read**, a compact failure row stands in the list's place, with its own **Try again**
  where trying again can help.

While the page shows, resting the mouse on a console's avatar does not float the sidebar in. In a window too narrow for
the sidebar to dock, choosing a task, or starting New scheduled task, closes the drawer, as choosing a session does.

### A saved task

The content area shows the selected task under a header, then two tabs, **Settings** and **Run history**.

**The header** holds:

- the task's name, as the page's heading, with an **Off** badge beside it while the task is disabled, and **Run now**,
  there whatever the schedule and whether the task is enabled, and showing that it is under way while its request awaits
  the daemon's answer;
- under the name, one quiet line: a glyph and the name of the console (a console task) or the project (a project task),
  then the schedule in words ("Every day at 9:00 AM", "Mon, Wed, and Fri at 9:30 AM", "On day 15 of every month at 9:00
  AM", "Every hour at minute 15", "Once, …", "Manual only");
- a card with two figures, side by side or stacked where the area is narrow: **Next run**, a countdown in up to two
  units ("in 2 hr 15 min") with the exact time under it, or **Off** for a disabled task, or **None** where there is no
  next run; and **Last run**, how long ago with a mark of what came of it (a check for a run that started a session, an
  alert for one that failed) and the exact time under it, or **Never**. The countdown and the time ago are hidden from
  assistive technology, which reads the times under them;
- where the daemon's zone differs from the window's, a line saying which zone the times are in.

**Settings** holds the form (see "The form" below) and under it, in a card of its own, the **Delete scheduled task** row
with **Delete**. **Run history** holds the runs (see "Run history" below). Switching between the tabs keeps what was
changed on the form and not saved; the tab chosen is kept as another task is chosen.

**Run now**, from the header or the row's menu, runs the task once (see "Running a task" in
`docs/product/scheduled-tasks.md`); a request the daemon refuses is an error alert titled that the task could not be
run. What the run then does is recorded in Run history, and only a failed run is announced, by its notification.
**Enable** and **Disable** turn the task on or off at once; a refusal is an error alert titled that the task could not be
enabled, or disabled.

**Delete**, from the Settings tab or the row's menu, asks first, in a destructive confirmation naming the task and saying
it will not run again and that the sessions it already started stay as they are; confirmed, it deletes the task and its
runs. The page then shows the first task, or, with none left, the empty state.

### Run history

The task's kept runs, most recent first, in a card, each with a mark of what came of it, when it ran and what started it
(**Scheduled**, **Catch-up** or **Run now**), then its outcome:

- **Started**, followed by the session it started as a link: choosing it leaves the page and selects that session, as
  choosing it in the sidebar would. A session the window no longer holds is said to no longer exist, with no link. A run
  that started independent says the session started without reporting to a console session, because the one it was to
  report to could not take it.
- **Could not start a session**, with a **Details** button whose popover holds the reason in the application's words,
  the one thing to do about it and what more there is, such as the raw text (see "Failures shown on the screen" in
  `docs/product/error-alerts.md`). No reason is written into the run's line itself.

With no runs, the card says there are none yet. Nothing is drawn until the runs have been read; when they cannot be, a
failure takes the card's place, with **Try again** where trying again can help.

### Adding a task

**New scheduled task** — the sidebar header's **+**, the empty row or the empty state's button — shows a blank form in
the content area, headed **New scheduled task**, with focus in its Name field, and no tabs. It starts on the current
console (else the first) as a console task, with an Every day schedule at 9:00, enabled; switching to Once starts the
date and time at the next full hour, to Every month starts on today's day, and to Every week on Monday. Its **Save**
creates the task and then shows it; **Cancel** returns to the task shown before, or to the empty state.

### With no task, and with no console

With no task, the content area shows only the empty state, centred (see "The full panel" in
`docs/product/empty-states.md`): a clock, the heading **No scheduled tasks yet**, as the page's heading, a line saying
that a scheduled task starts a session at the times chosen and hands it a prompt to work on, and **New scheduled task**
as its action.

With no console, the empty state is headed **No consoles yet**, with a line saying a scheduled task starts its session
in a console, and **New console** as its action, which opens the new-console dialog (see `docs/product/consoles.md`).

### When the tasks cannot be read

A listing that fails takes the content area as the failure of a whole subject, with **Try again** (see "Failures shown
on the screen" in `docs/product/error-alerts.md`), and the sidebar shows its failure row; no error alert opens. The tasks
are read again whenever the window's connection to the daemon is made again.

## The form

The form holds a task's fields, whose meaning and rules are in `docs/product/scheduled-tasks.md`, in two cards, with
**Save** and **Revert** (**Save** and **Cancel** for a new task) in a bar pinned along the bottom of the content area, so
they stay in view while the form scrolls.

- **Session**: **Console** and **Project**, side by side where the card is wide enough; Project lists **None (console
  session)**, which makes a console task, then the chosen console's projects; changing the console sets Project back to
  None (console session) and Report to back to none. Then **Name**, whose empty field previews the session's default name, saying it names the task and the sessions
  it starts; **Agent** with its account picker; **Report to**, for a project task only (see below); **Prompt**, required,
  with an example as its placeholder and saying it is handed to each session as its opening task; and the **Advanced**
  section of the session dialog, with its note on what is inherited (see "The session dialog" in
  `docs/product/opening-sessions.md`).
- **Schedule**: **Runs**, the kind of schedule, then the fields that kind needs, in two columns where the card is wide
  enough: **Date and time** for Once; **Minute past the hour** (0 to 59) for Every hour; **Time** for Every day,
  Weekdays, Every week and Every month; **Day of month** (1 to 31) for Every month, which for days 29 to 31 notes that a
  month without that day runs on its last day; and **Days**, the days of the week as checkboxes, Monday first, for Every
  week. The date and time fields are the application's own (see "Date and time fields" in
  `docs/product/report-page-components.md`). Then **Enabled**, a switch, saying the task runs on its schedule, or will
  once turned on.

**Report to** offers **No console session**, the default, then the console's console sessions that are not archived,
each with its colour. A saved choice that is no longer among them stays listed while it is the value, so an untouched form does not
change it; an archived one is marked Archived, since a run cannot report to it and starts independent. A choice the
window does not hold shows as No console session.

- **Save** is available whenever the form would save something other than what is saved — always, for a new task — and
  is disabled only then or while a save is under way. Only the parts changed are sent, so a change made elsewhere to
  another part stands. A save that fails is an error alert titled that the scheduled task could not be saved, and leaves
  the form as it was.
- **Revert** drops what was changed; **Cancel** drops the new task. Neither asks.
- **What is wrong with a field is said under it** once the field was edited and then left, and from a pressed Save on:
  a blank prompt, a missing date and time or time, a one-off's time not in the future (checked only while the schedule
  is being changed), no day chosen for Every week, and an agent's account that is turned off. Pressing Save while
  anything is wrong saves nothing and moves focus to the first such field. An account that is not logged in or has used
  up its quota is accepted, being decided when the task runs.
- **A part not changed follows the saved task** when that changes elsewhere — another window's edit, Enable or Disable
  from the row's menu, an account removed — and a changed part the saved task comes to match follows it again from then
  on. A part being edited keeps the user's value, and what is changed while a save is under way is kept as a change
  once the save lands; a save that fails leaves its values on the form as changes.
- **A task whose agent or account can no longer be picked** shows both pickers empty, saying the agent or account it
  uses is no longer available and to choose another, with the Advanced section unavailable; it is saved as it is until
  one is picked.

## Values a newer daemon sends

A schedule of a kind this window does not know is shown as no schedule: the row and the header show the target alone,
and the form's Runs select is empty, with no field after it; the schedule is left as it is unless another kind is
picked. A run's trigger of a kind the window does not know is left blank, and an outcome it does not know shows no mark
and no outcome (see `docs/product/values-a-newer-daemon-sends.md`).

## Keyboard and assistive technology

- **Opening the page moves keyboard focus into it**, once the tasks have been read or could not be: to a new task's
  Name field while one is being added, otherwise to the page's first control. It does not while a dialog is open, nor
  where focus is already in the page, nor while the narrow window's drawer is open over it, where focus moves in once
  the drawer closes. A page opened with the mouse shows no focus ring there.
- **The content area is a region of its own** in the `F6` cycle, right after the sidebar (see "Moving focus between
  regions with F6" in `docs/product/moving-focus-between-regions.md`), named by the page's heading: the task's name, New
  scheduled task, or the empty state's heading. Where the window would hand focus to the terminal, it hands it to the
  page.
- **Each button and menu is named with its task**: "Run now: Nightly backup check", "Delete Nightly backup check", and the
  row's ⋮ "Actions for scheduled task Nightly backup check".
- **Focus mode's shortcuts do nothing** while the page shows (see "The focus mode shortcuts" in
  `docs/product/focus-mode.md`).
