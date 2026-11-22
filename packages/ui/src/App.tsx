import { AttendedStatusAnnouncer } from "./components/announce/AttendedStatusAnnouncer";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { ArchiveLayer } from "./archive/ArchiveLayer";
import { forgetProjectsOtherThan } from "./project-browser/browserState";
import { LazyLinkedFileViewer } from "./project-browser/files/LazyLinkedFileViewer";
import { OpenShellTerminal } from "./project-browser/RowActions";
import { ConnectionBanner } from "./shell/ConnectionBanner";
import { IncompatibleDaemonScreen } from "./shell/IncompatibleDaemonScreen";
import { KeyRefusedScreen } from "./shell/KeyRefusedScreen";
import { ContentMain } from "./components/panel/ContentMain";
import { ContentPanel } from "./shell/ContentPanel";
import { HandoffOnUnmount } from "./components/focus/HandoffOnUnmount";
import { PaneResizeHandle } from "./layout/PaneResizeHandle";
import { refocusIfLost } from "./components/focus/refocusIfLost";
import { focusLanding, focusTopBar } from "./components/focus/regionFocus";
import { ringForAutomaticFocus } from "./input/lastInput";
import { focusNewConsole, Rail } from "./shell/Rail";
import { Scrim } from "./shell/Scrim";
import { BareTitleBar, TitleBar } from "./shell/TitleBar";
import { Toasts } from "./shell/Toasts";
import { useFirstConsoleHandoff } from "./shell/useFirstConsoleHandoff";
import { UnsavedChangesProvider, useUnsavedChangesGuard } from "./components/dialog/UnsavedChanges";
import { QuoteTargetContext } from "./components/host/quoteTarget";
import { useStableHandler } from "./components/hooks/useLatest";
import { focusAfterLoss, setFocusAfterModalLoss } from "./components/focus/modalFocus";
import type { DialogRequest } from "./dialogs/dialogRequest";
import { PendingQuestionDialog, usePendingQuestion } from "./dialogs/PendingQuestionDialog";
import { RequestedDialog } from "./dialogs/RequestedDialog";
import { useConfirmIncompleteLaunch } from "./dialogs/useConfirmIncompleteLaunch";
import { useQuickCreateSession } from "./dialogs/useQuickCreateSession";
import { raiseError } from "./components/alerts/raiseError";
import { CodedError } from "./errors/codedError";
import { useT } from "./i18n/react";
import type { Console, Project, Session } from "./protocol";
import { usePlatform } from "./platform/react";
import { RemoteDevicePage } from "./remoteDaemons/RemoteDevicePage";
import { RemoteDevicesProvider } from "./remoteDaemons/RemoteDevices";
import { RemoteDevicesSidebar } from "./remoteDaemons/RemoteDevicesSidebar";
import { remoteDaemonsOffered } from "./remoteDaemons/remoteDaemonsOffered";
import { liveOwner, ownerKey, type AsideOwner } from "./layout/asideOwner";
import { columnSpan, isDockEasing } from "./layout/paneDockMotion";
import { useRemoteDevicesPage } from "./app/useRemoteDevicesPage";
import { useScheduledTasksPage } from "./app/useScheduledTasksPage";
import { ScheduledTaskPage } from "./scheduledTasks/ScheduledTaskPage";
import { ScheduledTasksProvider } from "./scheduledTasks/ScheduledTasks";
import { ScheduledTasksSidebar } from "./scheduledTasks/ScheduledTasksSidebar";
import { taskName } from "./scheduledTasks/taskName";
import { useScheduledRunNotifications } from "./scheduledTasks/useScheduledRunNotifications";
import { useRegionCycle } from "./layout/useRegionCycle";
import { ExitConfirmDialog } from "./lifecycle/ExitConfirmDialog";
import { useAppExit } from "./lifecycle/useAppExit";
import { useGitStatusSchedule } from "./lifecycle/useGitStatusSchedule";
import { useInterfaceLanguage } from "./lifecycle/useInterfaceLanguage";
import { useNativeAppsSync } from "./lifecycle/useNativeAppsSync";
import { useStrayFileDrops } from "./lifecycle/useStrayFileDrops";
import { useDropUploadsShown } from "./terminal/dropUploads";
import { useStatusItemMenu } from "./statusItem/useStatusItemMenu";
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { sessionDisplayTitle } from "./sessions/sessionLabel";
import { PANEL_ENTRIES } from "./panels/panelRegistry";
import { useProcessJump } from "./app/useProcessJump";
import { ProcessPanelProvider } from "./process/useProcessPanel";
import { RightPane } from "./panels/RightPane";
import { panelId, panelRegion, shownPanelRegions } from "./panels/stackState";
import type { PanelKind } from "./layout/panelKind";
import { LoginPageOpener } from "./settings/accounts/AccountLogin";
import { AgentCliUpdateAlerts } from "./settings/accounts/AgentCliUpdateAlerts";
import { SettingsDialog } from "./settings/SettingsDialog";
import { registerSettingsLauncher } from "./settings/settingsLauncher";
import { registerDialogLauncher } from "./dialogs/dialogLauncher";
import { registerSessionModelLauncher } from "./dialogs/sessionModelLauncher";
import { useSettingsDialog } from "./settings/useSettingsDialog";
import { updateNoticeKind } from "./updates/noticeKind";
import { UpdateNotice } from "./updates/UpdateNotice";
import { useUpdateSelector, useUpdateStore } from "./updates/UpdatesProvider";
import { useReloadAfterRestart } from "./updates/useReloadAfterRestart";
import { useUpdatedNotice } from "./updates/useUpdatedNotice";
import { useUpdateMenuLabel } from "./updates/useUpdateMenuLabel";
import { belongsToFocus } from "./sidebar/focus";
import { consoleSessionAbove } from "./sessions/order";
import { useWaitingCount } from "./store";
import { indexSessions, NO_SESSIONS, sessionsOf } from "./sessions/sessionIndex";
import { isKnownSessionStatus } from "./sessions/sessionStatus";
import { Sidebar } from "./sidebar/Sidebar";
import { useSidebarView } from "./sidebar/sidebarView";
import { useDaemon, useDaemonStore, useDaemonStoreEqual, useNativeAppsRefresh, useProviders } from "./store";
import { useOpenSessionInApp } from "./components/host/useOpenSessionInApp";
import { terminalEmptyState } from "./app/emptyState";
import { TerminalPane } from "./terminal/TerminalPane";
import { useTerminalFontShortcuts } from "./terminal/terminalFontShortcut";
import { useFindInFilesShortcut } from "./app/useFindInFilesShortcut";
import { StartupFramed } from "./StartupScreen";
import { WelcomeScreen } from "./WelcomeScreen";
import { useAnnouncement } from "./app/useAnnouncement";
import { useAppNavigation } from "./app/useAppNavigation";
import { useArchiveSession } from "./app/useArchiveSession";
import { useArchiveView } from "./app/useArchiveView";
import { useConsoleSwitch } from "./app/useConsoleSwitch";
import { DRAWER_CONTROLS, useDialogRouting } from "./app/useDialogRouting";
import { useFocusMode } from "./app/useFocusMode";
import { useLinkedFile } from "./app/useLinkedFile";
import { useTerminalQuoteTarget } from "./app/useTerminalQuoteTarget";
import { usePaneLayout } from "./app/usePaneLayout";
import { usePaneSwipe } from "./app/usePaneSwipe";
import { usePaneWidths } from "./app/usePaneWidths";
import { useProjectPanels } from "./app/useProjectPanels";
import { useRowRequests } from "./app/useRowRequests";
import { useSelectedSession } from "./app/useSelectedSession";
import { useSessionActions } from "./app/useSessionActions";
import { useSessionRequests } from "./app/useSessionRequests";
import { useTerminalFocus } from "./app/useTerminalFocus";

/** What stands in for the consoles and projects while the daemon cannot be used, whatever an earlier connection loaded. */
const NO_CONSOLES = new Map<string, Console>();
const NO_PROJECTS = new Map<string, Project>();

/**
 * The window of the local daemon. While that daemon cannot be used (it is connecting or not answering, speaks another
 * protocol generation, refused the page's key, or `startupScreen` says it failed to start) the window shows that state
 * where the session UI would be, and the rail holds only the Remote devices button: remote connections are the shell's,
 * and need no daemon, so the Remote devices page works all the same, when it is asked for.
 */
export function App({ startupScreen }: { startupScreen?: React.ReactNode } = {}): React.ReactElement {
  const t = useT();
  const { request, reconnect, store, isLocal, retriesUntilConnected = false } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const retryPending = useDaemonStore((s) => s.retryPending);
  const hosts = useDaemonStore((s) => s.hosts);
  const offline = startupScreen !== undefined || connectionState === "incompatible" || connectionState === "keyRefused" || !hosts;
  const storedConsoles = useDaemonStore((s) => s.consoles);
  const storedProjects = useDaemonStore((s) => s.projects);
  const consoles = offline ? NO_CONSOLES : storedConsoles;
  const projects = offline ? NO_PROJECTS : storedProjects;
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultAccountNames = useDaemonStore((s) => s.settings.default_account_names);
  const providers = useProviders();
  // Keeps Settings out of the way of the question's dialog.
  const questionPending = usePendingQuestion();
  const { terminalRef, welcomeRef, agentAreaRef, focusTerminal, handOffToTerminal } = useTerminalFocus();
  const { selectedSessionId, selectedSession, setSelectedSessionId } = useSelectedSession();
  const [dialogRequest, setDialogRequest] = useState<DialogRequest>();
  // What stands before every way of leaving a page form that holds changes not saved (`components/dialog/UnsavedChanges`),
  // made before the page hooks, which keep their page while a form is held.
  const unsaved = useUnsavedChangesGuard();
  const { linkedFile, viewLinkedFile, setLinkedFile, closeLinkedFile } = useLinkedFile({ focusTerminal });
  const quoteTarget = useTerminalQuoteTarget(terminalRef);

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
  useGitStatusSchedule();
  useInterfaceLanguage();
  useNativeAppsSync();
  useStrayFileDrops();

  const currentConsole = sidebarView.currentConsole;
  // The Remote devices page, in the application's own window (`useRemoteDevicesPage`): while it shows it takes the
  // content area's place, and the sidebar lists the saved devices in place of the console's sessions. The session UI is
  // hidden and not unmounted, so the terminals and everything else go on as they were.
  const platform = usePlatform();
  const devicesOffered = remoteDaemonsOffered(platform);
  const devices = useRemoteDevicesPage({
    capability: devicesOffered ? platform.remoteDaemons : undefined,
    consoleId: currentConsole?.id,
    sessionId: selectedSession?.id,
    offline,
    held: unsaved.dirty,
  });
  const devicesShown = devices.shown;
  // The Scheduled tasks page, in every window of a daemon while that daemon can be used (`useScheduledTasksPage`): it
  // takes the content area's place as the Remote devices page does, the sidebar listing the tasks. One page shows at a
  // time, so opening either leaves the other.
  const tasks = useScheduledTasksPage({ available: !offline, consoleId: currentConsole?.id, sessionId: selectedSession?.id, held: unsaved.dirty });
  const tasksShown = tasks.shown;
  const pageShown = devicesShown || tasksShown;
  // A way of leaving what the content area shows that keeps the page where it is (the other page, Back and Forward, a
  // notification's task) goes through `leaveForm`, which asks first while a form holds changes not saved. One that puts
  // a session or a console on screen goes through `leavePage`, which also leaves the page itself: a page stays on
  // for a change nobody asked for (`held`), so a confirmed leave has to end it.
  const leaveForm = unsaved.run;
  const leavePage = useStableHandler((go: () => void) =>
    unsaved.run(() => {
      if (devicesShown) devices.leave();
      if (tasksShown) tasks.leave();
      go();
    }),
  );
  const showDevices = useStableHandler(() => {
    if (devicesShown) return;
    leaveForm(() => {
      tasks.leave();
      devices.show();
    });
  });
  const showTasks = useStableHandler(() => {
    if (tasksShown) return;
    leaveForm(() => {
      devices.leave();
      tasks.show();
    });
  });
  // A connection's question or failure, or what the shell handed over, shows the page only where that replaces no form
  // with changes not saved: its dialog opens over the window all the same.
  const showDevicesUnasked = useStableHandler(() => {
    if (!unsaved.guard.isDirty()) showDevices();
  });
  const {
    noConsoles,
    noSidebar,
    ownerKind,
    ownerOrder,
    reportSessionId,
    availableKinds,
    asideFloats,
    asideOverlay,
    asidePinned,
    asideDockable,
    toggleAsideDocked,
    notePanelOpened,
    notePanelClosed,
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
    sidebarWithoutConsole: pageShown,
  });
  // A run's session chosen on the page (`openRunSession`) is given the focus once the page is gone: `focusTerminal` would
  // otherwise find it still mounted.
  const terminalFocusPending = useRef(false);
  useLayoutEffect(() => {
    if (tasksShown || !terminalFocusPending.current) return;
    terminalFocusPending.current = false;
    focusTerminal();
  }, [tasksShown]);
  const chooseTask = useStableHandler((id: string) => {
    tasks.select(id);
    if (panes.sidebarOpen) panes.closeSidebar();
  });
  const addTask = useStableHandler(() => {
    tasks.startAdd();
    if (panes.sidebarOpen) panes.closeSidebar();
  });
  const addDevice = useStableHandler(() => {
    devices.startAdd();
    if (panes.sidebarOpen) panes.closeSidebar();
  });
  const chooseDevice = useStableHandler((id: string) => {
    devices.select(id);
    // A drawer (a narrow window) has done what it was opened for, as it has for a session.
    if (panes.sidebarOpen) panes.closeSidebar();
  });
  // A maximized panel (`covered`) or a floating one over it takes the agent area out of reach: inert, so its
  // regions are not pressable and their status text is lost until it is exposed again.
  const agentAreaInert = covered || floatingShown;

  const {
    archiveScope,
    setArchiveScopeByUser,
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
  } = useArchiveView({ consoles, projects, revealArchive, sidebarOpen: panes.sidebarOpen, closeSidebar: panes.closeSidebar, handOffToTerminal });

  useFirstConsoleHandoff(!offline && noConsoles, dialogRequest?.kind === "new-console");

  // The terminal offers the next action for the current console and focus scope, even while the
  // sidebar previews another console. A selected session or an archive keeps that offer hidden.
  const emptyState = useDaemonStoreEqual((s) =>
    selectedSession === undefined && !archiveOpen
      ? terminalEmptyState(hosts !== undefined, currentConsole, sidebarView.focus, projectList, currentConsole ? sessionsOf(indexSessions(s.sessions).byConsole, currentConsole.id) : NO_SESSIONS, agentAvailability, s.settings.added_agents, s.providers)
      : undefined,
  );
  // The "no agent added" state offers Add agent; only "no agent installed" has nothing to press.
  const emptyHasAction = emptyState !== undefined && !(emptyState.kind === "no-agents" && emptyState.reason === "install");
  // Only the store and whether the notice is shown: the whole state changes with every tick of a download.
  const updateStore = useUpdateStore();
  const updateNoticeShown = useUpdateSelector((state) => updateNoticeKind(state) !== undefined) ?? false;
  // F6 includes only regions currently on screen, including a revealed floating group.
  // The top bar is always available.
  // The notice with Cancel over a session whose dropped files are uploading is a stop of its own, as the terminal takes Tab.
  const uploadsShown = useDropUploadsShown(selectedSession?.id) && !archiveOpen && !agentAreaInert && !pageShown;
  useRegionCycle({
    shown: {
      topbar: true,
      rail: !railAway,
      devices: devicesShown,
      tasks: tasksShown,
      welcome: noConsoles && !pageShown,
      sidebar: !noSidebar && panes.sidebarShown,
      archive: archiveOpen && !agentAreaInert && !pageShown,
      uploads: uploadsShown,
      terminal: (selectedSession !== undefined || emptyHasAction) && !archiveOpen && !agentAreaInert && !pageShown,
      banner: connectionState === "reconnecting" || connectionState === "closed",
      update: updateNoticeShown,
    },
    panels: pageShown ? {} : shownPanelRegions(asideOverlay && !panes.asidePeek.active ? undefined : ownerStack, ownerKind),
    focusTerminal,
  });

  const { sidebarColumnPhase, setSidebarColumnPhase, sidebarWidth, asideWidth, asideLayout, asideFloat } = usePaneWidths({
    noSidebar,
    isNarrow,
    sidebarDocked: panes.sidebarDocked,
    stackedPanelCount: stackedPanels.length,
    stackedPanelKinds: stackedPanels,
    columnShare: ownerStack?.columnShare,
    panelWidthKey: ownerKey(asideOwner),
    maximized: covered,
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
    setArchiveScopeByUser,
    leavePage,
  });
  // A removed project's browser state has nothing left to apply to. `projects` is empty while `offline`, so pruning then
  // would wipe every project's persisted state.
  useEffect(() => {
    if (!offline) forgetProjectsOtherThan(new Set(projects.keys()));
  }, [projects, offline]);
  // What the aside showed goes with its owner; focus that was inside it goes to the terminal.
  useEffect(() => refocusIfLost(focusTerminal), [ownerKey(asideOwner)]);
  // The banner of a window with nothing to show yet goes when the connection opens, before there is anything to hand
  // its focus to: focus it held is handed on once the window's content is there, where it still lies on nothing.
  const focusLeftBareBanner = useRef(false);
  useEffect(() => {
    if (offline || !focusLeftBareBanner.current) return;
    focusLeftBareBanner.current = false;
    if (document.activeElement !== document.body) return;
    ringForAutomaticFocus();
    focusTerminal();
    // The welcome screen, with no console, takes focus only from somewhere else.
    const welcome = document.querySelector("[data-region=welcome]");
    if (document.activeElement === document.body && welcome) focusLanding(welcome);
    if (document.activeElement === document.body) focusTopBar();
  }, [offline]);

  useWaitingNotifications(hosts !== undefined);
  // A failed run of a scheduled task notifies; choosing the notification opens the page on that task.
  useScheduledRunNotifications((taskId) => {
    if (tasksShown && !tasks.adding && tasks.task === taskId) return;
    leaveForm(() => {
      devices.leave();
      tasks.show();
      chooseTask(taskId);
    });
  });
  const updatesLabel = useUpdateMenuLabel();
  useStatusItemMenu({
    updatesLabel,
    selectedSessionId,
    currentConsoleId: sidebarView.currentConsole?.id,
    ready: hosts !== undefined,
    onSessionChosen: (session) =>
      leavePage(() => {
        selectSession(session);
        focusTerminal();
      }),
  });

  // The exit flow's frontend owner: it registers with the shell on mount, which is what makes the
  // window quittable. The main app has no "Quit" control of its own; the screen for a daemon of another
  // protocol generation has a Restart button. That daemon, like one that refused the page's key, is not talked to,
  // and neither screen has room for the quit's confirmation, so a quit or restart while one is shown asks about
  // nothing.
  const reachable = () => {
    const state = store.getState().connectionState;
    return state !== "incompatible" && state !== "keyRefused";
  };
  // In this order: the status that reloads the page that asked for the restart is the one that says it was updated, and
  // the notice holds back once the reload has begun, which it can only see when the reload's effect runs first.
  const reloadAfterRestart = useReloadAfterRestart();
  useUpdatedNotice();
  const exit = useAppExit({
    // A browser tab has no shell to relaunch, so a restart (into an update the daemon installed) is the daemon's own.
    restartDaemon: async (targetVersion) => {
      // Nothing is sent on a connection that is not open, and no reload is waited for: say so as any request would.
      if (store.getState().connectionState !== "open") {
        raiseError(new CodedError(t("connection.error.connectionIsClosed"), "connection_closed", {}), { title: t("exit.daemonRestartFailed.title") });
        return;
      }
      await reloadAfterRestart.restart(targetVersion, () => request({ type: "restart_daemon" }));
    },
    // A quit, or a restart for an update, asks about changes not saved first, as every other way of leaving does.
    unsaved: unsaved.guard,
    getSessions: () => (reachable() ? allSessions() : []),
    getShellTerminals: () => (reachable() ? Array.from(store.getState().shellTerminals.values()) : []),
    requestShutdown: () => request({ type: "shutdown" }),
    requestPlannedStop: () => request({ type: "planned_stop" }),
    checkHandover: async (holder_floor, target_version) => {
      const { known, sessions, shell_terminals, unreached_shell_terminals } = await request({ type: "handover_check", holder_floor, target_version });
      return { known, sessions, shellTerminals: shell_terminals, unreachedShellTerminals: unreached_shell_terminals };
    },
  });

  // An installed update takes effect on a restart, which only ever happens through the exit flow above, so the
  // user is asked, and can cancel, whenever it would or may interrupt a session or end a busy terminal. The update notice
  // and Settings ask for it here, and hold their button pending until the confirmation opens or the relaunch starts.
  const answerRestart = useStableHandler((holderFloor: string | null, targetVersion: string | null) => exit.requestRestart(holderFloor, targetVersion));
  useEffect(() => updateStore?.onRestartRequested(answerRestart), [updateStore, answerRestart]);

  // The question before a launch on an account whose sharing change did not finish; this window's
  // own launches (a resume, a quick create) go through it and render it at this level.
  const { confirmLaunch, confirmation: launchConfirmation } = useConfirmIncompleteLaunch();

  const ownModalOpen = questionPending || exit.exitConfirmOpen || launchConfirmation !== undefined || unsaved.asking;
  const otherModalOpen = dialogRequest !== undefined || ownModalOpen;
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
    setArchiveScopeByUser,
    closeArchive,
    focusTerminal,
    resumeSession,
    asideFloats,
    sidebarOpen: panes.sidebarOpen,
    closeSidebar: panes.closeSidebar,
    consoleList,
    projectList,
    selectedSessionId,
    leavePage,
  });

  const quickCreateSession = useQuickCreateSession(showOpenedSession, sidebarView.currentConsole?.id, confirmLaunch);

  const { openDialog, closeDialog, openConsoleSession, focusDrawerTrigger, stableOpenDialog, stableOpenDialogWhenFree, stableOpenConsoleSession } = useDialogRouting({
    dialogRequest,
    windowModalOpen: ownModalOpen || settingsOpen,
    consoles,
    consoleList,
    sidebarOpen: panes.sidebarOpen,
    setDialogRequest,
    switchConsole,
  });
  // A question raised from the narrow window's drawer hands focus back to the control that was pressed there, as a
  // dialog opened from it does.
  const focusPressedDrawerControl = useStableHandler(() => void focusDrawerTrigger());
  useEffect(() => unsaved.guard.setBeforeAsk(focusPressedDrawerControl), [unsaved.guard, focusPressedDrawerControl]);
  const openSettingsFromDrawer = () => {
    // Focus already in the drawer (Settings opened by a key) is where the dialog hands it back to.
    settingsFromDrawer.current = focusDrawerTrigger() || (panes.sidebarOpen && !!document.activeElement?.closest(DRAWER_CONTROLS));
    openSettings();
  };

  const { stableOpenProjectPanel, stableOpenShellTerminal, stableShowPanel } = useProjectPanels({
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

  const openProcesses = useProcessJump({
    asideFloats,
    switchConsole,
    openProjectPanel: stableOpenProjectPanel,
    showPanel: stableShowPanel,
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

  useTerminalFontShortcuts();
  useFindInFilesShortcut();

  const { stableFocus } = useFocusMode({
    focus: sidebarView.focus,
    setFocus: sidebarView.setFocus,
    selectConsole: sidebarView.selectConsole,
    currentConsoleId: sidebarView.currentConsole?.id,
    selectedSession,
    sidebarShown: panes.sidebarShown,
    selectSession,
    focusTerminal,
    suspended: pageShown,
  });

  // Nothing is recorded while the page shows and the list of devices is not read yet, which would name a device that is
  // not the one it settles on. Without the daemon the status it shows is the first place there is to go back to, so it
  // is recorded from the start, as a place that is gone once the daemon can be used.
  const { canGoBack, canGoForward, stableBack, stableForward, stableCommitPreview } = useAppNavigation({
    ready: !(devicesShown && !devices.list.entriesRead) && !(tasksShown && !tasks.tasks),
    offline,
    focus: sidebarView.focus,
    setFocus: sidebarView.setFocus,
    selectConsole: sidebarView.selectConsole,
    currentConsoleId: sidebarView.currentConsole?.id,
    selectedSession,
    selectedSessionId,
    setSelectedSessionId,
    archiveOpen,
    archiveScope,
    setArchiveScopeByUser,
    setArchiveRestore,
    archiveViewStates,
    revealArchive,
    consoles,
    projects,
    asideOwner,
    setAsideOwner,
    confirm: leaveForm,
    holdForPress: followUp.holdForPress,
    releasePress: followUp.releasePress,
    previewing,
    shownConsole,
    focusTerminal,
    devices: devicesShown ? (devices.device ?? "") : undefined,
    deviceIds: new Set(devices.list.entries.map((entry) => entry.id)),
    setDevices: devices.follow,
    tasks: tasksShown ? (tasks.task ?? "") : undefined,
    taskIds: tasks.tasks && new Set(tasks.tasks.map((task) => task.id)),
    setTasks: tasks.follow,
  });

  const { stableArchiveSession } = useArchiveSession({
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
  const nativeApps = useDaemonStore((s) => s.nativeApps);
  const nativeAppsUnseen = useDaemonStore((s) => s.nativeAppsUnseen);
  const stableOpenSessionInApp = useStableHandler(useOpenSessionInApp((session, label) => openDialog({ kind: "open-session-in-app", session, label })));
  const stableRefreshNativeApps = useStableHandler(useNativeAppsRefresh());
  const {
    stableSetSessionColour,
    stableOpenProjectDirectory,
    stableOpenRepository,
    stableOpenSystemTerminal,
    stableCopySessionId,
    stableCopyProjectName,
    stableLoadProjectCopyPathItem,
    stablePullProjectBranch,
    stablePushProjectBranch,
    stableLoadProjectBranches,
    stableSetLocked,
    stableBindSession,
    stableSetPinned,
  } = useRowRequests(stableOpenDialogWhenFree);
  usePaneSwipe({
    asideFloats,
    noSidebar,
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
    // A session of a status this client does not know has nothing that opens its model.
    if (session && isKnownSessionStatus(session.status) && !(otherModalOpen || settingsOpen)) openDialog({ kind: "session-model", session });
  });
  useEffect(() => registerSessionModelLauncher(openSessionModel), [openSessionModel]);
  useEffect(() => registerDialogLauncher(stableOpenDialog), [stableOpenDialog]);
  // Where focus goes once the last alert or pending question, or Settings with no dialog under it, closes with nothing to
  // give it back to.
  const focusAfterModal = useStableHandler(() => focusAfterLoss(focusTerminal));
  useEffect(() => setFocusAfterModalLoss(focusAfterModal), [focusAfterModal]);

  // What the window says while the local daemon cannot be used, where the session UI would be. A daemon of another
  // protocol generation: nothing it sent is shown, whatever an earlier connection had loaded. The page's key is no longer
  // the daemon's, so nothing can load or be retried: the same, with the way to a working address. Nothing to show until
  // the first snapshot: the banner with the client's Retry is there with it once Retry is offered (below), otherwise this
  // would say it is connecting forever with nothing the user can do about it.
  let status: React.ReactNode;
  if (startupScreen !== undefined) status = startupScreen;
  else if (connectionState === "incompatible") status = <IncompatibleDaemonScreen onRestart={exit.restartNow} />;
  else if (connectionState === "keyRefused") status = <KeyRefusedScreen />;
  else if (!hosts) {
    status = (
      <ContentMain className="items-center justify-center text-app-muted">
        {connectionState === "closed" || (retriesUntilConnected && connectionState === "reconnecting")
          ? t("app.daemonNotAnswering")
          : t("app.connecting")}
      </ContentMain>
    );
  }

  // A window with no Remote devices page (a browser, a remote daemon's window) has nothing else to show while the daemon
  // cannot be used: the status is the whole window. While the first snapshot is awaited the toasts belong there all
  // the same.
  if (offline && !devicesOffered) {
    if (connectionState === "incompatible" || connectionState === "keyRefused" || startupScreen !== undefined) return <>{status}</>;
    return (
      <div className="flex h-full flex-col">
        <BareTitleBar />
        <Toasts focusTerminal={focusTerminal} />
        <ContentPanel bare>{status}</ContentPanel>
        {/* A client that retries until it connects offers Retry from its first failed attempt on, where another offers it
            once its attempts are spent: either way the window does not say it is connecting with nothing to do about it. */}
        {(connectionState === "closed" || (retriesUntilConnected && connectionState === "reconnecting")) && (
          <>
            <HandoffOnUnmount from="[data-region=banner]" to={() => (focusLeftBareBanner.current = true)} ring={false} />
            <ConnectionBanner
              state={connectionState}
              retryWhileReconnecting={retriesUntilConnected}
              retryPending={retryPending}
              onRetry={reconnect}
              focusTerminal={focusTerminal}
            />
          </>
        )}
      </div>
    );
  }

  // The sidebar with its scrim and the handle of its width: the sessions' of the console, or while the Remote devices page
  // shows, the devices'.
  const sidebar = (
    <>
      <Scrim open={panes.sidebarOpen} label={t(devicesShown ? "app.closeDevices" : tasksShown ? "app.closeScheduledTasks" : "app.closeSessions")} onClose={panes.closeSidebar} />
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
        onOpenProjectDirectory={isLocal === false ? undefined : stableOpenProjectDirectory}
        onOpenSystemTerminal={isLocal === false ? undefined : stableOpenSystemTerminal}
        onOpenRepository={stableOpenRepository}
        onCopyProjectName={stableCopyProjectName}
        onLoadProjectCopyPathItem={stableLoadProjectCopyPathItem}
        onCopySessionId={stableCopySessionId}
        onPullProjectBranch={stablePullProjectBranch}
        onPushProjectBranch={stablePushProjectBranch}
        onLoadProjectBranches={stableLoadProjectBranches}
        onSetSessionColour={stableSetSessionColour}
        onSetLocked={stableSetLocked}
        onBindSession={stableBindSession}
        onSetPinned={stableSetPinned}
        onReopenSession={stableReopenSession}
        onArchiveSession={stableArchiveSession}
        onOpenSettings={stableOpenSettingsAt}
        open={panes.sidebarOpen || (!asideFloats && panes.sidebarPeek.active)}
        drawerAway={isNarrow && !panes.sidebarOpen && !panes.sidebarPeek.active}
        peek={isNarrow || !panes.sidebarDocked ? panes.sidebarPeek : undefined}
        onPeekPress={pageShown ? undefined : stableCommitPreview}
        page={
          devicesShown
            ? { label: t("remoteDevices.label"), content: <RemoteDevicesSidebar /> }
            : tasksShown
              ? { label: t("scheduledTasks.label"), content: <ScheduledTasksSidebar /> }
              : undefined
        }
        onColumnPhase={setSidebarColumnPhase}
        sidebarWidth={sidebarWidth}
      />
      {!isNarrow && panes.sidebarDocked && sidebarColumnPhase === "open" && (
        <PaneResizeHandle side="sidebar" paneWidth={sidebarWidth} />
      )}
    </>
  );

  const screen = (
    <div data-sidebar-dockable={!isNarrow} data-pane-docked={(!isNarrow && panes.sidebarDocked) || asideDocked} className="relative flex h-full flex-col">
      <Toasts focusTerminal={focusTerminal} />
      <TitleBar
        devicesShown={devicesShown}
        tasksShown={tasksShown}
        sidebarShown={panes.sidebarShown}
        sidebarRevealOnly={isNarrow && !asideFloats}
        hasSidebar={!noSidebar}
        sidebarSpan={noSidebar ? 0 : columnSpan(sidebarColumnPhase, sidebarWidth.width)}
        sidebarEasing={isDockEasing(sidebarColumnPhase)}
        onToggleSidebar={stableToggleSidebar}
        onSidebarToggleEnter={panes.sidebarPeek.reveal}
        onSidebarToggleLeave={panes.sidebarPeek.leave}
        asideSpan={asideDocked && !pageShown ? asideWidth.width : 0}
        asidePinned={asidePinned}
        onToggleAsideDocked={ownerKind && !asideFloats ? toggleAsideDocked : undefined}
        asideAutomatic={!asideDockable}
        asideRevealed={panes.asidePeek.active}
        onAsideHoverChange={stableHoverAside}
        panelToggles={ownerKind && !pageShown ? availableKinds.map((kind) => {
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
        selectedSession={offline ? undefined : selectedSession}
        viewTrail={
          devicesShown
            ? [t("remoteDevices.label"), ...(devices.device !== undefined && !devices.adding ? [devices.list.entries.find((entry) => entry.id === devices.device)!.name] : [])]
            : tasksShown
              ? [t("scheduledTasks.label"), ...(tasks.task !== undefined && !tasks.adding ? [taskName(tasks.tasks!.find((task) => task.id === tasks.task)!)] : [])]
              : archiveOpen && archiveConsole
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
          // The sidebar lists the devices while the page shows, so a console's avatar has no sessions to float in.
          onConsoleEnter={pageShown ? noop : panes.sidebarPeek.hoverConsole}
          onConsoleLeave={pageShown ? noop : panes.sidebarPeek.leaveConsole}
          onRailHoverChange={(hovered) => hovered ? panes.sidebarPeek.hoverRail() : panes.sidebarPeek.leaveRail()}
          hoverOpensSidebar={panes.sidebarPeek.peekable && !pageShown}
          remoteDevicesShown={devicesShown}
          onOpenRemoteDevices={showDevices}
          scheduledTasksShown={tasksShown}
          onOpenScheduledTasks={showTasks}
          offline={offline}
          onOpenDialog={stableOpenDialog}
          onSelectSession={stableSelectFromSidebar}
          onOpenArchive={stableOpenArchive}
          waitingCount={waitingCount}
          onNextWaiting={stableNextWaiting}
          onOpenProcesses={openProcesses}
          onOpenSettings={openSettingsFromDrawer}
          onOpenSettingsAt={stableOpenSettingsAt}
          focusTerminal={focusTerminal}
        />
        <ContentPanel startsAtEdge={railOnDemand}>
          {/* With no console the welcome screen stands in for the session UI (read off the console itself, so that it
              narrows), unless the page of devices is shown, which has the sidebar of its own. Without the daemon there is
              no session UI to keep: the status stands where it would be, and the page, once asked for, has its sidebar
              alone. */}
          {offline && !devicesShown ? (
            // The region F6 reaches the status's Restart or Report issue by, as it does the welcome screen's controls.
            <div data-region="welcome" className="flex min-h-0 min-w-0 flex-1 flex-col">
              <StartupFramed.Provider value>{status}</StartupFramed.Provider>
            </div>
          ) : shownConsole === undefined && !pageShown ? (
            <WelcomeScreen ref={welcomeRef} onOpenDialog={openDialog} />
          ) : (
            <>
              {/* Before anything else here, so focus is handed to the rail while the sidebar, the
                  terminal and the aside it may be in are still in the document: the last console
                  going (here or in another window) removes all of them at once. */}
              <HandoffOnUnmount
                from="[data-region=sidebar], [data-region=archive], [data-region=terminal], [data-region=aside], [data-region^=panel-]"
                to={focusNewConsole}
              />
              {/* A panel floating over the agent terminal is dismissed by a press beside it, as the
                  drawer is; the top bar, with the panel's own toggle, stays out of it. */}
              <HiddenWhile hidden={pageShown}>
                <Scrim open={floatingShown} label={t("panel.close")} onClose={floating.close} clearOfRail={false} />
              </HiddenWhile>
              {sidebar}
              {/* Without the daemon there is no session UI at all, only the devices' sidebar and the page. */}
              {!offline && (
                <HiddenWhile hidden={pageShown}>
                  {/* Floating panes leave the agent area its full width; horizontal overflow remains a
                  guard while the docked sidebar's column is changing width. The scroll container's
                  own shadow reaches the sidebar without being clipped by its overflow. */}
                  <ContentMain className="overflow-x-auto">
                    {/* The flex basis matches the width allocator's 400px floor, so the pane keeps its
                    allocated width and the terminal grows into the remaining space. Phone mode removes the floor. */}
                    {/* The agent area and docked pane share a layer below resize handles and floating
                    cards, so the cards' edges and shadows can paint over the terminal's outer inset.
                    The scroll container casts the sidebar shadow. */}
                    <div ref={agentAreaRef} inert={agentAreaInert} className={`relative flex min-h-0 ${asideFloats ? "min-w-0 flex-1" : "min-w-[400px] flex-[1_1_400px]"} pane-docked:z-1`}>
                      <ArchiveLayer
                        view={archiveOpen && archiveConsole ? {
                          console: archiveConsole,
                          project: archiveProject,
                          boundTo: archiveBoundTo,
                          sessions: archiveSessions,
                          consoles,
                          projects,
                          accounts,
                          defaultNames: defaultAccountNames,
                          providers,
                          selectedSessionId,
                          onSelect: stableSelectSession,
                          onReopen: stableReopenSession,
                          nativeApps,
                          nativeAppsUnseen,
                          onOpenInApp: stableOpenSessionInApp,
                          onRefreshNativeApps: stableRefreshNativeApps,
                          onOpenDialog: stableOpenDialog,
                          dialogOpen: dialogRequest !== undefined,
                          onClose: closeArchive,
                          restore: archiveRestore,
                          onViewState: onArchiveViewState,
                        } : undefined}
                      />
                      <TerminalPane
                        exposed={!agentAreaInert && !pageShown}
                        ref={terminalRef}
                        resizing={isDockEasing(sidebarColumnPhase)}
                        onViewFile={viewLinkedFile}
                        onSelectSession={stableSelectSession}
                        restoreFocus={focusTerminal}
                        behindArchive={archiveOpen}
                        noSelection={selectedSessionId === undefined}
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
                            peekKind={peekKind}
                            onViewFile={viewLinkedFile}
                            onDismiss={() => panes.asidePeek.hide()}
                            onPanelClose={notePanelClosed}
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
                  </ContentMain>
                  {asideOwner && asideDocked && (
                    <>
                      {/* First among what goes away with the pane, so focus on its handle is still found. */}
                      <HandoffOnUnmount from="[role=separator][data-pane=aside]" to={focusTerminal} />
                      <PaneResizeHandle side="aside" paneWidth={asideWidth} region={panelRegion(stackedPanels[0] as PanelKind)} />
                    </>
                  )}
                </HiddenWhile>
              )}
            </>
          )}
          {devicesShown && <RemoteDevicePage drawerOpen={panes.sidebarOpen} />}
          {tasksShown && <ScheduledTaskPage consoleId={currentConsole?.id} drawerOpen={panes.sidebarOpen} onNewConsole={() => openDialog({ kind: "new-console" })} />}
        </ContentPanel>
      </div>
      {/* Without the daemon none of this has anything to work on. */}
      {/* A login started in Settings opens its authorization page for as long as the window lives, Settings open or not. */}
      {!offline && <LoginPageOpener />}
      {/* An update of an agent's CLI asked for in Settings that fails is told whether or not Settings is still open. */}
      {!offline && <AgentCliUpdateAlerts />}
      {!offline && <UpdateNotice focusTerminal={focusTerminal} />}
      {/* Without the daemon, mounted only while closed, so that its focus handoff does not run as the daemon connects;
          nothing is lost, as there is no terminal and an empty bar to hand focus to. */}
      {(!offline || (connectionState === "closed" && startupScreen === undefined)) && (
        <ConnectionBanner
          state={connectionState}
          retryWhileReconnecting={retriesUntilConnected}
          retryPending={retryPending}
          onRetry={reconnect}
          focusTerminal={focusTerminal}
        />
      )}
      {!offline && (
        <div ref={announcementRef}>
          <AttendedStatusAnnouncer text={announcementText} />
        </div>
      )}
      {settingsOpen && !offline && <SettingsDialog navigation={settingsNavigation} onClose={closeSettings} />}
      {dialogRequest && !offline && (
        <RequestedDialog
          dialog={dialogRequest}
          onClose={() => closeDialog(dialogRequest)}
          onSessionOpened={showOpenedSession}
          onSwitchAccounts={switchAccounts}
          onChangeSessionModel={changeSessionModel}
          onResumeSession={resumeFromDialog}
        />
      )}
      {!offline && launchConfirmation}
      {linkedFile && !offline && (
        <LazyLinkedFileViewer
          file={linkedFile}
          onOpenFile={setLinkedFile}
          onClose={closeLinkedFile}
        />
      )}
      {!offline && <PendingQuestionDialog />}
      {!offline && <ExitConfirmDialog exit={exit} />}
    </div>
  );
  // A session a scheduled task's run started, chosen on the page: shown as one chosen from elsewhere in the window is,
  // leaving the page even when it is the session the page shows over.
  const openRunSession = (session: Session) =>
    leavePage(() => {
      selectSession(session);
      // The terminal takes focus once the page is gone: now, `focusTerminal` would find the page still mounted.
      terminalFocusPending.current = true;
    });
  const withTasks = (
    <ScheduledTasksProvider
      tasks={tasks.tasks}
      listing={tasks.listing}
      selectedId={tasks.task}
      adding={tasks.adding}
      onSelect={chooseTask}
      onAdd={addTask}
      onCancelAdd={tasks.cancelAdd}
      onOpenSession={openRunSession}
    >
      {screen}
    </ScheduledTasksProvider>
  );
  // The question stands outside every `!offline` gate: the Remote devices page works without the daemon.
  return (
    <>
      <UnsavedChangesProvider guard={unsaved.guard}>
        <QuoteTargetContext.Provider value={quoteTarget}>
          {devicesOffered ? (
            <RemoteDevicesProvider capability={platform.remoteDaemons!} list={devices.list} selectedId={devices.device} adding={devices.adding} onShow={showDevices} onAttention={showDevicesUnasked} onSelect={chooseDevice} onAdd={addDevice} onCancelAdd={devices.cancelAdd}>
              {withTasks}
            </RemoteDevicesProvider>
          ) : (
            withTasks
          )}
        </QuoteTargetContext.Provider>
      </UnsavedChangesProvider>
      {unsaved.confirmation}
    </>
  );
}

/** What stays mounted while the Remote devices or Scheduled tasks page takes its place: out of the layout, the document's outline and the
 * focus order (`display: none`), as the terminals and the panes keep their state, and as part of the content panel's
 * row (`display: contents`) otherwise. */
function HiddenWhile({ hidden, children }: { hidden: boolean; children: React.ReactNode }): React.ReactElement {
  return <div className={hidden ? "hidden" : "contents"}>{children}</div>;
}

const noop = () => {};
