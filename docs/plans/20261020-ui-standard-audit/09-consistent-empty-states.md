# 09 Use shared neutral empty presentations

> Goal: use shared neutral empty presentations.
> Completion criteria: The directory-picker empty listing uses the shared compact presentation and ordinary unavailable-agent empty states use a neutral/purpose glyph. All listed validation passes, and the independently reviewed result is accepted.

Status: **ready; not started**.

## Scope

The accepted findings, re-inventoried against main at `19f58fc79`, are reproduced below so this brief remains
self-contained. The original finding records are in Git: `git show 7d7fdb2da^:docs/plans/20261020-ui-standard-audit/{audit-states,audit-interactions,audit-gallery}.md`.

- **Directory picker (S03):** `DirectoryPicker.tsx:247-252` still renders the empty listing as a disabled
  `EMPTY_KEY` `ListBox.Item` with `FadeOverflow` text and no glyph. Keep that valid option and collection semantics,
  but give it the shared compact glyph/message presentation. The Parent directory row is pinned and rows show their
  paths, so the empty option sits under the pinned parent row, or alone at `/`. Preserve parent navigation, keyboard
  skipping and the absence of row actions. Which form the empty list row takes (the compact `EmptyPanel`, whose text
  wraps, or the current single-line marquee) is an implementation decision: pick the one consistent with the empty-state
  rules in `docs/ui-accessibility.md`, or settle it with the user.
- **No installed agent (S05/G03):** `terminal/TerminalPane.tsx:540` and `sidebar/Sidebar.tsx:653` use `TriangleAlert`
  for `agents.noneFound`, an ordinary absence; rewrite the comment at `Sidebar.tsx:650-652` with the change. Use a
  neutral or purpose glyph in these two empty states, consistent with `quota/RailQuotaEntry.tsx:323,325` (Bot/Gauge)
  and `settings/AgentAccountsSection.tsx:191` (Bot). Retain PATH explanations, disabled creation actions, genuine load
  failures and the approved warning glyph at `AgentTabList.tsx:189`. The copy differs between the two ("No agent found
  on {PATH}" and "No agent available"); that is noted only, and copy is out of scope.
- **Session history:** since `2451736c6` both `history-unavailable` and `history-failed` are `PaneFailure`
  (`SessionHistoryView.tsx:77-81`), not Alerts. **User decision:** `history-unavailable` is reclassified as a neutral
  empty state, since no retry would change it (`docs/ui-accessibility.md`, "An empty state is a glyph, a heading, and
  what fills it"). Update its fixture (`gallery/fixtures/sessionHistory.ts:200`) and
  `docs/product/session-history.md:63-78` during implementation. `history-failed` stays a `PaneFailure` with its reason,
  details, retry and reconnect behavior; it is only re-verified.

## Implementation contract

- [ ] Give the empty directory listing the shared compact glyph/message shape within valid collection markup; preserve disabled option semantics and parent navigation.
- [ ] Replace warning imagery only in ordinary terminal/sidebar no-installed-agent absence; retain PATH explanations and disabled creation actions.
- [ ] Reclassify `history-unavailable` as a neutral empty state with its fixture and product doc; keep `history-failed` as a `PaneFailure`.
- [ ] Preserve intentional warning glyph on AgentTabList, true failed-load error states, shared full-panel headings and silent/action exclusivity.

## Validation requirements

- [ ] Compare the current directory-picker fixtures in `gallery/fixtures/dialogs.ts` (`dialog-directory-picker`, `-failed`, `-outside-home`, `-scrolled`, `-root-scrolled`) and add two empty fixtures: an empty list under the pinned parent row, and an empty list alone at `/`. Review `empty-console-no-agent` against `empty-console-no-agent-added`, and `rail-quota-none` / `rail-quota-none-added`, in both themes and Chinese/ar. Verify no empty row action, keyboard skipping, valid list semantics, no duplicate creation entry and matching sidebar/main shapes. Verify `history-unavailable` and `history-failed` fixtures. Run affected gallery and dialog/empty-panel tests and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

None; no token redesign or unrelated empty-state copy changes. 09 and 11 both edit `TerminalPane.tsx`, and 09 and
10 both edit `Sidebar.tsx`; merge them one at a time.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the linked finding records. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence in the finding records and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
