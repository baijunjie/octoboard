import React, { Children, cloneElement, isValidElement } from "react";

import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";

const SCROLL_LABELS = new Map<string, PlainMessageKey>([
  ["Scroll tabs up", "tabs.scroll.up"],
  ["Scroll tabs down", "tabs.scroll.down"],
  ["Scroll tabs left", "tabs.scroll.left"],
  ["Scroll tabs right", "tabs.scroll.right"],
]);

/** TODO: replace this render adapter when HeroUI exposes localized scroll-button names. Its
 * list container currently offers no button props and bypasses react-aria's locale. Change only
 * the names of the native buttons it renders, keeping its scrolling, refs and styles intact. */
export function useTabScrollLabels(): (children: React.ReactNode) => React.ReactNode {
  const t = useT();
  return (children) => Children.map(children, (child) => {
    if (!isValidElement<React.ComponentProps<"button">>(child) || child.type !== "button") return child;
    const key = SCROLL_LABELS.get(child.props["aria-label"] ?? "");
    return key ? cloneElement(child, { "aria-label": t(key) }) : child;
  });
}
