# HeroUI UI Rewrite Development Plan

## Problem and approach

The UI is a React application welded to the Tauri shell. The intended final shape needs one desktop UI that the macOS
shell and a Linux browser both run, so it has to stand on its own: in a package of its own, runnable in a plain browser,
reaching native capabilities only through an adapter. The decision is also to rebuild the UI on HeroUI (React), and to
make the new UI the macOS application's UI rather than a second UI beside it.

That fixes the order of work: the current UI is frozen now, so that polishing is done once, on the new UI, instead of
twice. The new UI is built slice by slice in its own package while the frozen current UI keeps being the shipped
application, and the shell switches over as soon as the new UI reaches parity. The product docs describe the behavior
and are the parity specification: the new UI is done when it does what they say.

## Key design decisions

- **React with HeroUI for all UI**, in its own package (`packages/ui`), not inside the desktop shell. It is a rebuild,
  not an in-place migration of the current UI.
- **The latest HeroUI release**, with what it requires: at the time of writing that is HeroUI 3, which needs React 19
  and Tailwind CSS 4. Those versions apply to the new package; the frozen current UI stays on its own versions until it
  is removed.
- **A platform adapter is the only route to native capabilities** — the quit flow and exit heartbeat, desktop
  notifications, window control. It has a Tauri implementation and a browser implementation, selected at startup by
  environment so that one build runs in both places. In a browser, a capability that is unavailable means the feature is
  absent, not an error.
- **The daemon client and the terminal stay framework-independent** in behavior: they already speak WebSocket only and
  xterm.js is framework-agnostic, so they are carried over rather than redesigned.
- **The UI locates the daemon without Tauri**: by an address handed to it, or same-origin when the daemon serves it.
- **The current UI is frozen from the start of this plan**: only fixes for what stops the application being used, no
  new features and no polish. Anything outstanding on the macOS application — polish and the checks that need a real
  build — is done on the new UI, after the cutover.
- **Cut over as soon as there is parity.** The new UI is not wired into the shipped application before then, so the
  application stays usable throughout; the cutover is its own milestone and is not to be delayed for anything beyond
  parity.
- **The phone's browser is not a design target**: a layout that works is enough. The native mobile apps have their own UI.

## Milestones

- [01 Foundation](01-foundation.md) — the package, the adapter, the daemon client, the state, running in a browser
- [02 Core screens](02-core-screens.md) — sidebar, sessions, terminal
- [03 Remaining screens](03-remaining-screens.md) — dialogs, report panel, notifications and the rest
- [04 Cutover](04-cutover.md) — the desktop shell loads the new UI, the old UI is removed

Dependency order: 01 → 02 → 03 → 04. The package location, `packages/ui`, already exists as a placeholder. Until 04
lands, the macOS application's polish is on hold.

## Open

- Whether the macOS window loads the UI bundled in the application (as now) or the one the daemon serves.
- State management; decide when starting 01.
