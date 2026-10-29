import { Tooltip } from "@heroui/react";
import React, { useContext, useEffect, useRef, useState } from "react";
import { mergeProps, useFocusable } from "react-aria";
import { Popover as SurfacePopover, TooltipContext, TooltipTriggerStateContext, useSlottedContext } from "react-aria-components";

import { useT } from "../i18n/react";
import { isImeKey } from "../imeKey";
import { interactionIsTouch } from "../touchInput";
import { useLatest } from "./useLatest";
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
  const trigger = useSlottedContext(TooltipContext);
  useCloseWhenTriggerInert(props.triggerRef ?? trigger?.triggerRef);
  return <Tooltip.Content {...props} placement={placement} offset={OFFSET} className="control-fills pointer-events-none" />;
}

/** Closes the open tooltip once what it belongs to is inside an inert region: a modal overlay opening over it (a
 * dialog, or a modal popover such as a menu), which react-aria marks with `inert` as it opens (`ariaHideOutside`), or
 * a region the app makes inert itself. The overlay covers the trigger without the pointer leaving it, so a pointer at
 * rest sends no `pointerleave`, and the tooltip would stay drawn over the overlay, naming a control that can no longer
 * be reached, until the pointer next moves. A combo box's open list hides the page with `aria-hidden` instead, which
 * leaves it usable, so it does not close the tooltip. */
function useCloseWhenTriggerInert(triggerRef: React.RefObject<Element | null> | undefined): void {
  const state = useContext(TooltipTriggerStateContext);
  const close = useLatest(state?.close);
  const isOpen = state?.isOpen ?? false;
  useEffect(() => {
    if (!isOpen || !triggerRef) return;
    const closeIfInert = () => {
      if (triggerRef.current?.closest("[inert]")) close.current?.(true);
    };
    closeIfInert();
    const observer = new MutationObserver(closeIfInert);
    observer.observe(document.body, { attributes: true, attributeFilter: ["inert"], subtree: true });
    return () => observer.disconnect();
  }, [isOpen, triggerRef, close]);
}

/** The open state of a touch popover and the element it is placed against, which the press that
 * opens it hands in. The shape keeps its tooltip shut while it is open (a pen hovering above the
 * screen opens a tooltip before it presses). */
function useTouchPopover() {
  const [isOpen, setOpen] = useState(false);
  const anchor = useRef<Element | null>(null);
  const open = (target: Element) => {
    anchor.current = target;
    setOpen(true);
  };
  return { isOpen, setOpen, anchor, open };
}

/**
 * The popover a touch press opens in place of a tooltip, drawn like it: it carries HeroUI's own
 * `tooltip` class, so the surface, padding, text size, width cap and word breaking are the
 * tooltip's one definition, not a copy of it, and so are the fills a dialog gives it (`style.css`).
 * It has no arrow because the tooltip has none.
 *
 * It is modal, as a popover is: a tap outside closes it (and is not passed on to what lies
 * there), so does Escape, and focus, which it takes onto itself, goes back to what held it. It has
 * a dialog's role, so it is named: `label` is the trigger's accessible name. A dialog must not be named by
 * the very text it holds, which a screen reader would say twice, so the dialog takes `label` only where
 * there is one, it differs from the content (`children`, when that is a string), and the caller has not
 * declared that it repeats the tooltip's words (`labelRepeatsTooltip`, for a content that is markup);
 * otherwise it is named by a generic word. Being modal it hides the trigger from assistive technology
 * while open, so it is not linked to the trigger as its description, as the tooltip is.
 */
function TouchPopover({
  state,
  label,
  labelRepeatsTooltip = false,
  placement,
  children,
}: {
  state: ReturnType<typeof useTouchPopover>;
  label?: string;
  labelRepeatsTooltip?: boolean;
  placement?: "end";
  children: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const named = label && label !== children && !labelRepeatsTooltip;
  return (
    <SurfacePopover
      isOpen={state.isOpen}
      onOpenChange={state.setOpen}
      triggerRef={state.anchor}
      placement={placement ?? PLACEMENT}
      offset={OFFSET}
      aria-label={named ? label : t("hint.name")}
      className={TOOLTIP_SURFACE}
    >
      {children}
    </SurfacePopover>
  );
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
 * A touch has no hover, so a tooltip never reaches it; a tap on a control is the control's own action
 * (an icon-only button, a menu trigger), so this kind opens nothing on touch. A control whose only
 * action is to show its text is a `popover` hint instead, as `HelpTip` is. */
export function ControlHint({
  tooltip: title,
  children,
  placement,
  focusOnly,
  isDisabled,
  keepOpenOnPress,
  onOpenChange,
}: {
  /** What the tooltip says. */
  tooltip: React.ReactNode;
  children: React.ReactElement<{ "aria-label"?: string; "aria-describedby"?: string }>;
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
  /** Told when the tooltip opens or closes. */
  onOpenChange?: (isOpen: boolean) => void;
}): React.ReactElement {
  if (!title) return children;
  const own = children.props;
  const repeatsName = own["aria-label"] === title && own["aria-describedby"] === undefined;
  const extra: Partial<typeof own> = {};
  if (repeatsName) extra["aria-describedby"] = "";
  return (
    <Tooltip trigger={focusOnly ? "focus" : undefined} isDisabled={isDisabled} shouldCloseOnPress={keepOpenOnPress ? false : undefined} onOpenChange={onOpenChange}>
      {Object.keys(extra).length > 0 ? React.cloneElement(children, extra) : children}
      <TooltipSurface placement={placement}>{title}</TooltipSurface>
    </Tooltip>
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
 * The trigger is a `div`, which is not valid inside a paragraph or a heading; `phrasing` makes it a `span`
 * for an indicator in one (the viewer title's status chip). `Tooltip.Trigger` has no element choice, so that form is the same trigger
 * built on `useFocusable`, which is where `Tooltip.Trigger` takes the tooltip's trigger props from.
 *
 * The trigger takes no `aria-hidden`: `role="presentation"` keeps the trigger itself out of the
 * way, but not what it wraps, so an indicator carrying text rather than a hidden glyph has to hide
 * its own contents, in a wrapper that leaves them the trigger's own layout children (`aria-hidden`
 * on a `display: contents` span). Nothing inside such a wrapper may be focusable.
 *
 * The `tooltip` is any content, for an indicator whose detail has a structure; the `label` stays a
 * plain string, so the accessible name never depends on the tooltip's markup. Where the label says the
 * very words the structured tooltip draws, `labelRepeatsTooltip` has the touch popover named by a
 * generic word rather than by the text it holds.
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
  labelRepeatsTooltip = false,
  className,
  touchPopover = true,
  phrasing = false,
  children,
}: {
  tooltip: React.ReactNode;
  label?: string;
  /** The label says the same words as the tooltip, which is not a string to compare it with, so the touch
   * popover is not named by it; see above. */
  labelRepeatsTooltip?: boolean;
  className?: string;
  /** The trigger is a `span`, for a mark inside a paragraph; see above. */
  phrasing?: boolean;
  /** A touch or pen tap opens the tooltip's content in a popover; see above. */
  touchPopover?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  const popover = useTouchPopover();
  const Trigger = phrasing ? PhrasingTrigger : Tooltip.Trigger;
  const naming = label ? ({ role: "img", "aria-label": label } as const) : ({ role: "presentation" } as const);
  // The click that ends a press does not carry the pointer; what the interaction came from is
  // recorded at its pointerdown.
  const touch = touchPopover ? { onClick: (event: React.MouseEvent<HTMLElement>) => interactionIsTouch() && popover.open(event.currentTarget) } : undefined;
  return (
    <>
      <Tooltip isDisabled={popover.isOpen}>
        <Trigger
          {...naming}
          {...touch}
          tabIndex={-1}
          className={touchPopover ? `touch-target ${className ?? ""}` : className}
          onMouseDown={(event) => event.preventDefault()}
        >
          {children}
        </Trigger>
        <TooltipSurface>{tooltip}</TooltipSurface>
      </Tooltip>
      {touchPopover && <TouchPopover state={popover} label={label} labelRepeatsTooltip={labelRepeatsTooltip}>{tooltip}</TouchPopover>}
    </>
  );
}

/** `Tooltip.Trigger` as a `span`: the tooltip's trigger props come from context through `useFocusable`,
 * and the caller's own props win, as they do on `Tooltip.Trigger`. */
function PhrasingTrigger(props: React.ComponentProps<"span">): React.ReactElement {
  const ref = useRef<HTMLSpanElement>(null);
  const { focusableProps } = useFocusable({}, ref);
  return <span ref={ref} {...mergeProps(focusableProps, props)} />;
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
 * With no trigger element, react-aria holds no Escape listener for it either. Inside a popup that
 * holds focus (a menu, a select's list) it takes an Escape for itself all the same, as a tooltip with
 * a trigger does there (see `OpenWhileMounted`); anywhere else the key is not swallowed while it is
 * open. It is marked `data-anchored-tooltip` for the code that, outside such a popup, tells a
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
      <OpenWhileMounted immediate={immediate} anchor={anchor} />
      <TooltipSurface triggerRef={anchor} placement={placement} data-anchored-tooltip="">
        {tooltip}
      </TooltipSurface>
    </Tooltip>
  );
}

/** Reports to the tooltip's own state the hover that a trigger element would have reported, and takes the
 * Escape that one would have taken inside a popup holding focus. */
function OpenWhileMounted({ immediate, anchor }: { immediate: boolean; anchor: React.RefObject<Element | null> }): null {
  const state = useContext(TooltipTriggerStateContext);
  const isOpen = useLatest(state?.isOpen ?? false);
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
      if (event.key === "Escape" && !isImeKey(event) && isOpen.current && anchor.current?.closest("[data-trigger]")?.contains(document.activeElement)) {
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
