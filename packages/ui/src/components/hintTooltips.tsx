import { Tooltip } from "@heroui/react";
import React, { useContext, useEffect, useId, useRef, useState } from "react";
import type { PressEvent } from "react-aria";
import { Popover as SurfacePopover, TooltipTriggerStateContext } from "react-aria-components";

import { interactionIsTouch } from "../touchInput";
import { OFFSET, PLACEMENT, TOOLTIP_SURFACE } from "./hintSurface";

// The tooltip kinds of `Hint` (see Hint.tsx): `control`, `indicator` and `anchored`, and the touch popover
// they open in place of a tooltip, which a finger has no hover to answer with.

/** The tooltip's own surface, for a shape to draw its text on. It is `pointer-events-none`, which
 * has no token, so every tooltip sets it itself: react-aria's tooltip otherwise stays open while it
 * is hovered and would swallow clicks aimed at what lies beneath it. `control-fills` because a
 * tooltip is portalled out of the pane or dialog that carries the fills. */
function TooltipSurface({
  placement = PLACEMENT,
  ...props
}: Omit<React.ComponentProps<typeof Tooltip.Content>, "className">): React.ReactElement {
  return <Tooltip.Content {...props} placement={placement} offset={OFFSET} className="control-fills pointer-events-none" />;
}

/** Whether a press of this pointer type opens the touch popover on the `control` kind: a finger's,
 * a pen's, or `virtual`, which is what assistive technology activating the control sends. A mouse
 * has a hover to answer with the tooltip, and a keyboard has focus, which shows it. */
function opensTouchPopover(pointerType: string | null | undefined): boolean {
  return pointerType === "touch" || pointerType === "pen" || pointerType === "virtual";
}

/** The open state of a touch popover and the element it is placed against, which the press that
 * opens it hands in. The shape keeps its tooltip shut while it is open (a pen hovering above the
 * screen opens a tooltip before it presses). `id` is the popover's text, which names the popover
 * where the trigger has no name. */
function useTouchPopover() {
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
function TouchPopover({
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
    <SurfacePopover
      isOpen={state.isOpen}
      onOpenChange={state.setOpen}
      triggerRef={state.anchor}
      placement={placement ?? PLACEMENT}
      offset={OFFSET}
      aria-label={label}
      aria-labelledby={label ? undefined : state.id}
      className={TOOLTIP_SURFACE}
    >
      <div id={state.id}>{children}</div>
    </SurfacePopover>
  );
}

/** Closes the tooltip it is rendered in (a child of `Tooltip`) as `active` turns true. A disabled
 * tooltip still stays open if it was, and the popover that took its place would then have the
 * tooltip's Escape listener ahead of its own, swallowing the key. */
function CloseTooltipWhile({ active }: { active: boolean }): null {
  const state = useContext(TooltipTriggerStateContext);
  useEffect(() => {
    if (active) state?.close(true);
  }, [active]);
  return null;
}

/** Gives a control a HeroUI tooltip, which shows on hover and on keyboard focus. HeroUI's buttons
 * are react-aria-components', which drop a native `title` prop (`filterDOMProps`). The child must
 * be a single react-aria-components control (a HeroUI `Button`, `CloseButton`, `Dropdown.Trigger`,
 * `Modal.CloseTrigger`); any other child silently gets no tooltip. Such a control picks the
 * tooltip's trigger behaviour up from context, so no element is added around it, which keeps a
 * mouse press from taking focus (`preventFocusOnPress` still holds) and leaves layout alone.
 * `Tooltip.Trigger` is deliberately not used: it renders a focusable `role="button"` wrapper.
 *
 * A tooltip is a label, not a surface to interact with: it closes the instant the pointer leaves
 * the trigger (`--tooltip-close-delay: 0s` in `style.css`, where HeroUI's default lingers), and it
 * is `pointer-events-none` (which has no token, so every tooltip sets it itself), because
 * react-aria's tooltip otherwise stays open while it is hovered and would swallow clicks aimed at
 * what lies beneath it. Nothing relies on a hoverable tooltip.
 *
 * While a tooltip is open react-aria links it to the control as its description
 * (`aria-describedby`), and VoiceOver speaks a description after the name. A tooltip that only
 * repeats the control's `aria-label` would thus be heard twice, so such a control is given an empty
 * `aria-describedby`, which wins over the tooltip's and refers to nothing. A tooltip that says more
 * than the name stays its description, and a control that sets its own `aria-describedby` keeps
 * it.
 *
 * A touch has no hover, so a tooltip never reaches it. With `touchPopover` a touch or pen press, or
 * an assistive technology's activation, opens the same text in a popover drawn like the tooltip
 * (`TOOLTIP_SURFACE`). A mouse press opens nothing, and neither does Enter or Space: keyboard focus
 * has the tooltip open already, and a press does not close it (as with `keepOpenOnPress`), so the
 * text stays on screen with no popover to close and hold the tooltip back for. It is off by
 * default, because a tap on a control is the control's action (an icon-only button, a menu
 * trigger); it is for a control whose only action is to show its text, such as `HelpTip`'s "?". The
 * control then has to take `onPress`, which it keeps: its own handler runs first. */
export function ControlHint({
  tooltip: title,
  children,
  placement,
  focusOnly,
  isDisabled,
  keepOpenOnPress,
  touchPopover = false,
  onOpenChange,
}: {
  /** What the tooltip says. */
  tooltip: React.ReactNode;
  children: React.ReactElement<{ "aria-label"?: string; "aria-describedby"?: string; onPress?: (event: PressEvent) => void }>;
  /** Where the tooltip opens relative to the control; HeroUI's default is above it. `end` is the
   * reading direction's, so a control on a vertical strip at the window's start edge opens it
   * toward the content under either direction. */
  placement?: "end";
  /** The tooltip opens on keyboard focus only, not on hover. */
  focusOnly?: boolean;
  /** Holds the tooltip back, closing it if open, while the control is showing what it says some other way,
   * such as in a popover the control opened. */
  isDisabled?: boolean;
  /** A press does not close the tooltip, for a control whose tooltip answers the press (a copy
   * button's changes to "Copied"). It still closes as the pointer or focus leaves. */
  keepOpenOnPress?: boolean;
  /** A touch, pen or virtual press opens the text in a popover drawn like the tooltip; see above. */
  touchPopover?: boolean;
  /** Told when the tooltip opens or closes. */
  onOpenChange?: (isOpen: boolean) => void;
}): React.ReactElement {
  const popover = useTouchPopover();
  if (!title) return children;
  const own = children.props;
  const repeatsName = own["aria-label"] === title && own["aria-describedby"] === undefined;
  const extra: Partial<typeof own> = {};
  if (repeatsName) extra["aria-describedby"] = "";
  if (touchPopover) {
    extra.onPress = (event) => {
      own.onPress?.(event);
      if (opensTouchPopover(event.pointerType)) popover.open(event.target);
    };
  }
  return (
    <>
      <Tooltip trigger={focusOnly ? "focus" : undefined} isDisabled={isDisabled || popover.isOpen} shouldCloseOnPress={keepOpenOnPress || touchPopover ? false : undefined} onOpenChange={onOpenChange}>
        {Object.keys(extra).length > 0 ? React.cloneElement(children, extra) : children}
        <TooltipSurface placement={placement}>{title}</TooltipSurface>
        {touchPopover && <CloseTooltipWhile active={popover.isOpen} />}
      </Tooltip>
      {touchPopover && <TouchPopover state={popover} label={own["aria-label"]} placement={placement}>{title}</TouchPopover>}
    </>
  );
}

/**
 * A tooltip on a non-control indicator (a badge, a mark): it shows on pointer hover only, and a
 * touch or pen tap on it opens the same content in a popover drawn like the tooltip
 * (`TOOLTIP_SURFACE`), since a finger has no hover. The indicator is no control, so it is no tab
 * stop (`tabIndex={-1}`, which `Tooltip.Trigger` lets a prop override: its own is a focusable
 * `role="button"` `div`) and a press does not focus it, which would take focus off the terminal.
 * That is why it is `Tooltip.Trigger` rather than the `control` kind, which needs a
 * react-aria-components control as its child. With a `label` the trigger is a named `role="img"`;
 * without one it is `role="presentation"`, for an indicator whose meaning reaches assistive
 * technology some other way (the row's own accessible name); browsers ignore that role on a
 * focusable element, so the trigger is still exposed as a generic one, which is harmless.
 * `className` lays out the trigger around `children`.
 *
 * The trigger takes no `aria-hidden`: `role="presentation"` keeps the trigger itself out of the
 * way, but not what it wraps, so an indicator carrying text rather than a hidden glyph has to hide
 * its own contents, in a wrapper that leaves them the trigger's own layout children (`aria-hidden`
 * on a `display: contents` span). Nothing inside such a wrapper may be focusable.
 *
 * The `tooltip` is any content, for an indicator whose detail has a structure; the `label` stays a
 * plain string, so the accessible name never depends on the tooltip's markup.
 *
 * `touchPopover` is on by default, as a tap on a mark has no action of its own. It has to be turned
 * off for a mark inside something that handles the same tap (a row, a button, an option), or the
 * popover would open over what the press does; the caller says why where it does. A mark that opens
 * the popover is a touch target, so on a device with a coarse pointer its hit area grows to 24 by
 * 24 pixels (`touch-target` in `style.css`) without changing what is drawn.
 *
 * One use is not an indicator: focus mode's include-console-sessions switch, a control whose hover
 * is sent to a hidden input, so it takes no tooltip trigger from context and the `control` kind cannot
 * serve it. Its trigger is this one, with the popover off because a tap on the switch toggles it.
 * Cancelling the mouse press's focus (`onMouseDown`) does not stop the switch: react-aria's press
 * takes its own focus and toggles on the click.
 */
export function IndicatorHint({
  tooltip,
  label,
  className,
  touchPopover = true,
  children,
}: {
  tooltip: React.ReactNode;
  label?: string;
  className?: string;
  /** A touch or pen tap opens the tooltip's content in a popover; see above. */
  touchPopover?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  const popover = useTouchPopover();
  const naming = label ? ({ role: "img", "aria-label": label } as const) : ({ role: "presentation" } as const);
  // The click that ends a press does not carry the pointer; what the interaction came from is
  // recorded at its pointerdown.
  const touch = touchPopover ? { onClick: (event: React.MouseEvent<HTMLElement>) => interactionIsTouch() && popover.open(event.currentTarget) } : undefined;
  return (
    <>
      <Tooltip isDisabled={popover.isOpen}>
        <Tooltip.Trigger
          {...naming}
          {...touch}
          tabIndex={-1}
          className={touchPopover ? `touch-target ${className ?? ""}` : className}
          onMouseDown={(event) => event.preventDefault()}
        >
          {children}
        </Tooltip.Trigger>
        <TooltipSurface>{tooltip}</TooltipSurface>
      </Tooltip>
      {touchPopover && <TouchPopover state={popover} label={label}>{tooltip}</TouchPopover>}
    </>
  );
}

/**
 * A tooltip on something that cannot be a tooltip's trigger: a run of text, a mark inside a row
 * that walks its focusable descendants with the arrow keys, where a trigger of its own would become
 * a stop in that walk and open on focus, or a control that cannot take a trigger nested in it (a
 * tab). None of them takes the trigger props a control picks up from context, and that walk reaches
 * whatever is wrapped around the mark as well. So the caller reports the hover itself and names the
 * element the tooltip is placed against, which may be an ancestor of what it belongs to — the row a
 * mark sits in.
 *
 * Nothing is rendered while the pointer is away: no state, no portal, no measuring. That is what
 * lets every clipped label in the window carry one.
 *
 * The open state is react-aria's own, opened as this mounts and closed as it unmounts, which is
 * what a trigger element does on the pointer's way in and out. So the wait before it shows is the
 * app's, one tooltip at a time still holds across the window, and every path react-aria closes a
 * tooltip on applies — above all the one that closes it when something under it scrolls. Driving
 * `isOpen` from the hover flag instead would silently disable them all: react-aria's own closes
 * would change nothing, and the position is recomputed on a resize alone, so scrolling a list by
 * less than a row with the pointer at rest would leave the tooltip over an unrelated one. Once it
 * has closed, what re-opens it is the pointer leaving and coming back, which is what unmounts and
 * mounts this again, or the caller's `key` changing.
 *
 * Hover is the way in, as the anchor is usually no tab stop and this adds no element of its own. The
 * caller of an anchor that is a tab stop may report its keyboard focus through `hovered` as well, and
 * `immediate` then skips the wait; re-opening then also comes from a change of `key` the caller makes
 * as focus arrives. Nothing links the tooltip to the anchor for assistive technology. What it says has
 * to reach a screen reader another way: the text it belongs to, which is in the document whether or not
 * it is clipped, the name of the row it is in, or the control's own name.
 *
 * It never opens a touch popover: with no trigger of its own there is nothing to tap, and a tap on
 * the row it sits in is the row's action.
 *
 * With no trigger element, react-aria holds no Escape listener for it either, so the key is not
 * swallowed while it is open. It is marked `data-anchored-tooltip` for the code that tells a
 * tooltip which swallows the key from one that does not.
 */
export function AnchoredHint({
  tooltip,
  anchor,
  hovered,
  placement,
  immediate = false,
}: {
  tooltip: React.ReactNode;
  /** The element the tooltip is placed against. */
  anchor: React.RefObject<Element | null>;
  /** Whether the tooltip shows: the pointer is on what it belongs to, or, for a caller that reports it, the
   * control has keyboard focus. */
  hovered: boolean;
  /** Where it opens relative to the anchor; HeroUI's default is above it. `end` is the reading
   * direction's, so it is mirrored under right-to-left. */
  placement?: "end";
  /** Show with no wait, as a tooltip does when its control takes keyboard focus. Read as this opens. */
  immediate?: boolean;
}): React.ReactElement | null {
  if (!hovered) return null;
  return (
    <Tooltip>
      <OpenWhileMounted immediate={immediate} />
      <TooltipSurface triggerRef={anchor} placement={placement} data-anchored-tooltip="">
        {tooltip}
      </TooltipSurface>
    </Tooltip>
  );
}

/** Reports to the tooltip's own state the hover that a trigger element would have reported. */
function OpenWhileMounted({ immediate }: { immediate: boolean }): null {
  const state = useContext(TooltipTriggerStateContext);
  useEffect(() => {
    state?.open(immediate);
    // A press can cover the anchor with an overlay without sending pointerleave. Close the
    // tooltip, including its pending opening, before the press reaches that overlay's trigger.
    const close = () => state?.close(true);
    document.addEventListener("pointerdown", close, true);
    document.addEventListener("keydown", close, true);
    // Closing on the way out is not optional: whether a tooltip has just been shown is tracked
    // globally, over every tooltip in the window, and one that goes away without closing leaves
    // that state standing, so the next tooltip to open shows with no delay.
    return () => {
      document.removeEventListener("pointerdown", close, true);
      document.removeEventListener("keydown", close, true);
      state?.close(true);
    };
    // Mount and unmount alone: opening again would undo a close react-aria made for a reason of
    // its own, with the pointer still where it was.
  }, []);
  return null;
}
