# Launch at login

The desktop application can have the system open it when the user logs in, through the **Launch at login** switch in
Settings' App behavior section (the row itself is described in "Launch at login" in `docs/product/app-behavior-settings.md`). It is
offered on Windows and on macOS 13 and later. macOS 11 and 12 have no row for it, and neither does a browser, which has
nothing to open.

## The switch shows the system's state

Octoboard keeps no choice of its own for this: the system's login entry is the setting, and the switch shows where it
stands. It is read once when Settings opens, whichever section it opens on, and the row appears only once that read has
answered; it is read again every time the window regains focus while Settings is open, so a change the user made in the
system's own list (below) shows on returning to Octoboard. A read that fails shows the switch off. A read that was under way when the user turned the switch is dropped, so it cannot put back
the state from before the change.

The entry is in one of three states:

| State | The switch | Where |
|---|---|---|
| Off | Off | Both |
| On | On | Both |
| Waiting for approval | On, with the description and button of "Waiting for approval" below | macOS |

Turning the switch on registers the entry and turning it off removes it; the switch then shows the state the system
reports. Because the entry belongs to the system, a configuration export never carries it and an import never changes
it (see "Which settings travel" in `docs/product/settings-in-a-configuration.md`).

## Where the entry lives

- **macOS**: Octoboard's own login item, registered for the application itself. It is listed in System Settings under
  General > Login Items, where the user can also turn it off, or leave it waiting for approval.
- **Windows**: a value named `Octoboard` under the current user's
  `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, holding the quoted path of the running
  executable followed by `--launched-at-login`. Task Manager's Startup apps tab can turn it off without removing it;
  the switch then shows it as off, and turning the switch on clears that so the entry runs again. Turning the switch off
  removes the value.

## Waiting for approval

On macOS the system can hold a registered login item until the user allows it. While it does, the switch is on, the
row's description says that Octoboard is waiting for the user's approval and to allow it in System Settings, and an
**Open System Settings** button beside the switch opens System Settings at General > Login Items. Once the user has
allowed it, the next read shows the entry on and the button goes; keyboard focus on the button moves to the switch.
Turning the switch on while the user has left the entry unapproved is not a failure: it shows as waiting for approval.

When System Settings does not open, an error alert titled "Could not open System Settings" says that it did not open
and to open it by hand and choose General > Login Items. It has only its acknowledgement button, with no remedy button
and no request to report an issue, like the Notifications section's (see "Opening the system's notification settings" in
`docs/product/notification-settings.md`).

## When a change fails

A change the system refuses is shown in an error alert titled "Could not change Launch at login", saying that the system
did not accept the change. It is an unexpected failure, offering **Report issue**, with what the system said behind
**Error details** (see "Recovery and unexpected errors" in `docs/product/error-alerts.md`). The switch then shows the
state the system reports, which the refused change may still have altered.

## A launch at login

Octoboard tells a launch the system made for the entry from any other: on macOS by the launch request that opened it,
on Windows by the `--launched-at-login` argument. Only the first time a launch's window would be shown counts.

- **While Keep running in the background is on**, a launch at login starts hidden, in the state of a window closed into
  the background: the window is never shown, and on macOS Octoboard is not in the Dock, though its Dock icon may appear
  for a moment while it starts. The daemon and the sessions run as with the window shown, and the window comes back by
  any of the ways in "Closing the window" in `docs/product/application-lifecycle.md`: the menu bar icon's menu while
  the icon is shown, opening Octoboard again, or one of its notifications.
- **While it is off**, a launch at login opens the window as any launch does.
- The four-second fallback that shows a window the UI has not revealed (see "Starting up" in
  `docs/product/application-lifecycle.md`) is not armed at a launch at login, whichever way the switch is set: should
  the UI never load, the window stays hidden until one of the ways above brings it back.
- On Windows, the entry firing while Octoboard already runs starts no second Octoboard and does not bring the running
  one's window forward.
