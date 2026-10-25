import { runAccountSwitchBatch } from "./accountSwitchBatch";
import { StatusAnnouncer } from "./components/StatusAnnouncer";
import type { PlainMessageKey } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import { errorAlerts } from "./errorAlerts";
import { noAgentState, pickableAgents } from "./agents";
import { reportExternalLinkFailure } from "./components/ExternalLink";
import { copyText } from "./components/copyText";
import { ActionableError } from "./errorRecovery";
import { FadeOverflow } from "./components/FadeOverflow";
import { GitBranch } from "lucide-react";
import type { ActionMenuEntry } from "./components/ActionMenu";
import { displayWirePath } from "./wirePath";
import React, { useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { errorPresentation } from "./errorPresentation";
import { flushSync } from "react-dom";

import { ArchiveLayer } from "./archive/ArchiveLayer";
import type { ArchiveViewState } from "./archive/ArchiveView";
import { forgetProjectsOtherThan, noteSessionSwitch, setProjectBrowserMode } from "./browser/browserState";
import { LazyTerminalFileViewer } from "./browser/LazyTerminalFileViewer";
import { ConnectionBanner } from "./components/ConnectionBanner";
import { ContentPanel } from "./components/ContentPanel";
import { HandOffOnUnmount } from "./components/HandOffOnUnmount";
import { PaneResizeHandle } from "./components/PaneResizeHandle";
import { refocusIfLost } from "./components/refocusIfLost";
import { FOCUSABLE, focusLanding, focusLandingWithoutText, focusTopBar } from "./components/regionFocus";
import { focusNewConsole, Rail } from "./components/Rail";
import { Scrim } from "./components/Scrim";
import { BareTitleBar, TitleBar } from "./components/TitleBar";
import { Toasts } from "./components/Toasts";
import { useFirstConsoleHandoff } from "./components/useFirstConsoleHandoff";
import { useLatest, useStableHandler } from "./components/useLatest";
import { focusAfterLoss } from "./components/regionFocus";
import { DaemonRequestError } from "./daemon-client";
import { ConfirmDialog } from "./dialogs/ConfirmDialog";
import type { DialogRequest } from "./dialogs/dialogRequest";
import { PendingQuestionDialog, usePendingQuestion } from "./dialogs/PendingQuestionDialog";
import { RequestedDialog } from "./dialogs/RequestedDialog";
import { useConfirmIncompleteLaunch } from "./dialogs/useConfirmIncompleteLaunch";
import { useQuickCreateSession } from "./dialogs/useQuickCreateSession";
import { forcibleBranches, inspectSessionBranches, lockedWorktrees } from "./dialogs/useSessionBranches";
import { Message, useT } from "./i18n/react";
import { useOctoboardTheme } from "./theme";
import type { AsideLayout } from "./layout/AsidePane";
import { liveOwner, ownerAfterMove, ownerForSession, ownerKey, sameOwner, type AsideOwner } from "./layout/asideOwner";
import { useIsNarrow } from "./layout/breakpoint";
import { NARROW_TERMINAL_PANEL_ASIDE_MIN, useAsideFloats, usePaneWidth } from "./layout/paneWidth";
import { columnSpan, isDockEasing, type DockPhase } from "./layout/paneDockMotion";
import { type HeldChoices, useConsoleSwitchFollowUp } from "./layout/useConsoleSwitchFollowUp";
import { usePaneToggles } from "./layout/usePaneToggles";
import { useRegionCycle } from "./layout/useRegionCycle";
import { useAppExit } from "./lifecycle/useAppExit";
import { useGitStatusSchedule } from "./lifecycle/useGitStatusSchedule";
import { useStatusItemMenu } from "./lifecycle/useStatusItemMenu";
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { archiveKey, isLive, type Location } from "./navigation/history";
import { useNavigationHistory } from "./navigation/useNavigationHistory";
import { usePlatform } from "./platform/react";
import { ALREADY_RUNNING_CODES, type Console, type ConsoleSessionColour, type Project, type Session } from "./protocol";
import { sessionDisplayTitle } from "./sessionLabel";
import { PANEL_ENTRIES } from "./panels/panelRegistry";
import { RightPane } from "./panels/RightPane";
import { maximizedKind, openPanel, OWNER_KINDS, panelId, panelRegion, OWNER_TOGGLE_KINDS, restoreStack, shownKinds, shownPanelRegions, stackedKinds, toggleKind, type OwnerKind, type PanelKind } from "./panels/stackState";
import { useFloatingPanel } from "./panels/useFloatingPanel";
import { usePanelStacks } from "./panels/usePanelStacks";
import { LoginPageOpener } from "./settings/AccountLogin";
import { SettingsDialog } from "./settings/SettingsDialog";
import { registerSettingsLauncher } from "./settings/settingsLauncher";
import { useSettingsDialog } from "./settings/useSettingsDialog";
import { updateNoticeKind } from "./updates/noticeKind";
import { UpdateNotice } from "./updates/UpdateNotice";
import { useUpdateSelector, useUpdateStore } from "./updates/UpdatesProvider";
import { useSessionActionShortcuts } from "./sidebar/useSessionActionShortcuts";
import { belongsToFocus, focusAfterSelect, focusFor, focusKey, KEY_SWITCH_SELECTION, resolveFocus, shortcutOutcome } from "./sidebar/focus";
import { useFocusShortcut } from "./sidebar/focusShortcut";
import { useLastSessionPerConsole } from "./sidebar/lastSessionPerConsole";
import { archiveNeedsConfirmation, consoleSessionAbove, ordersByStatus, switchStrip } from "./sidebar/order";
import { useSession, useWaitingCount } from "./sessions/useSessions";
import { indexSessions, sessionsOf } from "./sessions/sessionIndex";
import { Sidebar } from "./sidebar/Sidebar";
import { useSidebarView } from "./sidebar/sidebarView";
import { useSwitchShortcut } from "./sidebar/switchShortcut";
import type { ArchiveScope } from "./sidebar/types";
import { useDaemon, useDaemonStore, useDaemonStoreEqual } from "./store";
import { terminalEmptyState } from "./terminal/emptyState";
import { startShellTerminal } from "./terminal/shellTerminals";
import { TerminalPane, type TerminalPaneHandle } from "./terminal/TerminalPane";
import { interactionIsTouch, ringForAutomaticFocus } from "./touchInput";
import type { ViewedFile } from "./browser/TerminalFileViewer";
import { nextWaitingSession, waitingSessionsInTreeOrder } from "./waiting";
import { WelcomeScreen, type WelcomeScreenHandle } from "./WelcomeScreen";

/** Shared and never changed, so that no sessions is the same value every time. */
const NO_SESSIONS: Session[] = [];

/** How long the result of a batch of account switches stays in the status region. */
const SWITCH_ANNOUNCEMENT_MS = 10_000;

export function App(): React.ReactElement {
  const t = useT();
  const { resolved: appearance } = useOctoboardTheme();
  const { clipboard, links } = usePlatform();
  const daemon = useDaemon();
  const { request, toastError, toastNotice, reconnect, store } = daemon;
  const connectionState = useDaemonStore((s) => s.connectionState);
  const hosts = useDaemonStore((s) => s.hosts);
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultAccountNames = useDaemonStore((s) => s.settings.default_account_names);
  // Keeps Settings out of the way of the question's dialog.
  const questionPending = usePendingQuestion();
  const [selectedSessionId, setSelectedSessionIdState] = useState<string>();
  // Every change of the selected session is noted for the project panes, which follow it even when
  // one was not mounted while it changed (`browserState.ts`).
  const selectedSessionRef = useRef<string>(undefined);
  const setSelectedSessionId = (id: string | undefined) => {
    if (id !== selectedSessionRef.current) noteSessionSwitch();
    selectedSessionRef.current = id;
    setSelectedSessionIdState(id);
  };
  const [dialogRequest, setDialogRequest] = useState<DialogRequest>();
  // The file a path in the terminal's output named, while the file viewer shows it over the window.
  const [linkedFile, setLinkedFile] = useState<ViewedFile>();
  // Where the viewer returns focus to when it closes, as its opener gave it; the agent terminal when
  // the opener gave none or could not take it (its view is gone). Kept across the viewer opening
  // another file from within.
  const linkedReturnFocus = useRef<() => boolean>(undefined);
  const viewLinkedFile = (file: ViewedFile, returnFocus?: () => boolean) => {
    linkedReturnFocus.current = returnFocus;
    setLinkedFile(file);
  };
  const [archiveScope, setArchiveScope] = useState<ArchiveScope>();
  // How far each scope's archive list was loaded and scrolled, by `archiveKey`, for Back and
  // Forward to put it back. A visit opened fresh (`openArchive`) forgets it and starts at the top.
  const archiveViewStates = useRef(new Map<string, ArchiveViewState>());
  const [archiveRestore, setArchiveRestore] = useState<ArchiveViewState>();
  // Sessions whose `resume_session` is in flight, for the terminal's loading state.
  const [resumingIds, setResumingIds] = useState<ReadonlySet<string>>(new Set());
  // What the window says once a batch of account switches is done, for assistive technology.
  const [switchAnnouncement, setSwitchAnnouncement] = useState<string>();
  const switchAnnouncementTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const terminalRef = useRef<TerminalPaneHandle>(null);
  const welcomeRef = useRef<WelcomeScreenHandle>(null);
  // The agent terminal's area, which a maximized panel covers and makes inert.
  const agentAreaRef = useRef<HTMLDivElement>(null);
  // Only one of the two is mounted: the welcome screen stands in for the terminal while there is no
  // console. While a maximized panel covers the agent terminal, focus meant for it goes where F6
  // lands in that panel instead. Read off the DOM rather than the state, so a hand-off that runs in
  // the commit restoring the layout (the maximized or floating panel closing) already finds the terminal
  // uncovered: it comes before the pane in the tree, so its `inert` is gone by the time the panel's
  // own hand-off runs.
  //
  // After a tap it lands on nothing that takes text (`interactionIsTouch`): the terminal, or a
  // shell terminal covering it, would raise the on-screen keyboard over what was just shown. It
  // lands on a covering panel's first control that takes none, else on the top bar's first control,
  // so focus still never falls to `<body>` from what is going away.
  const focusTerminal = () => {
    const touch = interactionIsTouch();
    const covering = agentAreaRef.current?.hasAttribute("inert") ? document.querySelector("[data-panel-maximized], [data-floating-panel] [data-panel-card]") : null;
    if (covering && (touch ? focusLandingWithoutText : focusLanding)(covering)) return;
    if (touch) return focusTopBar();
    (terminalRef.current ?? welcomeRef.current)?.focus();
  };

  const consoleList = Array.from(consoles.values());
  const projectList = Array.from(projects.values());
  // The sessions are not read here as a whole: every status change of every session would render
  // this component and everything it lays out. What it needs of them — the selected one, the
  // aside owner's, a count — is read as that alone, and the rest looked up in the store by whoever
  // acts (`allSessions`).
  const selectedSession = useSession(selectedSessionId);
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
  const archiveConsole = archiveScope ? consoles.get(archiveScope.console) : undefined;
  const archiveProjectId = archiveScope && "project" in archiveScope ? archiveScope.project : undefined;
  const archiveProject = archiveProjectId ? projects.get(archiveProjectId) : undefined;
  const archiveConsoleSessionId = archiveScope && "consoleSession" in archiveScope ? archiveScope.consoleSession : undefined;
  const archiveBoundTo = useSession(archiveConsoleSessionId);
  // The archive view closes by itself once what it lists is gone.
  const archiveOpen =
    archiveConsole !== undefined &&
    (archiveProjectId === undefined || archiveProject !== undefined) &&
    (archiveConsoleSessionId === undefined || archiveBoundTo !== undefined);
  // What the archive view lists, read here only while it is open, and the same array until one of
  // those sessions changes.
  const archiveSessions = useDaemonStore(
    useShallow((s) => {
      if (!archiveOpen || !archiveConsole) return NO_SESSIONS;
      const index = indexSessions(s.sessions);
      if (archiveProject) return sessionsOf(index.byProject, archiveProject.id);
      const inConsole = sessionsOf(index.byConsole, archiveConsole.id);
      return archiveBoundTo ? inConsole : inConsole.filter((session) => session.role === "console");
    }),
  );

  // `currentConsole` falls back to the first console (`sidebarView.ts`), so it is missing exactly
  // when there is no console at all, and `shownConsole` below with it. No console means no project
  // and no session either, so the content area has nothing for its sidebar, terminal and aside and
  // shows the welcome screen instead. The panes' saved visibility and widths are left as they are:
  // this is an absence of content, not a choice, and they come back as the user left them.
  const noConsoles = sidebarView.currentConsole === undefined;
  // The right pane is a stack of its owner's panels (`panels/stackState.ts`) and shows while one is
  // open: the top bar's toggles, the F6 cycle and the pane itself all read the owner's stack. With
  // none open the pane floats in while the pointer is on a kind's toggle, showing that panel.
  const { stacks: panelStacks, setStack: setPanelStack } = usePanelStacks();
  const ownerKind = asideOwner?.kind;
  const ownerOrder = ownerKind ? OWNER_KINDS[ownerKind] : [];
  // Too narrow a window to dock the pane (a phone): one panel at a time floats over the agent
  // terminal, which keeps the whole width (`useAsideFloats`, `panels/useFloatingPanel.ts`). The
  // drawer's open state is kept here, as the floating panel has to know it before the pane toggles
  // run.
  const asideFloats = useAsideFloats();
  const drawer = useState(false);
  const floating = useFloatingPanel({ floats: asideFloats, owner: ownerKind, stacks: panelStacks, drawerOpen: drawer[0], focusTerminal });
  const floatingShown = floating.kind !== undefined;
  const storedStack = ownerKind ? panelStacks[ownerKind] : undefined;
  const ownerStack = asideFloats && ownerKind ? { open: floating.kind ? [floating.kind] : [], shares: {} } : storedStack;
  const openPanels = ownerStack ? shownKinds(ownerStack, ownerOrder) : [];
  const paneShown = openPanels.length > 0;
  // A maximized panel (the Terminal panel's, `PanelStack.tsx`) leaves the pane's column and covers
  // the agent terminal's area, which is inert meanwhile and not an F6 stop; the sidebar and the
  // panels left in the column stay as they are. With none left there, the column folds away and the
  // area, with the panel over it, reaches the window's end.
  const covered = ownerStack !== undefined && maximizedKind(ownerStack, ownerOrder) !== undefined;
  const stackedPanels = ownerStack ? stackedKinds(ownerStack, ownerOrder) : [];
  const columnShown = stackedPanels.length > 0;
  // With no card in the column (none open, or the one open maximized) a panel that is not open
  // floats in while the pointer rests on its toggle.
  const panes = usePaneToggles({
    hasSidebar: !noConsoles,
    // Below the width that floats panels over the terminal a panel opens only on a press.
    asidePeekable: hasAside && !columnShown && !asideFloats,
    focusTerminal,
    floatingPanel: floatingShown,
    closeFloatingPanel: floating.close,
    drawer,
  });
  const isNarrow = useIsNarrow();
  const asideDocked = hasAside && columnShown && !asideFloats;
  /** Floats `kind` of `owner`, out of the sidebar's drawer's way. */
  const openFloating = (owner: OwnerKind, kind: PanelKind) => {
    floating.open(owner, kind);
    if (panes.sidebarOpen) panes.closeSidebar();
  };
  const [peekKind, setPeekKind] = useState<PanelKind>();
  const filesOnScreen =
    ownerKind === "project" && (openPanels.includes("files") || (panes.asidePeek.active && peekKind === "files"));
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
  // console session, which has no Terminal panel) or to none, it is restored, so coming back shows
  // the stack as it was. Moving between projects keeps it.
  useEffect(() => {
    for (const kind of Object.keys(OWNER_KINDS) as OwnerKind[]) {
      if (kind !== ownerKind && panelStacks[kind].maximized !== undefined) setPanelStack(kind, restoreStack(panelStacks[kind]), { persist: false });
    }
  }, [ownerKind]);
  const hoverPanelToggle = (kind: PanelKind, hovered: boolean) => {
    if (!hovered) return panes.asidePeek.leave();
    if (columnShown || openPanels.includes(kind)) return;
    setPeekKind(kind);
    panes.asidePeek.reveal();
  };
  // The hidden sidebar floats in showing the console whose avatar the pointer is on, which is not
  // the current one until something is pressed in it. That is a plain console view: a focus mode
  // belongs to the current console.
  const currentConsole = sidebarView.currentConsole;
  const peekedId = panes.sidebarPeek.consoleId;
  const peekedConsole = peekedId ? consoles.get(peekedId) : undefined;
  const shownConsole = peekedConsole ?? currentConsole;
  const previewing = shownConsole !== currentConsole;
  // A previewed console that is deleted takes its avatar, and so the pointer's leaving it, with it.
  const { active: peeking, leaveConsole } = panes.sidebarPeek;
  useEffect(() => {
    if (peekedId && peeking && !consoles.has(peekedId)) leaveConsole();
  }, [peekedId, peeking, consoles]);

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
  // A region off screen is skipped by F6: a hidden docked pane, a closed drawer, and a floating pane
  // too, which is only a hover away rather than shown. The top bar and the rail are always there.
  const regionCycle = useRegionCycle({
    shown: {
      topbar: true,
      rail: true,
      welcome: noConsoles,
      sidebar: !noConsoles && panes.sidebarShown,
      archive: archiveOpen && !covered && !floatingShown,
      terminal: (selectedSession !== undefined || emptyHasAction) && !archiveOpen && !covered && !floatingShown,
      banner: connectionState === "reconnecting" || connectionState === "closed",
      update: updateNoticeShown,
    },
    panels: shownPanelRegions(ownerStack, ownerKind),
    focusTerminal,
  });

  // The sidebar's handle sits at its column's settled edge. During the ease that edge is still
  // moving, and the handle's own position is the settled width, so it stays off until the ease has
  // finished. The aside does not ease.
  const [sidebarColumnPhase, setSidebarColumnPhase] = useState<DockPhase>(panes.sidebarDocked ? "open" : "closed");
  // A closing sidebar column keeps its width clamp until it leaves the row, so its content cannot
  // widen and reflow on the first frame. The sidebar is a column in the row only at and above the
  // breakpoint, where it is docked; below it, a drawer, which takes no room from the aside beside it.
  const asideMin = isNarrow && stackedPanels.includes("terminal") ? NARROW_TERMINAL_PANEL_ASIDE_MIN : 320;
  const sidebarWidth = usePaneWidth("sidebar", {
    sidebar: !noConsoles && !isNarrow && (panes.sidebarDocked || sidebarColumnPhase !== "closed"),
    aside: asideDocked,
  }, isNarrow, asideMin);
  const asideWidth = usePaneWidth("aside", {
    sidebar: !noConsoles && !isNarrow && panes.sidebarDocked,
    aside: asideDocked,
  }, isNarrow, asideMin);
  const asideLayout: AsideLayout = {
    width: asideWidth.width,
    minWidth: asideWidth.min,
    peek: paneShown ? undefined : panes.asidePeek,
    folded: paneShown && !columnShown,
    overlay: asideFloats,
    floatingKind: floating.kind,
  };
  // While the column is folded, a panel floats in a pane of its own (`RightPane`).
  const asideFloat = asideLayout.folded ? panes.asidePeek : undefined;

  // An owner that is gone leaves no aside, and is not brought back by anything that appears later.
  useEffect(() => {
    if (asideOwnerState && !asideOwner && hosts) setAsideOwner(undefined);
  }, [asideOwnerState, asideOwner, hosts]);
  const lastSessionIn = useLastSessionPerConsole(selectedSession, () => store.getState().sessions);
  /** Shows the session last selected in `consoleId`, if it is still there, and the aside of that
   * session; each part only where `choices` says so. Like Back and Forward it only shows it: an
   * interrupted session is not resumed, an archived one not reopened. */
  const showLastSession = (consoleId: string | undefined, { selection, owner }: HeldChoices) => {
    const session = consoleId ? lastSessionIn(consoleId) : undefined;
    if (selection) setSelectedSessionId(session?.id);
    if (owner) setAsideOwner(ownerForSession(session));
  };
  // Making another console current closes a browser of a project of the console left behind (a
  // console the floating sidebar only previews is not current), and coming back to the selected
  // session's console gives the aside back to the session's own owner. A console made current by a
  // press in the previewing sidebar then shows its last session and that session's aside once the
  // press has ended, except what the press chose itself (`commitPreview`).
  const followUp = useConsoleSwitchFollowUp(
    sidebarView.currentConsole?.id,
    () => {
      const next = ownerAfterMove(asideOwnerState, selectedSession, sidebarView.currentConsole?.id, projects);
      if (!sameOwner(next, asideOwnerState)) setAsideOwner(next);
    },
    // Rendered at once: the history's recording effect has to run before the navigation join ends
    // (`commitPreview`), or the restore would be a visit of its own.
    (choices) => flushSync(() => showLastSession(sidebarView.currentConsole?.id, choices)),
  );
  /** The owner as the user just set it, which settles a console change held for the same press. */
  const setOwnerByUser = (owner: AsideOwner | undefined) => {
    followUp.claim("owner");
    setAsideOwner(owner);
  };
  /** The selection as the user just made it, which settles a console change held for the same press. */
  const setSelectionByUser = (id: string | undefined) => {
    followUp.claim("selection");
    setSelectedSessionId(id);
  };
  /** Makes a console current by a press on the rail, or a console this window created: the terminal
   * and the aside go with it, and an open archive view gives way. */
  const switchConsole = (consoleId: string) => {
    if (consoleId === sidebarView.currentConsole?.id) return;
    sidebarView.selectConsole(consoleId);
    setArchiveScope(undefined);
    showLastSession(consoleId, { selection: true, owner: true });
  };
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
  const { settingsOpen, settingsNavigation, openSettings, openSettingsAt, closeSettings } = useSettingsDialog({
    ready: hosts !== undefined,
    otherModalOpen,
    focusTerminal,
  });

  // A dormant session can be resumed through several controls, so overlapping requests for the
  // same session share one in-flight guard.
  const inFlightRef = useRef<Set<string>>(new Set());

  const runOnce = (key: string, action: () => Promise<void>): Promise<boolean> =>
    runOnceGuarded(inFlightRef.current, key, action, toastError);

  /** Resumes a session; resolves to whether the daemon accepted it. A console may now
   * hold any number of live console sessions, so there is nothing to pre-emptively refuse here —
   * selecting, resuming or reopening one simply works, whatever else is running in the console. An
   * account whose last sharing change did not finish is asked about first; cancelling resolves
   * false and sends nothing. */
  const resumeSession = async (sessionId: string): Promise<boolean> => {
    let accepted = false;
    await confirmLaunch(store.getState().sessions.get(sessionId)?.account_id, async () => {
      accepted = await runOnce(`resume:${sessionId}`, () => startResume(sessionId));
    });
    return accepted;
  };

  const startResume = (sessionId: string): Promise<void> =>
    whileResuming(setResumingIds, sessionId, () => request({ type: "resume_session", session: sessionId, terminal_appearance: appearance }));

  /** Resumes an archived or interrupted session for the session dialog: the same request as
   * `resumeSession`, with the terminal's resuming state and in-flight guard, but rejecting with the
   * daemon's reason (an unresumable conversation, a failed unarchive) for the dialog to show, rather
   * than toasting it. */
  const resumeFromDialog = (sessionId: string): Promise<void> =>
    runOnceRejecting(inFlightRef.current, `resume:${sessionId}`, () => startResume(sessionId));

  /** Switches sessions to an account in the background, the dialog that asked being gone by then.
   * Each session shows as switching its account until its turn is over (see
   * `runAccountSwitchBatch`), so its terminal says so instead of offering a Resume that would race
   * the switch; the failures come as one error alert. */
  const switchAccounts = (sessionIds: string[], account: string | null, accountName: string): void => {
    void runAccountSwitchBatch(store, sessionIds, account, {
      accountName,
      switchOne: (sessionId, target) =>
        request({ type: "switch_session_account", session: sessionId, account: target, terminal_appearance: appearance }).then(() => undefined),
      onFinished: ({ switched, failed }) => {
        // The dialog that asked is gone, and with it anything that would say the outcome. A batch with
        // failures is told by its error alert alone. The text is cleared again so that an identical
        // result of a later batch is written afresh. A switch takes seconds, so this is never within the
        // focus move of the dialog closing.
        if (failed > 0 || switched === 0) return;
        clearTimeout(switchAnnouncementTimer.current);
        setSwitchAnnouncement(t("dialog.switchAccount.done", { count: switched, account: isolate(accountName) }));
        switchAnnouncementTimer.current = setTimeout(() => setSwitchAnnouncement(undefined), SWITCH_ANNOUNCEMENT_MS);
      },
    });
  };

  const openConsoleSession = (console_: Console) => openDialog({ kind: "new-console-session", console: console_ });

  /** Puts a session this window just opened on screen; it is in the console the sidebar shows
   * (a dialog holds the window still, and a quick create does not call this once the console has
   * changed under it), so only the archive view, were it open, has to give way. */
  const showOpenedSession = (sessionId: string) => {
    setSelectionByUser(sessionId);
    // The reply that names the session has already been applied to the store by the time this runs.
    setOwnerByUser(ownerForSession(store.getState().sessions.get(sessionId)));
    setArchiveScope(undefined);
  };

  const quickCreateSession = useQuickCreateSession(showOpenedSession, sidebarView.currentConsole?.id, confirmLaunch);

  // `create_console` is answered with a bare ack, so the console this window just created is
  // recognised as the one that was not there when its dialog opened, and the sidebar switches to it.
  const consolesBeforeNew = useRef<Set<string> | undefined>(undefined);
  // The control last pressed in the sidebar's open drawer, or in a menu it opened. A tap does not
  // move focus to what it presses, so a dialog it opens would otherwise give focus back, as it
  // closes, to whatever held it before: the terminal behind the drawer.
  const drawerPress = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!panes.sidebarOpen) return;
    // Taken as the press ends, before the press's own handler runs: a pointer that goes down to
    // scroll the drawer ends in a cancel, and records nothing.
    const onPointerUp = (event: PointerEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest("[role=menu], [role=listbox]")) return;
      drawerPress.current = target?.closest("[data-pane=sidebar]") ? target.closest<HTMLElement>(FOCUSABLE) : null;
    };
    document.addEventListener("pointerup", onPointerUp, true);
    return () => {
      document.removeEventListener("pointerup", onPointerUp, true);
      drawerPress.current = null;
    };
  }, [panes.sidebarOpen]);

  const openDialog = (dialog: DialogRequest) => {
    // Nothing to offer without an agent that is added and installed; the controls are disabled already.
    const { agentAvailability, settings } = store.getState();
    if ((dialog.kind === "new-session" || dialog.kind === "new-console-session") && noAgentState(agentAvailability, settings.added_agents)) return;
    if (dialog.kind === "hand-off" && !pickableAgents(agentAvailability, settings.added_agents).some((agent) => agent !== dialog.session.agent)) return;
    if (dialog.kind === "new-console") consolesBeforeNew.current = new Set(consoles.keys());
    // Opened from the open drawer, it hands focus back to what opened it there as it closes: focused
    // before the dialog mounts, that is what the dialog restores focus to.
    const trigger = drawerPress.current;
    drawerPress.current = null;
    if (panes.sidebarOpen && trigger?.isConnected && !document.activeElement?.closest("[data-pane=sidebar]")) trigger.focus({ preventScroll: true });
    setDialogRequest(dialog);
  };
  useEffect(() => {
    const before = consolesBeforeNew.current;
    if (!before) return;
    const created = consoleList.find((c) => !before.has(c.id));
    if (created) {
      consolesBeforeNew.current = undefined;
      switchConsole(created.id);
    }
  }, [consoleList]);
  const closeDialog = () => {
    setDialogRequest(undefined);
    // The new console's broadcast can trail the ack that closed the dialog by a moment.
    if (consolesBeforeNew.current) setTimeout(() => (consolesBeforeNew.current = undefined), 2000);
  };

  const selectSession = (session: Session, { enterFocus = false, resume = true } = {}) => {
    setSelectionByUser(session.id);
    // A console session brings its report into the aside, a project session its project's browser.
    setOwnerByUser(ownerForSession(session));
    // Whatever led here — the sidebar, the archive, the waiting-count button — the sidebar follows
    // the session: its console is the one shown, and focus mode on something the session does not
    // belong to is left, unless `enterFocus` asks for the focus mode of this console session itself.
    sidebarView.selectConsole(session.console_id);
    const nextFocus = focusAfterSelect(sidebarView.focus, session, enterFocus, store.getState().sessions);
    if (nextFocus !== sidebarView.focus) sidebarView.setFocus(nextFocus);
    if (archiveOpen) {
      setArchiveScope(undefined);
      focusTerminal();
    }
    // An interrupted session is resumed by selecting it, unless `resume` is off; an archived one only
    // shows its history, and is reopened by the Reopen button or an explicit Reopen (`reopenSession`).
    // One an account switch holds reads interrupted while its process is down, and the switch relaunches it.
    const switching = (store.getState().sessions.get(session.id) ?? session).switching_account === true;
    if (resume && session.status === "interrupted" && !switching) void resumeSession(session.id);
  };

  const reopenSession = (session: Session) => {
    selectSession(session);
    if (session.status === "archived") void resumeSession(session.id);
  };

  /** Opens a project's browser in the aside, putting the aside on screen (its Files panel open;
   * a maximized panel stays maximized), without selecting or starting any session: the terminal
   * beside it stays as it is. */
  const browseProject = (project: Project) => {
    setProjectBrowserMode(project.id, "files");
    setOwnerByUser({ kind: "project", project: project.id });
    if (asideFloats) return openFloating("project", "files");
    setPanelStack("project", openPanel(panelStacks.project, "files", OWNER_KINDS.project));
    floating.noteOpened("project", "files");
    // The sidebar's drawer in a narrow window covers what was asked for.
    if (panes.sidebarOpen) panes.closeSidebar();
  };

  const pullProjectBranch = (project: Project) => {
    request({ type: "pull_project_branch", project: project.id }).catch((err: unknown) => toastError(errorPresentation(err as Error)));
  };

  const openRepository = (url: string) => {
    void links?.open(url).catch((error: unknown) => reportExternalLinkFailure(t, url, error));
  };

  const openProjectDirectory = (project: Project) => {
    request({ type: "open_project_directory", project: project.id }).catch((err: unknown) => toastError(errorPresentation(err as Error)));
  };

  const copyProjectName = (project: Project) => {
    copyText(project.name, { clipboard }).then(
      () => toastNotice(t("sidebar.project.nameCopied")),
      (cause: unknown) => toastError(new ActionableError(t("sidebar.project.copyNameFailed"), "clipboard", { cause })),
    );
  };

  const copySessionId = (session: Session) => {
    copyText(session.id, { clipboard }).then(
      () => toastNotice(t("sidebar.session.idCopied")),
      (cause: unknown) => toastError(new ActionableError(t("sidebar.session.copyIdFailed"), "clipboard", { cause })),
    );
  };

  const openProjectTerminal = (project: Project) => {
    request({ type: "open_project_terminal", project: project.id }).catch((err: unknown) => toastError(errorPresentation(err as Error)));
  };

  const pushProjectBranch = (project: Project) => {
    request({ type: "push_project_branch", project: project.id })
      .then(() => toastNotice(t("sidebar.project.pushed")))
      .catch((err: unknown) => toastError(errorPresentation(err as Error)));
  };

  const loadProjectBranches = async (project: Project): Promise<ActionMenuEntry[]> => {
    const [reply, source] = await Promise.all([
      request({ type: "list_project_branches", project: project.id }),
      request({ type: "get_project_source", project: project.id }),
    ]);
    if (reply.type !== "project_branches") throw new Error(t("sidebar.project.branchesFailed"));
    const git = source.type === "project_source" ? source.source.git : undefined;
    const currentBranch = git?.worktrees.find((worktree) => worktree.id === git.worktree)?.branch;
    const entries: ActionMenuEntry[] = reply.branches.map((branch) => {
      const name = displayWirePath(branch.name);
      const current = currentBranch === branch.name;
      return {
        label: name,
        content: <FadeOverflow as="span" dir="ltr" className="block" titleWhenClipped={name}>{name}</FadeOverflow>,
        icon: GitBranch,
        selected: current,
        onClick: () => {
          if (current) return;
          request({ type: "switch_project_branch", project: project.id, branch: branch.name })
            .catch((err: unknown) => toastError(errorPresentation(err as Error)));
        },
      };
    });
    if (!entries.length) entries.push({ label: t("sidebar.project.noBranches"), icon: GitBranch, disabled: true, onClick: () => {} });
    if (!reply.complete) entries.push({ label: t("sidebar.project.branchesIncomplete"), icon: GitBranch, disabled: true, onClick: () => {} });
    return entries;
  };

  // The archive view is in the agent terminal's area, which a maximized panel covers: restoring
  // it is how the user sees what was asked for, whether opened or arrived at through the history.
  const revealArchive = () => {
    if (ownerKind && storedStack && maximizedKind(storedStack, ownerOrder) !== undefined) setPanelStack(ownerKind, restoreStack(storedStack), { persist: false });
    // So does a panel floating over that area.
    floating.close();
  };

  const openArchive = (scope: ArchiveScope) => {
    // Asking for the archive already open changes nothing, so what Back would restore stays.
    const alreadyOpen = archiveOpen && archiveScope !== undefined && archiveKey(archiveScope) === archiveKey(scope);
    if (!alreadyOpen) {
      archiveViewStates.current.delete(archiveKey(scope));
      setArchiveRestore(undefined);
    }
    setArchiveScope(scope);
    revealArchive();
    // Below the `docked` breakpoint the sidebar is a drawer over the terminal, and would keep
    // covering the archive that was just asked for.
    if (panes.sidebarOpen) panes.closeSidebar();
  };

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

  // ⇧⌘F: into the focus mode of the selected session's context, or back out of focus mode.
  useFocusShortcut(() => {
    const outcome = shortcutOutcome(sidebarView.focus, selectedSession, projects);
    if (!outcome) return;
    if (selectedSession && outcome.focus) sidebarView.selectConsole(selectedSession.console_id);
    sidebarView.setFocus(outcome.focus);
  });

  // The console sessions a console session's focus mode offers to switch between (its switch strip),
  // for ⌃Tab.
  const currentConsoleId = sidebarView.currentConsole?.id;
  // Only while a console session is in focus, where the strip is on screen and the shortcut exists.
  const stripConsoleId = sidebarView.focus && "consoleSession" in sidebarView.focus ? currentConsoleId : undefined;
  const switchableConsoleSessions = useDaemonStore(
    useShallow((s) =>
      stripConsoleId !== undefined
        ? switchStrip(sessionsOf(indexSessions(s.sessions).byConsole, stripConsoleId), stripConsoleId).map((e) => e.consoleSession)
        : NO_SESSIONS,
    ),
  );
  // Pressing a chip is a selection like pressing the session's row, which also enters its focus mode.
  const switchConsoleSession = (session: Session) => selectSession(session, { enterFocus: true });
  // ⌃Tab only shows the console session, without resuming an interrupted one (`KEY_SWITCH_SELECTION`),
  // and hands keyboard focus to its terminal, as selecting a session does. The switch is applied in
  // one render first, so the terminal is already showing the new session when it takes focus: taken
  // any earlier, the focus-in report xterm sends would reach the session being left.
  const switchConsoleSessionByKey = (session: Session) => {
    flushSync(() => selectSession(session, KEY_SWITCH_SELECTION));
    focusTerminal();
  };
  const moveConsoleSession = useSwitchShortcut({
    focus: sidebarView.focus,
    strip: switchableConsoleSessions,
    shown: panes.sidebarShown,
    onSwitch: switchConsoleSessionByKey,
  });

  const closeArchive = () => {
    setArchiveScope(undefined);
    focusTerminal();
  };

  // What the content area shows, for Back and Forward. A session that is gone counts as none selected.
  const location: Location = {
    console: sidebarView.currentConsole?.id,
    session: selectedSession?.id,
    focus: sidebarView.focus && focusKey(sidebarView.focus),
    archive: archiveOpen ? archiveScope : undefined,
  };
  const navigation = useNavigationHistory({
    location,
    ready: hosts !== undefined,
    isLive: (target, sessions) => isLive(target, { consoles, projects, sessions }),
    // Sets the state straight, not through `selectSession`: going back to an interrupted session
    // must not resume it. A focus mode outside the session's context is left, as selecting does.
    apply: (target) => {
      if (target.console) sidebarView.selectConsole(target.console);
      const sessions = store.getState().sessions;
      const session = target.session ? sessions.get(target.session) : undefined;
      sidebarView.setFocus(focusFor(resolveFocus(target.focus, target.console, projects, sessions), session, sessions));
      setSelectedSessionId(target.session);
      // The aside follows the session moved to, as selecting it does, and a move that keeps the
      // session leaves it to whoever owns it; either way under the rule of making another console
      // current, as the place moved to may have another current console than its session's.
      const owner = target.session !== selectedSessionId ? ownerForSession(session) : asideOwner;
      setAsideOwner(ownerAfterMove(owner, session, target.console, projects));
      setArchiveRestore(target.archive && archiveViewStates.current.get(archiveKey(target.archive)));
      setArchiveScope(target.archive);
      if (target.archive) revealArchive();
    },
    // Focus a move leaves nowhere goes to the terminal, or, where that takes none (no session, or
    // a touch screen, `focusTerminal`), to the top bar's first control.
    focusTerminal: () => {
      focusTerminal();
      const active = document.activeElement;
      if (!active || active === document.body || active.matches(":disabled")) focusTopBar();
    },
  });

  // A press in the previewing sidebar makes its console current. The console change and that
  // selection, or the console change and the restore of what the console last showed (the follow-up
  // `releasePress` runs), are one visit in the history, and neither the last session nor the aside
  // of the console change shows for the length of the press. The follow-up's release comes first,
  // then the join ends.
  const commitPreview = () => {
    if (!previewing || !shownConsole) return;
    followUp.holdForPress();
    navigation.joinPressVisits(followUp.releasePress);
    sidebarView.selectConsole(shownConsole.id);
    setArchiveScope(undefined);
  };

  const setSessionColour = (session: Session, colour: ConsoleSessionColour) => {
    request({ type: "set_session_colour", session: session.id, colour }).catch((err: unknown) => toastError(errorPresentation(err as Error)));
  };

  const setLocked = (session: Session, locked: boolean) => {
    request({ type: "set_session_locked", session: session.id, locked }).catch((err: unknown) => toastError(errorPresentation(err as Error)));
  };

  const releaseSession = (session: Session) => {
    request({ type: "release_session", session: session.id }).catch((err: unknown) => toastError(errorPresentation(err as Error)));
  };

  const setPinned = (target: { project: Project } | { session: Session }, pinned: boolean) => {
    const body =
      "project" in target
        ? ({ type: "update_project", project: target.project.id, pinned } as const)
        : ({ type: "set_session_pinned", session: target.session.id, pinned } as const);
    request(body).catch((err: unknown) => toastError(errorPresentation(err as Error)));
  };

  // Archiving at once takes the row away from under a menu that returns focus to its trigger in
  // that row, so once the session reads archived focus is handed to the terminal, where the
  // keyboard user works next. The session's status is read here, as a value, because nothing else
  // renders this component for a session that is not selected.
  const [archivingId, setArchivingId] = useState<string>();
  const archivingStatus = useDaemonStore((s) => (archivingId ? s.sessions.get(archivingId)?.status : undefined));
  useEffect(() => {
    if (archivingStatus !== "archived") return;
    setArchivingId(undefined);
    if (document.activeElement !== document.body) return;
    ringForAutomaticFocus();
    focusTerminal();
  }, [archivingStatus]);
  // A session being resumed, or switched to another account, still asks: archiving it would cut that
  // short. So does one with a bound branch or worktree git refuses to remove, or a locked worktree,
  // which only the confirmation can take along; the daemon removes the rest once the process has
  // ended. Finding that out takes a round trip, during which the session can change, a repeated
  // press is ignored, and a dialog opened meanwhile is left alone.
  const resuming = useLatest(resumingIds);
  const openedDialog = useLatest(dialogRequest);
  const launchQuestionOpen = useLatest(launchConfirmation !== undefined);
  const inspectingArchive = useRef(new Set<string>());
  const asksBeforeArchiving = (session: Session) =>
    resuming.current.has(session.id) || session.switching_account === true || archiveNeedsConfirmation(allSessions(), session);
  const archiveSession = (session: Session) => {
    const live = store.getState().sessions.get(session.id) ?? session;
    if (asksBeforeArchiving(live)) {
      openDialog({ kind: "archive-session", session });
      return;
    }
    if (inspectingArchive.current.has(live.id)) return;
    inspectingArchive.current.add(live.id);
    void inspectSessionBranches(request, [live.id]).then((branches) => {
      inspectingArchive.current.delete(live.id);
      const now = store.getState().sessions.get(live.id);
      if (!now || now.status === "archived") return;
      if (asksBeforeArchiving(now) || forcibleBranches(branches).length > 0 || lockedWorktrees(branches).length > 0) {
        if (!openedDialog.current && !launchQuestionOpen.current) openDialog({ kind: "archive-session", session: now, branches });
        return;
      }
      setArchivingId(now.id);
      request({ type: "archive_session", session: now.id }).catch((err: unknown) => {
        setArchivingId(undefined);
        toastError(errorPresentation(err as Error));
      });
    });
  };

  const waitingCount = useWaitingCount();
  const selectNextWaiting = () => {
    const next = nextWaitingSession(waitingSessionsInTreeOrder(consoleList, projectList, allSessions(), ordersByStatus(store.getState().settings)), selectedSessionId);
    if (next) selectSession(next);
  };

  // Handed to the sidebar's and the rail's rows, the archive view's rows and the top bar. The React
  // Compiler cannot keep these closures from changing with every render (they capture the selected
  // session, the focus, or write refs), so each is one function for good: the memoised rows are then
  // rendered again only for their own records.
  const stableSelectSession = useStableHandler((session: Session) => selectSession(session));
  // On a phone the drawer covers most of the terminal: picking a session in it closes it, so the
  // session's terminal is what shows (focused, except after a tap: `interactionIsTouch`). A wider
  // window leaves it open, beside the terminal.
  const stableSelectFromSidebar = useStableHandler((session: Session) => {
    selectSession(session);
    if (asideFloats && panes.sidebarOpen) panes.closeSidebar();
  });
  const stableSwitchConsoleSession = useStableHandler(switchConsoleSession);
  const stableOpenConsoleSession = useStableHandler(openConsoleSession);
  const stableOpenDialog = useStableHandler(openDialog);
  const stableQuickCreateSession = useStableHandler(quickCreateSession);
  const stableReopenSession = useStableHandler(reopenSession);
  const stableArchiveSession = useStableHandler(archiveSession);
  useSessionActionShortcuts(selectedSession, {
    onSetLocked: setLocked,
    onSetPinned: setPinned,
    onArchiveSession: archiveSession,
    onCopySessionId: copySessionId,
  });
  const stableSetSessionColour = useStableHandler(setSessionColour);
  const stableOpenArchive = useStableHandler(openArchive);
  const stableBrowseProject = useStableHandler(browseProject);
  const stableSwitchConsole = useStableHandler(switchConsole);
  const stableCommitPreview = useStableHandler(commitPreview);
  const stableNextWaiting = useStableHandler(selectNextWaiting);
  const stableToggleSidebar = useStableHandler(panes.toggleSidebar);
  const stableTogglePanel = useStableHandler(togglePanel);
  const stableHoverPanelToggle = useStableHandler(hoverPanelToggle);
  const stableFocus = useStableHandler(sidebarView.setFocus);
  const stableOpenProjectDirectory = useStableHandler(openProjectDirectory);
  const stableOpenRepository = useStableHandler(openRepository);
  const stableOpenProjectTerminal = useStableHandler(openProjectTerminal);
  const stableCopySessionId = useStableHandler(copySessionId);
  const stableCopyProjectName = useStableHandler(copyProjectName);
  const stablePullProjectBranch = useStableHandler(pullProjectBranch);
  const stablePushProjectBranch = useStableHandler(pushProjectBranch);
  const stableLoadProjectBranches = useStableHandler(loadProjectBranches);
  const stableSetLocked = useStableHandler(setLocked);
  const stableReleaseSession = useStableHandler(releaseSession);
  const stableSetPinned = useStableHandler(setPinned);
  const stableOpenSettingsAt = useStableHandler(openSettingsAt);
  useEffect(() => registerSettingsLauncher(stableOpenSettingsAt), [stableOpenSettingsAt]);
  // Where focus goes once the last alert is acknowledged with nothing to give it back to.
  const focusAfterAlert = useStableHandler(() => focusAfterLoss(focusTerminal));
  useEffect(() => errorAlerts.setFocusFallback(focusAfterAlert), [focusAfterAlert]);
  const stableBack = useStableHandler(navigation.back);
  const stableForward = useStableHandler(navigation.forward);

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
        hasSidebar={!noConsoles}
        sidebarSpan={noConsoles ? 0 : columnSpan(sidebarColumnPhase, sidebarWidth.width)}
        sidebarEasing={isDockEasing(sidebarColumnPhase)}
        onToggleSidebar={stableToggleSidebar}
        onSidebarToggleEnter={panes.sidebarPeek.reveal}
        onSidebarToggleLeave={panes.sidebarPeek.leave}
        asideSpan={asideDocked ? asideWidth.width : 0}
        panelToggles={ownerKind ? OWNER_TOGGLE_KINDS[ownerKind].map((kind) => {
          const entry = PANEL_ENTRIES[kind];
          const open = openPanels.includes(kind);
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
        canGoBack={navigation.canGoBack}
        canGoForward={navigation.canGoForward}
        onBack={stableBack}
        onForward={stableForward}
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
          selectedSessionId={selectedSessionId}
          consoles={consoleList}
          currentConsoleId={sidebarView.currentConsole?.id}
          onSelectConsole={stableSwitchConsole}
          onConsoleEnter={panes.sidebarPeek.hoverConsole}
          onConsoleLeave={panes.sidebarPeek.leaveConsole}
          hoverOpensSidebar={panes.sidebarPeek.peekable}
          onOpenDialog={stableOpenDialog}
          onSelectSession={stableSelectSession}
          onOpenArchive={stableOpenArchive}
          waitingCount={waitingCount}
          onNextWaiting={stableNextWaiting}
          onOpenSettings={openSettings}
          onOpenSettingsAt={stableOpenSettingsAt}
          focusTerminal={focusTerminal}
        />
        <ContentPanel>
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
              <Scrim open={floatingShown} label={t("panel.close")} onClose={floating.close} />
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
                visibleBrowserProjectId={filesOnScreen ? asideProject?.id : undefined}
                onBrowseProject={stableBrowseProject}
                onOpenProjectDirectory={stableOpenProjectDirectory}
                onOpenProjectTerminal={stableOpenProjectTerminal}
                onOpenRepository={links ? stableOpenRepository : undefined}
                onCopyProjectName={stableCopyProjectName}
                onCopySessionId={stableCopySessionId}
                onPullProjectBranch={stablePullProjectBranch}
                onPushProjectBranch={stablePushProjectBranch}
                onLoadProjectBranches={stableLoadProjectBranches}
                onSetSessionColour={stableSetSessionColour}
                onSetLocked={stableSetLocked}
                onReleaseSession={stableReleaseSession}
                onSetPinned={stableSetPinned}
                onReopenSession={stableReopenSession}
                onArchiveSession={stableArchiveSession}
                onOpenSettings={stableOpenSettingsAt}
                open={panes.sidebarOpen}
                drawerAway={isNarrow && !panes.sidebarOpen}
                peek={panes.sidebarDocked ? undefined : panes.sidebarPeek}
                onPeekPress={stableCommitPreview}
                onColumnPhase={setSidebarColumnPhase}
                sidebarWidth={sidebarWidth}
              />
              {panes.sidebarDocked && sidebarColumnPhase === "open" && (
                <PaneResizeHandle side="sidebar" paneWidth={sidebarWidth} />
              )}
              {/* Below the `docked` breakpoint the terminal keeps a 398px floor beside the open
                  aside's own floor, and a window too narrow for both floats the aside over it
                  (`asideFloats`) instead; `overflow-x-auto` stays a guard that lets focus reveal
                  either pane should their combined minimum still exceed the viewport. */}
              <main className="relative flex min-w-0 flex-1 overflow-x-auto docked:overflow-visible">
                {/* Takes the terminal's place in the row, so the archive covers the terminal alone, not the
                    aside beside it; the terminal fills it. Above the `docked` breakpoint the 520px basis
                    and floor are the aside's counterpart: with a 0 basis free space stays positive at any
                    window wider than the aside's own basis, flexbox never leaves the grow phase, and the
                    aside clip's shrink factor is never consulted. The open clip can shrink while the
                    sidebar's width eases, keeping the report within the viewport. 520px is about 53 columns at ~9.2px/column
                    off a real agent CLI (the terminal's padding eats the rest). Below the breakpoint the
                    sidebar is an overlay, while the open aside stays beside the terminal. The terminal
                    takes a much smaller floor: 398px is 40 columns at
                    the same ~9.2px/column plus the same padding allowance, under which the terminal stops
                    being usable at all. A window too narrow for that floor and the aside's (a phone)
                    floats the aside over the terminal instead, and the terminal takes whatever width
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
                      onViewState: (state) => {
                        if (archiveScope) archiveViewStates.current.set(archiveKey(archiveScope), state);
                      },
                    } : undefined}
                  />
                  <TerminalPane
                    ref={terminalRef}
                    resizing={isDockEasing(sidebarColumnPhase)}
                    onViewFile={viewLinkedFile}
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
                  <RightPane
                    owner={asideOwner}
                    project={asideProject}
                    layout={asideLayout}
                    active={paneShown || panes.asidePeek.active}
                    selectedSessionId={selectedSessionId}
                    focusTerminal={focusTerminal}
                    frameKeys={{
                      onEscape: panes.dismissOverlays,
                      onCycleRegion: regionCycle.cycle,
                      onMoveHistory: navigation.moveByShortcut,
                      onMoveConsoleSession: moveConsoleSession,
                    }}
                    peekKind={peekKind}
                    onViewFile={viewLinkedFile}
                    onDismiss={() => panes.asidePeek.hide()}
                    maximizeArea={agentAreaRef}
                    areaEasing={isDockEasing(sidebarColumnPhase)}
                    float={asideFloat}
                    floating={asideFloats && ownerStack ? { stack: ownerStack, onClose: floating.close } : undefined}
                  />
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
      <StatusAnnouncer text={switchAnnouncement} />
      {settingsOpen && <SettingsDialog navigation={settingsNavigation} onClose={closeSettings} />}
      {dialogRequest && (
        <RequestedDialog
          dialog={dialogRequest}
          onClose={closeDialog}
          onSessionOpened={showOpenedSession}
          onSwitchAccounts={switchAccounts}
          onResumeSession={resumeFromDialog}
        />
      )}
      {launchConfirmation}
      {linkedFile && (
        <LazyTerminalFileViewer
          file={linkedFile}
          onOpenFile={setLinkedFile}
          onClose={() => {
            setLinkedFile(undefined);
            // Back where the click came from, once the viewer has gone and taken its own focus
            // restoring with it.
            queueMicrotask(() => {
              if (!linkedReturnFocus.current?.()) focusTerminal();
            });
          }}
        />
      )}
      <PendingQuestionDialog />
      {exitConfirmOpen && (
        <ConfirmDialog
          title={<Message id={exitRestarts ? "app.restart.title" : "app.quit.title"} />}
          message={t(exitMessageKey(exitRestarts, exitHolders))}
          confirmLabel={t(exitRestarts ? "common.restart" : "common.quit")}
          destructive
          onCancel={closeExitConfirm}
          onConfirm={confirmExit}
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

// The helpers below sit outside `App` because the React Compiler cannot compile a component
// holding a `try` with a `finally`.

/** Runs `action` unless one with the same `key` is in `inFlight`; says whether it went through. A
 * second call while one is in flight counts as having gone through, since what it asks is under
 * way, and so does an "already running" refusal that is not shown. */
async function runOnceGuarded(
  inFlight: Set<string>,
  key: string,
  action: () => Promise<void>,
  toastError: (message: string | Error) => void,
): Promise<boolean> {
  try {
    await runOnceRejecting(inFlight, key, action);
    return true;
  } catch (err) {
    toastError(errorPresentation(err as Error));
    return false;
  }
}

/** `runOnceGuarded` without the toast: a failure is rethrown for the caller to show. The same
 * calls count as having gone through: one while another is in flight, and an "already running"
 * refusal. */
async function runOnceRejecting(inFlight: Set<string>, key: string, action: () => Promise<void>): Promise<void> {
  if (inFlight.has(key)) return;
  inFlight.add(key);
  try {
    await action();
  } catch (err) {
    // The in-flight guard above already stops this client from causing the double-click kind of
    // "already running" (see `ALREADY_RUNNING_CODES`), but another path to the same session — the
    // sidebar row and the terminal's Resume button, for instance — can still race it.
    const isAlreadyRunning = err instanceof DaemonRequestError && ALREADY_RUNNING_CODES.includes(err.code);
    if (!isAlreadyRunning) throw err;
  } finally {
    inFlight.delete(key);
  }
}

/** Runs `work` with `sessionId` counted as resuming, for the terminal's loading state, until it
 * settles. A failure goes to `onError` first and is then passed on. */
async function whileResuming(
  setResumingIds: React.Dispatch<React.SetStateAction<ReadonlySet<string>>>,
  sessionId: string,
  work: () => Promise<unknown>,
  onError?: (err: unknown) => void,
): Promise<void> {
  setResumingIds((ids) => new Set(ids).add(sessionId));
  try {
    await work();
  } catch (err) {
    onError?.(err);
    throw err;
  } finally {
    setResumingIds((ids) => {
      const next = new Set(ids);
      next.delete(sessionId);
      return next;
    });
  }
}
