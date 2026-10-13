# Tooltips for presentational graphics that have none

## Problem

A purely presentational icon, one with no handler of its own, shows its meaning only through a tooltip: the user
already knows a control does something, but an icon's meaning is in its tooltip, and a user who does not know it
has one will never hover long enough to find out. So such an icon should carry a tooltip. The project's non-control
indicator shape (`IndicatorTooltip`) is the existing way to give it one, and the four presentational icons that had
tooltips use it. Every tooltip in the app waits the same default delay, controls and indicators alike.

An audit against the rule found the real gap: several purely presentational graphics have no tooltip at all. Their
meaning is reachable only through an `aria-label`, so a sighted user has no way to learn what they mean. Each is an
`aria-hidden` or `role="img"` element with no handler of its own; where one sits inside a pressable row, the press
belongs to the row.

## Plan outline

Give each such graphic a tooltip, using the sanctioned shape that fits where it sits: the indicator shape for a mark
that can be wrapped where it is, and the anchored shape (`AnchoredTooltip`) for a mark inside a react-aria collection
item. Both wait the app's default delay. The graphics are grouped by what they signify, so that each milestone settles
one piece of wording and one set of placements.

## Key design decisions

- **One delay for every tooltip.** A tooltip on a graphic waits the same default delay as a control's, so sweeping the
  pointer down a dense list does not pop a tooltip per row. This is the premise, not a choice to revisit per graphic.
- **Where a visible text already names the thing, no tooltip.** Focus mode's card shows the status icon beside a
  visible status label, so that placement is left as it is.
- **The tooltip adds to, and does not replace, the accessible name.** The meaning keeps reaching assistive
  technology through the name of the row, chip or badge the graphic sits in, as for the existing indicator
  tooltips; the tooltip is for sighted users and shows on mouse hover only, since the graphic is not a tab stop.
- **Copy goes through the message catalog in both `en` and `zh-Hans`.**
- **Done includes the product doc.** `docs/product/labels-and-tooltips.md` is a closed list of the indicators that
  carry tooltips; each tooltip added by a milestone is added to it, and the state is shown and checked in the UI
  state gallery (owned by the `gallery-writer` subagent).

## Milestones

1. [Session status glyph](01-session-status-glyph.md)
2. [Activity marker](02-activity-marker.md)
3. [Confirmation dialog callout icons](03-confirmation-dialog-callout-icons.md)

## Not yet written

Three further graphics are left out until the design questions below are answered; each would leave its milestone's
goal or completion criteria unclear.

- **The change row's single-letter status chip** (M, A, D, R and the rest, in the change list and the change tree).
  It sits inside a pressable react-aria collection item (`GridListItem` / `TreeItem`), so an `AnchoredTooltip`
  anchored to an ancestor is the fitting shape. The unanswered question is whether a tooltip on a
  roughly 20 px letter inside a pressable row is acceptable at all, or whether this target needs rethinking. If the
  target changes, the milestone's goal changes with it.
- **The git badge** (its state glyph: checking, syncing, detached HEAD or a branch; and its ahead and behind counts).
  The unanswered question is one tooltip for the whole badge (a branch-state sentence, probably the existing
  `aria-label` text) or one per element, which needs the glyph and each count wrapped separately. The branch name
  beside them is a clipped-text tooltip of its own with the normal delay, which a whole-badge tooltip would overlap.
  The two answers give different placements and different completion criteria.
- **The activity badge on a console avatar in the rail** (the corner disc, the same activity as milestone 2). It
  conflicts with the tile it sits on: the tile is a control whose tooltip names the console and is delayed, and that
  tile tooltip opens on keyboard focus only while the hidden sidebar floats in on hover. A tooltip on the disc would
  therefore appear exactly when the tile's own tooltip is deliberately suppressed. Whether that is
  acceptable, and what the disc should do instead, is undecided.

## Open

These do not change the milestone split or the completion criteria of the written milestones.

- **Whether the status tooltip text is simply the existing `aria-label` string.** Milestone 1 is where this is
  decided for the status glyph, and milestone 3 follows it. The same question applies to the activity marker's
  wording in milestone 2.
- **What the confirmation dialog's agent icon says.** It already has an `sr-only` name; whether it also gets a
  tooltip, and with what text (for instance the same account wording the agent icon on a session row uses), is not
  settled. Milestone 3 covers the status icon and leaves this to be decided when it is built.
