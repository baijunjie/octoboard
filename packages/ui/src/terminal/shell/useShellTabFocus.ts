import type React from "react";

import { useStableHandler } from "../../components/hooks/useLatest";
import { afterCollectionFocus } from "../../components/focus/afterCollectionFocus";
import { focusFellToPage } from "../../components/focus/regionFocus";
import type { ShellTerminalViewHandle } from "./ShellTerminalView";

/**
 * Where the shell terminal panel puts focus once what held it has gone (a closed tab, its context
 * menu, its view): onto the shown terminal's view, the shown tab, or New terminal when no tab is
 * left. Each keeps its identity across renders (`useStableHandler`) and reads the shown tab from `latest`
 * when it runs, as they are handed to hand-offs that outlive a render.
 */
export function useShellTabFocus({
  latest,
  views,
  tabs,
  newRef,
}: {
  latest: { readonly current: { activeId: string | undefined } };
  views: React.RefObject<Map<string, ShellTerminalViewHandle>>;
  tabs: React.RefObject<Map<string, HTMLElement>>;
  newRef: React.RefObject<HTMLButtonElement | null>;
}) {
  /** Focus for the panel once what held it has gone: the shown terminal, or New terminal
   * when it has none. Deferred to the frame after the commit that changed the tabs. */
  const focusShown = useStableHandler(() =>
    requestAnimationFrame(() => {
      const view = latest.current.activeId === undefined ? undefined : views.current.get(latest.current.activeId);
      if (view) view.focus();
      else newRef.current?.focus();
    }));

  /** The shown tab, or New terminal when no tab is left. */
  const focusShownTabNow = useStableHandler(() => {
    const id = latest.current.activeId;
    const tab = id === undefined ? undefined : tabs.current.get(id);
    if (tab) tab.focus();
    else newRef.current?.focus();
  });

  /** Focus for the strip once the tab that held it (or its context menu) has gone: the shown tab, or
   * the panel's first control when no tab is left. After the strip's own re-aiming of focus, and only
   * while focus has fallen to the page, so a place the user or a dialog has put it stays. */
  const focusShownTab = useStableHandler(() =>
    afterCollectionFocus(() => {
      if (focusFellToPage()) focusShownTabNow();
    }));

  return { focusShown, focusShownTabNow, focusShownTab };
}
