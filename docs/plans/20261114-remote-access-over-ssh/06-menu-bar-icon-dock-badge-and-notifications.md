# 06 Menu bar icon, Dock badge and notifications per daemon

> Goal: every daemon the application shows has a menu bar icon of its own, and the Dock badge and the notifications
> account for all of them.
> Completion criteria: with the local daemon and one connected remote daemon there are two icons, the remote one
> marked (on macOS the host's short name beside the icon; on Windows a mark drawn into the icon and the name in its
> tooltip); each icon's menu lists its own daemon's sessions, and its attention dot, quota and keep-awake eyes are
> its own daemon's; choosing a session in a menu opens it in that daemon's window; disconnecting the remote daemon
> removes its icon; hiding the menu bar icon in Settings hides them all; the Dock badge is the sum of the windows'
> counts; clicking a notification brings forward the window of the daemon that raised it.

## Technical design

- [ ] One menu bar icon per window's daemon, created with the window and removed with it; its menu, look and menu
      tracking kept per icon.
- [ ] Menu choices routed to the window of the icon's daemon.
- [ ] The remote mark: on macOS the host's short name beside the icon; on Windows a mark in the icon and the name in
      the tooltip.
- [ ] The Dock badge as the sum of what each window reports.
- [ ] A notification click brings its own window forward.

## Implementation plan

- [ ] First, settle at run time what reading the code could not: that several of the application's custom status
      items coexist on macOS, and that a notification click can be routed to the window it came from on both
      systems.

## Notes for the developer

- **Reusable capabilities**: the menu bar icon's model, which carries no agent knowledge and is filled by each
  window's UI; the status item's layers (the attention dot, the quota bars, the eyes).
- **Development notes**: the icon's identifier, its menu items' identifiers, the state of the menu being tracked and
  the layer views are each held once for the application today; the Dock badge is set by whichever window wrote
  last.
- **Reference docs**: `docs/product/menu-bar-icon.md`, `docs/product/session-statuses.md` (the raised hand),
  `docs/structure/desktop.md`.
