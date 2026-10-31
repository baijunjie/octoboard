import { useStableHandler } from "../components/hooks/useLatest";
import { textDirection } from "../i18n/languages";
import { useCurrentLanguage } from "../i18n/react";
import { resolveSwipe, type SwipeToward } from "../layout/paneSwipe";
import { useSwipe } from "../layout/useSwipe";
import type { OwnerKind, PanelKind } from "../panels/stackState";

/** Swipes open and close the drawer and the floating panel through the paths the toggles use, in a window narrow
 * enough to float the right pane's panels (`asideFloats`). */
export function usePaneSwipe({
  asideFloats,
  noConsoles,
  sidebarOpen,
  toggleSidebar,
  closeSidebar,
  swipeKinds,
  ownerKind,
  ownerOrder,
  floatingKind,
  closeFloating,
  lastOpened,
  togglePanel,
}: {
  asideFloats: boolean;
  noConsoles: boolean;
  sidebarOpen: boolean;
  toggleSidebar: () => void;
  closeSidebar: () => void;
  /** The owner's panel kinds a swipe may open. */
  swipeKinds: readonly PanelKind[];
  ownerKind: OwnerKind | undefined;
  ownerOrder: readonly PanelKind[];
  floatingKind: PanelKind | undefined;
  closeFloating: () => void;
  lastOpened: (owner: OwnerKind) => PanelKind | undefined;
  togglePanel: (kind: PanelKind) => void;
}): void {
  const rtl = textDirection(useCurrentLanguage()) === "rtl";
  const swipePanes = useStableHandler((toward: SwipeToward) => {
    const action = resolveSwipe(toward, {
      hasDrawer: !noConsoles,
      drawerOpen: sidebarOpen,
      panelKinds: swipeKinds,
      panelOrder: ownerOrder,
      panel: floatingKind,
      lastPanel: ownerKind ? lastOpened(ownerKind) : undefined,
    });
    if (!action) return;
    // Focus stays where it is, so a tooltip open on a focused top bar control (a hardware keyboard)
    // stays showing until focus moves.
    if (action.type === "open-drawer") toggleSidebar();
    else if (action.type === "close-drawer") closeSidebar();
    else if (action.type === "close-panel") closeFloating();
    else togglePanel(action.kind);
  });
  useSwipe(asideFloats, rtl, swipePanes);
}
