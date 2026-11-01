import { AttendedStatusAnnouncer } from "./components/announce/AttendedStatusAnnouncer";
import type { PlainMessageKey } from "./i18n/catalog";
import React, { useEffect, useRef, useState } from "react";

import { ArchiveLayer } from "./archive/ArchiveLayer";
import { forgetProjectsOtherThan } from "./project-browser/browserState";
import { LazyTerminalFileViewer } from "./project-browser/files/LazyTerminalFileViewer";
import { OpenShellTerminal } from "./project-browser/RowActions";
import { ConnectionBanner } from "./shell/ConnectionBanner";
import { ContentPanel } from "./shell/ContentPanel";
import { HandOffOnUnmount } from "./components/focus/HandOffOnUnmount";
import { PaneResizeHandle } from "./layout/PaneResizeHandle";
import { refocusIfLost } from "./components/focus/refocusIfLost";
import { focusNewConsole, Rail } from "./shell/Rail";
import { Scrim } from "./shell/Scrim";
import { BareTitleBar, TitleBar } from "./shell/TitleBar";
import { Toasts } from "./shell/Toasts";
import { useFirstConsoleHandoff } from "./shell/useFirstConsoleHandoff";
import { useStableHandler } from "./components/hooks/useLatest";
import { focusAfterLoss, setFocusAfterModalLoss } from "./components/focus/regionFocus";
import { ConfirmDialog } from "./components/dialog/ConfirmDialog";
import type { DialogRequest } from "./dialogs/dialogRequest";
import { PendingQuestionDialog, usePendingQuestion } from "./dialogs/PendingQuestionDialog";
import { RequestedDialog } from "./dialogs/RequestedDialog";
import { useConfirmIncompleteLaunch } from "./dialogs/useConfirmIncompleteLaunch";
import { useQuickCreateSession } from "./dialogs/useQuickCreateSession";
import { useT } from "./i18n/react";
import { liveOwner, ownerKey, type AsideOwner } from "./layout/asideOwner";
import { columnSpan, isDockEasing } from "./layout/paneDockMotion";
import { useRegionCycle } from "./layout/useRegionCycle";
import { useAppExit } from "./lifecycle/useAppExit";
import { useGitStatusSchedule } from "./lifecycle/useGitStatusSchedule";
import { useInterfaceLanguageSync } from "./lifecycle/useInterfaceLanguageSync";
import { useStatusItemMenu } from "./lifecycle/useStatusItemMenu";
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { sessionDisplayTitle } from "./sessionLabel";
import { PANEL_ENTRIES } from "./panels/panelRegistry";
import { ProcessPanelProvider } from "./process/useProcessPanel";
import { RightPane } from "./panels/RightPane";
import { panelId, panelRegion, shownPanelRegions, type PanelKind } from "./panels/stackState";
import { LoginPageOpener } from "./settings/accounts/AccountLogin";
import { SettingsDialog } from "./settings/SettingsDialog";
import { registerSettingsLauncher } from "./settings/settingsLauncher";
import { registerSessionModelLauncher } from "./dialogs/sessionModelLauncher";
import { useSettingsDialog } from "./settings/useSettingsDialog";
import { updateNoticeKind } from "./updates/noticeKind";
import { UpdateNotice } from "./updates/UpdateNotice";
import { useUpdateSelector, useUpdateStore } from "./updates/UpdatesProvider";
import { useSessionActionShortcuts } from "./sidebar/useSessionActionShortcuts";
import { belongsToFocus } from "./sidebar/focus";
import { consoleSessionAbove } from "./sessions/order";
import { useWaitingCount } from "./sessions/useSessions";
import { indexSessions, NO_SESSIONS, sessionsOf } from "./sessions/sessionIndex";
import { Sidebar } from "./sidebar/Sidebar";
import { useSidebarView } from "./sidebar/sidebarView";
import { useDaemon, useDaemonStore, useDaemonStoreEqual } from "./store";
import { terminalEmptyState } from "./terminal/emptyState";
import { TerminalPane } from "./terminal/TerminalPane";
import { WelcomeScreen } from "./WelcomeScreen";
import { useAnnouncement } from "./app/useAnnouncement";
import { useAppNavigation } from "./app/useAppNavigation";
import { useArchiveSession } from "./app/useArchiveSession";
import { useArchiveView } from "./app/useArchiveView";
import { useConsoleSwitch } from "./app/useConsoleSwitch";
import { DRAWER_CONTROLS, useDialogRouting } from "./app/useDialogRouting";
import { useFocusMode } from "./app/useFocusMode";
import { useLinkedFile } from "./app/useLinkedFile";
import { usePaneLayout } from "./app/usePaneLayout";
import { usePaneSwipe } from "./app/usePaneSwipe";
import { usePaneWidths } from "./app/usePaneWidths";
import { useProjectPanels } from "./app/useProjectPanels";
import { useRowRequests } from "./app/useRowRequests";
import { useSelectedSession } from "./app/useSelectedSession";
import { useSessionActions } from "./app/useSessionActions";
import { useSessionRequests } from "./app/useSessionRequests";
import { useTerminalFocus } from "./app/useTerminalFocus";

export function App(): React.ReactElement {
  const t = useT();
  const { request, reconnect, store } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const hosts = useDaemonStore((s) => s.hosts);
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultAccountNames = useDaemonStore((s) => s.settings.default_account_names);
  // Keeps Settings out of the way of the question's dialog.
  const questionPending = usePendingQuestion();
  const { terminalRef, welcomeRef, agentAreaRef, focusTerminal } = useTerminalFocus();
  const { selectedSessionId, selectedSession, setSelectedSessionId } = useSelectedSession();
  const [dialogRequest, setDialogRequest] = useState<DialogRequest>();
  const { linkedFile, viewLinkedFile, setLinkedFile, closeLinkedFile } = useLinkedFile({ focusTerminal });

  const consoleList = Array.from(consoles.values());
  const projectList = Array.from(projects.values());
  // The sessions are not read here as a whole: every status change of every session would render
  // this component and everything it lays out. What it needs of them — the selected one, the
  // aside owner's, a count — is read as that alone, and the rest looked up in the store by whoever
  // acts.
  const allSessions = () => Array.from(store.getState().sessions.values());
  // Who the aside belongs to (`asideOwner.ts`): set by what the user does, and gone with what it
  // names. Computed here (rather than where it is consumed below) because the pane toggles, the
  // region cycle and the panes' width clamps need it too.
  const [asideOwnerState, setAsideOwner] = useState<AsideOwner>();
  const asideOwner = useDaemonStore((s) => liveOwner(asideOwnerState, s.projects, s.sessions));
  const asideProject = asideOwner?.kind === "project" ? projects.get(asideOwner.project) : undefined;
  const hasAside = asideOwner !== undefined;
  const sidebarView = useSidebarView(consoleList);
  useGitStatusSchedule(sidebarView.currentConsole?.id);
  useInterfaceLanguageSync();

  const currentConsole = sidebarView.currentConsole;
  const {
    noConsoles,
    ownerKind,
    ownerOrder,
    reportSessionId,
    availableKinds,
    asideFloats,
    asideOverlay,
    asidePinned,
    toggleAsideDocked,
    notePanelOpened,
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
  } = usePaneLayout({
    asideOwner,
    hasAside,
    asideProjectId: asideProject?.id,
    consoles,
    currentConsole,
    selectedSession,
    selectedSessionId,
    focusTerminal,
  });
  const {
    archiveScope,
    setArchiveScope,
    archiveConsole,
    archiveProject,
    archiveBoundTo,
    archiveOpen,
    archiveSessions,
    archiveRestore,
    setArchiveRestore,
    archiveViewStates,
    closeArchive,
    onArchiveViewState,
    stableOpenArchive,
  } = useArchiveView({ consoles, projects, revealArchive, sidebarOpen: panes.sidebarOpen, closeSidebar: panes.closeSidebar, focusTerminal });

  useFirstConsoleHandoff(hosts !== undefined && noConsoles, dialogRequest?.kind === "new-console");

  // The terminal offers the next action for the current console and focus scope, even while the
  // sidebar previews another console. A selected session or an archive keeps that offer hidden.
  const emptyState = useDaemonStoreEqual((s) =>
    selectedSession === undefined && !archiveOpen
      ? terminalEmptyState(hosts !== undefined, currentConsole, sidebarView.focus, projectList, currentConsole ? sessionsOf(indexSessions(s.sessions).byConsole, currentConsole.id) : NO_SESSIONS, agentAvailability, s.settings.added_agents)
      : undefined,
  );
  // The "no agent added" state offers Add agent; only "no agent installed" has nothing to press.
  const emptyHasAction = emptyState !== undefined && !(emptyState.kind === "no-agents" && emptyState.reason === "install");
  // Only the store and whether the notice is shown: the whole state changes with every tick of a download.
  const updateStore = useUpdateStore();
  const updateNoticeShown = useUpdateSelector((state) => updateNoticeKind(state) !== undefined) ?? false;
  // F6 includes only regions currently on screen, including a revealed floating group.
  // The top bar is always available.
  const regionCycle = useRegionCycle({
    shown: {
      topbar: true,
      rail: !railAway,
      welcome: noConsoles,
      sidebar: !noConsoles && panes.sidebarShown,
      archive: archiveOpen && !covered && !floatingShown,
      terminal: (selectedSession !== undefined || emptyHasAction) && !archiveOpen && !covered && !floatingShown,
      banner: connectionState === "reconnecting" || connectionState === "closed",
      update: updateNoticeShown,
    },
    panels: shownPanelRegions(asideOverlay && !panes.asidePeek.active ? undefined : ownerStack, ownerKind),
    focusTerminal,
  });

  const { sidebarColumnPhase, setSidebarColumnPhase, sidebarWidth, asideWidth, asideLayout, asideFloat } = usePaneWidths({
    noConsoles,
    isNarrow,
    sidebarDocked: panes.sidebarDocked,
    terminalPanelStacked: stackedPanels.includes("terminal"),
    stackedPanelCount: stackedPanels.length,
    asideOverlay,
    asideDocked,
    paneShown,
    columnShown,
    asideFloats,
    asidePeek: panes.asidePeek,
    floatingKind: floating.kind,
  });

  const { followUp, setOwnerByUser, setSelectionByUser, switchConsole, stableSwitchConsole } = useConsoleSwitch({
    ready: hosts !== undefined,
    projects,
    currentConsoleId: sidebarView.currentConsole?.id,
    selectConsole: sidebarView.selectConsole,
    selectedSession,
    setSelectedSessionId,
    asideOwnerState,
    asideOwner,
    setAsideOwner,
    setArchiveScope,
  });
  // A removed project's browser state has nothing left to apply to.
  useEffect(() => {
    if (hosts) forgetProjectsOtherThan(new Set(projects.keys()));
  }, [projects, hosts]);
  // What the aside showed goes with its owner; focus that was inside it goes to the terminal.
  useEffect(() => refocusIfLost(focusTerminal), [ownerKey(asideOwner)]);

  useWaitingNotifications(consoles, projects, hosts !== undefined);
  useStatusItemMenu({
    selectedSessionId,
    currentConsoleId: sidebarView.currentConsole?.id,
    ready: hosts !== undefined,
    onSessionChosen: (session) => {
      selectSession(session);
      focusTerminal();
    },
  });

  // The exit flow's frontend owner: it registers with the shell on mount, which is what makes the
  // window quittable. Only its confirmation is needed here: the main app has no separate "Quit"
  // control.
  const { exitConfirmOpen, exitHolders, exitRestarts, closeExitConfirm, confirmExit, requestRestart } = useAppExit({
    getSessions: allSessions,
    getShellTerminals: () => Array.from(store.getState().shellTerminals.values()),
    requestShutdown: () => request({ type: "shutdown" }),
  });

  // An installed update takes effect on a restart, which only ever happens through the exit flow above, so the
  // user is asked about the sessions it ends and can cancel. The update notice and Settings ask for it here.
  const answerRestart = useStableHandler(() => void requestRestart());
  useEffect(() => updateStore?.onRestartRequested(answerRestart), [updateStore, answerRestart]);

  // The question before a launch on an account whose sharing change did not finish; this window's
  // own launches (a resume, a quick create) go through it and render it at this level.
  const { confirmLaunch, confirmation: launchConfirmation } = useConfirmIncompleteLaunch();

  const otherModalOpen = dialogRequest !== undefined || questionPending || exitConfirmOpen || launchConfirmation !== undefined;
  // Settings opened from a control of the open drawer (the rail's, on a phone) hands focus back to it
  // as it closes, as any dialog opened there does (`focusDrawerTrigger`).
  const settingsFromDrawer = useRef(false);
  const { settingsOpen, settingsNavigation, openSettings, openSettingsAt, closeSettings } = useSettingsDialog({
    ready: hosts !== undefined,
    otherModalOpen,
    focusTerminal: () => {
      if (!(settingsFromDrawer.current && panes.sidebarOpen)) focusTerminal();
    },
  });
  // After the dialog's own close effect (declared above) has read it, whether or not that one asked.
  useEffect(() => {
    if (!settingsOpen) settingsFromDrawer.current = false;
  }, [settingsOpen]);

  const { announce, announcementRef, announcementText } = useAnnouncement({ covered: otherModalOpen || settingsOpen });
  const { resumingIds, resumeSession, resumeFromDialog, switchAccounts, changeSessionModel } = useSessionRequests({ confirmLaunch, announce });

  const {
    showOpenedSession,
    selectSession,
    stableSelectSession,
    stableSelectFromSidebar,
    stableSwitchConsoleSession,
    stableReopenSession,
    stableNextWaiting,
  } = useSessionActions({
    focus: sidebarView.focus,
    setFocus: sidebarView.setFocus,
    selectConsole: sidebarView.selectConsole,
    setSelectionByUser,
    setOwnerByUser,
    archiveOpen,
    setArchiveScope,
    focusTerminal,
    resumeSession,
    asideFloats,
    sidebarOpen: panes.sidebarOpen,
    closeSidebar: panes.closeSidebar,
    consoleList,
    projectList,
    selectedSessionId,
  });

  const quickCreateSession = useQuickCreateSession(showOpenedSession, sidebarView.currentConsole?.id, confirmLaunch);

  const { openDialog, closeDialog, openConsoleSession, focusDrawerTrigger, stableOpenDialog, stableOpenConsoleSession } = useDialogRouting({
    consoles,
    consoleList,
    sidebarOpen: panes.sidebarOpen,
    setDialogRequest,
    switchConsole,
  });
  const openSettingsFromDrawer = () => {
    // Focus already in the drawer (Settings opened by a key) is where the dialog hands it back to.
    settingsFromDrawer.current = focusDrawerTrigger() || (panes.sidebarOpen && !!document.activeElement?.closest(DRAWER_CONTROLS));
    openSettings();
  };

  const { stableOpenProjectPanel, stableOpenShellTerminal } = useProjectPanels({
    setOwnerByUser,
    asideFloats,
    openFloating,
    panelStacks,
    setPanelStack,
    noteOpened: notePanelOpened,
    sidebarOpen: panes.sidebarOpen,
    closeSidebar: panes.closeSidebar,
    showAllOf: processPanel.showAllOf,
  });

  // A selection can leave a console session's focus mode after a status or binding change. Its
  // context follows the owner chain too (`belongsToFocus`), so the console session above it is a
  // dependency of its own. The whole session map is not: entering focus mode through a summary
  // chip deliberately keeps the selection, and an unrelated status event must not drop that view.
  // Project context includes its hidden console-bound sessions, whatever the card visibility.
  const consoleSessionAboveSelection = useDaemonStore(
    (s) => selectedSession && consoleSessionAbove(selectedSession, (id) => s.sessions.get(id))?.id,
  );
  useEffect(() => {
    if (sidebarView.focus && selectedSession && !belongsToFocus(sidebarView.focus, selectedSession, store.getState().sessions)) {
      sidebarView.setFocus(undefined);
    }
  }, [selectedSession?.id, selectedSession?.status, selectedSession?.bound_to, consoleSessionAboveSelection]);

  const { moveConsoleSession, stableFocus } = useFocusMode({
    focus: sidebarView.focus,
    setFocus: sidebarView.setFocus,
    selectConsole: sidebarView.selectConsole,
    currentConsoleId: sidebarView.currentConsole?.id,
    selectedSession,
    projects,
    sidebarShown: panes.sidebarShown,
    selectSession,
    focusTerminal,
  });

  const { canGoBack, canGoForward, moveByShortcut, stableBack, stableForward, stableCommitPreview } = useAppNavigation({
    ready: hosts !== undefined,
    focus: sidebarView.focus,
    setFocus: sidebarView.setFocus,
    selectConsole: sidebarView.selectConsole,
    currentConsoleId: sidebarView.currentConsole?.id,
    selectedSession,
    selectedSessionId,
    setSelectedSessionId,
    archiveOpen,
    archiveScope,
    setArchiveScope,
    setArchiveRestore,
    archiveViewStates,
    revealArchive,
    consoles,
    projects,
    asideOwner,
    setAsideOwner,
    holdForPress: followUp.holdForPress,
    releasePress: followUp.releasePress,
    previewing,
    shownConsole,
    focusTerminal,
  });

  const { archiveSession, stableArchiveSession } = useArchiveSession({
    resumingIds,
    dialogRequest,
    launchQuestionOpen: launchConfirmation !== undefined,
    openDialog,
    focusTerminal,
  });

  const waitingCount = useWaitingCount();
  // Handed to the memoised rows (the sidebar's, the rail's and the archive view's) and the top bar as
  // one function for good, so the rows are rendered again only for their own records.
  const stableQuickCreateSession = useStableHandler(quickCreateSession);
  const {
    stableSetSessionColour,
    stableOpenProjectDirectory,
    stableOpenRepository,
    stableOpenSystemTerminal,
    stableCopySessionId,
    stableCopyProjectName,
    stablePullProjectBranch,
    stablePushProjectBranch,
    stableLoadProjectBranches,
    stableSetLocked,
    stableSetPinned,
  } = useRowRequests();
  useSessionActionShortcuts(selectedSession, {
    onSetLocked: stableSetLocked,
    onSetPinned: stableSetPinned,
    onArchiveSession: archiveSession,
    onCopySessionId: stableCopySessionId,
  });
  usePaneSwipe({
    asideFloats,
    noConsoles,
    sidebarOpen: panes.sidebarOpen,
    toggleSidebar: panes.toggleSidebar,
    closeSidebar: panes.closeSidebar,
    swipeKinds,
    ownerKind,
    ownerOrder,
    floatingKind: floating.kind,
    closeFloating: floating.close,
    lastOpened: floating.lastOpened,
    togglePanel,
  });
  const stableOpenSettingsAt = useStableHandler(openSettingsAt);
  useEffect(() => registerSettingsLauncher(stableOpenSettingsAt), [stableOpenSettingsAt]);
  // The error alert's Change model button; it opens nothing over another dialog or Settings, which would lose what the
  // user has typed there.
  const openSessionModel = useStableHandler((sessionId: string) => {
    const session = store.getState().sessions.get(sessionId);
    if (session && !(otherModalOpen || settingsOpen)) openDialog({ kind: "session-model", session });
  });
  useEffect(() => registerSessionModelLauncher(openSessionModel), [openSessionModel]);
  // Where focus goes once the last alert or pending question, or Settings with no dialog under it, closes with nothing to
  // give it back to.
  const focusAfterModal = useStableHandler(() => focusAfterLoss(focusTerminal));
  useEffect(() => setFocusAfterModalLoss(focusAfterModal), [focusAfterModal]);

  // Nothing to show until the first snapshot. The toasts belong here all the same, and so does the
  // banner with the Retry the client offers once it has given up — otherwise this screen would say
  // it is connecting forever with nothing the user can do about it.
  if (!hosts) {
    return (
      <div className="flex h-full flex-col">
        <BareTitleBar />
        <Toasts focusTerminal={focusTerminal} />
        <ContentPanel bare>
          <div className="flex flex-1 items-center justify-center text-app-muted">
            {connectionState === "closed" ? t("app.daemonNotAnswering") : t("app.connecting")}
          </div>
        </ContentPanel>
        {/* Mounted only while closed, unlike the main screen's always-mounted banner, so its focus
            hand-off does not run as it goes; nothing is lost, as this screen has no terminal and an
            empty bar to hand focus to. */}
        {connectionState === "closed" && (
          <ConnectionBanner state={connectionState} onRetry={reconnect} focusTerminal={focusTerminal} />
        )}
      </div>
    );
  }

  return (
    <div className="relative flex h-full flex-col">
      <Toasts focusTerminal={focusTerminal} />
      <TitleBar
        sidebarShown={panes.sidebarShown}
        sidebarRevealOnly={isNarrow && !asideFloats}
        hasSidebar={!noConsoles}
        sidebarSpan={noConsoles ? 0 : columnSpan(sidebarColumnPhase, sidebarWidth.width)}
        sidebarEasing={isDockEasing(sidebarColumnPhase)}
        onToggleSidebar={stableToggleSidebar}
        onSidebarToggleEnter={panes.sidebarPeek.reveal}
        onSidebarToggleLeave={panes.sidebarPeek.leave}
        asideSpan={asideDocked ? asideWidth.width : 0}
        asidePinned={asidePinned}
        onToggleAsideDocked={ownerKind && !asideFloats ? toggleAsideDocked : undefined}
        asideAutomatic={isNarrow}
        asideRevealed={panes.asidePeek.active}
        onAsideHoverChange={stableHoverAside}
        panelToggles={ownerKind ? availableKinds.map((kind) => {
          const entry = PANEL_ENTRIES[kind];
          const open = openPanels.includes(kind) && (!asideOverlay || panes.asidePeek.active);
          return {
            kind,
            label: t(open ? entry.toggle.hide : entry.toggle.show),
            Icon: entry.icon,
            open,
            regionId: open ? panelId(kind) : undefined,
            onPress: () => stableTogglePanel(kind),
            onHoverChange: (hovered: boolean) => stableHoverPanelToggle(kind, hovered),
          };
        }) : undefined}
        canGoBack={canGoBack}
        canGoForward={canGoForward}
        onBack={stableBack}
        onForward={stableForward}
        focusTerminal={focusTerminal}
        selectedSession={selectedSession}
        viewTrail={
          archiveOpen && archiveConsole
            ? archiveProject
              ? [archiveConsole.name, archiveProject.name, t("archive.heading.project")]
              : archiveBoundTo
                ? [archiveConsole.name, sessionDisplayTitle(t, archiveBoundTo), t("archive.heading.boundSessions")]
                : [archiveConsole.name, t("archive.heading.consoleSessions")]
            : undefined
        }
      />
      <div className="flex min-h-0 flex-1">
        <Rail
          onDemand={railOnDemand}
          away={railAway}
          selectedSessionId={selectedSessionId}
          consoles={consoleList}
          currentConsoleId={sidebarView.currentConsole?.id}
          onSelectConsole={stableSwitchConsole}
          onConsoleEnter={panes.sidebarPeek.hoverConsole}
          onConsoleLeave={panes.sidebarPeek.leaveConsole}
          onRailHoverChange={(hovered) => hovered ? panes.sidebarPeek.hoverRail() : panes.sidebarPeek.leaveRail()}
          hoverOpensSidebar={panes.sidebarPeek.peekable}
          onOpenDialog={stableOpenDialog}
          onSelectSession={stableSelectFromSidebar}
          onOpenArchive={stableOpenArchive}
          waitingCount={waitingCount}
          onNextWaiting={stableNextWaiting}
          onOpenSettings={openSettingsFromDrawer}
          onOpenSettingsAt={stableOpenSettingsAt}
          focusTerminal={focusTerminal}
        />
        <ContentPanel startsAtEdge={railOnDemand}>
          {/* The same as `noConsoles`, read off the console itself so that it narrows. */}
          {shownConsole === undefined ? (
            <WelcomeScreen ref={welcomeRef} onOpenDialog={openDialog} />
          ) : (
            <>
              {/* Before anything else here, so focus is handed to the rail while the sidebar, the
                  terminal and the aside it may be in are still in the document: the last console
                  going (here or in another window) removes all of them at once. */}
              <HandOffOnUnmount
                from="[data-region=sidebar], [data-region=archive], [data-region=terminal], [data-region=aside], [data-region^=panel-]"
                to={focusNewConsole}
              />
              <Scrim open={panes.sidebarOpen} label={t("app.closeSessions")} onClose={panes.closeSidebar} />
              {/* A panel floating over the agent terminal is dismissed by a press beside it, as the
                  drawer is; the top bar, with the panel's own toggle, stays out of it. */}
              <Scrim open={floatingShown} label={t("panel.close")} onClose={floating.close} clearOfRail={false} />
              <Sidebar
                projects={projectList}
                selectedSessionId={selectedSessionId}
                shownConsole={shownConsole}
                focus={previewing ? undefined : sidebarView.focus}
                onSelectSession={stableSelectFromSidebar}
                onSwitchConsoleSession={stableSwitchConsoleSession}
                onOpenConsoleSession={stableOpenConsoleSession}
                onOpenDialog={stableOpenDialog}
                onQuickCreateSession={stableQuickCreateSession}
                onFocus={stableFocus}
                onOpenArchive={stableOpenArchive}
                visibleProjectPanels={visibleProjectPanels}
                onOpenProjectPanel={stableOpenProjectPanel}
                onOpenProjectDirectory={stableOpenProjectDirectory}
                onOpenSystemTerminal={stableOpenSystemTerminal}
                onOpenRepository={stableOpenRepository}
                onCopyProjectName={stableCopyProjectName}
                onCopySessionId={stableCopySessionId}
                onPullProjectBranch={stablePullProjectBranch}
                onPushProjectBranch={stablePushProjectBranch}
                onLoadProjectBranches={stableLoadProjectBranches}
                onSetSessionColour={stableSetSessionColour}
                onSetLocked={stableSetLocked}
                onSetPinned={stableSetPinned}
                onReopenSession={stableReopenSession}
                onArchiveSession={stableArchiveSession}
                onOpenSettings={stableOpenSettingsAt}
                open={panes.sidebarOpen || (!asideFloats && panes.sidebarPeek.active)}
                drawerAway={isNarrow && !panes.sidebarOpen && !panes.sidebarPeek.active}
                peek={isNarrow || !panes.sidebarDocked ? panes.sidebarPeek : undefined}
                onPeekPress={stableCommitPreview}
                onColumnPhase={setSidebarColumnPhase}
                sidebarWidth={sidebarWidth}
              />
              {panes.sidebarDocked && sidebarColumnPhase === "open" && (
                <PaneResizeHandle side="sidebar" paneWidth={sidebarWidth} />
              )}
              {/* Floating panes leave the agent area its full width; horizontal overflow remains a
                  guard while the docked sidebar's column is changing width. */}
              <main className="relative flex min-w-0 flex-1 overflow-x-auto docked:overflow-visible">
                {/* Takes the terminal's place in the row, so the archive covers the terminal alone, not the
                    aside beside it; the terminal fills it. Above the `docked` breakpoint the 520px basis
                    and floor are the aside's counterpart: with a 0 basis free space stays positive at any
                    window wider than the aside's own basis, flexbox never leaves the grow phase, and the
                    aside clip's shrink factor is never consulted. The open clip can shrink while the
                    sidebar's width eases, keeping the report within the viewport. 520px is about 53 columns at ~9.2px/column
                    off a real agent CLI (the terminal's padding eats the rest). Below the breakpoint the
                    side panes both float over the terminal. Its 398px floor is 40 columns at
                    the same ~9.2px/column plus the same padding allowance, under which the terminal stops
                    being usable at all. Mobile mode shows a single floating card and removes this
                    floor, so the terminal takes whatever width
                    there is, so the row never scrolls. */}
                {/* At the breakpoint it casts the content panel's soft shadow (`--panel-shadow`) over the
                    docked panes on either side, so the terminal reads as the layer above them. `z-1`
                    lifts it, with what it holds, over everything in the panes, the aside's positioned
                    content (sticky headings, scroll fades) included, which comes later in the row and
                    would otherwise cover the shadow; it stays under the resize handles and the floating
                    cards. The content panel clips the shadow on the other sides and wherever a pane is
                    closed. The clip-path cuts it on the aside's side: the aside's cards sit apart from the
                    terminal across a gap of the terminal's own background, which a shadow would draw a
                    line in. */}
                <div ref={agentAreaRef} inert={covered || floatingShown} className={`relative flex min-h-0 ${asideFloats ? "min-w-0 flex-1" : "min-w-[398px] flex-[1_1_398px]"} docked:z-1 docked:min-w-[520px] docked:flex-[1_1_520px] docked:shadow-(--panel-shadow) docked:[clip-path:inset(-12px_0_-12px_-12px)] docked:rtl:[clip-path:inset(-12px_-12px_-12px_0)]`}>
                  <ArchiveLayer
                    focusTerminal={focusTerminal}
                    view={archiveOpen && archiveConsole ? {
                      console: archiveConsole,
                      project: archiveProject,
                      boundTo: archiveBoundTo,
                      sessions: archiveSessions,
                      consoles,
                      projects,
                      accounts,
                      defaultNames: defaultAccountNames,
                      selectedSessionId,
                      onSelect: stableSelectSession,
                      onReopen: stableReopenSession,
                      onOpenDialog: stableOpenDialog,
                      dialogOpen: dialogRequest !== undefined,
                      onClose: closeArchive,
                      restore: archiveRestore,
                      onViewState: onArchiveViewState,
                    } : undefined}
                  />
                  <TerminalPane
                    ref={terminalRef}
                    resizing={isDockEasing(sidebarColumnPhase)}
                    onViewFile={viewLinkedFile}
                    onSelectSession={stableSelectSession}
                    restoreFocus={focusTerminal}
                    session={selectedSession}
                    onResume={resumeSession}
                    resuming={selectedSessionId !== undefined && resumingIds.has(selectedSessionId)}
                    modalOpen={otherModalOpen || settingsOpen}
                    onAddProject={emptyState?.kind === "add-project" && currentConsole
                      ? () => openDialog({ kind: "new-project", console: currentConsole })
                      : undefined}
                    noAgents={emptyState?.kind === "no-agents" ? emptyState.reason : undefined}
                    onAddAgent={() => openSettingsAt("accounts", { addAgent: true })}
                    newSession={emptyState && currentConsole && (emptyState.kind === "new-session" || emptyState.kind === "new-console-session")
                      ? {
                        message: t(emptyState.kind === "new-session" ? "sidebar.project.openSession" : "sidebar.consoleSessions.empty"),
                        label: t(emptyState.kind === "new-session" ? "sidebar.project.openSession" : "sidebar.consoleSessions.new"),
                        onPress: () => emptyState.kind === "new-session"
                          ? openDialog({ kind: "new-session", console: currentConsole, project: emptyState.project, binding: emptyState.binding })
                          : openConsoleSession(currentConsole),
                      }
                      : undefined}
                  />
                </div>
                {asideOwner && (
                  <OpenShellTerminal.Provider value={stableOpenShellTerminal}>
                    <ProcessPanelProvider value={processPanel}>
                      <RightPane
                        owner={asideOwner}
                        project={asideProject}
                        layout={asideLayout}
                        active={asideOverlay ? panes.asidePeek.active : paneShown || panes.asidePeek.active}
                        selectedSessionId={selectedSessionId}
                        reportSessionId={reportSessionId}
                        focusTerminal={focusTerminal}
                        frameKeys={{
                          onEscape: panes.dismissOverlays,
                          onCycleRegion: regionCycle.cycle,
                          onMoveHistory: moveByShortcut,
                          onMoveConsoleSession: moveConsoleSession,
                        }}
                        peekKind={peekKind}
                        onViewFile={viewLinkedFile}
                        onDismiss={() => panes.asidePeek.hide()}
                        maximizeArea={agentAreaRef}
                        areaEasing={isDockEasing(sidebarColumnPhase)}
                        float={asideFloat}
                        overlay={asideOverlay}
                        onHoverChange={stableHoverAside}
                        resizeHandle={asideOverlay ? <PaneResizeHandle side="aside" paneWidth={asideWidth} floating region={panelRegion((stackedPanels[0] ?? peekKind) as PanelKind)} /> : undefined}
                        floating={asideFloats && ownerStack ? { stack: ownerStack, onClose: floating.close } : undefined}
                      />
                    </ProcessPanelProvider>
                  </OpenShellTerminal.Provider>
                )}
              </main>
              {asideOwner && asideDocked && (
                <>
                  {/* First among what goes away with the pane, so focus on its handle is still found. */}
                  <HandOffOnUnmount from="[role=separator][data-pane=aside]" to={focusTerminal} />
                  <PaneResizeHandle side="aside" paneWidth={asideWidth} region={panelRegion(stackedPanels[0] as PanelKind)} />
                </>
              )}
            </>
          )}
        </ContentPanel>
      </div>
      {/* A login started in Settings opens its authorization page for as long as the window lives, Settings open or not. */}
      <LoginPageOpener />
      <UpdateNotice focusTerminal={focusTerminal} />
      <ConnectionBanner state={connectionState} onRetry={reconnect} focusTerminal={focusTerminal} />
      <div ref={announcementRef}>
        <AttendedStatusAnnouncer text={announcementText} />
      </div>
      {settingsOpen && <SettingsDialog navigation={settingsNavigation} onClose={closeSettings} />}
      {dialogRequest && (
        <RequestedDialog
          dialog={dialogRequest}
          onClose={closeDialog}
          onSessionOpened={showOpenedSession}
          onSwitchAccounts={switchAccounts}
          onChangeSessionModel={changeSessionModel}
          onResumeSession={resumeFromDialog}
        />
      )}
      {launchConfirmation}
      {linkedFile && (
        <LazyTerminalFileViewer
          file={linkedFile}
          onOpenFile={setLinkedFile}
          onClose={closeLinkedFile}
        />
      )}
      <PendingQuestionDialog />
      {exitConfirmOpen && (
        <ConfirmDialog
          title={t(exitRestarts ? "app.restart.title" : "app.quit.title")}
          message={t(exitMessageKey(exitRestarts, exitHolders))}
          confirmLabel={t(exitRestarts ? "common.restart" : "common.quit")}
          destructive
          onCancel={closeExitConfirm}
          onConfirm={confirmExit}
          // A quit or restart that fails is reported by `useAppExit` under its own title; `confirmExit` never rejects.
          errorTitle={t(exitRestarts ? "exit.restartFailed.title" : "exit.failed.title")}
        />
      )}
    </div>
  );
}

/** The confirmation's text for what a restart or a quit would end: sessions, terminals, both or neither. */
function exitMessageKey(restarts: boolean, holders: { sessions: boolean; terminals: boolean }): PlainMessageKey {
  const prefix = restarts ? "app.restart.message" : "app.quit.message";
  if (!holders.terminals) return prefix;
  return holders.sessions ? `${prefix}.both` : `${prefix}.terminals`;
}
