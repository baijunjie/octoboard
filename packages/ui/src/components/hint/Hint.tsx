import type React from "react";

import { PopoverHint } from "./hintPopover";
import { AnchoredHint, ControlHint, IndicatorHint } from "./hintTooltips";

export type ControlHintProps = React.ComponentProps<typeof ControlHint>;
export type IndicatorHintProps = React.ComponentProps<typeof IndicatorHint>;
export type AnchoredHintProps = React.ComponentProps<typeof AnchoredHint>;
type PopoverHintProps = React.ComponentProps<typeof PopoverHint>;

export type HintProps =
  | ({ kind: "control" } & ControlHintProps)
  | ({ kind: "indicator" } & IndicatorHintProps)
  | ({ kind: "anchored" } & AnchoredHintProps)
  | ({ kind: "popover" } & PopoverHintProps);

/**
 * The one component for every floating hint of the application, a tooltip or a popover, chosen by `kind`,
 * which is what carries the trigger:
 *
 * - `control`: a react-aria control (a HeroUI `Button`) is the trigger and takes the tooltip's trigger props
 *   from context.
 * - `indicator`: a non-control mark, wrapped in a trigger element of its own.
 * - `anchored`: the caller reports the hover and names the element the tooltip is placed against.
 * - `popover`: the content is interactive, a body that holds a control or text to select (a status mark's
 *   Try again and raw text, the rail's quota panel), and the caller draws the trigger from the wiring
 *   `Hint` hands it.
 *
 * Interactivity is decided by `kind`, never sniffed from the content: a caller whose content has something
 * to press or select says `popover`. The first three are tooltips (`role="tooltip"`, react-aria's
 * `Tooltip`), described to their trigger, opened by hover and keyboard focus and by nothing on a mouse
 * click, with a touch popover where the kind allows it. The fourth cannot be a tooltip, since a tooltip can
 * hold neither focus nor a button: it is a dialog popover, opened by a mouse resting on its trigger
 * (non-modal, no focus moved) and by a tap, a pen, assistive technology or the keyboard (modal, focus
 * inside), never by a mouse click. All four are drawn with one surface (`TOOLTIP_SURFACE` in
 * `hintSurface.ts`). Each kind's props and rules are documented where it is implemented: `hintTooltips.tsx`
 * and `hintPopover.tsx`.
 *
 * `TitledControl`, `IndicatorTooltip` and `AnchoredTooltip` are `Hint` of the first three kinds under
 * shorter names.
 */
export function Hint(props: HintProps): React.ReactElement | null {
  switch (props.kind) {
    case "control":
      return <ControlHint {...props} />;
    case "indicator":
      return <IndicatorHint {...props} />;
    case "anchored":
      return <AnchoredHint {...props} />;
    case "popover":
      return <PopoverHint {...props} />;
  }
}

export { HOVER_CLOSE_DELAY_MS, HOVER_OPEN_DELAY_MS, keepsEscapeFromPanel } from "./hintPopover";
