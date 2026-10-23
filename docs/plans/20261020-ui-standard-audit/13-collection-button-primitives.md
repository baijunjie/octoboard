# 13 Use the shared button primitive for collection chevrons

> Goal: use the shared button primitive for collection chevrons.
> Completion criteria: Files/Git tree chevrons follow the shared HeroUI Button contract, preserving React Aria collection slot, expansion semantics, row focus and equivalent row action. All listed validation passes and the result is independently reviewed.

Status: **in progress**. The shared-button migration is implemented on its separate branch and has its own
initial validation, and the menu-focus return is now fixed within this milestone; final actual interaction
coverage and closure remain incomplete. The separate branch must not be merged into main until 13 closes.

## Scope

The accepted finding and its open scope decision are reproduced below so this brief remains self-contained.

- `treeRow.tsx` uses a bare React Aria Button for the chevron even though the installed HeroUI Button wraps the
  same primitive and forwards `slot`. Migrate the chevron to the shared Button without adding a Tab stop; retain
  collection-managed expansion/focus and the documented custom section-heading control.
- Existing Files traversal reached the folder action, returned to the row, expanded with Enter and moved to the
  next row with ArrowDown. The 16px glyph is not itself a pointer-target defect because the whole row is the
  equivalent larger action. Recheck Files and grouped Changes in en/ar light/dark plus Chinese, including arrows,
  Home/End, fold focus, menus, visible rings and the 24px equivalent action.
- **Scope decision (made):** closing a keyboard-opened row menu returned focus to the row, while the product
  documentation promises return to the exact surviving Actions button; the same happened on the untouched baseline.
  The user chose to fix it inside 13, as a bounded public-API repair. `ActionMenu` gained an `onClosed` hook,
  called once the popover has really left the page (its content unmounts after the exit animation) with the trigger
  when the menu was opened from the focused trigger by the keyboard and no pointer press came before; pointer opens
  are told apart by `usePointerFocusReturn`'s new `isPointerOpened`, since React Aria focuses the trigger on a pointer
  press too. `RowActions` hands that trigger to `focusRowControl` in `packages/ui/src/browser/focusRow.ts`, which, two
  animation frames on like `focusRow`, focuses it once if its row is still found by key in the same collection, still
  holds it, and focus sits on the page or inside that row. Pointer-opened menus keep their existing focus return, a
  keyboard-opened menu dismissed by a pointer press leaves focus where the press put it, and a removed or
  virtualized-away row is left to the collection. **Known limitation:** a menu opened with the Menu key, Shift+F10 or
  a right-click goes through the context-menu path, counts as a pointer open and keeps the pointer-style return.
  Tests in `FileTree.test.tsx` and `ChangeTree.test.tsx` cover the exact trigger after Escape and after a
  non-destructive item, the removed row, focus moved elsewhere, a pointer dismissal and a pointer open from a focused
  row; each except the removed-row case fails with its guard disabled. A headless Chromium check over the
  `files-tree` and `git-changes-tree` gallery scenarios in en/ar light/dark and zh-Hans light confirmed the exact
  trigger after Escape and after Copy path, and an unchanged pointer return.

## Implementation and validation contract

- [x] Use the supported shared HeroUI button with the collection chevron slot, retaining compact layout and collection-managed focus without adding Tab stops.
- [x] Preserve the documented custom section-heading band exception; its reason and successful heading focus probe do not justify every bare collection button.
- [x] Return focus to the exact Actions button when a keyboard-opened row menu closes (unit-tested in jsdom and checked in headless Chromium on the gallery; the final traversal below still covers it).
- [ ] Verify whole-row Enter expansion, Left/Right expansion and folding (mirrored under RTL), chevron pointer expansion, arrow traversal, Home/End, fold focus and rows’ controls in en/ar light/dark, plus Chinese. Preserve supported row-Space no-op under the owner-agreed library constraint; section headings retain Enter/Space activation. Keep 24px equivalent row action; do not infer a pointer failure from glyph size.
- [ ] Run tree/collection focus tests, actual gallery traversal, affected gallery and static checks. If a real library constraint prevents the wrapper, document source evidence and agree the smallest justified exception rather than silently rewriting keyboard handling.
- [ ] Complete project gallery/static/change-check/documentation gates; retain unresolved checks explicitly.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

Keep generated validation artifacts in a task-specific temporary directory. No evidence additions or modifications
under this topic's `evidence/` may enter a commit; inspect staged and committed paths before commit, squash and integration.
Keep historical provenance as archive identifiers; do not restore removed evidence files into the repository.

## Notes for the developer

Reuse the existing focus handoff, shared Button and collection primitives where applicable. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery” and the project’s gallery/change-check workflows. Use gallery fixtures only. This is independent of the visual Alert, copy, field and motion milestones; coordinate shared collection edits with milestone 10.

## Mid-task handoff

- The shared Button migration and collection tests are prepared on `refactor/collection-chevron-buttons`; no heading-band change. The menu-focus fix changed shared helper contracts: `ActionMenu` has a new `onClosed` prop, `usePointerFocusReturn` a new `isPointerOpened`, and `focusRow.ts` a new `focusRowControl`. Independent review passed in round two after preserving the 24px button against flex shrinking and awaiting virtual-click focus settlement.
- **Owner-agreed smallest exception:** React Aria 3.52.1 `dist/private/selection/useSelectableItem.mjs` assigns Enter to action (lines 178–188, 307–312) and Space to selection. These trees have selection disabled; supported action APIs do not change that mapping. Clean fixture-backed WebKit checks observed Space doing nothing, Enter and pointer chevron toggling, across Files, Git folders and flat/tree commit groups in en/ar/zh-Hans and both themes. Existing product docs prescribe this baseline; headings retain Enter/Space. Preserve row-Space no-op without a custom adapter or a fabricated Space-expansion pass. I-08's original primitive finding and audit evidence are unchanged. The direct terminal question remains unanswered; owner agreement does not dismiss that prompt.
- Pre-migration application/test snapshot: base `0ae28459f8e4dc36546573f3048bd1474a242370`, binary diff SHA-256 `fe3c54487c5dd9a46f4e7382d323049e171ce42cc72b474d96f844117c190841` (before this handoff-only edit). Clean WebKit activation and navigation checks each covered 24 cases: four fixtures × three languages × both themes. Navigation included Home/End, row controls, child-focused pointer fold, heading Space and chevron Tab exclusion. Buttons measured 24×24, glyphs 14×14, rows 28px high; system fonts were loaded.
- `pnpm --filter @octoboard/ui exec vitest run src/browser/FileTree.test.tsx src/browser/ChangeTree.test.tsx src/browser/RowActions.test.tsx src/browser/ChangeList.test.tsx`: 4 files, 50 tests passed. Typecheck, lint, Rust format, whitespace check, documentation review and 31 gallery fixture tests passed. These results predate migration onto aggregate `b5bcee2f93f6d2edb3d75d69d36885964d88923a`; they do not certify the migrated source. Build and clean affected-gallery sweep remain held during the parent's rebuilt-main baseline sweep. Add direct Left/Right fold/unfold verification to the final actual traversal; existing clean probes covered vertical navigation and horizontal row controls. An intermediate scoped commit preserves the work for target migration; final delivery verification and commit preparation remain open.
- The first scoped sweep is **not delivery evidence**: 294 loads, 291 clean and 3 initial failures, exit 1; its three built-in isolated retries passed. An uncoordinated invalid-slot mutation overlapped it at 22:34:14–26 JST, with no reliable per-case timestamps. All potentially affected Project pane cases need clean coverage on frozen source. The initial sandbox listen rejection was a setup failure, distinct from application failures. The mutation's three failing unit cases are sensitivity evidence only. Earlier incomplete/incorrect browser probes are excluded; the clean probes used stable row identities across folds and leaf controls for horizontal action traversal.
- The aggregate `fix/ui-standard-audit` was merged into main after milestone 08 and deleted; this branch was rebased onto main and now targets main. Validation recorded above predates that rebase and does not certify the rebased source. Keep this milestone open until final validation passes.
