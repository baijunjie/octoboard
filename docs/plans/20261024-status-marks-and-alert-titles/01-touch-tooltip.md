# 01 Tooltip wrapper that works on touch

> Goal: every tooltip in the window goes through one wrapper, which shows a tooltip to a mouse and keyboard and, where
> the trigger has no action of its own, a tooltip-styled popover to a touch press; `HelpTip` rebuilt on it.
> Completion criteria: on a mouse, hover and keyboard focus open the tooltip as today in every shape; a touch press on a
> mark (and on `HelpTip`'s "?") opens the same content in a popover drawn like the tooltip, closed by an outside tap or
> Escape; a touch press on an icon-only button or a row runs its action and opens no popover; no tooltip is rendered
> from HeroUI's `Tooltip` outside the wrapper; `HelpTip` has no separate press popover on a mouse and none of its
> tooltip/popover hold-back logic; the UI standard states the rule; unit tests cover both inputs per shape, and the
> affected gallery scenarios pass the quick check.

## Technical design

- [ ] One shared core with three shapes, keeping today's names: the control shape (`TitledControl`, a react-aria
  control child), the mark shape (`IndicatorTooltip`, a non-control indicator) and the anchored shape
  (`AnchoredTooltip`, hover handed in by the caller). Each keeps its current options.
- [ ] A touch-popover parameter on the control and mark shapes; default off for the control shape, on for the mark
  shape. The anchored shape has none: it never opens a touch popover (see the overview's design decision for why
  automatic recognition was rejected).
- [ ] Which surface opens is decided by the input of the interaction: a touch (or pen) press opens the popover where
  the parameter allows it; mouse hover and keyboard focus open the tooltip; a mouse press opens nothing extra.
- [ ] The touch popover shares the tooltip's visual definition (surface, padding, text size, max width, arrow) from one
  source, so the two cannot drift apart.
- [ ] Accessibility: the trigger keeps its accessible name; the content is its description while open, as now; the
  touch popover is dismissible by an outside tap and Escape and returns focus to the trigger if it took it.
- [ ] A mark that becomes a touch target meets the pointer target size rule.

## Implementation plan

- [ ] Extract the shared core and rebuild the three shapes on it; route the one remaining direct HeroUI `Tooltip` use
  through a shape.
- [ ] Rebuild `HelpTip` as the control shape around its "?" with the touch popover on; remove its press popover on a
  mouse and its hold-back logic.
- [ ] Check each mark-shape caller (status glyph, Git badge, binding and branch-origin badges, session metadata,
  activity marker) on touch: the popover must not fight a press the surrounding row or button handles.
- [ ] Update `docs/ui-accessibility.md` ("An icon-only control: tooltip and `aria-label`", "A help tip", "A tooltip on
  a mark that is not a control"): one wrapper, behaviour by input; a touch popover drawn like the tooltip on marks and
  on the "?", none on a trigger with its own action.

## Notes for the developer

- **Reusable capabilities**: touch detection (`interactionIsTouch`, `CANNOT_HOVER`); the existing tooltip test support;
  `useEscapeWhileTooltipOpen` for Escape handling in dialogs.
- **Development notes**: tooltip copy rules are in `docs/product/labels-and-tooltips.md`; the control shape adds no
  element around its control and keeps `preventFocusOnPress`, and tooltips stay `pointer-events-none` — the touch
  popover is a separate surface and must not change either.
- **Reference docs**: `docs/ui-accessibility.md` ("An icon-only control: tooltip and `aria-label`", "A help tip", "A
  tooltip on a mark that is not a control", "Pointer target size").
