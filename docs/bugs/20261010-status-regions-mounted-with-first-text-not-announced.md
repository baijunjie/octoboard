> Severity: Moderate

## Symptom

Suspected: three status regions are mounted already holding their first text, which VoiceOver in WebKit does not
announce for the project pane's placeholders, so those states may be silent for VoiceOver users.

## Reproduction steps

1. Build the packaged app (`pnpm build:app --bundles app` in `apps/desktop`) and launch it, then turn VoiceOver on
   with Cmd+F5.
2. Connection banner: with the app connected, quit the daemon (or otherwise drop the connection). The banner
   ("reconnecting", then "disconnected" with Retry) appears. Listen for speech.
3. Terminal pane loading: select a session that has to be attached or resumed, so the terminal shows its loading
   cover. The pane's status region mounts at that moment already holding the loading text. Listen for speech.
4. Terminal connection: select a session whose terminal connection is already down, so the title bar's connection
   control mounts with a problem. Listen for speech.

## Expected vs. actual

- Expected: a state that changes without the user acting is announced (`docs/memory/writing-ui-components.md`, "The
  UI meets WCAG 2.2 AA", "Roles and states": a `role="status"` region stays mounted, empty, and only its contents
  change).
- Actual: Unknown for these three regions. Verified for the project pane's placeholders: a `role="status"` inserted
  holding its text was silent, while one already mounted whose text changed was spoken.

## Environment

- macOS, packaged app built from `main` at 487f99b plus the `fix/viewer-a11y-names` branch; VoiceOver on.

## Scope of impact

If confirmed, VoiceOver users may not hear that the connection dropped, that a terminal is loading or resuming, or
that a terminal connection is down. Workaround: reading the screen with the VoiceOver cursor.

## Leads

- Verified: `packages/ui/src/terminal/TerminalPane.tsx` ~295: the `role="status"` for the loading text is rendered
  with `loading && t(...)` as its first content, so it can mount already holding text when a session is selected.
- Verified: `packages/ui/src/components/TerminalConnection.tsx` ~49: the `role="status"` span renders the
  reconnecting or disconnected text from its first render when `problem` is set on mount.
- Verified: `packages/ui/src/components/ConnectionBanner.tsx` ~84: `role="status"` sits on the banner itself, which
  is rendered only while visible, so it is inserted together with its text.
- Verified: in Safari and in the packaged app, for the project pane's placeholders, a `role="status"` inserted
  holding its text was silent, while one already mounted whose text changed was spoken.
- Inferred: the three regions above are silent for the same reason. Not tested with VoiceOver; the three states
  above have not been listened to.

## Acceptance criteria

- [ ] With VoiceOver on, losing the connection is spoken when the banner appears.
- [ ] With VoiceOver on, selecting a session that loads or resumes speaks the loading text.
- [ ] With VoiceOver on, a terminal connection problem present when a session is selected is spoken.
