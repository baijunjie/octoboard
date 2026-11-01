import { useEffect, useLayoutEffect, useState } from "react";

import { useStableHandler } from "../components/hooks/useLatest";
import type { AsideOwner } from "../layout/asideOwner";
import { useDockedPanelVisible } from "../layout/panelVisibility";
import { useIsNarrow } from "../breakpoint";
import { useAsideFloats } from "../layout/paneWidth";
import { usePaneToggles } from "../layout/usePaneToggles";
import { maximizedKind, openPanel, OWNER_KINDS, restoreStack, shownKinds, stackedKinds, toggleKind, unavailableKinds as unavailableKindsOf, type OwnerKind, type PanelKind } from "../panels/stackState";
import { useFloatingPanel } from "../panels/useFloatingPanel";
import { usePanelStacks } from "../panels/usePanelStacks";
import { useProcessPanel } from "../process/useProcessPanel";
import type { Console, Session } from "../protocol";
import { useProjectReportSession } from "../report/useProjectReportSession";
import { useDaemon, useDaemonStore } from "../store";
import { startShellTerminal } from "../terminal/shell/shellTerminals";

/**
 * The window's panes around the agent terminal: the sidebar, as a docked column, a drawer or a
 * peek, the right pane's stack of its owner's panels, docked, folded or floating over the terminal,
 * and the rail, which comes in with the drawer on a phone. It hands back what the layout, the
 * toggles, the F6 cycle and the panes' widths read, and the handlers the top bar and the swipes use;
 * `revealArchive` clears what covers the agent terminal's area for the archive view.
 */
export function usePaneLayout({
  asideOwner,
  hasAside,
  asideProjectId,
  consoles,
  currentConsole,
  selectedSession,
  selectedSessionId,
  focusTerminal,
}: {
  asideOwner: AsideOwner | undefined;
  hasAside: boolean;
  /** The aside owner's project, while it is a project the store holds. */
  asideProjectId: string | undefined;
  consoles: ReadonlyMap<string, Console>;
  currentConsole: Console | undefined;
  selectedSession: Session | undefined;
  selectedSessionId: string | undefined;
  focusTerminal: () => void;
}) {
  const daemon = useDaemon();
  // `currentConsole` falls back to the first console (`sidebarView.ts`), so it is missing exactly
  // when there is no console at all, and `shownConsole` below with it. No console means no project
  // and no session either, so the content area has nothing for its sidebar, terminal and aside and
  // shows the welcome screen instead. The panes' saved visibility and widths are left as they are:
  // this is an absence of content, not a choice, and they come back as the user left them.
  const noConsoles = currentConsole === undefined;
  // The right pane is a stack of its owner's panels (`panels/stackState.ts`) and shows while one is
  // open: the top bar's toggles, the F6 cycle and the pane itself all read the owner's stack. With
  // none open the pane floats in while the pointer is on a kind's toggle, showing that panel.
  // Its Report panel is a session's, not the project's (`panels/usePanelStacks.ts`): the pages are
  // those of the console session that owns the pane, or of the selected project session.
  const projectReportSession = useProjectReportSession(asideOwner, selectedSession);
  const { stacks: panelStacks, setStack: setPanelStack } = usePanelStacks(projectReportSession);
  const ownerKind = asideOwner?.kind;
  const ownerOrder = ownerKind ? OWNER_KINDS[ownerKind] : [];
  const reportSessionId = asideOwner?.kind === "report" ? asideOwner.consoleSession : projectReportSession;
  // A session's kind has no toggle and no panel while its session has no pages to show.
  const unavailableKinds = unavailableKindsOf(ownerKind, reportSessionId);
  const availableKinds = ownerKind ? OWNER_KINDS[ownerKind].filter((kind) => !unavailableKinds.includes(kind)) : [];
  // Too narrow a window to dock the pane (a phone): one panel at a time floats over the agent
  // terminal, which keeps the whole width (`useAsideFloats`, `panels/useFloatingPanel.ts`). The
  // drawer's open state is kept here, as the floating panel has to know it before the pane toggles
  // run.
  const asideFloats = useAsideFloats();
  const isNarrow = useIsNarrow();
  const [asidePinned, setAsidePinned] = useDockedPanelVisible("aside");
  const asideOverlay = !asideFloats && (isNarrow || !asidePinned);
  const [revealAside, setRevealAside] = useState(false);
  const drawer = useState(false);
  const floating = useFloatingPanel({ floats: asideFloats, owner: ownerKind, stacks: panelStacks, drawerOpen: drawer[0], focusTerminal, unavailable: unavailableKinds });
  const floatingShown = floating.kind !== undefined;
  const storedStack = ownerKind ? panelStacks[ownerKind] : undefined;
  const ownerStack = asideFloats && ownerKind ? { open: floating.kind ? [floating.kind] : [], shares: {} } : storedStack;
  const openPanels = ownerStack ? shownKinds(ownerStack, ownerOrder) as PanelKind[] : [];
  const paneShown = openPanels.length > 0;
  // A maximized panel (`PanelStack.tsx`) leaves the pane's column and covers
  // the agent terminal's area, which is inert meanwhile and not an F6 stop; the sidebar and the
  // panels left in the column stay as they are. With none left there, the column folds away and the
  // area, with the panel over it, reaches the window's end.
  const covered = !asideOverlay && ownerStack !== undefined && maximizedKind(ownerStack, ownerOrder) !== undefined;
  const stackedPanels = ownerStack ? stackedKinds(asideOverlay ? restoreStack(ownerStack) : ownerStack, ownerOrder) : [];
  const columnShown = stackedPanels.length > 0;
  // With no card in the column (none open, or the one open maximized) a panel that is not open
  // floats in while the pointer rests on its toggle.
  const panes = usePaneToggles({
    hasSidebar: !noConsoles,
    // Below the width that floats panels over the terminal a panel opens only on a press.
    asidePeekable: hasAside && (asideOverlay || !columnShown) && !asideFloats,
    focusTerminal,
    floatingPanel: floatingShown,
    closeFloatingPanel: floating.close,
    drawer,
  });
  const asideDocked = hasAside && columnShown && !asideFloats && !asideOverlay;
  useEffect(() => {
    if (!revealAside || !asideOverlay) return;
    panes.asidePeek.reveal(true);
    setRevealAside(false);
  }, [revealAside, asideOverlay]);
  /** Floats `kind` of `owner`, out of the sidebar's drawer's way. */
  const openFloating = (owner: OwnerKind, kind: PanelKind) => {
    floating.open(owner, kind);
    if (panes.sidebarOpen) panes.closeSidebar();
  };
  const [peekKind, setPeekKind] = useState<PanelKind>();
  // The panels the project menu need not offer, as they are on screen: open, or floating in on hover.
  const onScreen = asideOverlay && !panes.asidePeek.active ? [] : openPanels;
  const panelsOnScreen = panes.asidePeek.active && peekKind !== undefined && !openPanels.includes(peekKind) ? [...onScreen, peekKind] : onScreen;
  // Every sidebar row takes it, so it keeps its identity until the panels on screen change.
  const panelsKey = panelsOnScreen.join(",");
  const ownerProjectId = ownerKind === "project" ? asideProjectId : undefined;
  const visibleProjectPanels = ownerProjectId === undefined ? undefined : { project: ownerProjectId, kinds: panelsKey === "" ? [] : panelsKey.split(",") };
  // Where the pane floats, the list is also kept up to date, slowly, for the swipe to know whether the panel has anything to show.
  const processPanel = useProcessPanel(asideOwner, selectedSessionId, panelsOnScreen.includes("processes") ? "live" : asideFloats ? "background" : "off");
  const hasShellTerminals = useDaemonStore((s) => asideOwner?.kind === "project" && Array.from(s.shellTerminals.values()).some((terminal) => terminal.project_id === asideOwner.project));
  // A swipe opens only a panel with something to show: processes until the list is known to be empty, a terminal once there is one.
  const swipeable = (kind: PanelKind) => {
    if (kind === "processes") return processPanel.error !== undefined || processPanel.trees === undefined || processPanel.trees.length > 0;
    return kind !== "terminal" || hasShellTerminals;
  };
  const swipeKinds = availableKinds.filter(swipeable);
  // A panel kind's toggle shows or hides its panel; pressing it while the pane floats in docks it.
  const togglePanel = (kind: PanelKind) => {
    if (!ownerKind || !ownerStack) return;
    if (asideFloats) {
      // A card floating in on hover is the one this opens, so it keeps focus where it lands.
      panes.asidePeek.hide(false);
      if (floating.kind === kind) return floating.close();
      openFloating(ownerKind, kind);
      if (kind === "terminal" && asideOwner?.kind === "project") void startShellTerminal(daemon, asideOwner.project, { ifNone: true });
      return;
    }
    if (asideOverlay) {
      const opened = !openPanels.includes(kind);
      const closing = !opened && panes.asidePeek.active;
      const next = closing ? toggleKind(ownerStack, kind, ownerOrder) : openPanel(ownerStack, kind, ownerOrder);
      const remaining = shownKinds(next, ownerOrder) as PanelKind[];
      setPanelStack(ownerKind, next);
      setPeekKind(closing ? remaining[0] : kind);
      if (remaining.length === 0) panes.asidePeek.hide();
      else panes.asidePeek.reveal(true);
      if (!closing) floating.noteOpened(ownerKind, kind);
      if (kind === "terminal" && opened && asideOwner?.kind === "project") void startShellTerminal(daemon, asideOwner.project, { ifNone: true });
      return;
    }
    // A card floating in docks in place, keeping focus, except beside a folded column, where it
    // floats in a pane of its own and is shown again in the docked one.
    panes.asidePeek.hide(paneShown && !columnShown);
    const next = toggleKind(ownerStack, kind, ownerOrder);
    setPanelStack(ownerKind, next);
    // Opening the Terminal panel on a project with no terminal starts one; floating it in does not.
    const opened = !shownKinds(ownerStack, ownerOrder).includes(kind) && shownKinds(next, ownerOrder).includes(kind);
    if (opened) floating.noteOpened(ownerKind, kind);
    if (kind === "terminal" && opened && asideOwner?.kind === "project") void startShellTerminal(daemon, asideOwner.project, { ifNone: true });
  };
  // A maximized panel is its owner kind's: once the pane passes to an owner of another kind (a
  // console session) or to none, it is restored, so coming back shows
  // the stack as it was. Moving between projects keeps it.
  useEffect(() => {
    for (const kind of Object.keys(OWNER_KINDS) as OwnerKind[]) {
      if (kind !== ownerKind && panelStacks[kind].maximized !== undefined) setPanelStack(kind, restoreStack(panelStacks[kind]), { persist: false });
    }
  }, [ownerKind]);
  // The pointer resting on a toggle that goes away (the Report toggle, once its last page is
  // deleted) never leaves it, so no pointerleave would end the peek, and the card would float back
  // by itself when a page arrives long after the pointer has moved on.
  const peekGone = peekKind !== undefined && unavailableKinds.includes(peekKind);
  const hidePeek = panes.asidePeek.hide;
  useLayoutEffect(() => {
    if (peekGone) hidePeek();
  }, [peekGone]);
  const hoverPanelToggle = (kind: PanelKind, hovered: boolean) => {
    if (!hovered) return panes.asidePeek.leave();
    if (!asideOverlay && (columnShown || openPanels.includes(kind))) return;
    setPeekKind(kind);
    panes.asidePeek.reveal();
  };
  const hoverAside = (hovered: boolean) => {
    if (!asideOverlay || !ownerKind) return;
    if (!hovered) return panes.asidePeek.leave();
    setPeekKind(openPanels[0] ?? availableKinds[0]);
    panes.asidePeek.reveal();
  };
  // The hidden sidebar floats in showing the console whose avatar the pointer is on, which is not
  // the current one until something is pressed in it. That is a plain console view: a focus mode
  // belongs to the current console.
  const peekedId = panes.sidebarPeek.consoleId;
  const peekedConsole = peekedId ? consoles.get(peekedId) : undefined;
  const shownConsole = peekedConsole ?? currentConsole;
  const previewing = shownConsole !== currentConsole;
  // A previewed console that is deleted takes its avatar, and so the pointer's leaving it, with it.
  const { active: peeking, leaveConsole } = panes.sidebarPeek;
  useEffect(() => {
    if (peekedId && peeking && !consoles.has(peekedId)) leaveConsole();
  }, [peekedId, peeking, consoles]);
  // Below the `phone` breakpoint the rail leaves the row and comes in with the sidebar's drawer; the
  // welcome screen has no drawer, and the rail stays as the way to New console and Settings.
  const railOnDemand = !noConsoles;
  const railAway = railOnDemand && asideFloats && !panes.sidebarOpen;
  // The drawer closing takes the rail with it: focus left in it would fall to `<body>`. A layout
  // effect, so it runs before the browser's own focus fix-up for what just became `inert`, which a
  // swipe's close (from a native listener) would otherwise reach first.
  useLayoutEffect(() => {
    if (railAway && document.activeElement?.closest("[data-region=rail]")) focusTerminal();
  }, [railAway]);

  // The archive view is in the agent terminal's area, which a maximized panel covers: restoring
  // it is how the user sees what was asked for, whether opened or arrived at through the history.
  const revealArchive = () => {
    if (ownerKind && storedStack && maximizedKind(storedStack, ownerOrder) !== undefined) setPanelStack(ownerKind, restoreStack(storedStack), { persist: false });
    // So does a panel floating over that area.
    floating.close();
  };
  const stableToggleSidebar = useStableHandler(panes.toggleSidebar);
  const stableTogglePanel = useStableHandler(togglePanel);
  const stableHoverPanelToggle = useStableHandler(hoverPanelToggle);
  const stableHoverAside = useStableHandler(hoverAside);
  return {
    noConsoles,
    ownerKind,
    ownerOrder,
    reportSessionId,
    availableKinds,
    asideFloats,
    asideOverlay,
    asidePinned,
    notePanelOpened: (owner: OwnerKind, kind: PanelKind) => {
      floating.noteOpened(owner, kind);
      if (asideOverlay) panes.asidePeek.reveal(true);
    },
    toggleAsideDocked: () => {
      if (isNarrow) {
        setPeekKind(openPanels[0] ?? availableKinds[0]);
        panes.asidePeek.reveal(true);
        return;
      }
      setAsidePinned(!asidePinned);
      if (asidePinned) setRevealAside(true);
      else panes.asidePeek.hide(false);
    },
    floating,
    floatingShown,
    ownerStack,
    openPanels,
    paneShown,
    covered,
    stackedPanels,
    columnShown,
    panes,
    isNarrow,
    asideDocked,
    openFloating,
    panelStacks,
    setPanelStack,
    peekKind,
    visibleProjectPanels,
    processPanel,
    swipeKinds,
    togglePanel,
    shownConsole,
    previewing,
    railOnDemand,
    railAway,
    revealArchive,
    stableToggleSidebar,
    stableTogglePanel,
    stableHoverPanelToggle,
    stableHoverAside,
  };
}
