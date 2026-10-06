import { useEffect, useRef } from "react";

import { usePlatform } from "../platform/react";
import { useOctoboardTheme } from "../theme";

/**
 * Pushes the user's theme choice onto the native window's own chrome, so the titlebar matches
 * the in-app choice instead of trailing the OS appearance — CSS reaches only the page's own
 * content. "system" is pushed as `undefined` rather than as whatever it resolved to, so the window
 * keeps following a live OS appearance change.
 *
 * Also fires once on mount, which duplicates the initial push `main.tsx` already made
 * imperatively, ahead of the window's own reveal, from the choice `index.html`'s bootstrap script
 * resolved onto `<html>` before any of this module existed — `nativeWindow.setTheme` is
 * idempotent, so the harm is one redundant IPC round trip, not a stale titlebar. What this hook is
 * actually for is every push *after* that one, when the switcher calls `setChoice`.
 *
 * Best-effort, like the platform's other optional capabilities: a window stuck on the OS
 * appearance is a cosmetic defect, not one worth surfacing to the user.
 */
export function useNativeWindowTheme(): void {
  const { nativeWindow } = usePlatform();
  const { choice } = useOctoboardTheme();
  // Each call is a dynamic `import()` plus an IPC round trip, with nothing else sequencing two
  // in-flight ones — chaining onto the previous call's settling, rather than firing independently,
  // is what keeps a rapid choice change from settling out of order and leaving the titlebar on a
  // stale value until the next change.
  const pendingRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    if (!nativeWindow) return;
    const theme = choice === "system" ? undefined : choice;
    pendingRef.current = pendingRef.current.then(() => nativeWindow.setTheme(theme).catch(() => {}));
  }, [nativeWindow, choice]);
}
