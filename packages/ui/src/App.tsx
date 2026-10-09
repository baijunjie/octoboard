import React, { useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";

import { ArchiveView } from "./archive/ArchiveView";
import { ConnectionBanner } from "./components/ConnectionBanner";
import { ContentPanel } from "./components/ContentPanel";
import { PaneResizeHandle } from "./components/PaneResizeHandle";
import { Rail } from "./components/Rail";
import { Scrim } from "./components/Scrim";
import { BareTitleBar, TitleBar } from "./components/TitleBar";
import { Toasts } from "./components/Toasts";
import { DaemonRequestError } from "./daemon-client";
import { ConfirmDialog } from "./dialogs/ConfirmDialog";
import type { DialogRequest } from "./dialogs/dialogRequest";
import { RequestedDialog } from "./dialogs/RequestedDialog";
import { TrustPromptDialog } from "./dialogs/TrustPromptDialog";
import { useT } from "./i18n/react";
import { usePaneWidth } from "./layout/paneWidth";
import { usePaneToggles } from "./layout/usePaneToggles";
import { useRegionCycle } from "./layout/useRegionCycle";
import { useAppExit } from "./lifecycle/useAppExit";
import { useGitStatusSchedule } from "./lifecycle/useGitStatusSchedule";
import { useStatusItemMenu } from "./lifecycle/useStatusItemMenu";
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { isLive, type Location } from "./navigation/history";
import { useNavigationHistory } from "./navigation/useNavigationHistory";
import { ALREADY_RUNNING_CODES, type Console, type Project, type Session } from "./protocol";
import { ReportPanel } from "./report/ReportPanel";
import { SettingsDialog } from "./settings/SettingsDialog";
import { useSettingsDialog } from "./settings/useSettingsDialog";
import { belongsToFocus, focusAfterSelect, focusFor, focusKey, KEY_SWITCH_SELECTION, resolveFocus, shortcutOutcome } from "./sidebar/focus";
import { useFocusShortcut } from "./sidebar/focusShortcut";
import { switchStrip } from "./sidebar/order";
import { Sidebar } from "./sidebar/Sidebar";
import { useSidebarView } from "./sidebar/sidebarView";
import { useSwitchShortcut } from "./sidebar/switchShortcut";
import type { ArchiveScope } from "./sidebar/types";
import { useDaemon, useDaemonStore } from "./store";
import { TerminalPane, type TerminalPaneHandle, type TerminalProblem } from "./terminal/TerminalPane";
import { nextWaitingSession, waitingSessionsInTreeOrder } from "./waiting";

export function App(): React.ReactElement {
  const t = useT();
  const { request, toastError, reconnect } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const hosts = useDaemonStore((s) => s.hosts);
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const sessions = useDaemonStore((s) => s.sessions);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  // One dialog at a time, oldest prompt first; answering or declining it brings up the next.
  const trustPrompt = useDaemonStore((s) => s.trustPrompts[0]);
  const [selectedSessionId, setSelectedSessionId] = useState<string>();
  const [dialogRequest, setDialogRequest] = useState<DialogRequest>();
  const dialogRequestRef = useRef(dialogRequest);
  dialogRequestRef.current = dialogRequest;
  const [terminalProblem, setTerminalProblem] = useState<TerminalProblem>();
  const [archiveScope, setArchiveScope] = useState<ArchiveScope>();
  // Sessions whose `resume_session` is in flight, for the terminal's loading state.
  const [resumingIds, setResumingIds] = useState<ReadonlySet<string>>(new Set());

  const terminalRef = useRef<TerminalPaneHandle>(null);
  const focusTerminal = () => terminalRef.current?.focus();

  const consoleList = useMemo(() => Array.from(consoles.values()), [consoles]);
  const projectList = useMemo(() => Array.from(projects.values()), [projects]);
  const sessionList = useMemo(() => Array.from(sessions.values()), [sessions]);
  const selectedSession = selectedSessionId ? sessions.get(selectedSessionId) : undefined;
  // Only a console session has a report panel at all; computed here (rather than where it is
  // consumed below) because the pane toggles and the panes' width clamps need it too.
  const hasReportPanel = selectedSession?.role === "console";
  const sidebarView = useSidebarView(consoleList, projects, sessions);
  useGitStatusSchedule(sidebarView.currentConsole?.id);
  const archiveConsole = archiveScope ? consoles.get(archiveScope.console) : undefined;
  const archiveProjectId = archiveScope && "project" in archiveScope ? archiveScope.project : undefined;
  const archiveProject = archiveProjectId ? projects.get(archiveProjectId) : undefined;
  const archiveConsoleSessionId = archiveScope && "consoleSession" in archiveScope ? archiveScope.consoleSession : undefined;
  const archiveBoundTo = archiveConsoleSessionId ? sessions.get(archiveConsoleSessionId) : undefined;
  // The archive view closes by itself once what it lists is gone.
  const archiveOpen =
    archiveConsole !== undefined &&
    (archiveProjectId === undefined || archiveProject !== undefined) &&
    (archiveConsoleSessionId === undefined || archiveBoundTo !== undefined);

  const panes = usePaneToggles({ hasReportPanel, focusTerminal });
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

  // A region off screen is skipped by F6: a hidden docked pane, a closed drawer, and a floating pane
  // too, which is only a hover away rather than shown. The top bar and the rail are always there.
  const regionCycle = useRegionCycle({
    shown: {
      topbar: true,
      rail: true,
      sidebar: panes.sidebarShown,
      archive: archiveOpen,
      terminal: selectedSession !== undefined && !archiveOpen,
      report: hasReportPanel && panes.reportShown,
      banner: connectionState === "reconnecting" || connectionState === "closed",
    },
    focusTerminal,
  });

  // A hidden report panel gives its width back, and a hidden pane takes none of the row.
  const dockedPanes = { sidebar: panes.sidebarDocked, report: hasReportPanel && panes.reportDocked };
  const sidebarWidth = usePaneWidth("sidebar", dockedPanes);
  const reportWidth = usePaneWidth("report", dockedPanes);

  useWaitingNotifications(sessionList, consoles, projects, hosts !== undefined);
  useStatusItemMenu({
    consoles: consoleList,
    projects: projectList,
    sessions: sessionList,
    consoleMap: consoles,
    projectMap: projects,
    ready: hosts !== undefined,
    onSessionChosen: (session) => {
      selectSession(session);
      focusTerminal();
    },
  });

  // The exit flow's frontend owner: it registers with the shell on mount, which is what makes the
  // window quittable. Only its confirmation is needed here: the main app has no separate "Quit"
  // control.
  const { exitConfirmOpen, closeExitConfirm, confirmExit } = useAppExit({
    getSessions: () => sessionList,
    requestShutdown: () => request({ type: "shutdown" }),
    toastError,
  });

  const { settingsOpen, settingsSection, openSettings, openSettingsAt, closeSettings } = useSettingsDialog({
    ready: hosts !== undefined,
    otherModalOpen: dialogRequest !== undefined || trustPrompt !== undefined || exitConfirmOpen,
    focusTerminal,
  });

  // Every click that could resume or create a session is guarded against its own double-click: two
  // fast clicks on the console sessions section's "New console session" button otherwise create two
  // console sessions, and two fast clicks on a dormant session fire two `resume_session` calls.
  const inFlightRef = useRef<Set<string>>(new Set());

  /** Runs `action` unless one with the same `key` is in flight; says whether it went through. A
   * second call while one is in flight counts as having gone through, since what it asks is under
   * way, and so does an "already running" refusal that is not shown. */
  const runOnce = async (key: string, action: () => Promise<void>, suppressAlreadyRunning = true): Promise<boolean> => {
    if (inFlightRef.current.has(key)) return true;
    inFlightRef.current.add(key);
    try {
      await action();
      return true;
    } catch (err) {
      // The in-flight guard above already stops this client from causing the double-click kind of
      // "already running" (see `ALREADY_RUNNING_CODES`), but another path to the same session — the
      // sidebar row and the terminal's Resume button, for instance — can still race it;
      // `suppressAlreadyRunning` decides per call whether that race is worth showing.
      const isAlreadyRunning = err instanceof DaemonRequestError && ALREADY_RUNNING_CODES.includes(err.code);
      if (!isAlreadyRunning || !suppressAlreadyRunning) {
        toastError((err as Error).message);
        return false;
      }
      return true;
    } finally {
      inFlightRef.current.delete(key);
    }
  };

  /** Resumes a session; resolves to whether the daemon accepted it, so input held for a session
   * that could not start is not replayed later (`TerminalController.armWake`). A console may now
   * hold any number of live console sessions, so there is nothing to pre-emptively refuse here —
   * selecting, resuming or reopening one simply works, whatever else is running in the console. */
  const resumeSession = async (sessionId: string): Promise<boolean> =>
    runOnce(`resume:${sessionId}`, async () => {
      setResumingIds((ids) => new Set(ids).add(sessionId));
      try {
        await request({ type: "resume_session", session: sessionId });
      } finally {
        setResumingIds((ids) => {
          const next = new Set(ids);
          next.delete(sessionId);
          return next;
        });
      }
    });

  /** Switches a session's account. The session counts as resuming while it runs, so its terminal
   * says so instead of offering a Resume that would race the switch. Rejects with the daemon's
   * reason, which the confirmation shows. */
  const switchAccount = async (sessionId: string, account: string | null): Promise<void> => {
    setResumingIds((ids) => new Set(ids).add(sessionId));
    try {
      await request({ type: "switch_session_account", session: sessionId, account });
    } catch (err) {
      // The confirmation shows a failure in place and cannot be dismissed while the switch runs;
      // were it gone anyway, the reason must not be lost with it.
      if (dialogRequestRef.current?.kind !== "switch-account") toastError((err as Error).message);
      throw err;
    } finally {
      setResumingIds((ids) => {
        const next = new Set(ids);
        next.delete(sessionId);
        return next;
      });
    }
  };

  const openConsoleSession = (console_: Console) =>
    void runOnce(`console-session:${console_.id}`, async () => {
      const reply = await request({ type: "open_session", console_id: console_.id });
      if (reply.type === "session_opened") showOpenedSession(reply.session.id);
    });

  /** Puts a session this window just opened on screen; it is always in the console and project the
   * sidebar already shows, so only the archive view, were it open, has to give way. */
  const showOpenedSession = (sessionId: string) => {
    setSelectedSessionId(sessionId);
    setArchiveScope(undefined);
  };

  // `create_console` is answered with a bare ack, so the console this window just created is
  // recognised as the one that was not there when its dialog opened, and the sidebar switches to it.
  const consolesBeforeNew = useRef<Set<string> | undefined>(undefined);
  const openDialog = (dialog: DialogRequest) => {
    if (dialog.kind === "new-console") consolesBeforeNew.current = new Set(consoles.keys());
    setDialogRequest(dialog);
  };
  useEffect(() => {
    const before = consolesBeforeNew.current;
    if (!before) return;
    const created = consoleList.find((c) => !before.has(c.id));
    if (created) {
      consolesBeforeNew.current = undefined;
      sidebarView.selectConsole(created.id);
    }
  }, [consoleList]);
  const closeDialog = () => {
    setDialogRequest(undefined);
    // The new console's broadcast can trail the ack that closed the dialog by a moment.
    if (consolesBeforeNew.current) setTimeout(() => (consolesBeforeNew.current = undefined), 2000);
  };

  const selectSession = (session: Session, { enterFocus = false, resume = true } = {}) => {
    setSelectedSessionId(session.id);
    // Whatever led here — the sidebar, the archive, the waiting-count button — the sidebar follows
    // the session: its console is the one shown, and focus mode on something the session does not
    // belong to is left, unless `enterFocus` asks for the focus mode of this console session itself.
    sidebarView.selectConsole(session.console_id);
    const nextFocus = focusAfterSelect(sidebarView.focus, session, enterFocus, sessions);
    if (nextFocus !== sidebarView.focus) sidebarView.setFocus(nextFocus);
    if (archiveOpen) {
      setArchiveScope(undefined);
      focusTerminal();
    }
    // An interrupted session is resumed by selecting it, unless `resume` is off; an archived one only
    // shows, and is reopened by typing to it (`TerminalPane`) or an explicit Reopen (`reopenSession`).
    if (resume && session.status === "interrupted") void resumeSession(session.id);
  };

  const reopenSession = (session: Session) => {
    selectSession(session);
    if (session.status === "archived") void resumeSession(session.id);
  };

  const openArchive = (scope: ArchiveScope) => {
    setArchiveScope(scope);
    // Below the `docked` breakpoint the sidebar is a drawer over the terminal, and would keep
    // covering the archive that was just asked for.
    if (panes.sidebarOpen) panes.closeSidebar();
  };

  // The selected session can stop belonging to the focus mode without being selected again: an
  // archived session of a project, reopened from the archive view, becomes live and bound to a
  // console session, and is then not listed there.
  useEffect(() => {
    if (sidebarView.focus && selectedSession && !belongsToFocus(sidebarView.focus, selectedSession, sessions)) {
      sidebarView.setFocus(undefined);
    }
  }, [selectedSession?.id, selectedSession?.status, selectedSession?.bound_to]);

  // ⇧⌘F: into the focus mode of the selected session's context, or back out of focus mode.
  useFocusShortcut(() => {
    const outcome = shortcutOutcome(sidebarView.focus, selectedSession, projects);
    if (!outcome) return;
    if (selectedSession && outcome.focus) sidebarView.selectConsole(selectedSession.console_id);
    sidebarView.setFocus(outcome.focus);
  });

  // The console sessions a console session's focus mode offers to switch between (its switch strip),
  // for ⌃Tab.
  const switchableConsoleSessions = useMemo(
    () => (sidebarView.currentConsole ? switchStrip(sessionList, sidebarView.currentConsole.id).map((e) => e.consoleSession) : []),
    [sessionList, sidebarView.currentConsole?.id],
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
    isLive: (target) => isLive(target, { consoles, projects, sessions }),
    // Sets the state straight, not through `selectSession`: going back to an interrupted session
    // must not resume it. A focus mode that does not show the session is left, as selecting does.
    apply: (target) => {
      if (target.console) sidebarView.selectConsole(target.console);
      const session = target.session ? sessions.get(target.session) : undefined;
      sidebarView.setFocus(focusFor(resolveFocus(target.focus, target.console, projects, sessions), session, sessions));
      setSelectedSessionId(target.session);
      setArchiveScope(target.archive);
    },
    focusTerminal,
  });

  // A press in the previewing sidebar makes its console current. If that press goes on to select a
  // session there, the two changes are one visit in the history.
  const commitPreview = () => {
    if (!previewing || !shownConsole) return;
    navigation.joinPressVisits();
    sidebarView.selectConsole(shownConsole.id);
  };

  const setPinned = (target: { project: Project } | { session: Session }, pinned: boolean) => {
    const body =
      "project" in target
        ? ({ type: "update_project", project: target.project.id, pinned } as const)
        : ({ type: "set_session_pinned", session: target.session.id, pinned } as const);
    request(body).catch((err: unknown) => toastError((err as Error).message));
  };

  const waitingSessions = useMemo(
    () => waitingSessionsInTreeOrder(consoleList, projectList, sessionList),
    [consoleList, projectList, sessionList],
  );
  const selectNextWaiting = () => {
    const next = nextWaitingSession(waitingSessions, selectedSessionId);
    if (next) selectSession(next);
  };

  // Nothing to show until the first snapshot. The toasts belong here all the same, and so does the
  // banner with the Retry the client offers once it has given up — otherwise this screen would say
  // it is connecting forever with nothing the user can do about it.
  if (!hosts) {
    return (
      <div className="flex h-full flex-col">
        <BareTitleBar />
        <Toasts focusTerminal={focusTerminal} />
        <ContentPanel bare>
          <div className="flex flex-1 items-center justify-center text-muted">
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
        sidebarWidth={panes.sidebarDocked ? sidebarWidth.width : undefined}
        sidebarShown={panes.sidebarShown}
        onToggleSidebar={panes.toggleSidebar}
        canGoBack={navigation.canGoBack}
        canGoForward={navigation.canGoForward}
        onBack={navigation.back}
        onForward={navigation.forward}
        selectedSession={selectedSession}
        viewTrail={
          archiveOpen && archiveConsole
            ? archiveProject
              ? [archiveConsole.name, archiveProject.name, t("archive.heading.project")]
              : archiveBoundTo
                ? [archiveConsole.name, archiveBoundTo.title, t("archive.heading.boundSessions")]
                : [archiveConsole.name, t("archive.heading.consoleSessions")]
            : undefined
        }
      />
      <div className="flex min-h-0 flex-1">
        <Rail
          consoles={consoleList}
          sessions={sessionList}
          currentConsoleId={sidebarView.currentConsole?.id}
          onSelectConsole={sidebarView.selectConsole}
          onConsoleEnter={panes.sidebarPeek.hoverConsole}
          onConsoleLeave={panes.sidebarPeek.leaveConsole}
          hoverOpensSidebar={panes.sidebarPeek.peekable}
          onOpenDialog={openDialog}
          waitingCount={waitingSessions.length}
          onNextWaiting={selectNextWaiting}
          terminalProblem={terminalProblem}
          hasReportPanel={hasReportPanel}
          reportShown={panes.reportShown}
          onToggleReport={panes.toggleReport}
          onReportToggleEnter={() => panes.reportPeek.reveal()}
          onReportToggleLeave={panes.reportPeek.leave}
          onOpenSettings={openSettings}
          focusTerminal={focusTerminal}
        />
        <ContentPanel>
          {panes.sidebarOpen && <Scrim label={t("app.closeSessions")} onClose={panes.closeSidebar} />}
          <Sidebar
            projects={projectList}
            sessions={sessionList}
            selectedSessionId={selectedSessionId}
            shownConsole={shownConsole}
            focus={previewing ? undefined : sidebarView.focus}
            onSelectSession={(session) => selectSession(session)}
            onSwitchConsoleSession={switchConsoleSession}
            onOpenConsoleSession={openConsoleSession}
            onOpenDialog={openDialog}
            onFocus={sidebarView.setFocus}
            onOpenArchive={openArchive}
            onSetPinned={setPinned}
            onOpenSettings={openSettingsAt}
            open={panes.sidebarOpen}
            peek={panes.sidebarDocked ? undefined : panes.sidebarPeek}
            onPeekPress={commitPreview}
            sidebarWidth={sidebarWidth}
          />
          {panes.sidebarDocked && <PaneResizeHandle side="sidebar" paneWidth={sidebarWidth} />}
          {/* Below the `docked` breakpoint the terminal is the row's only content and the floor
              drops to 398px (see the terminal's wrapper below); `overflow-x-auto` is what makes a
              viewport narrower than that scroll instead of clipping. */}
          <main className="relative flex min-w-0 flex-1 overflow-x-auto docked:overflow-visible">
            {/* Takes the terminal's place in the row, so the archive covers the terminal alone, not the
                report panel beside it; the terminal fills it. Above the `docked` breakpoint the 520px
                basis and floor are the report panel's counterpart: with a 0 basis free space stays
                positive at any window wider than the panel's own basis, flexbox never leaves the grow
                phase, and the panel's shrink factor is never consulted. 520px is about 53 columns
                at ~9.2px/column off a real agent CLI (the terminal's padding eats the rest). Below the
                breakpoint the sidebar and the report panel are overlays rather than row siblings
                (`usePaneToggles`), so this is the row's only content and takes a much smaller floor:
                398px is 40 columns at the same ~9.2px/column plus the same padding allowance, under
                which the terminal stops being usable at all, so `overflow-x-auto` on `main` scrolls
                rather than squeezing it further. */}
            <div className="relative flex min-h-0 min-w-[398px] flex-[1_1_398px] docked:min-w-[520px] docked:flex-[1_1_520px]">
              {archiveOpen && archiveConsole && (
                <ArchiveView
                  key={`${archiveScope?.console}:${archiveProjectId ?? ""}:${archiveConsoleSessionId ?? ""}`}
                  console={archiveConsole}
                  project={archiveProject}
                  boundTo={archiveBoundTo}
                  sessions={
                    archiveProject
                      ? sessionList.filter((s) => s.project_id === archiveProject.id)
                      : archiveBoundTo
                        ? sessionList.filter((s) => s.console_id === archiveConsole.id)
                        : sessionList.filter((s) => s.console_id === archiveConsole.id && s.role === "console")
                  }
                  accounts={accounts}
                  onReopen={reopenSession}
                  onOpenDialog={openDialog}
                  dialogOpen={dialogRequest !== undefined}
                  onClose={closeArchive}
                />
              )}
              <TerminalPane
                ref={terminalRef}
                session={selectedSession}
                onResume={resumeSession}
                resuming={selectedSessionId !== undefined && resumingIds.has(selectedSessionId)}
                onProblemChange={setTerminalProblem}
              />
            </div>
            {/* Only a console session has a report panel, and it is that console session's own.
                Keyed on its id so switching console sessions mounts a fresh instance, which does
                not carry one's position over to the other. */}
            {/* The `selectedSession &&` is only for narrowing: `hasReportPanel` already implies it. */}
            {hasReportPanel && selectedSession && (
              <ReportPanel
                key={selectedSession.id}
                consoleSessionId={selectedSession.id}
                open={panes.reportOpen}
                reportWidth={reportWidth.width}
                peek={panes.reportDocked ? undefined : panes.reportPeek}
                onEscape={panes.dismissOverlays}
                onCycleRegion={regionCycle.cycle}
                onMoveHistory={navigation.moveByShortcut}
                onMoveConsoleSession={moveConsoleSession}
              />
            )}
          </main>
          {hasReportPanel && panes.reportDocked && <PaneResizeHandle side="report" paneWidth={reportWidth} />}
          {panes.reportOpen && <Scrim label={t("app.closeReport")} onClose={panes.closeReport} />}
        </ContentPanel>
      </div>
      <ConnectionBanner state={connectionState} onRetry={reconnect} focusTerminal={focusTerminal} />
      {settingsOpen && <SettingsDialog initialSection={settingsSection} onClose={closeSettings} />}
      {dialogRequest && (
        <RequestedDialog
          dialog={dialogRequest}
          onClose={closeDialog}
          onSessionOpened={showOpenedSession}
          onSwitchAccount={switchAccount}
        />
      )}
      {trustPrompt && (
        <TrustPromptDialog prompt={trustPrompt} sessionTitle={sessions.get(trustPrompt.session)?.title} />
      )}
      {exitConfirmOpen && (
        <ConfirmDialog
          title={t("app.quit.title")}
          message={t("app.quit.message")}
          confirmLabel={t("common.quit")}
          destructive
          onCancel={closeExitConfirm}
          onConfirm={confirmExit}
        />
      )}
    </div>
  );
}
