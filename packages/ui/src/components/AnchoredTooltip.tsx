import { Tooltip } from "@heroui/react";
import React, { useContext, useEffect } from "react";
import { TooltipTriggerStateContext } from "react-aria-components";

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
 * With no trigger element, react-aria holds no Escape listener for it either, so the key is not
 * swallowed while it is open. It is marked `data-anchored-tooltip` for the code that tells a
 * tooltip which swallows the key from one that does not.
 */
export function AnchoredTooltip({
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
      <Tooltip.Content
        triggerRef={anchor}
        placement={placement}
        className="control-fills pointer-events-none"
        data-anchored-tooltip=""
      >
        {tooltip}
      </Tooltip.Content>
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
