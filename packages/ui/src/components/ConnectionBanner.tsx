import { Button } from "@heroui/react";
import React, { useLayoutEffect, useRef } from "react";

import type { ConnectionState } from "../daemon-client";

/** The CSS custom property this banner publishes its own height to, read by `Toasts` so the toast
 * stack clears it without hardcoding a height that follows the banner's text metrics. */
const BANNER_HEIGHT_VAR = "--connection-banner-height";

/**
 * The strip across the top while the control connection is down: it says the client is retrying on
 * its own while the automatic attempts last, then offers a Retry once they are spent, because
 * nothing retries forever (see "Losing the daemon connection" in
 * `docs/product/application-lifecycle.md`). Renders nothing in any other state.
 *
 * Retry keeps keyboard focus where it is: once the connection is back the banner unmounts, and a
 * press that focused the button would drop focus to `<body>`, away from the terminal.
 *
 * Publishes its own height to `document.documentElement` as it mounts and resizes, and clears it
 * back to `0px` as it unmounts (`BANNER_HEIGHT_VAR`), since it is the banner's text metrics that
 * decide its height, not a fixed value — `Toasts` reads the published value rather than guessing
 * it, and would otherwise stay offset by a banner that is no longer on screen.
 */
export function ConnectionBanner({
  state,
  onRetry,
}: {
  state: ConnectionState;
  onRetry: () => void;
}): React.ReactElement | null {
  const ref = useRef<HTMLDivElement>(null);
  const visible = state === "reconnecting" || state === "closed";

  // Keyed on `visible` rather than `ref.current`: a deps array is diffed during render, while
  // React only detaches a removed element's ref in the commit phase, after render — so on the
  // very render where the banner returns `null`, `ref.current` still points at the old node, the
  // deps would compare equal, and the cleanup below (the one that resets the property back to
  // `0px`) would never run. Any re-render between the banner mounting and its removal arms that
  // bug; keying on the boolean the render itself decides with avoids it regardless of how many
  // re-renders happen in between.
  useLayoutEffect(() => {
    if (!visible) return;
    const node = ref.current;
    if (!node) return;
    // Written synchronously here too, not only from the observer's first callback, so the toast
    // stack is never misplaced for the one frame between this banner appearing and its first
    // `ResizeObserver` callback.
    document.documentElement.style.setProperty(BANNER_HEIGHT_VAR, `${node.offsetHeight}px`);
    const observer = new ResizeObserver(() => {
      document.documentElement.style.setProperty(BANNER_HEIGHT_VAR, `${node.offsetHeight}px`);
    });
    observer.observe(node);
    return () => {
      observer.disconnect();
      document.documentElement.style.setProperty(BANNER_HEIGHT_VAR, "0px");
    };
  }, [visible]);

  if (!visible) return null;
  return (
    <div
      ref={ref}
      role="status"
      className="flex flex-none items-center justify-center gap-3 bg-warning px-3 py-1 text-xs text-warning-foreground"
    >
      {state === "closed" ? (
        <>
          <span>Disconnected from the daemon.</span>
          <Button size="sm" variant="outline" preventFocusOnPress onPress={onRetry}>
            Retry
          </Button>
        </>
      ) : (
        <span>Disconnected from the daemon — reconnecting…</span>
      )}
    </div>
  );
}
