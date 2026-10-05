# 02 Core screens

> Goal: the screens a user lives in — the console and project sidebar, sessions, and the terminal — work in the new UI.
> Completion criteria: in a browser against a daemon, a user can browse consoles, projects and sessions, open a session's
> terminal, type into it, and see status and records update live, matching the behavior in the product docs.

## Technical design

- [ ] The sidebar with consoles, projects and sessions, and its action menus.
- [ ] The terminal pane, including attach, replay, resize and input.
- [ ] Session status presentation.

## Implementation plan

- [ ] Build screen by screen against the product docs, checking each against the current UI side by side.

## Handoff

- `packages/ui/src/App.tsx` is a placeholder screen, marked `TODO`, that lists consoles, projects and sessions to show
  the daemon's state arriving. Replace it with the real screens. The placeholder is also where `useAppExit` and
  `useWaitingNotifications` are mounted today, so move those across to the replacement too.

## Notes for the developer

- **Reusable capabilities**: the existing terminal controller's behavior (replay then live output, resize on attach).
- **Development notes**: behavior is what is carried over, not markup; where the current code and the product docs
  disagree, report it rather than choose.
- **Reference docs**: `docs/product/sessions.md`, `docs/product/consoles-and-projects.md`, `docs/memory/verifying-the-desktop-ui.md`.
