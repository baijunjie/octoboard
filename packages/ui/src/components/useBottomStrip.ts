import { useLayoutEffect, useRef } from "react";

import { focusOpenDialog } from "./regionFocus";
import { useFocusHandoff } from "./useFocusHandoff";

/**
 * What a strip along the window's bottom edge (the connection banner, the update notice) shares: it publishes its own
 * height to `document.documentElement` as the custom property `heightVar` while `visible`, and clears it back to `0px`
 * as it goes, since the text decides that height and `--bottom-chrome-height` (`style.css`) adds the published heights
 * up rather than guessing them; and it hands keyboard focus on when a button pressed in it makes the strip change or
 * go away, which would otherwise drop focus to `<body>`: to an open dialog, otherwise to `focusTerminal`
 * (`useFocusHandoff`). The caller puts `ref` on the strip's element and spreads `focusProps` on it, and stays mounted
 * while the strip is not shown.
 */
export function useBottomStrip({
  visible,
  heightVar,
  focusTerminal,
}: {
  visible: boolean;
  heightVar: string;
  focusTerminal: () => void;
}): {
  ref: React.RefObject<HTMLDivElement | null>;
  focusProps: { onFocus: (event: React.FocusEvent<HTMLDivElement>) => void; onBlur: (event: React.FocusEvent<HTMLDivElement>) => void };
} {
  const ref = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<Element | null>(null);
  const held = useFocusHandoff(() => {
    if (!focusOpenDialog(previousFocus.current)) focusTerminal();
  });

  // Keyed on `visible` rather than `ref.current`: a deps array is diffed during render, while React only detaches a
  // removed element's ref in the commit phase, after render. So on the very render where the strip goes,
  // `ref.current` still points at the old node, the deps would compare equal, and the cleanup below (the one that
  // resets the property to `0px`) would never run. Any re-render between the strip appearing and its removal arms
  // that bug; keying on the boolean the render itself decides with avoids it regardless of how many re-renders happen
  // in between.
  useLayoutEffect(() => {
    if (!visible) return;
    const node = ref.current;
    if (!node) return;
    // Written synchronously here too, not only from the observer's first callback, so nothing that ends above
    // `--bottom-chrome-height` is misplaced for the one frame between the strip appearing and its first
    // `ResizeObserver` callback.
    document.documentElement.style.setProperty(heightVar, `${node.offsetHeight}px`);
    const observer = new ResizeObserver(() => {
      document.documentElement.style.setProperty(heightVar, `${node.offsetHeight}px`);
    });
    observer.observe(node);
    return () => {
      observer.disconnect();
      document.documentElement.style.setProperty(heightVar, "0px");
    };
  }, [visible, heightVar]);

  return {
    ref,
    focusProps: {
      onFocus: (event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) previousFocus.current = event.relatedTarget;
        held.onFocus(event);
      },
      onBlur: held.onBlur,
    },
  };
}
