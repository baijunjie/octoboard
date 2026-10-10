> Severity: Moderate

## Symptom

VoiceOver reads the terminal's input field with the English name "Terminal input" (then the role word "text entry
area") in a zh-Hans UI.

## Reproduction steps

1. Build the packaged app (`pnpm build:app --bundles app` in `apps/desktop`) and launch it with the UI language
   zh-Hans and no session selected.
2. Turn VoiceOver on with Cmd+F5 and bring the Octoboard window forward.
3. Move the VoiceOver cursor over the terminal pane and listen: "Terminal input", then the role "text entry area",
   is read.

## Expected vs. actual

- Expected: a control's accessible name is in the UI language (`docs/memory/writing-ui-components.md`, "The UI meets
  WCAG 2.2 AA", "Name"; `docs/product/language.md`, "Choosing the language").
- Actual: the name is the English "Terminal input".

## Environment

- macOS, packaged app built from `main` at 487f99b plus the `fix/viewer-a11y-names` branch; zh-Hans UI; VoiceOver
  speech read through VoiceOver's AppleScript `content of last phrase`.

## Scope of impact

VoiceOver users of a non-English UI hear an English name on the terminal's input. No workaround.

## Leads

- Verified: the name comes from xterm.js 6.0.0, whose helper textarea is given `aria-label` from
  `Strings.promptLabel.get()`, defaulting to `'Terminal input'`
  (`@xterm/xterm/src/browser/CoreBrowserTerminal.ts:442`, `src/browser/LocalizableStrings.ts:8`); the `promptLabel`
  property of xterm's `Terminal` strings sets it (`browser/public/Terminal.ts:253`).
- Verified: `packages/ui/src/terminal/` never sets `promptLabel`; `TerminalController.ts:147` constructs `Terminal`
  without it.

## Acceptance criteria

- [ ] The terminal's input is announced with a name in the UI language.
