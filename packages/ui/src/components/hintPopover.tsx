import { Popover } from "@heroui/react";
import React, { useEffect, useId, useRef, useState } from "react";
import type { PressEvent } from "react-aria";
import { Dialog, Popover as SurfacePopover } from "react-aria-components";

import { isImeKey } from "../imeKey";
import { keepsEscape } from "../layout/usePaneToggles";
import { OFFSET, PLACEMENT, TOOLTIP_SURFACE } from "./hintSurface";
import { useStableHandler } from "./useLatest";

/** How long the pointer rests on the trigger before the panel floats in. */
export const HOVER_OPEN_DELAY_MS = 250;
/** How long the pointer may be off both the trigger and the panel before the panel goes, so that
 * crossing the gap between them does not close it. */
export const HOVER_CLOSE_DELAY_MS = 150;

/** Whether `target` is inside either of `elements`. */
function insideAny(target: EventTarget | null, ...elements: Array<Element | null>): boolean {
  return target instanceof Node && elements.some((element) => element?.contains(target));
}

/** Whether Escape aimed at `target` belongs to it rather than to a panel that holds no focus: what
 * keeps it from a floating pane (`keepsEscape`: a shell terminal, a text field with text in it), and
 * any terminal, the agent's too, whose program takes Escape as an interrupt. */
export function keepsEscapeFromPanel(target: Element | null): boolean {
  return keepsEscape(target) || target?.closest(".xterm") != null;
}

/**
 * A floating panel that a mouse resting on its trigger opens, without moving focus, and that stays
 * open while the pointer is on the trigger or the panel: the timers, the trigger's hover handlers,
 * the panel's pointer enter and leave, and the document's Escape and outside-press watch, which
 * react-aria's non-modal popover does not do itself.
 *
 * Only a mouse hovers (touch and pen never open it). `hovered` is whether the hover has opened the
 * panel; `pressed` is whether a press holds it open instead, in its own modal form, so that leaving
 * does not close it.
 *
 * The watch's Escape is the panel's only where focus is not in a target that keeps the key
 * (`keepsEscapeFromPanel`), and never while an input method is composing; `yieldsEscape` lets a
 * caller stand aside for more. The key the panel takes is consumed, so it neither reaches the
 * terminal nor closes a drawer.
 * An outside press is one beyond the trigger, the panel and what `isInside` adds. Both end in
 * `onDismiss`.
 */
function useHoverPopover({
  pressed = false,
  isInside,
  yieldsEscape,
  onDismiss,
}: {
  pressed?: boolean;
  isInside?: (target: EventTarget | null) => boolean;
  yieldsEscape?: (event: KeyboardEvent) => boolean;
  onDismiss: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const openTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const closeTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const cancelTimers = () => {
    clearTimeout(openTimer.current);
    clearTimeout(closeTimer.current);
  };
  useEffect(() => cancelTimers, []);
  /** Shuts the hover-opened panel at once. */
  const close = useStableHandler(() => {
    cancelTimers();
    setHovered(false);
  });
  const dismiss = useStableHandler(onDismiss);
  const check = useStableHandler((event: KeyboardEvent | PointerEvent) => {
    if (event instanceof KeyboardEvent) {
      if (event.key !== "Escape" || isImeKey(event) || yieldsEscape?.(event)) return;
      if (keepsEscapeFromPanel(event.target instanceof Element ? event.target : null)) return;
      event.preventDefault();
      event.stopPropagation();
      dismiss();
      return;
    }
    if (insideAny(event.target, trigger.current, panel.current) || isInside?.(event.target)) return;
    dismiss();
  });

  useEffect(() => {
    if (!hovered) return;
    document.addEventListener("pointerdown", check, true);
    document.addEventListener("keydown", check, true);
    return () => {
      document.removeEventListener("pointerdown", check, true);
      document.removeEventListener("keydown", check, true);
    };
  }, [hovered]);

  const onMouseHoverChange = (isHovered: boolean) => {
    cancelTimers();
    if (isHovered) {
      if (!hovered && !pressed) openTimer.current = setTimeout(() => setHovered(true), HOVER_OPEN_DELAY_MS);
    } else if (!pressed) {
      closeTimer.current = setTimeout(() => setHovered(false), HOVER_CLOSE_DELAY_MS);
    }
  };

  return {
    hovered,
    trigger,
    panel,
    close,
    onMouseHoverChange,
    /** For the panel's element: the pointer inside it keeps it open. */
    panelProps: {
      onPointerEnter: (event: React.PointerEvent) => {
        if (event.pointerType === "mouse") clearTimeout(closeTimer.current);
      },
      onPointerLeave: (event: React.PointerEvent) => {
        if (event.pointerType !== "mouse" || pressed) return;
        closeTimer.current = setTimeout(() => setHovered(false), HOVER_CLOSE_DELAY_MS);
      },
    },
  };
}

/** How the surface is open: `hover` is a mouse resting on the trigger (no focus moved, non-modal),
 * `press` is a tap, a pen, assistive technology or the keyboard (focus inside, modal). */
type HoverPopoverForm = "hover" | "press";

/** What the trigger is wired with: spread on a react-aria button (`ref`, `preventFocusOnPress`,
 * `onPress`, `onHoverStart`, `onHoverEnd`), or picked from for a wrapper that names them its own way. */
interface HoverPopoverTrigger {
  ref: React.RefObject<HTMLButtonElement | null>;
  /** Nothing a press does takes focus: a mouse click does nothing, and a tap must not pull focus off
   * the terminal or an editor. */
  preventFocusOnPress: true;
  onPress: (event: PressEvent) => void;
  onHoverStart: (event: { pointerType: string }) => void;
  onHoverEnd: (event: { pointerType: string }) => void;
  onMouseHoverChange: (hovered: boolean) => void;
  expanded: boolean;
  /** The id of the surface's dialog. */
  popupId: string;
}

/**
 * The `popover` kind of `Hint`: a floating surface for interactive content, a body that holds a control or
 * text to select. A tooltip is `role="tooltip"`, described to its trigger, and can hold neither focus nor a
 * control, so this is a dialog popover drawn on the same surface (`TOOLTIP_SURFACE`).
 *
 * Inputs: a mouse resting on the trigger opens it without moving focus (non-modal, `aria-modal="false"`,
 * `data-escape-topmost`); a mouse click opens nothing; a tap, a pen, assistive technology and the keyboard
 * open it with focus inside (modal), and such a press on a hover-opened surface turns it into that form.
 * After any close but the grace's own, a pointer still resting on the trigger does not open it again until
 * it has left and come back. What each says is in `docs/ui-accessibility.md`.
 *
 * Props:
 * - `trigger` draws the trigger from the wiring it is handed.
 * - `children` is the body, given a function that closes the surface, the form it is open in and the id of
 *   its dialog.
 * - `label`, or `labelledBy` for a body that names itself, names the dialog.
 * - `placement` and `offset` default to the tooltip's; `contentClassName` and `bodyClassName` style the
 *   surface and its body; `contentProps` adds `data-*` attributes (a marker for what watches presses
 *   outside itself).
 * - `isInside` and `yieldsEscape` are for a surface that holds others (see `useHoverPopover`).
 * - `onOpenChange` is told when it opens or closes, `onClosed` when it closes and whether it was the press form.
 */
export function PopoverHint({
  trigger,
  label,
  labelledBy,
  placement,
  offset,
  contentClassName,
  bodyClassName,
  contentProps,
  isInside,
  yieldsEscape,
  onOpenChange,
  onClosed,
  children,
}: {
  trigger: (wiring: HoverPopoverTrigger) => React.ReactElement;
  label?: string;
  labelledBy?: string;
  placement?: React.ComponentProps<typeof Popover.Content>["placement"];
  offset?: number;
  contentClassName?: string;
  bodyClassName?: string;
  /** Extra attributes on the floating element (a marker for what watches presses outside itself). */
  contentProps?: Record<`data-${string}`, string>;
  isInside?: (target: EventTarget | null) => boolean;
  yieldsEscape?: (event: KeyboardEvent) => boolean;
  onOpenChange?: (open: boolean) => void;
  onClosed?: (closed: { wasPressed: boolean; trigger: HTMLElement | null }) => void;
  children: (surface: { close: () => void; form: HoverPopoverForm; popupId: string }) => React.ReactNode;
}): React.ReactElement {
  const [pressed, setPressed] = useState(false);
  const popupId = useId();
  // Set by a close that is not the grace's own (Escape, an outside press, an action): the pointer may
  // still be resting on the trigger, where the hover would open it again. The hover is armed again once
  // a move of the pointer is seen outside the trigger and the surface.
  const rested = useRef(false);
  const hover = useHoverPopover({ pressed, isInside, yieldsEscape, onDismiss: () => close() });
  const { trigger: triggerRef, panel } = hover;
  const mode: HoverPopoverForm | null = pressed ? "press" : hover.hovered ? "hover" : null;
  const open = mode !== null;
  // The form it is drawn in stays as it was through its exit: a swap to the press form then would mount
  // react-aria's dialog, which takes focus as it mounts.
  const [form, setForm] = useState<HoverPopoverForm>("press");
  if (mode && mode !== form) setForm(mode);

  const close = useStableHandler(() => {
    rested.current = true;
    hover.close();
    setPressed(false);
    onClosed?.({ wasPressed: pressed, trigger: triggerRef.current });
  });
  useEffect(() => onOpenChange?.(open), [open]);

  useEffect(() => {
    if (open || !rested.current) return;
    const onPointerMove = (event: PointerEvent) => {
      if (insideAny(event.target, triggerRef.current, panel.current)) return;
      rested.current = false;
      document.removeEventListener("pointermove", onPointerMove, true);
    };
    document.addEventListener("pointermove", onPointerMove, true);
    return () => document.removeEventListener("pointermove", onPointerMove, true);
  }, [open]);

  const onPress = (event: PressEvent) => {
    // Where a pointer can hover, the surface is the hover's: a mouse click does nothing (and does not
    // cancel a pending hover-open).
    if (event.pointerType === "mouse") return;
    if (pressed) {
      close();
      return;
    }
    // A press of another kind on a hover-opened surface turns it into the press-opened one.
    hover.close();
    setPressed(true);
  };
  const onMouseHoverChange = (hovered: boolean) => {
    if (hovered && rested.current) return;
    hover.onMouseHoverChange(hovered);
  };
  const naming = labelledBy ? { "aria-labelledby": labelledBy } : { "aria-label": label };
  return (
    // The popover root only supplies the panel's styling and the trigger's popup attributes: the state is
    // held here, since the trigger's own press would toggle it where this opens by the kind of press.
    <Popover isOpen={open} onOpenChange={() => {}}>
      {trigger({
        ref: triggerRef,
        preventFocusOnPress: true,
        onPress,
        onHoverStart: (event) => event.pointerType === "mouse" && onMouseHoverChange(true),
        onHoverEnd: (event) => event.pointerType === "mouse" && onMouseHoverChange(false),
        onMouseHoverChange,
        expanded: open,
        popupId,
      })}
      {/* Keyed by the form so that a turn from hover to press mounts the overlay afresh: its focus scope
          then records the trigger as the place to give focus back to, as for a press from scratch,
          rather than whatever held focus while the surface was hover-opened. */}
      <SurfacePopover
        key={form}
        isOpen={open}
        isNonModal={form === "hover"}
        triggerRef={triggerRef}
        placement={placement ?? PLACEMENT}
        offset={offset ?? OFFSET}
        {...contentProps}
        data-escape-topmost={mode === "hover" ? "" : undefined}
        onOpenChange={(isOpen) => {
          if (!isOpen) close();
        }}
        className={`${TOOLTIP_SURFACE} ${contentClassName ?? ""}`}
      >
        {form === "hover" ? (
          // Focusable but no tab stop: a mouse press on the panel's text has to leave focus in the panel, or a
          // focus-containing dialog it floats over (react-aria's `FocusScope contain`) takes focus back a frame
          // later and the drag-selection collapses.
          <div ref={panel} id={popupId} role="dialog" tabIndex={-1} {...naming} aria-modal="false" className={`outline-none ${bodyClassName ?? ""}`} {...hover.panelProps}>
            {children({ close, form, popupId })}
          </div>
        ) : (
          <Dialog id={popupId} {...naming} className={`outline-none ${bodyClassName ?? ""}`}>
            {children({ close, form, popupId })}
          </Dialog>
        )}
      </SurfacePopover>
    </Popover>
  );
}
