import { Tooltip as HeroTooltip } from "@heroui/react";
import { tooltipVariants } from "@heroui/styles";
import React, { useContext, useEffect, useLayoutEffect, useRef } from "react";
import { mergeProps, useFocusable } from "react-aria";
import { TooltipContext, TooltipTriggerStateContext, useSlottedContext } from "react-aria-components";

import { type DataAttributes, definedProps } from "./props";
import type { OverlayPlacement } from "./Popover";

/** HeroUI's tooltip around its trigger and its `TooltipContent`, the parts `Tooltip` is assembled from. The trigger is a
 * single control (a `HeroUI` button, a `MenuTrigger`), which takes the tooltip's trigger behaviour from context with no
 * element added around it, or a `TooltipTrigger`; with neither, the content is placed against its own `anchor` and
 * opened by the caller (`useTooltipState`). It shows on hover and on keyboard focus, or with `focusOnly` on keyboard
 * focus alone; `disabled` holds it back, closing it if open; a press on the trigger closes it unless
 * `keepOpenOnPress`. */
export function TooltipRoot({ focusOnly, disabled, keepOpenOnPress, onOpenChange, children }: {
  focusOnly?: boolean;
  disabled?: boolean;
  keepOpenOnPress?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroTooltip
      {...definedProps({
        trigger: focusOnly ? ("focus" as const) : undefined,
        isDisabled: disabled,
        shouldCloseOnPress: keepOpenOnPress ? false : undefined,
        onOpenChange,
      })}
    >
      {children as React.ReactElement}
    </HeroTooltip>
  );
}

/** The trigger element of a `TooltipRoot` around something that is no control (a badge, a mark): a `div`, or with
 * `phrasing` a `span`, for one inside a paragraph or a heading. It is no tab stop, and a mouse press does not focus it,
 * which would take focus off the terminal. `role` and `label` say what it is: a named `img`, or `presentation` for a
 * mark whose meaning reaches assistive technology some other way. */
export function TooltipTrigger({ phrasing = false, role, label, className, children }: {
  phrasing?: boolean;
  role: "img" | "presentation";
  label?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const props = {
    role,
    ...definedProps({ "aria-label": label, className }),
    // HeroUI's trigger is a focusable `role="button"` `div`; these override its own.
    tabIndex: -1,
    onMouseDown: (event: React.MouseEvent) => event.preventDefault(),
    children,
  };
  return phrasing ? <PhrasingTrigger {...props} /> : <HeroTooltip.Trigger {...props} />;
}

/** HeroUI's tooltip trigger as a `span`: the tooltip's trigger props come from context through `useFocusable`, which is
 * where HeroUI's trigger takes them from, and the caller's own props win, as they do on HeroUI's. */
function PhrasingTrigger(props: React.ComponentProps<"span">): React.ReactElement {
  const ref = useRef<HTMLSpanElement>(null);
  const { focusableProps } = useFocusable({}, ref);
  return <span ref={ref} {...mergeProps(focusableProps, props)} />;
}

/** The tooltip's own floating text: HeroUI's tooltip content, placed against the trigger, or against `anchor` for a
 * tooltip with no trigger element.
 *
 * Every tooltip closes once what it belongs to is inside an inert region: a modal overlay opening over it (a dialog, or
 * a modal popover such as a menu), which react-aria marks with `inert` as it opens (`ariaHideOutside`), or a region the
 * app makes inert itself. The overlay covers the trigger without the pointer leaving it, so a pointer at rest sends no
 * `pointerleave`, and the tooltip would stay drawn over the overlay, naming a control that can no longer be reached,
 * until the pointer next moves. A combo box's open list hides the page with `aria-hidden` instead, which leaves it
 * usable, so it does not close the tooltip. */
export function TooltipContent({ placement, offset, anchor, className, children, ...data }: DataAttributes & {
  placement?: OverlayPlacement;
  offset?: number;
  anchor?: React.RefObject<Element | null>;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const trigger = useSlottedContext(TooltipContext);
  useCloseWhenInert(anchor ?? trigger?.triggerRef);
  return (
    <HeroTooltip.Content
      {...data}
      {...definedProps({
        placement: placement as React.ComponentProps<typeof HeroTooltip.Content>["placement"],
        offset,
        triggerRef: anchor,
        className,
      })}
    >
      {children as React.ReactElement}
    </HeroTooltip.Content>
  );
}

function useCloseWhenInert(triggerRef: React.RefObject<Element | null> | undefined): void {
  const state = useContext(TooltipTriggerStateContext);
  const close = useRef(state?.close);
  useLayoutEffect(() => {
    close.current = state?.close;
  });
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
  }, [isOpen, triggerRef]);
}

/** The one visual definition of every floating surface that says something: HeroUI's tooltip surface (surface, padding,
 * text size, radius, width cap, word breaking) with the application's `control-fills`, which a surface portalled out of
 * the pane or dialog that carries the fills needs. The tooltip's own text is drawn with it, as are the touch popover and
 * the hover popover (`PopoverHint`), which differ in the element underneath: a tooltip cannot hold focus or a button,
 * and so cannot be what holds a Try again or selectable text. */
export const tooltipSurfaceClassName = `${tooltipVariants().base()} control-fills outline-none`;

/** The open state of the `TooltipRoot` around the caller, for a tooltip that has no trigger element to open it: `open`
 * shows it, after the wait a hover has unless `immediate`, and `close` hides it, at once if `immediate`. */
export interface TooltipState {
  isOpen: boolean;
  open: (immediate: boolean) => void;
  close: (immediate: boolean) => void;
}

export function useTooltipState(): TooltipState | undefined {
  const state = useContext(TooltipTriggerStateContext);
  return state ? { isOpen: state.isOpen, open: (immediate) => state.open(immediate), close: (immediate) => state.close(immediate) } : undefined;
}
