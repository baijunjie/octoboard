> Severity: Minor

## Symptom

VoiceOver announces the project row's action menu with its name twice.

## Reproduction steps

1. Build the packaged app (`pnpm build:app --bundles app` in `apps/desktop`) and launch it with the UI language
   zh-Hans, with a project named "octoboard" in the sidebar.
2. Turn VoiceOver on with Cmd+F5 and bring the Octoboard window forward.
3. Open the project row's action menu ("项目 octoboard 的操作") and listen to the speech.

## Expected vs. actual

- Expected: the menu is announced with its name once (`docs/memory/writing-ui-components.md`, "The UI meets WCAG 2.2
  AA", "Name": "every control has an accessible name — its visible text, or an `aria-label` when it has none").
- Actual: VoiceOver says "项目 octoboard 的操作 web dialog 包含3个项目 menu项目 octoboard 的操作": the name once
  for the dialog and again for the menu.

## Environment

- macOS, packaged app built from `main` at 487f99b plus the `fix/viewer-a11y-names` branch, zh-Hans UI, VoiceOver
  speech read from the packaged app. The menu has 3 items.

## Scope of impact

Screen reader users hear the menu's name twice each time they open a project row's menu. No workaround needed to use
the menu.

## Leads

- Verified: the speech above, from the packaged app.
- Verified: `packages/ui/src/components/ActionMenu.tsx` passes `label` only as the trigger's `aria-label`
  (`sidebar.project.actions`, used by the project row's menu in `packages/ui/src/sidebar/`); the menu itself gets
  no label of its own for a plain menu.
- Verified: react-aria's `useMenuTrigger` sets `aria-labelledby` of the menu to the trigger's id, so the menu is named
  by the trigger's `aria-label`.
- Inferred: the popover's dialog role is named from the same trigger, which gives the name once on the dialog and
  once on the menu. Not checked in the DOM.

## Acceptance criteria

- [ ] VoiceOver speaks the project row's action menu name once when the menu opens.
- [ ] The other action menus (console, session) are checked and speak their names once.
