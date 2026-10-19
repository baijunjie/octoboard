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
3. Fix what the audit found, in milestones split from its findings; moving every Alert onto HeroUI's stock Alert is
   one of them.

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
- **The Alert loses its custom styling** (user decision): the shared error/tinted wrappers' own fills, text sizes,
  shadow overrides and custom content structure are removed, and every Alert uses HeroUI's Alert with its original
  styles and structure (status, indicator, content, title, description). The custom content structure is why the
  indicator keeps landing out of line with the text and had to be patched per call site. HeroUI's description cannot
  hold paragraphs, lists or diagnostic blocks; with the recovery moved out of the Alert, what stays inside is a
  title and a short description, so the stock structure suffices. The standard's sentences that describe the custom
  wrapper (the danger-soft fill, a heading-element title, block content inside the callout) are rewritten to match.
- **Fixes are split after the audit**, by area or by rule as the findings suggest, so each fix milestone stays
  independently reviewable in the gallery.

## Milestones

1. [Write the preview's rules into the UI standard](01-write-preview-rules-into-the-standard.md)
2. [Audit the UI against the standard](02-audit-the-ui.md)

## Open

- How the fix milestones are split (by area or by rule) is decided from the audit's findings.
