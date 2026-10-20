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

The [coverage and delivery record](audit-coverage.md) maps every standard section, confirmed finding and evidence
artifact. The user accepted the exact 03–13 split and independent work on 04, 12 and 13. Completed work now
accumulates on the dedicated UI audit integration branch. Each milestone is independently verified; shared
contracts have one owner at a time. Only the complete plan is rebased onto current main, verified and integrated
into main. The closed audit changed no production UI; fix milestones implement its findings.

3. Restore stock Alerts (closed)
4. Render structured error names and diagnostics (closed)
5. [Use rich name chips and tooltip content](05-rich-names-and-tooltips.md)
6. [Separate choice status and direction](06-choice-status-and-direction.md)
7. [Correct form defaults and blocked validity](07-form-defaults-and-validity.md)
8. [Keep horizontal tab strips at content width](08-content-width-tabs.md)
9. [Use consistent neutral empty states](09-consistent-empty-states.md)
10. [Apply shared collection and reduced-motion behavior](10-collection-and-reduced-motion.md)
11. [Defer initial terminal status announcements](11-terminal-status-announcements.md)
12. Hand focus off when report pages are replaced (closed)
13. [Use the shared button primitive for tree chevrons](13-collection-button-primitives.md)

Milestones 05–11 and 13 remain open. The open milestones carry their current-source handoffs; the original audit
inventories remain historical evidence, with check limitations in [the check record](audit-checks.md).

## Evidence baseline and concurrent integration

Evidence and the interpolation inventory describe `3a65c903`, even after this documentation is rebased onto local
main's quota integration `7a7a2f34e`. Open rich-copy work must re-inventory its Alert, account-name, ReasonText
and recovery routes and validate the five named exhausted/refused quota scenarios. Original counts and
captures do not certify the later 95-file change. Resume-by-ID subsequently integrated locally at `368a1f1f3`, followed by archived history (`501bd872d`) and
MCP defaults (`d6b69d18a`). The shared Alert and error router now include those current callers; their source
provenance stays separate from the original audit. Open rich-copy work must re-inventory the current source.

## Coordination

- Keep generated validation outputs in a task-specific `/tmp` directory. Do not commit additions or modifications
  under `evidence/`. Historical captures and probes are archived outside Git; evidence names in these records are
  archive identifiers. Recreate current-source measurements rather than treating those captures as current validation.
- Independent work may proceed before unrelated milestones close. Coordinate changes to shared useFocusHandoff,
  Button and collection contracts through the parent before editing them. Each worker writes only its assigned
  worktree and cleans only its own services. Milestone closure does not authorize integration into main.
- The accepted 03–13 split has no pending user confirmation. The shared Alert, semantic
  error map, rich/plain option contract and AgentTabList each need coordinated ownership; integrated quota and resume-by-ID
  callers are handed off through the coordinating session, not edited by this audit.

## Implementation decisions

- Do not introduce a broad lint/static-scanner framework. Use the full source inventory for manual closure and
  focused, meaningful regression tests at shared rich/plain parameter and Alert component contracts during fixes.
- Change-check briefs for 04–06 explicitly name the rule that names referenced in visible copy use InlineChip;
  no global skill/agent configuration change is needed.
