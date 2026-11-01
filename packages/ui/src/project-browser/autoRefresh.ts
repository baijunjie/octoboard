import { useEffect } from "react";

import { useLatest } from "../components/hooks/useLatest";

/** How often the browser's lists on screen (directories, branches, changes) are read again while the browser is on
 * screen and the window is in front, so what an agent adds, removes or commits shows up without asking. */
export const AUTO_REFRESH_MS = 10_000;

/** What a list's hook gives the refresh loop: whether a read is out, when the last one was sent, and how to send one. */
export interface AutoRefresh {
  busy(): boolean;
  lastAt(): number;
  run(): void;
}

/**
 * The periodic refresh of a browser list, while `active`: `run` every `AUTO_REFRESH_MS` unless the window is hidden or
 * a read is `busy`, and once on coming back to the window or to a pane that was hidden, if the last read (`lastAt`)
 * was more than half an interval ago. A new `resetKey` starts the loop afresh. The callbacks are those of the latest
 * render, so a refresh that fails words its error as the current language does.
 */
export function useAutoRefresh(active: boolean, resetKey: unknown, refresh: AutoRefresh) {
  const latest = useLatest(refresh);
  useEffect(() => {
    if (!active) return;
    const tick = () => {
      if (document.visibilityState !== "visible" || latest.current.busy()) return;
      latest.current.run();
    };
    const timer = setInterval(tick, AUTO_REFRESH_MS);
    const onVisible = () => {
      if (Date.now() - latest.current.lastAt() > AUTO_REFRESH_MS / 2) tick();
    };
    onVisible();
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
    };
  }, [active, resetKey]);
}
