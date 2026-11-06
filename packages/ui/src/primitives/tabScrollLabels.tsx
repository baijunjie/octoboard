import React, { Children, cloneElement, isValidElement } from "react";

import { type PrimitiveLabels, usePrimitiveLabels } from "./labels";

const SCROLL_LABELS = new Map<string, keyof PrimitiveLabels["scrollTabs"]>([
  ["Scroll tabs up", "up"],
  ["Scroll tabs down", "down"],
  ["Scroll tabs left", "left"],
  ["Scroll tabs right", "right"],
]);

/** TODO: replace this render adapter when HeroUI exposes localized scroll-button names. Its
 * list container currently offers no button props and bypasses react-aria's locale. Change only
 * the names of the native buttons it renders, keeping its scrolling, refs and styles intact. The names
 * are the application's (`PrimitiveLabels`); without them, HeroUI's own stay. */
export function useTabScrollLabels(): (children: React.ReactNode) => React.ReactNode {
  const labels = usePrimitiveLabels()?.scrollTabs;
  return (children) => labels ? Children.map(children, (child) => {
    if (!isValidElement<React.ComponentProps<"button">>(child) || child.type !== "button") return child;
    const key = SCROLL_LABELS.get(child.props["aria-label"] ?? "");
    return key ? cloneElement(child, { "aria-label": labels[key] }) : child;
  }) : children;
}
