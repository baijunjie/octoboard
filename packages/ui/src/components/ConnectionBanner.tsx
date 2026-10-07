import { Button } from "@heroui/react";
import { RefreshCw, Unplug } from "lucide-react";
import React, { useLayoutEffect, useRef } from "react";

import type { ConnectionState } from "../daemon-client";
import { useT } from "../i18n/react";
import { useFocusHandoff } from "./useFocusHandoff";

/** The CSS custom property this banner publishes its own height to. `style.css` exposes it as
 * `--bottom-chrome-height`, the offset that everything overlaying the content area ends above, so
 * nothing hardcodes a height that follows the banner's text metrics. */
const BANNER_HEIGHT_VAR = "--connection-banner-height";

/** HeroUI's outline button retinted through the button's own color tokens to sit on the banner's
 * solid warning fill, whose stock tokens are neutral greys meant for a surface background: the
 * border in the banner's text color, faded, the fill still transparent at rest and HeroUI's
 * `--warning-hover` under the pointer and while pressed; sized down to the banner's text. */
const RETRY_CLASS =
  "h-6 rounded-lg border-warning-foreground/30 px-2.5 text-xs " +
  "[--button-bg-hover:var(--warning-hover)] [--button-bg-pressed:var(--warning-hover)] " +
  "[--button-fg:var(--warning-foreground)]";

/**
 * The strip along the bottom edge while the control connection is down: it says the client is
 * retrying on its own while the automatic attempts last, then offers a Retry once they are spent,
 * because nothing retries forever (see "Losing the daemon connection" in
 * `docs/product/application-lifecycle.md`). Renders nothing in any other state.
 *
 * Retry keeps keyboard focus where it is on a mouse press, but a keyboard user reaches it with
 * F6, and then the banner going away from under focus would drop it to `<body>`, so focus is
 * handed to the terminal instead (`useFocusHandoff`). The banner is a stop of the F6 region cycle
 * (`data-region="banner"`), which is how a keyboard reaches Retry from the terminal, whose Tab
 * belongs to the agent.
 *
 * Publishes its own height to `document.documentElement` as it mounts and resizes, and clears it
 * back to `0px` as it unmounts (`BANNER_HEIGHT_VAR`), since it is the banner's text metrics that
 * decide its height, not a fixed value — `--bottom-chrome-height` follows the published value rather
 * than guessing it, and would otherwise stay offset by a banner that is no longer on screen.
 */
export function ConnectionBanner({
  state,
  onRetry,
  focusTerminal,
}: {
  state: ConnectionState;
  onRetry: () => void;
  /** Where focus goes when Retry held it as the banner went away (`useFocusHandoff`). */
  focusTerminal: () => void;
}): React.ReactElement | null {
  const t = useT();
  const held = useFocusHandoff(focusTerminal);
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
    // Written synchronously here too, not only from the observer's first callback, so nothing that
    // ends above `--bottom-chrome-height` is misplaced for the one frame between this banner
    // appearing and its first `ResizeObserver` callback.
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
      data-region="banner"
      {...held}
      // `min-h-8` is the height the Retry button (h-6) and the padding take, so the strip is as tall
      // while it only says it is reconnecting as once it offers Retry.
      className="flex min-h-8 flex-none items-center justify-center gap-3 bg-warning px-3 py-1 text-xs text-warning-foreground"
    >
      {state === "closed" ? (
        <>
          <Unplug aria-hidden="true" className="size-3.5 shrink-0" />
          <span>{t("connection.disconnected")}</span>
          <Button
            size="sm"
            variant="outline"
            className={RETRY_CLASS}
            preventFocusOnPress
            onPress={onRetry}
          >
            {t("connection.retry")}
          </Button>
        </>
      ) : (
        <>
          <RefreshCw aria-hidden="true" className="size-3.5 shrink-0 motion-safe:animate-spin-slow" />
          <span>{t("connection.reconnecting")}</span>
        </>
      )}
    </div>
  );
}
