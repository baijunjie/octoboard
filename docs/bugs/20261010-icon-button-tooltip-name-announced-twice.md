> Severity: Minor

## Symptom

VoiceOver reads an enabled icon button that has a tooltip with its name twice, for example "交换分支 交换分支 button".

## Reproduction steps

1. Build the packaged app (`pnpm build:app --bundles app` in `apps/desktop`) and launch it with the UI language
   zh-Hans.
2. Open the project pane through the project's menu ("浏览文件" / Browse files), switch to Git mode, and open the
   Compare tab.
3. Turn VoiceOver on with Cmd+F5 and move the VoiceOver cursor onto the enabled "交换分支" (Swap branches) button, (if
   VoiceOver does not follow Tab focus, move the VO cursor onto the button).
4. Listen to the speech. Repeat with the "刷新" (Refresh) button in the project pane's header.

## Expected vs. actual

- Expected: the control's name is spoken once (`docs/memory/writing-ui-components.md`, "The UI meets WCAG 2.2 AA",
  "Name": every control has an accessible name, its `aria-label` when it has no visible text).
- Actual: enabled Swap branches reads "交换分支 交换分支 button"; Refresh reads "刷新 刷新 button".

## Environment

- macOS, packaged app built from `main` at 487f99b plus the `fix/viewer-a11y-names` branch; zh-Hans UI; VoiceOver
  speech read through VoiceOver's AppleScript `content of last phrase`.

## Scope of impact

VoiceOver users hear the name of every tooltip-bearing icon button twice. No workaround; the control is still usable.

## Leads

- Verified: the button's tooltip opens on focus (seen in the accessibility tree) and VoiceOver reads the tooltip text
  after the name.
- Verified: the disabled Swap button, which takes no focus and shows no tooltip, reads "交换分支 变暗 button" (name
  once).
- Verified (control): a button with only `aria-label` and `title` is read once.
- Verified: both buttons are wrapped in `TitledControl` (`packages/ui/src/components/TitledControl.tsx`) with the
  same string for the tooltip and the `aria-label` (`packages/ui/src/browser/BranchComparison.tsx:74-86`,
  `packages/ui/src/browser/ProjectBrowser.tsx:171-175`). `TitledControl` renders HeroUI's `Tooltip` around the
  control with `<Tooltip.Content>{title}</Tooltip.Content>`.
- Inferred: the tooltip's text, linked to the button while it is open, is spoken in addition to the `aria-label`.
  Not established which attribute the link uses.

## Acceptance criteria

- [ ] With VoiceOver, the enabled Swap branches button is read with its name once.
- [ ] With VoiceOver, the Refresh button is read with its name once.
- [ ] The tooltip still opens on hover and on keyboard focus.
