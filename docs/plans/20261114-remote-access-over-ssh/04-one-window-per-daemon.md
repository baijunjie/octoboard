# 04 One window per daemon

> Goal: the application can show several daemons at once, each in a window of its own that is independent of the
> others.
> Completion criteria: with the local daemon's window and a window opened on a second daemon (for verification, one
> started by hand against another data directory), each window shows only its own daemon's consoles, projects and
> sessions; Settings and About chosen from the menu open in the focused window alone; quitting asks about and stops
> only the local daemon, and so does a restart into an update, the second daemon and its sessions running on;
> closing the second window ends nothing on its daemon; closing the local window while the second is visible neither
> quits nor hides the second, and the existing closing rule applies once the last visible window is closed; what one
> window remembers of its daemon (the selected console, the file browsers' state) is neither changed nor removed by
> the other; a change of language, appearance, terminal font size or a keyboard shortcut in one window reaches the
> other; the second window names its daemon in its title and top bar, and offers nothing that assumes the daemon
> shares this machine (the native directory picker, opening the file manager or a system terminal, the Permissions
> section) and no application-level setting (updates, launch at login, running in the background, Remote access);
> each window's size and position are remembered separately; the local daemon ending or being restarted touches the
> local window alone.

## Technical design

- [ ] Opening a window on a daemon given by its address and key, with an identity of its own; the local daemon's
      window stays the main one.
- [ ] The shell tells the window whether its daemon is remote, and the name to show for it; the UI judges "the daemon
      is on this machine" from that, no longer from a loopback address.
- [ ] A remote window is allowed only the shell commands it needs: none of quitting, launch at login, the privacy
      grants or the updater.
- [ ] Shell commands act on the window that called them, and an event meant for one window reaches that window
      alone.
- [ ] Menu items that ask the UI for something go to the focused window; Quit goes to the local window.
- [ ] The exit flow belongs to the local window: a remote window never asks its daemon to stop, at a quit or at a
      restart into an update, and a quit closes the remote windows.
- [ ] Closing: a remote window closes for good; the application is in the background only when no window is visible,
      and hides from the Dock only then; reopening or activating it shows the window last focused, else the main
      one.
- [ ] What the UI keeps in the browser's storage is split into the application's (shared by every window) and a
      daemon's (kept under that daemon's identity); a change to the application's reaches the other open windows.
- [ ] Each window's size and position are remembered under its own identity.
- [ ] A remote window carries the remote host's name in its title and in its top bar.
- [ ] Until milestone 06, what a remote window reports for the menu bar icon and the Dock badge is ignored.

## Implementation plan

- [ ] First, settle at run time whether a change one window makes to the browser's storage reaches the other
      windows, which decides how a change to the application's preferences is passed on.
- [ ] Until milestone 05 gives the owner a way to open one, a second window is opened for verification only, on a
      daemon started by hand.

## Notes for the developer

- **Reusable capabilities**: the shell's way of pointing the window at a daemon restarted on another address; the
  one helper nearly every stored preference goes through; the UI's existing split between the client's host and the
  daemon's host, made for the browser on a server form's daemon; the browser platform adapter's absent capabilities.
- **Development notes**: the shell assumes one window named `main` and one daemon throughout: the IPC allowlist
  names that window only, events are broadcast to every window while the UI listens without naming its own, state
  registered per window is in fact application-wide, and hiding into the background hides every window. Pruning the
  stored state of projects a daemon no longer has would today delete another daemon's.
- **Reference docs**: `docs/structure/desktop.md`, `docs/structure/ui.md`, `docs/product/application-lifecycle.md`
  ("Closing the window", "Quitting"), `docs/product/window-size-and-position.md`,
  `docs/product/settings-in-a-configuration.md`.
