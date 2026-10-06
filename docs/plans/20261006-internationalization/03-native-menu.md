# Native Menu in the Current Language

> Goal: the macOS application menu shows its items in the UI's current language and follows a language change.
> Completion criteria: in the macOS app, every Octoboard-defined menu item (Settings… and the others the shell adds)
> reads in the current language, switches when the language changes in Settings or the system language changes while
> on "System", and falls back to English for a language without a translation.

### Technical design

- [ ] The shell learns the UI's current language (the language the UI renders, not the system locale) through the
  platform adapter, at startup and on every change.
- [ ] The shell's menu items take their labels in that language, by the mechanism decided in the overview's "Open",
  and are rebuilt when it changes.
- [ ] Menu items the system itself provides are left to the system.

## Notes for the developer

- **Reusable capabilities**: the platform adapter's capabilities between the UI and the shell (the window chrome and
  app-menu capabilities are the closest pattern); the way the UI pushes its appearance to the native window.
- **Development notes**: no daemon traffic goes over the shell's IPC; the language is UI-to-shell only.
- **Reference docs**: `apps/desktop/README.md`, `packages/ui/README.md`, `docs/architecture.md`.
