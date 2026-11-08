# macOS privacy permissions

The two macOS privacy permissions the agents running in Octoboard's terminals can need: **Screen Recording**, which
agents use to take screenshots of what they built, and **Accessibility**, which lets them drive another application's
interface. macOS attributes such a request to the application responsible for the process that made it, which is
Octoboard rather than the agent, so the system's own prompt names Octoboard, appears in the middle of a task and says
nothing about why — and Octoboard cannot intercept a prompt one of its agents raised. Settings' **Permissions** section
(see "Permissions" in `docs/product/settings.md`) is therefore the place to grant them deliberately beforehand, and a
paragraph above the rows says as much — shown only together with the rows, so not outside the macOS application, not
when the read of the grants failed and not before the first read is in. A grant the user gave another application does
not count for Octoboard; macOS ties a grant to one application. These two are the only macOS permissions Octoboard
itself asks for; the other consents a program in its terminals can raise are described in "Other consents programs
ask for" below.

One row per permission, naming it, what agents use it for and its current state. What the row offers follows that state:

| State | What the row shows |
|---|---|
| Granted | A large **Granted** badge in the success colour, with a circled check before its label, at the row's end in place of a control; the row's description says nothing more about the state. |
| Not granted | That it is not granted, with a **Grant** button. Pressing it raises the system's permission prompt; this is the only thing in Octoboard that ever raises one. |
| A request made in this window came back without the grant | That it is not granted and where to switch it on, with an **Open System Settings** button in place of the Grant button, which opens that permission's own pane in System Settings. |

The wording of that last state differs per permission: for Screen Recording it says macOS will not ask a second time,
which is what macOS does — a prompt the user refused is never raised again, and a later request is answered with a
silent no; for Accessibility it says to switch Octoboard on in System Settings, since that permission is never granted
in place at all.

When the system does not open the pane, an error alert titled "Could not open System Settings" says that System
Settings did not open and to open it by hand, choose Privacy & Security, then choose the permission's pane, named as
macOS 15 and later shows it: Screen & System Audio Recording for Screen Recording, Accessibility for Accessibility. It
has only its acknowledgement button, with no remedy button and no request to report an issue (see "The error alert" in
`docs/product/error-alerts.md`). Whether or not the pane opened, the row's grants are read again afterwards.

When pressing **Grant** fails to make the request at all, an error alert titled "Could not request the permission"
opens. It is an unexpected failure, not a refusal: its body says the request for that permission did not go through and
to grant it by hand in System Settings — choose Privacy & Security, then the permission's pane as named above, adding
Octoboard with the + button if it is not listed — followed by the request to report an issue, with **Report issue** and
what the failed request reported under **Error details** (see "Recovery and unexpected errors" in
`docs/product/error-alerts.md`). The button stops being pending and the row's grants are read again; since no refusal
was recorded, the row keeps offering the Grant button rather than the System Settings wording. A request macOS answers,
granted or not, opens no alert.

**Accessibility is granted in System Settings, not from the prompt.** Its Grant button raises the system's dialog
asking to let Octoboard control the computer, whose effect is that Octoboard appears in System Settings' own list, so
the user only flips a switch there instead of adding the application by hand. The request reports back at once, while
that dialog is still coming up, and normally reports the permission not granted, so the row moves to the System Settings
wording straight after the press. Screen Recording's Grant button instead waits for the user to answer the prompt, and
is marked pending until they have.

**A change made outside Octoboard is picked up when the window comes back.** The grants are read again every time the
window regains focus, so one granted or revoked in System Settings — the only way Accessibility is ever granted — takes
effect in this section on the user's return. For assistive technology the section has one status region of its own, not
shown on screen, which says where both permissions stand in one announcement, one message of whole sentences per
permission naming it and its state; for a permission whose request in this window came back without the grant, that
message carries the same guidance the row gives on switching it on in System Settings. It speaks about 1.5 seconds
after the rows appear, when the user switches to this section. After a change it speaks about 1.5 seconds after the user
is back in the window: the wait counts only time the window is visible and has focus, and starts over each time the user
comes back. A reading that changes nothing says nothing again, and a state replaced while it waits is never said. The
rows' own states stay readable where they stand but are not status regions themselves.

**A Screen Recording grant needs no restart of Octoboard.** Octoboard itself captures nothing; the capture commands
agents start do, and one started after the grant captures the screen straight away, also in a session opened before the
grant. After a grant made in System Settings while Octoboard is running, macOS offers to quit and reopen Octoboard; the
user can put that off, and the section asks for no restart either.

**Nothing about these permissions is stored.** Every row's state comes from macOS alone. macOS cannot tell a permission
that was never asked for from one the user refused, so a permission that is not granted starts at not granted with the
Grant button, and the System Settings wording appears only after a request made in this window has come back without
the grant. Switching to another Settings section and back puts the Grant button back; pressing it brings the guidance
back with it — answered by macOS with a silent no for Screen Recording, and with that dialog again for Accessibility,
which macOS keeps no refusal for.

**Outside the macOS application** — in a browser, or a desktop shell not running on macOS — the section is a single line
saying that permissions can only be managed in the macOS application. Where a desktop shell is there but the read of the
grants fails, the section is instead a red alert titled that the permissions could not be read, saying that the system
did not report which have been granted, which opens no error alert (see "Failures shown on the screen" in
`docs/product/error-alerts.md`). Until the first read comes back the section shows neither line and nothing else either.

## Other consents programs ask for

The programs agents run in Octoboard's terminals can ask macOS for further consents, which macOS likewise attributes to
Octoboard. The application is built so that macOS raises its own prompt for each of these, naming Octoboard, instead of
refusing the request without asking:

- controlling other applications (Apple Events, automation);
- the camera and the microphone;
- Bluetooth;
- contacts, calendars, reminders and photos;
- location;
- files in the Documents and Downloads folders and on the Desktop, and files on removable and network volumes.

The prompt's explanation is, for each, a neutral sentence saying that a program running in Octoboard would like the
access named; it cannot name the program, which Octoboard does not know. These explanations are available in all 17
languages listed in "The offered languages" in `docs/product/language.md`. macOS selects their language from the
application's native resources using its system language choice, independently of the language selected in Octoboard's
window; English is the fallback for an explanation without a matching translation. Octoboard never raises these prompts
itself, they have no row in Settings' Permissions section, and a grant or refusal is changed in System Settings' Privacy
& Security alone. Whether a notarized release build raises these prompts as described has not been confirmed yet.
