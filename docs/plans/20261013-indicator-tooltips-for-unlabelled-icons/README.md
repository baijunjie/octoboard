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
- **A graphic's tooltip says exactly what its accessible name says.** Where a catalog string already names the
  thing **on its own**, the tooltip reuses that string rather than a shorter one of its own, so the two cannot
  drift apart and no second set of keys has to be translated — this is how the session status glyph's tooltip is
  worded. Where the only string naming it folds it into the name of what carries it ("{name}, a session is
  waiting for you"), the tooltip gets its own key saying the same thing without the name, and the two keys
  carry a comment pointing at each other: reusing the folded string would make the tooltip repeat the row's
  visible name, and composing that name out of a fragment is worse to translate. That is how the activity
  marker's tooltip is worded.
- **Done includes the product doc.** `docs/product/labels-and-tooltips.md` is a closed list of the indicators that
  carry tooltips; each tooltip added by a milestone is added to it, and the state is shown and checked in the UI
  state gallery (owned by the `gallery-writer` subagent).

## Milestones

1. 01 session-status-glyph (closed)
2. 02 activity-marker (closed)
3. 03 confirmation-dialog-callout-icons (closed)
4. 04 callout-agent-icon (closed)
5. 05 rail-activity-badge (closed)
6. [The change row's status chip](06-change-row-status-chip.md)
7. [The branch badge](07-git-badge.md)

## Decisions the later milestones rest on

The four graphics left over from the audit were held back until these were settled. They are settled now, and each
milestone above is written to them.

- **A tooltip on a small mark inside a pressable row is acceptable**, which is what the change row's roughly 20 px
  letter needed answering. The binding badge on a session row and the creator badge in a branch list are smaller
  still and already carry one, and a tooltip is not a control, so a control's minimum pointer target does not apply.
- **The change row's letter takes the anchored shape, and the indicator shape is not available there.** A change row
  is a react-aria collection item whose left and right arrow keys walk every focusable descendant without filtering
  for tab stops, so the indicator shape's trigger would become a new arrow-key stop ahead of the row's ⋮ button and
  would open on keyboard focus — which these marks never do.
- **The branch badge gets one tooltip for the whole badge, not one per element.** Its accessible name is already one
  sentence about the glyph, the branch and both counts, so the tooltip is literally that name, with no new copy; the
  whole badge is then the hover target, and the branch name can still be read once the badge is squeezed down to its
  glyph and counts, which per-element tooltips could not offer. It replaces the two tooltips the badge carries today
  — the error's and the clipped branch name's — whose text that sentence contains.
- **The rail's activity badge says its meaning through the avatar's own tooltip**, not through one of its own. A
  tooltip on the disc would open exactly while the avatar's own tooltip is deliberately suppressed, beside the
  sidebar floating in, and would put a tooltip trigger inside a real button for the second time; folding the
  activity into the avatar's tooltip costs one expression and removes an exception from the product doc instead of
  adding one.
- **The agent icon in a confirmation's callout keeps the meaning it has elsewhere**, the account the session runs
  under, rather than repeating its screen-reader-only text. That icon's tooltip already has one settled meaning in
  the product, and the premise about a tooltip saying exactly what the accessible name says was written for marks
  whose tooltip is new, not to overturn an existing one.

None of the four depends on another, so they can be built in any order.
