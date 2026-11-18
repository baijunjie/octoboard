import React, { Children, cloneElement, isValidElement } from "react";

import { type PrimitiveLabels, usePrimitiveLabels } from "./labels";

const SCROLL_LABELS = new Map<string, keyof PrimitiveLabels["scrollTabs"]>([
  ["Scroll tabs up", "up"],
  ["Scroll tabs down", "down"],
  ["Scroll tabs left", "left"],
  ["Scroll tabs right", "right"],
]);

/** Localizes HeroUI's scroll buttons without changing their scrolling, refs or styles. HeroUI's list container
 * exposes no button props and bypasses React Aria's locale. When the application supplies no names, HeroUI's stay. */
export function useTabScrollLabels(): (children: React.ReactNode) => React.ReactNode {
  const labels = usePrimitiveLabels()?.scrollTabs;
  return (children) => labels ? Children.map(children, (child) => {
    if (!isValidElement<React.ComponentProps<"button">>(child) || child.type !== "button") return child;
    const key = SCROLL_LABELS.get(child.props["aria-label"] ?? "");
    return key ? cloneElement(child, { "aria-label": labels[key] }) : child;
  }) : children;
}
