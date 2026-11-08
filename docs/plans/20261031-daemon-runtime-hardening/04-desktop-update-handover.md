# 04 Desktop updates keep sessions running

> Goal: restarting the desktop application to apply an update hands every session over to the new version's daemon
> instead of interrupting it, on macOS and, once milestone 01 has passed, Windows; an application crash leaves them
> running too.
> Completion criteria: install an update and restart into it while sessions are working, with a shell terminal open.
> - Every session and shell terminal keeps running in the new version, with its exact screen. Nothing becomes
>   interrupted.
> - The adopted sessions' hooks, MCP tools and git gate work with the new daemon.
> - New sessions run under a holder of the new build. The old holder exits once its last terminal ends.
> - Kill the application outright while sessions work. On the next launch every session is still running.
> - A plain quit, with no update, still ends every session as today, and so does the quit of a window that stopped
>   responding.
> - A system shutdown asks for no confirmation, and on the next start every session is interrupted with its last
>   screen as its preview.
> - The update restart asks for confirmation only when some session would be interrupted, and names those sessions.

## Technical design

- [ ] The update restart sends milestone 03's planned-stop request rather than the ordinary shutdown. On macOS the
      application bundle has already been replaced, so the old daemon makes its planned stop and the new
      application's daemon adopts the holders. On Windows the installer runs only after that planned stop.
- [ ] The restart's escalation repeats the planned-stop request rather than the ordinary shutdown, and waits long
      enough for the screens to be saved. A daemon killed in the end leaves its sessions to crash recovery.
- [ ] Every quit tells the daemon it is quitting before the application exits, without waiting for an answer, the
      quits that do not wait for the daemon included. A daemon that finds the application gone after that makes an
      ordinary stop.
- [ ] The daemon finding the application gone makes the planned stop rather than an ordinary stop, so an
      application crash leaves the sessions to the next launch.
- [ ] A system shutdown, restart or logout is told apart from a quit: it asks for no confirmation, and the daemon
      makes its ordinary stop, freezing each preview, before the system goes on. On Windows the application passes
      the system's end-session notice on to the daemon, as far as the time the system allows.
- [ ] Where the planned stop has saved a screen and the holders then die with the system, the session interrupted at
      the next start shows that screen as its preview.
- [ ] The update restart's confirmation appears only when some session would be interrupted: a holder outside the
      compatible range, or every session on Windows while Windows keeps interrupting. It names those sessions.
- [ ] The new application starts its own daemon as today. That daemon adopts the holders through milestone 02's
      adoption, including a holder of an older build in the compatible range.
- [ ] The new build's database migration runs while sessions are adopted. Records of adopted sessions stay valid
      across it.
- [ ] The adopted sessions' hooks, MCP bridge and git gate run from their build's copy and keep working through
      their holder's relay (milestone 02), while the installation is replaced.

## Implementation plan

- [ ] Route the update restart and the application's loss through the planned stop on macOS, then on Windows once
      milestone 01 has passed. If it has not, Windows keeps interrupting on an update.
- [ ] Change the update restart's confirmation, the quit paths and the system shutdown path.

## Notes for the developer

- **Reusable capabilities**: the update restart flow, with its confirmed quit and its daemon stop; the daemon
  instance lock; the per-user port file in the data directory.
- **Development notes**:
  - Today an update restart is a confirmed quit. The application asks the daemon to shut down, follows up after
    2 seconds with a direct request or a termination signal, and kills it after 5 seconds. On macOS the daemon is
    never started again before the application restarts.
  - Milestone 02 changes the start-up rules for adopted sessions.
  - Logout, restart and shutdown arrive on macOS through the same application delegate selector as a quit. On
    Windows the system ends the daemon at sign-out or shutdown with no notice of its own.
  - The application-lifecycle and in-app-update product docs change with this milestone.
- **Reference docs**: `docs/product/in-app-updates.md` ("Downloading and installing"),
  `docs/product/application-lifecycle.md` ("Starting up", "Quitting"), `docs/architecture.md` ("Why the daemon is
  split out").
