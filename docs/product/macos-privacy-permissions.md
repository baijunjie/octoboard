# macOS privacy permissions

The two macOS privacy permissions the agents running in Octoboard's terminals can need: **Screen Recording**, which
agents use to take screenshots of what they built, and **Accessibility**, which lets them drive another application's
interface. macOS attributes such a request to the application responsible for the process that made it, which is
Octoboard rather than the agent, so the system's own prompt names Octoboard, appears in the middle of a task and says
nothing about why — and Octoboard cannot intercept a prompt one of its agents raised. Settings' **Permissions** section
(see "Permissions" in `docs/product/settings.md`) is therefore the place to grant them deliberately beforehand, and a
paragraph above the rows says as much — shown only together with the rows, so not outside the macOS application, not
when the read of the grants failed and not before the first read is in. A grant the user gave another application does
not count for Octoboard; macOS ties a grant to one application. These two are the only macOS permissions Octoboard ever
asks for.

One row per permission, naming it, what agents use it for and its current state. What the row offers follows that state:

| State | What the row shows |
|---|---|
| Granted | A large **Granted** badge in the success colour, with a circled check before its label, at the row's end in place of a control; the row's description says nothing more about the state. |
| Not granted | That it is not granted, with a **Grant…** button. Pressing it raises the system's permission prompt; this is the only thing in Octoboard that ever raises one. |
| A request made in this window came back without the grant | That it is not granted and where to switch it on, with an **Open System Settings** button in place of Grant…, which opens that permission's own pane in System Settings. |

The wording of that last state differs per permission: for Screen Recording it says macOS will not ask a second time,
which is what macOS does — a prompt the user refused is never raised again, and a later request is answered with a
silent no; for Accessibility it says to switch Octoboard on in System Settings, since that permission is never granted
in place at all.

**Accessibility is granted in System Settings, not from the prompt.** Its Grant… button raises the system's dialog
asking to let Octoboard control the computer, whose effect is that Octoboard appears in System Settings' own list, so
the user only flips a switch there instead of adding the application by hand. The request reports back at once, while
that dialog is still coming up, and normally reports the permission not granted, so the row moves to the System Settings
wording straight after the press. Screen Recording's Grant… button instead waits for the user to answer the prompt, and
is marked pending until they have.

**A change made outside Octoboard is picked up when the window comes back.** The grants are read again every time the
window regains focus, so one granted or revoked in System Settings — the only way Accessibility is ever granted — takes
effect in this section on the user's return. For assistive technology the section has one status region of its own, not
shown on screen, which says every row's state in one announcement, row by row: the permission's name, its state, and the
restart line (below) where that row shows it. It speaks about 1.5 seconds after the rows appear, when the user switches
to this section. After a change it speaks about 1.5 seconds after the user is back in the window: the wait counts only
time the window is visible and has focus, and starts over each time the user comes back. A reading that changes nothing
says nothing again, and a state replaced while it waits is never said. The rows' own states stay readable where they
stand but are not status regions themselves.

**Screen Recording reaches a process only at launch.** A grant made while Octoboard is already running does not reach
it, so a row that is granted but was not when this Octoboard started adds a line asking the user to restart Octoboard
for it to take effect. There is no restart button: the user quits Octoboard and opens it again, which goes through the
ordinary quit (see "Quitting" in `docs/product/application-lifecycle.md`).

**Nothing about these permissions is stored.** Every row's state comes from macOS alone. macOS cannot tell a permission
that was never asked for from one the user refused, so a permission that is not granted starts at not granted with the
Grant… button, and the System Settings wording appears only after a request made in this window has come back without
the grant. Switching to another Settings section and back puts the Grant… button back; pressing it brings the guidance
back with it — answered by macOS with a silent no for Screen Recording, and with that dialog again for Accessibility,
which macOS keeps no refusal for.

**Outside the macOS application** — in a browser, or a desktop shell not running on macOS — the section is a single line
saying that permissions can only be managed in the macOS application. Where a desktop shell is there but the read of the
grants fails, the section is instead a red alert titled that the permissions could not be read, saying that the system
did not report which have been granted, which opens no error alert (see "Failures shown on the screen" in
`docs/product/error-alerts.md`). Until the first read comes back the section shows neither line and nothing else either.
