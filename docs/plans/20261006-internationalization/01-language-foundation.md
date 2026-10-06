# Language Foundation in the Desktop UI

> Goal: the desktop UI resolves its current language and renders every string from a message catalog, with English
> as the only complete catalog so far.
> Completion criteria: no user-facing string in `packages/ui` is written inline any more; in English the UI reads
> exactly as before; the language option in Settings switches the UI between "System" and each listed language
> (untranslated strings fall back to English) and is remembered across restarts; a unit-level check covers the
> system-language mapping rules; `pnpm --filter @octoboard/ui build` passes.

### Technical design

- [ ] The language list (the 17 tags, in tag order) and the fallback language, defined once and shared by everything
  that needs the list.
- [ ] A function that maps the system's preferred languages onto the list by the rules in the overview, returning the
  fallback when nothing maps.
- [ ] The current language: the user's choice ("System" or one of the list), remembered per client in client storage
  with every access tolerant of storage being unavailable, resolved to a concrete language through the mapping above
  when it is "System", and re-resolved when the system language changes.
- [ ] The message catalog with the English base messages, plural-aware lookup and interpolation, and a fallback to the
  English message for any key a language lacks.
- [ ] Every user-facing string in the UI goes through the catalog: labels, tooltips, accessible names, dialog text,
  toasts, settings, status labels and the system notification text the UI composes.
- [ ] HeroUI / react-aria's own built-in strings and formatting follow the current language.
- [ ] The document's `lang` and `dir` follow the current language.
- [ ] A Language row in Settings, next to Appearance: "System" plus the 17 languages, each named in its own language,
  in list order.

## Notes for the developer

- **Reusable capabilities**: the per-client persisted-preference helper the layout preferences use; the appearance
  setting's handling of "System" (following a live system change) as the pattern for the language's.
- **Development notes**: text sent to the daemon or written to the repository is not translated; only what the user
  reads is. Keep accessible names and tooltips in step (they are the same string).
- **Reference docs**: `docs/product/settings.md`, `docs/product/appearance.md`, `packages/ui/README.md`,
  `docs/memory/writing-ui-components.md`.
