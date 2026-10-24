import { Tooltip } from "@heroui/react";
import React, { useContext, useEffect, useId, useRef, useState } from "react";
import { Popover, TooltipTriggerStateContext } from "react-aria-components";

/**
 * What the three tooltip shapes (`TitledControl`, `IndicatorTooltip`, `AnchoredTooltip`) share, so
 * that they cannot drift apart. They differ only in what carries the trigger; the surface the text
 * is drawn on, and the popover a touch press opens in its place, come from here.
 *
 * Which of the two shows is decided by the input of the interaction, not by the device: a mouse
 * hover or keyboard focus opens the tooltip, and a press opens the popover where the shape allows
 * it: on the control shape a touch, pen or assistive-technology (virtual) press, on the mark shape
 * a touch or pen tap, on a touch-screen laptop or a tablet with a trackpad as on a phone. A mouse
 * press and a keyboard press open nothing extra.
 */

/** Where a tooltip opens relative to what it belongs to, and how far from it. Both are HeroUI's own
 * defaults for a tooltip with no arrow, spelled out so the touch popover takes the same ones. */
const PLACEMENT = "top";
const OFFSET = 3;

/** The tooltip's own surface, for a shape to draw its text on. It is `pointer-events-none`, which
 * has no token, so every tooltip sets it itself: react-aria's tooltip otherwise stays open while it
 * is hovered and would swallow clicks aimed at what lies beneath it. `control-fills` because a
 * tooltip is portalled out of the pane or dialog that carries the fills. */
export function TooltipSurface({
  placement = PLACEMENT,
  ...props
}: Omit<React.ComponentProps<typeof Tooltip.Content>, "className">): React.ReactElement {
  return <Tooltip.Content {...props} placement={placement} offset={OFFSET} className="control-fills pointer-events-none" />;
}

/** Whether a press of this pointer type opens the touch popover on the control shape: a finger's,
 * a pen's, or `virtual`, which is what assistive technology activating the control sends. A mouse
 * has a hover to answer with the tooltip, and a keyboard has focus, which shows it. */
export function opensTouchPopover(pointerType: string | null | undefined): boolean {
  return pointerType === "touch" || pointerType === "pen" || pointerType === "virtual";
}

/** The open state of a touch popover and the element it is placed against, which the press that
 * opens it hands in. The shape keeps its tooltip shut while it is open (a pen hovering above the
 * screen opens a tooltip before it presses). `id` is the popover's text, which names the popover
 * where the trigger has no name. */
export function useTouchPopover() {
  const [isOpen, setOpen] = useState(false);
  const anchor = useRef<Element | null>(null);
  const id = useId();
  const open = (target: Element) => {
    anchor.current = target;
    setOpen(true);
  };
  return { isOpen, setOpen, anchor, id, open };
}

/**
 * The popover a touch press opens in place of a tooltip, drawn like it: it carries HeroUI's own
 * `tooltip` class, so the surface, padding, text size, width cap and word breaking are the
 * tooltip's one definition, not a copy of it, and so are the fills a dialog gives it (`style.css`).
 * It has no arrow because the tooltip has none.
 *
 * It is modal, as a popover is: a tap outside closes it (and is not passed on to what lies
 * there), so does Escape, and focus, which it takes onto itself, goes back to what held it. It has
 * a dialog's role, so it is named: `label` is the trigger's accessible name, and a mark with none is
 * named by the text itself. Being modal it hides the trigger from assistive technology while open,
 * so it is not linked to the trigger as its description, as the tooltip is.
 */
export function TouchPopover({
  state,
  label,
  placement,
  children,
}: {
  state: ReturnType<typeof useTouchPopover>;
  label?: string;
  placement?: "end";
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Popover
      isOpen={state.isOpen}
      onOpenChange={state.setOpen}
      triggerRef={state.anchor}
      placement={placement ?? PLACEMENT}
      offset={OFFSET}
      aria-label={label}
      aria-labelledby={label ? undefined : state.id}
      className="tooltip control-fills outline-none"
    >
      <div id={state.id}>{children}</div>
    </Popover>
  );
}

/** Closes the tooltip it is rendered in (a child of `Tooltip`) as `active` turns true. A disabled
 * tooltip still stays open if it was, and the popover that took its place would then have the
 * tooltip's Escape listener ahead of its own, swallowing the key. */
export function CloseTooltipWhile({ active }: { active: boolean }): null {
  const state = useContext(TooltipTriggerStateContext);
  useEffect(() => {
    if (active) state?.close(true);
  }, [active]);
  return null;
}
