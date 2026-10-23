# UI standard audit

## Problem

The preview of the added-agents work kept turning up UI that breaks the project's UI standard
(`docs/ui-accessibility.md`) or rules the user set while reviewing: a blocking reason shown as bare red text instead
of an Alert, a sentence telling the user where to go instead of a button that takes them there, tabs stretched across
the row, two entries for the same action in one panel, a dialog that opens into a state where it cannot do anything.
Each was fixed where it was found, but nobody has checked the rest of the UI. The user wants a global audit.

## Plan outline

1. Write the rules the user set during that preview into the UI standard, so the audit has them to check against.
2. Audit the whole UI against the whole standard and record every place that does not conform.
3. Fix what the audit found, in milestones split from its findings; moving every Alert onto HeroUI's stock structure
   and styles with only a status background fill is one of them.

## Key design decisions

- **The audit covers the entire standard**, not only the kinds of problem the preview exposed: errors and Alerts,
  empty states, copy and buttons, chips and badges, keyboard and focus, accessible names, contrast, "not by colour
  alone", pointer target size, text direction, motion. The user chose the whole standard over a narrow sweep.
- **The preview's rules become part of the standard first** (user decision), so the audit checks against written
  rules rather than against memory of the conversation. The rules to write:
  - An instruction pointing the user somewhere ("go to Settings → Agent accounts and …") is replaced by a button that
    takes them there, opening the target dialog when there is one. Copy explains; a button acts.
  - One view offers one entry for an action; a second entry for the same thing (e.g. "Add agent…" beside
    "Manage accounts…" in an empty panel) is removed.
  - A strip of tabs, agent tabs included, uses its content width and is start-aligned; tabs do not stretch to fill
    the row.
  - An error alert's recovery action is a secondary button in the alert's footer beside the acknowledgement button,
    not a button inside its body.
  - A state that must not happen is not shown as a screen: an entry whose action cannot succeed (e.g. New session
    with no agent to pick) is disabled; if the state arises while a dialog is open, the dialog closes and a plain
    error alert, with no action button, says why.
  - A normal absence (e.g. no agent added) is not an error: it gets a neutral or purpose glyph, not a warning glyph.
  - A status mark on a name (not added, not installed) is a badge after the name, not parenthesised text in the
    name's own style.
  - An inline Alert holds only what failed and why. How to recover — the remedy sentence and its actions — sits
    after the Alert in the surrounding content's ordinary text style, not inside it.
  - A standalone error, warning or blocking reason is never bare coloured text (already written into "Standalone
    errors use the shared Alert" during the added-agents work).
- **The Alert's only custom visual treatment is a status background fill** (user decision). Every Alert otherwise
  uses HeroUI's original styles and component structure (status, indicator, content, title, description). The title
  retains its original status colour and the description its default colour; indicator, typography, spacing, radius
  and shadow remain unchanged. Remove text-colour overrides such as `text-foreground`, text-size and shadow overrides,
  and custom content structure. HeroUI's description cannot hold paragraphs, lists or diagnostic blocks; with the
  recovery moved out of the Alert, what stays inside is a title and a short description, so the stock structure
  suffices. The standard preserves the status background fill and replaces the heading-element title and block
  content inside the callout with that stock structure.
- **Fixes are split after the audit**, by area or by rule as the findings suggest, so each fix milestone stays
  independently reviewable in the gallery.

## Milestones

1. 01 Write the preview's rules into the UI standard (closed)
2. 02 Audit the UI against the standard (closed)

### Accepted fix milestones

The remaining open briefs contain their own accepted finding scope and dependencies. A concise record of
completed checks and unresolved qualifications is in [audit-checks.md](audit-checks.md). Milestones merge
individually into the local main branch, each independently verified; shared contracts have one owner at a time.
The closed audit changed no production UI; fix milestones implement its findings.

- 03 Restore stock Alerts (closed)
- 03a Match the HeroUI v2 Alert background reference (closed)
- 04 Render structured error names and diagnostics (closed)
- 05 Use rich name chips and tooltip content (closed)
- 06 Separate choice status and direction (closed)
- 07 Correct form defaults and blocked validity (closed)
- 08 Keep horizontal tab strips at content width (closed)
- 09 Use consistent neutral empty states (closed)
- 10 Apply shared collection and reduced-motion behavior (closed)
- 11 Defer initial terminal status announcements (closed)
- 12 Hand focus off when report pages are replaced (closed)
- 13 Use the shared button primitive for tree chevrons (closed)
- [14 Final confirmation](14-final-confirmation.md)

Milestone 14 remains open. Its brief carries the findings, current-source handoffs and validation limits
needed to resume work. Detailed source inventories and generated index files were pruned after the open
scope was transferred; the original finding records stay in Git history:
`git show 7d7fdb2da^:docs/plans/20261020-ui-standard-audit/{audit-states,audit-interactions,audit-gallery}.md`.
Retained raw evidence stays outside Git.

## Evidence baseline

The detailed audit inventories were collected against an older source baseline and did not certify later quota,
resume, history or MCP integrations. Completed milestones refreshed their assigned callers.
Historical counts are not current coverage claims.

## Next steps

There is no integration branch: milestones merge individually into the local main branch. Continue from the
then-current main and re-inventory affected callers; do not resume from the historical audit baseline.

- **14** is the end point, placed last.

The 08 report and its machine-readable evidence remain under `/tmp/octoboard-08/`; `audit-checks.md` keeps the
concise historical outcomes and qualifications required for continuation. Do not push as part of local merges.

## Coordination

- Keep generated validation outputs in a task-specific `/tmp` directory. Do not commit additions or modifications
  under `evidence/`. Historical captures and probes are archived outside Git; evidence names in these records are
  archive identifiers. Recreate current-source measurements rather than treating those captures as current validation.
- Independent work may proceed before unrelated milestones close. Coordinate changes to shared useFocusHandoff,
  Button and collection contracts through the parent before editing them. Each worker writes only its assigned
  worktree and cleans only its own services.
- The accepted 03–14 split has no pending user confirmation. The shared Alert, semantic
  error map, rich/plain option contract and AgentTabList each need coordinated ownership. Quota callers are edited
  only by 10's quota account rows.

## Implementation decisions

- Do not introduce a broad lint/static-scanner framework. Use the full source inventory for manual closure and
  focused, meaningful regression tests at shared rich/plain parameter and Alert component contracts during fixes.
- Change-check briefs for 04–06 explicitly name the rule that names referenced in visible copy use InlineReference;
  no global skill/agent configuration change is needed.
