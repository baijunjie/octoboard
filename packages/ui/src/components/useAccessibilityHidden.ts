import { useLayoutEffect, useState } from "react";

/** Whether a fixed container is hidden from assistive technology by itself or an ancestor. Modals can live
 * in descendants that the container's owner does not render, so their actual accessibility cover is observed.
 * Each ancestor is watched only for the two attributes; terminal output never enters the observer. */
export function useAccessibilityHidden(): { ref: (element: HTMLDivElement | null) => void; hidden: boolean } {
  const [element, ref] = useState<HTMLDivElement | null>(null);
  const [hidden, setHidden] = useState(true);
  useLayoutEffect(() => {
    if (!element) return;
    const ancestors: Element[] = [];
    for (let node: Element | null = element; node; node = node.parentElement) ancestors.push(node);
    const update = () => setHidden(ancestors.some((node) => node.hasAttribute("inert") || node.getAttribute("aria-hidden") === "true"));
    const observer = new MutationObserver(update);
    for (const node of ancestors) observer.observe(node, { attributes: true, subtree: false, attributeFilter: ["inert", "aria-hidden"] });
    update();
    return () => observer.disconnect();
  }, [element]);
  return { ref, hidden };
}
