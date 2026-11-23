import React, { cloneElement, createContext, useContext, useEffect, useLayoutEffect, useRef, useState } from "react";
import { mergeProps, useHover } from "react-aria";
import { getInteractionModality, isFocusVisible } from "react-aria/private/interactions/useFocusVisible";
import { TooltipContext, useSlottedContext } from "react-aria-components";

import { usePrimitiveLabels } from "./labels";
import { FloatingPopover } from "./Popover";
import { TooltipContent, TooltipRoot, TooltipTrigger, tooltipSurfaceClassName, useTooltipState } from "./tooltipParts";

// Hover wiring for a mark whose tooltip is anchored independently of its keyboard focus.
export { useHover as useTooltipHover };

/** Where a tooltip opens relative to what it belongs to, and how far from it. Both are HeroUI's own defaults for a
 * tooltip with no arrow, spelled out so the popovers drawn as a tooltip (the touch popover, the hover popover) take the
 * same ones. */
export const TOOLTIP_PLACEMENT = "top";
export const TOOLTIP_OFFSET = 3;

/** What a tooltip says. Empty (`undefined`, `null`, `false` or `""`) is no tooltip: the `Tooltip` keeps its place in the
 * tree and holds itself back, so that a control whose tooltip comes and goes is not remounted, which would drop its
 * focus. */
const hasContent = (content: React.ReactNode): boolean => content !== undefined && content !== null && content !== false && content !== "";

interface TooltipBaseProps {
  /** What the tooltip says: text, or markup for a tooltip whose detail has a structure (`HeadedTooltip`). */
  content: React.ReactNode;
}

export type ControlTooltipPlacement = "end" | "end top" | "bottom";

interface ControlTooltipProps extends TooltipBaseProps {
  trigger?: "control";
  /** The control, a single react-aria control that takes the tooltip's trigger behaviour from context (a `ToggleButton`, a
   * `MenuTrigger`): no element is added around it. A `Button`, a `Switch` and a `Checkbox` carry their own tooltip
   * (their `tooltip` and `unavailable` props) and are never wrapped in one. */
  children: React.ReactElement<{ label?: string; describedBy?: string }>;
  /** Where the tooltip opens relative to the control; HeroUI's default is above it. `end` is the reading direction's, so a
   * control on a vertical strip at the window's start edge opens it toward the content under either direction.
   * `end top` aligns the top of the tooltip with the control. `bottom` keeps a header control's tooltip below the
   * window chrome. */
  placement?: ControlTooltipPlacement;
  /** The control's accessible name, for the touch popover's name when the child gives none as its `label`. */
  label?: string;
  /** The tooltip opens on keyboard focus only, not on hover. */
  focusOnly?: boolean;
  /** Holds the tooltip back, closing it if open, while the control is showing what it says some other way, such as in a
   * popover the control opened. */
  disabled?: boolean;
  /** A press does not close the tooltip, for a control whose tooltip answers the press (a copy button's changes to
   * "Copied"). It still closes as the pointer or focus leaves. */
  keepOpenOnPress?: boolean;
  /** Told when the tooltip opens or closes. */
  onOpenChange?: (isOpen: boolean) => void;
  /** A touch or pen tap on the control opens the tooltip's content in a popover drawn like it; see "Touch" below. Off by
   * default, as a tap on a control is the control's own action. */
  touchPopover?: boolean;
}

interface BoxTooltipProps extends TooltipBaseProps {
  trigger: "box";
  /** The visible part of a control whose focus is on a hidden input (a switch's track, a checkbox's box): the tooltip is
   * placed against it, its hover opens the tooltip, and a tap on it opens the touch popover. */
  children: React.ReactElement;
  /** The control's root, which holds its input: keyboard focus on the input opens the tooltip. */
  root: React.RefObject<Element | null>;
  placement?: "end" | "bottom";
  /** The control's accessible name, for the touch popover's name. */
  label?: string;
  /** A touch or pen tap on the box opens the tooltip's content in a popover; see "Touch" below. */
  touchPopover?: boolean;
}

interface MarkTooltipProps extends TooltipBaseProps {
  trigger: "mark";
  /** What the mark is, as the trigger's accessible name: a named `role="img"`, which stays a plain string so the name never
   * depends on the tooltip's markup. Without one the trigger is `role="presentation"`, for a mark whose meaning reaches
   * assistive technology some other way (the row's own accessible name); browsers ignore that role on a focusable element,
   * so the trigger is still exposed as a generic one, which is harmless. */
  label?: string;
  /** The label says the same words as the tooltip, which is not a string to compare it with, so the touch popover is not
   * named by it. */
  labelRepeatsTooltip?: boolean;
  /** Lays out the trigger around `children`. */
  className?: string;
  /** The trigger is a `span`, for a mark inside a paragraph or a heading (the viewer title's status chip). */
  phrasing?: boolean;
  /** A touch or pen tap on the mark opens the tooltip's content in a popover; see "Touch" below. On by default, as a tap on
   * a mark has no action of its own. It has to be turned off for a mark inside something that handles the same tap (a row,
   * a button, an option), or the popover would open over what the press does; the caller says why where it does. */
  touchPopover?: boolean;
  children: React.ReactNode;
}

interface AnchoredTooltipProps extends TooltipBaseProps {
  trigger: "anchored";
  /** The element the tooltip is placed against. */
  anchor: React.RefObject<Element | null>;
  /** Whether the tooltip shows: the pointer is on what it belongs to, or, for a caller that reports it, the control has
   * keyboard focus. */
  hovered: boolean;
  /** Where it opens relative to the anchor; HeroUI's default is above it. The sides follow the reading direction, so they
   * are mirrored under right-to-left. */
  placement?: "start" | "end";
  /** Show with no wait, as a tooltip does when its control takes keyboard focus. Read as this opens. */
  immediate?: boolean;
}

export type TooltipProps = ControlTooltipProps | MarkTooltipProps | AnchoredTooltipProps | BoxTooltipProps;

/**
 * The one tooltip of the application, in four kinds of trigger, chosen by `trigger`:
 *
 * - `control` (the default): `children` is a react-aria control (a `ToggleButton`, a `MenuTrigger`), which takes the
 *   tooltip's trigger behaviour from context, so no element is added around it, which keeps a mouse press from taking
 *   focus (`focusOnPress={false}` still holds) and leaves layout alone. A `Button` is this kind too, through its
 *   `tooltip` prop. `TooltipTrigger` is deliberately not used: it renders an element of its own.
 * - `mark`: a non-control mark (a badge, a status glyph), wrapped in a trigger element of its own. It is no control, so it
 *   is no tab stop and a press does not focus it, which would take focus off the terminal.
 * - `box`: the visible part of a control whose focus is on a hidden input (a switch's track, a checkbox's box), which the
 *   `Switch` and `Checkbox` use for their own tooltip (a `Button`'s is the `control` kind). The input is never under the pointer, and a trigger taken from
 *   context would be the input, anchoring the tooltip to it and, as react-aria spreads the trigger's props over the
 *   input's, dropping the input's own `aria-describedby` (its description, the reason it is unavailable). So the tooltip
 *   is placed against the box, opened by the box's hover and by keyboard focus reported from the input, and linked to
 *   nothing: what it says is the control's description already. The box is not focusable and adds no tab stop, and a
 *   press on it leaves focus to the label's own handling. Hover and press follow react-aria's own trigger: a hover opens
 *   it only after a real pointer move, and a press closes it.
 * - `anchored`: something that cannot be a trigger (a run of text, a mark inside a row that walks its focusable
 *   descendants with the arrow keys, a tab), whose hover the caller reports, placed against the element it names. Nothing
 *   is rendered while the pointer is away: no state, no portal, no measuring, which is what lets every clipped label in
 *   the window carry one. The open state is react-aria's own, opened as this mounts and closed as it unmounts, which is
 *   what a trigger element does on the pointer's way in and out. So the wait before it shows is the app's, one tooltip
 *   at a time still holds across the window, and every path react-aria closes a tooltip on applies, above all the one
 *   that closes it when something under it scrolls. Driving `isOpen` from the hover flag instead would silently disable
 *   them all. That close is also why the anchor is never an element the UI itself scrolls (a marquee): the tooltip
 *   would close as soon as it opened. Once it has closed, what re-opens it is the pointer leaving and coming back,
 *   which is what unmounts and mounts this again, or the caller's `key` changing. Hover is the way in, as the anchor is
 *   usually no tab stop; the caller of an anchor that is a tab stop may report its keyboard focus through `hovered` as
 *   well, and `immediate` then skips the wait. Nothing links the tooltip to the anchor for assistive technology: what
 *   it says has to reach a screen reader another way (the text it belongs to, the name of the row it is in, the
 *   control's own name).
 *
 * A tooltip is a label, not a surface to interact with: it shows on hover and on keyboard focus, closes the instant the
 * pointer leaves (`--tooltip-close-delay: 0s` in `styles/layout.css`, where HeroUI's default lingers), and is
 * `pointer-events-none` (which has no token, so every tooltip sets it itself), because react-aria's tooltip otherwise
 * stays open while it is hovered and would swallow clicks aimed at what lies beneath it. Nothing relies on a hoverable
 * tooltip. What has something to press or select in its content is not
 * a tooltip's to hold: that is a `PopoverHint`, never a `Tooltip`.
 *
 * While a tooltip is open react-aria links it to the control as its description (`aria-describedby`), and VoiceOver
 * speaks a description after the name. A tooltip that only repeats the control's accessible name (its `label`) would
 * thus be heard twice, so such a control is given an empty `aria-describedby` (`describedBy=""`), which wins over the
 * tooltip's and refers to nothing. A tooltip that says more than the name stays its description, and a control that sets
 * its own `aria-describedby` keeps it.
 *
 * Touch. A touch has no hover, so a tooltip never reaches it; where the tooltip is all there is to see, a tap opens the
 * same content in a popover drawn like it (`touchPopover`): a mark's, and the reason a `Button`, `Switch` or `Checkbox`
 * is unavailable for. A tap on a control that acts is its action, so that opens nothing. The popover is modal, as a
 * popover is: a tap outside closes it (and is not passed on to what lies there), so does Escape, and focus, which it
 * takes onto itself, goes back to what held it. It has a dialog's role, so it is named: `label` is the trigger's
 * accessible name, but a dialog must not be named by the very text it holds, which a screen reader would say twice, so
 * it takes `label` only where there is one, it differs from the content (when that is a string) and the caller has not
 * declared that it repeats the tooltip's words; otherwise it is named by a generic word. Being modal it hides the trigger
 * from assistive technology while open, so it is not linked to the trigger as its description, as the tooltip is. A mark
 * that opens it is a touch target, so on a device with a coarse pointer its hit area grows to 24 by 24 pixels
 * (`touch-target` in `components/hint/touchTarget.css`) without changing what is drawn. The tooltip stays shut while the
 * popover is open (a pen hovering above the screen opens a tooltip before it presses).
 */
export function Tooltip(props: TooltipProps): React.ReactElement | null {
  switch (props.trigger) {
    case "mark":
      return <MarkTooltip {...props} />;
    case "anchored":
      return <AnchoredTooltip {...props} />;
    case "box":
      return <BoxTooltip {...props} />;
    default:
      return <ControlTooltip {...props} />;
  }
}

/** Whether a `Tooltip` of the control kind is around this part of the tree, which a control that carries its own tooltip
 * (a `Button`, a `Switch`, a `Checkbox`) reads to refuse being wrapped by one: the outer tooltip's trigger is whatever
 * takes it from context first, so one of the two would never open. */
const WrappedByTooltipContext = createContext(false);

/** Marks what a tooltip draws (its surface, the touch popover) as no trigger: a control inside it is not the one the
 * tooltip belongs to, so `useOwnTooltipGuard` stays quiet for it. */
function NotTrigger({ children }: { children: React.ReactNode }): React.ReactElement {
  return <WrappedByTooltipContext.Provider value={false}>{children}</WrappedByTooltipContext.Provider>;
}

/** Dev-time guard for a control that carries its own tooltip (`Button`, `Switch`, `Checkbox`): reports one wrapped in a
 * `Tooltip` of the control kind, a mistake that silently drops the outer tooltip. */
export function useOwnTooltipGuard(control: string): void {
  const wrapped = useContext(WrappedByTooltipContext);
  useEffect(() => {
    if (import.meta.env.DEV && wrapped) console.error(`A ${control} carries its own tooltip: give it \`tooltip\` (and \`unavailable\`) instead of wrapping it in a Tooltip.`);
  }, [control, wrapped]);
}

function ControlTooltip({ content, children, placement, label, focusOnly, disabled, keepOpenOnPress = false, onOpenChange, touchPopover = false }: ControlTooltipProps): React.ReactElement {
  const present = hasContent(content);
  const showsPopover = touchPopover && present;
  const popover = useTouchPopover(showsPopover);
  const own = children.props;
  // The layer's controls (`ToggleButton`, `ModalCloseButton`) take the name and the description as `label` and `describedBy`.
  const repeatsName = present && own.label === content && own.describedBy === undefined;
  const held = disabled || !present || popover.isOpen;
  return (
    <WrappedByTooltipContext.Provider value>
      <TooltipRoot focusOnly={focusOnly} disabled={held} keepOpenOnPress={keepOpenOnPress} onOpenChange={onOpenChange}>
        {repeatsName ? cloneElement(children, { describedBy: "" }) : children}
        <TooltipSurface placement={placement}>{content}</TooltipSurface>
        {showsPopover && <TouchTap onTap={popover.open} />}
      </TooltipRoot>
      {showsPopover && <TouchPopover popover={popover} label={label ?? own.label} placement={placement}>{content}</TouchPopover>}
    </WrappedByTooltipContext.Provider>
  );
}

function MarkTooltip({ content, label, labelRepeatsTooltip = false, className, phrasing = false, touchPopover = true, children }: MarkTooltipProps): React.ReactElement {
  const popover = useTouchPopover(touchPopover);
  return (
    <>
      <TooltipRoot disabled={popover.isOpen}>
        <TooltipTrigger phrasing={phrasing} role={label ? "img" : "presentation"} label={label} className={touchPopover ? `touch-target ${className ?? ""}` : className}>
          {children}
        </TooltipTrigger>
        <TooltipSurface>{content}</TooltipSurface>
        {touchPopover && <TouchTap onTap={popover.open} />}
      </TooltipRoot>
      {touchPopover && <TouchPopover popover={popover} label={label} labelRepeatsTooltip={labelRepeatsTooltip}>{content}</TouchPopover>}
    </>
  );
}

function AnchoredTooltip({ content, anchor, hovered, placement, immediate = false }: AnchoredTooltipProps): React.ReactElement | null {
  if (!hovered) return null;
  return (
    <TooltipRoot>
      <OpenWhileMounted immediate={immediate} anchor={anchor} />
      <TooltipSurface anchor={anchor} placement={placement} data-anchored-tooltip="">
        {content}
      </TooltipSurface>
    </TooltipRoot>
  );
}

/** The tooltip's own surface, for its content to be drawn on. `control-fills` because a tooltip is portalled out of the
 * pane or dialog that carries the fills, and `pointer-events-none`, as above. */
function TooltipSurface({ placement = TOOLTIP_PLACEMENT, children, ...props }: Omit<React.ComponentProps<typeof TooltipContent>, "className" | "offset">): React.ReactElement {
  return (
    <TooltipContent {...props} placement={placement} offset={TOOLTIP_OFFSET} className="control-fills pointer-events-none">
      <NotTrigger>{children}</NotTrigger>
    </TooltipContent>
  );
}

function BoxTooltip({ content, children, root, placement, label, touchPopover = false }: BoxTooltipProps): React.ReactElement {
  const present = hasContent(content);
  const showsPopover = touchPopover && present;
  const popover = useTouchPopover(showsPopover);
  const box = useRef<Element | null>(null);
  const held = !present || popover.isOpen;
  return (
    <>
      <TooltipRoot disabled={held}>
        <BoxTrigger box={box} root={root} disabled={held}>{children}</BoxTrigger>
        <TooltipSurface placement={placement} anchor={box}>{content}</TooltipSurface>
        {showsPopover && <TouchTap anchor={box} onTap={popover.open} />}
      </TooltipRoot>
      {showsPopover && <TouchPopover popover={popover} label={label} placement={placement}>{content}</TouchPopover>}
    </>
  );
}

/** The box a `BoxTooltip` is placed against: it takes the hover and the press, and reports the keyboard focus of the input
 * inside `root` to the tooltip's own state. Rendered inside the tooltip's tree, where that state is. */
function BoxTrigger({ box, root, disabled, children }: { box: React.RefObject<Element | null>; root: React.RefObject<Element | null>; disabled: boolean; children: React.ReactElement }): React.ReactElement {
  const state = useTooltipState();
  const { hoverProps } = useHover({
    isDisabled: disabled,
    // As react-aria's trigger does: a hover restored beneath a pointer at rest (an overlay closing) is not a move.
    onHoverStart: () => getInteractionModality() === "pointer" && state?.open(false),
    onHoverEnd: () => state?.close(false),
  });
  const latest = useRef({ state, disabled });
  useLayoutEffect(() => {
    latest.current = { state, disabled };
  });
  useEffect(() => {
    const input = root.current?.querySelector("input");
    if (!input) return;
    const onFocus = () => {
      if (!latest.current.disabled && isFocusVisible()) latest.current.state?.open(true);
    };
    const close = () => latest.current.state?.close(true);
    input.addEventListener("focus", onFocus);
    input.addEventListener("blur", close);
    input.addEventListener("keydown", close);
    return () => {
      input.removeEventListener("focus", onFocus);
      input.removeEventListener("blur", close);
      input.removeEventListener("keydown", close);
    };
  }, [root]);
  return cloneElement(children, { ...mergeProps(hoverProps, { onPointerDown: () => state?.close(true) }), ref: box } as Record<string, unknown>);
}

/** The open state of a touch popover and the element it is placed against, which the tap that opens it hands in. While
 * the popover cannot show (`canShow` is false: the control became available, the reason went) it is closed, and stays
 * closed when it can show again, rather than opening by itself. */
function useTouchPopover(canShow: boolean) {
  const [isOpenState, setOpen] = useState(false);
  if (!canShow && isOpenState) setOpen(false);
  const isOpen = isOpenState && canShow;
  const anchor = useRef<Element | null>(null);
  const open = (target: Element) => {
    anchor.current = target;
    setOpen(true);
  };
  return { isOpen, setOpen, anchor, open };
}

/** Opens the touch popover on a tap of the tooltip's trigger (or of `anchor`): a touch or pen press, which the click that
 * ends it follows without carrying the pointer, so what the press came from is recorded at its pointerdown. A mouse press
 * and a click with no pointer behind it (assistive technology activating the control) open nothing. It listens on the
 * element itself, so the control's own press handling is left as it is. Rendered inside the tooltip's tree, where the
 * trigger's element is known. */
function TouchTap({ anchor, onTap }: { anchor?: React.RefObject<Element | null>; onTap: (target: Element) => void }): null {
  const trigger = useSlottedContext(TooltipContext)?.triggerRef;
  const target = anchor ?? trigger;
  const tap = useRef(onTap);
  useLayoutEffect(() => {
    tap.current = onTap;
  });
  useEffect(() => {
    const element = target?.current as HTMLElement | null | undefined;
    if (!element) return;
    let touched = false;
    const onPointerDown = (event: PointerEvent) => {
      touched = event.pointerType === "touch" || event.pointerType === "pen";
    };
    const onClick = () => {
      if (touched) tap.current(element);
      touched = false;
    };
    element.addEventListener("pointerdown", onPointerDown, true);
    element.addEventListener("click", onClick);
    return () => {
      element.removeEventListener("pointerdown", onPointerDown, true);
      element.removeEventListener("click", onClick);
    };
  }, [target]);
  return null;
}

/** The popover a tap opens in place of a tooltip, drawn like it (`tooltipSurfaceClassName`, so the surface, padding, text
 * size, width cap and word breaking are the tooltip's one definition, not a copy of it). It has no arrow because the
 * tooltip has none. */
function TouchPopover({ popover, label, labelRepeatsTooltip = false, placement, children }: {
  popover: ReturnType<typeof useTouchPopover>;
  label?: string;
  labelRepeatsTooltip?: boolean;
  placement?: ControlTooltipPlacement;
  children: React.ReactNode;
}): React.ReactElement {
  const labels = usePrimitiveLabels();
  const named = label && label !== children && !labelRepeatsTooltip;
  return (
    <FloatingPopover
      open={popover.isOpen}
      onOpenChange={popover.setOpen}
      anchor={popover.anchor}
      placement={placement ?? TOOLTIP_PLACEMENT}
      offset={TOOLTIP_OFFSET}
      label={named ? label : (labels?.tooltip ?? "Hint")}
      className={tooltipSurfaceClassName}
    >
      <NotTrigger>{children}</NotTrigger>
    </FloatingPopover>
  );
}

/** Reports to the tooltip's own state the hover that a trigger element would have reported, and takes the Escape that one
 * would have taken inside a popup holding focus. */
function OpenWhileMounted({ immediate, anchor }: { immediate: boolean; anchor: React.RefObject<Element | null> }): null {
  const state = useTooltipState();
  const isOpen = useRef(state?.isOpen ?? false);
  useLayoutEffect(() => {
    isOpen.current = state?.isOpen ?? false;
  });
  useEffect(() => {
    state?.open(immediate);
    // A press can cover the anchor with an overlay without sending pointerleave. Close the
    // tooltip, including its pending opening, before the press reaches that overlay's trigger.
    const close = () => state?.close(true);
    // Escape dismisses a shown tooltip on its own, without the pointer or focus moving (WCAG 2.2
    // SC 1.4.13), so where focus is in the popup the anchor sits in — a menu, a submenu, a select's
    // list: react-aria's popover, which alone carries `data-trigger` — the key goes no further. The
    // popup closes from a React handler, below this capture-phase listener, and would otherwise
    // close with the tooltip. Anywhere else the key carries on to whatever was waiting for it — a
    // dialog closes as it does under any tooltip, the agent's terminal takes it as an interrupt —
    // and a tooltip still waiting out its delay is not yet shown, so it leaves the key alone too.
    const closeOnKey = (event: KeyboardEvent) => {
      const composing = event.isComposing || event.keyCode === 229;
      if (event.key === "Escape" && !composing && isOpen.current && anchor.current?.closest("[data-trigger]")?.contains(document.activeElement)) {
        event.stopPropagation();
      }
      close();
    };
    document.addEventListener("pointerdown", close, true);
    document.addEventListener("keydown", closeOnKey, true);
    // Closing on the way out is not optional: whether a tooltip has just been shown is tracked
    // globally, over every tooltip in the window, and one that goes away without closing leaves
    // that state standing, so the next tooltip to open shows with no delay.
    return () => {
      document.removeEventListener("pointerdown", close, true);
      document.removeEventListener("keydown", closeOnKey, true);
      state?.close(true);
    };
    // Mount and unmount alone: opening again would undo a close react-aria made for a reason of
    // its own, with the pointer still where it was.
  }, []);
  return null;
}
