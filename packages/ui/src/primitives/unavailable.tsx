import React, { useEffect, useId } from "react";

// What a control unavailable for a reason shares (see "Buttons" in `docs/conventions/ui-standard.md`), used by `Button`,
// `Checkbox` and `Switch`: they stay focusable and read as `aria-disabled`, look faded, take no press, say the reason as
// their tooltip and as their description, and the reason replaces the control's own tooltip while it lasts. The three
// carry their tooltip themselves (`tooltip` and `unavailable`) and always render the same tree, the tooltip held back
// while it has no text, so toggling either never remounts the control and it keeps its focus.

/** The faded look of a control that is unavailable, HeroUI's own disabled opacity. */
export const UNAVAILABLE_FADE = "opacity-(--disabled-opacity)";

/** The look of an unavailable button over HeroUI's disabled one, which turns pointer events off for `aria-disabled`:
 * they are turned back on, for the tooltip with the reason to open, and the hover and pressed fills stay the resting one
 * (over the caller's own tints, hence `!`) and the press shrink is left off, as nothing is done. */
export const UNAVAILABLE_BUTTON_CLASS = "pointer-events-auto [--button-bg-hover:var(--button-bg)]! [--button-bg-pressed:var(--button-bg)]! ui-pressed:[transform:none] active:[transform:none]";

/** Whether `unavailable` says the control is unavailable: the reason, or `true` for no reason. `""`, `undefined`, `null`
 * and `false` are available. */
export const isUnavailable = (unavailable: React.ReactNode): boolean => unavailable !== undefined && unavailable !== null && unavailable !== false && unavailable !== "";

/** What a control unavailable for a reason is made of, from its `unavailable`: whether it is frozen, the reason when it
 * has one (`true` is none), and the reason as its description, a visually hidden copy the control's `aria-describedby`
 * names (`id`, with `element` to render inside the control), which replaces any other description while the control is
 * unavailable. */
export function useUnavailable(unavailable: React.ReactNode): { frozen: boolean; reason: React.ReactNode | undefined; id: string | undefined; element: React.ReactNode } {
  const id = useId();
  const frozen = isUnavailable(unavailable);
  const reason = frozen && typeof unavailable !== "boolean" ? unavailable : undefined;
  return { frozen, reason, id: reason === undefined ? undefined : id, element: reason === undefined ? null : <span id={id} className="sr-only">{reason}</span> };
}

/** Marks the input inside `root` as `aria-disabled` while `unavailable`: react-aria gives a checkbox's or a switch's input
 * no such prop, so it is set once the input is in the page. */
export function useInputAriaDisabled(root: React.RefObject<HTMLElement | null>, unavailable: boolean): void {
  useEffect(() => {
    const input = root.current?.querySelector("input");
    if (unavailable) input?.setAttribute("aria-disabled", "true");
    else input?.removeAttribute("aria-disabled");
  }, [root, unavailable]);
}
