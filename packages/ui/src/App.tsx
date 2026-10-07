import React, { useEffect, useMemo, useRef, useState } from "react";

import { ArchiveView } from "./archive/ArchiveView";
import { ConnectionBanner } from "./components/ConnectionBanner";
import { PaneResizeHandle } from "./components/PaneResizeHandle";
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
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { ALREADY_RUNNING_CODES, type Console, type Project, type Session } from "./protocol";
import { ReportPanel } from "./report/ReportPanel";
import { SettingsDialog } from "./settings/SettingsDialog";
import { useSettingsDialog } from "./settings/useSettingsDialog";
import { useFocusShortcut } from "./sidebar/focusShortcut";
import { Sidebar } from "./sidebar/Sidebar";
import { useSidebarView } from "./sidebar/sidebarView";
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
  // One dialog at a time, oldest prompt first; answering or declining it brings up the next.
  const trustPrompt = useDaemonStore((s) => s.trustPrompts[0]);
  const [selectedSessionId, setSelectedSessionId] = useState<string>();
  const [dialogRequest, setDialogRequest] = useState<DialogRequest>();
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
  const sidebarView = useSidebarView(consoleList, projects);
  useGitStatusSchedule(sidebarView.currentConsole?.id);
  const archiveConsole = archiveScope ? consoles.get(archiveScope.console) : undefined;
  const archiveProject = archiveScope?.project ? projects.get(archiveScope.project) : undefined;
  // The archive view closes by itself once what it lists is gone.
  const archiveOpen = archiveConsole !== undefined && (archiveScope?.project === undefined || archiveProject !== undefined);

  const panes = usePaneToggles({ hasReportPanel, focusTerminal });

  // A region off screen is skipped by F6: a hidden docked pane, a closed drawer, and a floating pane
  // too, which is only a hover away rather than shown. The top bar is always there.
  const regionCycle = useRegionCycle({
    shown: {
      topbar: true,
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

  // The exit flow's frontend owner: it registers with the shell on mount, which is what makes the
  // window quittable. Only its confirmation is needed here: the main app has no separate "Quit"
  // control.
  const { exitConfirmOpen, closeExitConfirm, confirmExit } = useAppExit({
    getSessions: () => sessionList,
    requestShutdown: () => request({ type: "shutdown" }),
    toastError,
  });

  const { settingsOpen, openSettings, closeSettings } = useSettingsDialog({
    ready: hosts !== undefined,
    otherModalOpen: dialogRequest !== undefined || trustPrompt !== undefined || exitConfirmOpen,
    focusTerminal,
  });

  // Every click that could resume or create a session is guarded against its own double-click: two
  // fast clicks on the console session row otherwise create two console sessions (only one of which
  // the tree can ever show again, since console sessions belong to no project node), and two fast
  // clicks on a dormant session fire two `resume_session` calls.
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

  const selectSession = (session: Session) => {
    setSelectedSessionId(session.id);
    // Whatever led here — the sidebar, the archive, the waiting-count button — the sidebar follows
    // the session: its console is the one shown, and focus mode on another project is left.
    sidebarView.selectConsole(session.console_id);
    if (sidebarView.focusProject && sidebarView.focusProject.id !== session.project_id) sidebarView.focusProjectId(undefined);
    if (archiveOpen) {
      setArchiveScope(undefined);
      focusTerminal();
    }
    // An interrupted session is resumed by selecting it; an archived one only shows, and is
    // reopened by typing to it (`TerminalPane`) or an explicit Reopen (`reopenSession`).
    if (session.status === "interrupted") void resumeSession(session.id);
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

  // ⇧⌘F: into the selected session's project's focus mode, or back out of it. A console session
  // belongs to no project, so with one selected there is nothing to enter.
  useFocusShortcut(() => {
    if (sidebarView.focusProject) sidebarView.focusProjectId(undefined);
    else if (selectedSession?.project_id) {
      sidebarView.selectConsole(selectedSession.console_id);
      sidebarView.focusProjectId(selectedSession.project_id);
    }
  });

  const closeArchive = () => {
    setArchiveScope(undefined);
    focusTerminal();
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
        <div className="flex flex-1 items-center justify-center text-muted">
          {connectionState === "closed" ? t("app.daemonNotAnswering") : t("app.connecting")}
        </div>
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
        onOpenSettings={openSettings}
        sidebarWidth={panes.sidebarDocked ? sidebarWidth.width : undefined}
        sidebarShown={panes.sidebarShown}
        onToggleSidebar={panes.toggleSidebar}
        onSidebarToggleEnter={() => panes.sidebarPeek.reveal()}
        onSidebarToggleLeave={panes.sidebarPeek.leave}
        onNewConsole={() => openDialog({ kind: "new-console" })}
        selectedSession={selectedSession}
        viewTrail={
          archiveOpen && archiveConsole
            ? archiveProject
              ? [archiveConsole.name, archiveProject.name, t("archive.heading.project")]
              : [archiveConsole.name, t("archive.heading.consoleSessions")]
            : undefined
        }
        terminalProblem={terminalProblem}
        waitingCount={waitingSessions.length}
        onNextWaiting={selectNextWaiting}
        hasReportPanel={hasReportPanel}
        reportShown={panes.reportShown}
        onToggleReport={panes.toggleReport}
        onReportToggleEnter={() => panes.reportPeek.reveal()}
        onReportToggleLeave={panes.reportPeek.leave}
        focusTerminal={focusTerminal}
      />
      <div className="flex min-h-0 flex-1">
        {panes.sidebarOpen && <Scrim label={t("app.closeSessions")} onClose={panes.closeSidebar} />}
        <Sidebar
          consoles={consoleList}
          projects={projectList}
          sessions={sessionList}
          selectedSessionId={selectedSessionId}
          currentConsole={sidebarView.currentConsole}
          focusProject={sidebarView.focusProject}
          onSelectSession={selectSession}
          onOpenConsoleSession={openConsoleSession}
          onOpenDialog={openDialog}
          onSelectConsole={sidebarView.selectConsole}
          onFocusProject={sidebarView.focusProjectId}
          onOpenArchive={openArchive}
          onSetPinned={setPinned}
          open={panes.sidebarOpen}
          peek={panes.sidebarDocked ? undefined : panes.sidebarPeek}
          sidebarWidth={sidebarWidth}
        />
        {panes.sidebarDocked && <PaneResizeHandle side="sidebar" paneWidth={sidebarWidth} />}
        {/* Below the `docked` breakpoint the terminal is the row's only content and the floor
            drops to 382px (see the terminal's wrapper below); `overflow-x-auto` is what makes a
            viewport narrower than that scroll instead of clipping. */}
        <main className="relative flex min-w-0 flex-1 overflow-x-auto docked:overflow-visible">
          {/* Takes the terminal's place in the row, so the archive covers the terminal alone, not the
              report panel beside it; the terminal fills it. Above the `docked` breakpoint the 520px
              basis and floor are the report panel's counterpart: with a 0 basis free space stays
              positive at any window wider than the panel's own basis, flexbox never leaves the grow
              phase, and the panel's shrink factor is never consulted. 520px is 55 columns at
              ~9.2px/column off a real agent CLI (the terminal's padding eats the rest). Below the
              breakpoint the sidebar and the report panel are overlays rather than row siblings
              (`usePaneToggles`), so this is the row's only content and takes a much smaller floor:
              382px is 40 columns at the same ~9.2px/column plus the same padding allowance, under
              which the terminal stops being usable at all, so `overflow-x-auto` on `main` scrolls
              rather than squeezing it further. */}
          <div className="relative flex min-h-0 min-w-[382px] flex-[1_1_382px] docked:min-w-[520px] docked:flex-[1_1_520px]">
            {archiveOpen && archiveConsole && (
              <ArchiveView
                key={`${archiveScope?.console}:${archiveScope?.project ?? ""}`}
                console={archiveConsole}
                project={archiveProject}
                sessions={sessionList.filter((s) =>
                  archiveProject ? s.project_id === archiveProject.id : s.console_id === archiveConsole.id && s.role === "console",
                )}
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
          {/* Only the console session's console has a report panel — it is that console's panel,
              not the session's. Keyed on the console id so switching console sessions mounts a
              fresh instance. */}
          {/* The `selectedSession &&` is only for narrowing: `hasReportPanel` already implies it. */}
          {hasReportPanel && selectedSession && (
            <ReportPanel
              key={selectedSession.console_id}
              consoleId={selectedSession.console_id}
              consoleSessionId={selectedSession.id}
              open={panes.reportOpen}
              reportWidth={reportWidth.width}
              peek={panes.reportDocked ? undefined : panes.reportPeek}
              onEscape={panes.dismissOverlays}
              onCycleRegion={regionCycle.cycle}
            />
          )}
        </main>
        {hasReportPanel && panes.reportDocked && <PaneResizeHandle side="report" paneWidth={reportWidth} />}
        {panes.reportOpen && <Scrim label={t("app.closeReport")} onClose={panes.closeReport} />}
      </div>
      <ConnectionBanner state={connectionState} onRetry={reconnect} focusTerminal={focusTerminal} />
      {settingsOpen && <SettingsDialog onClose={closeSettings} />}
      {dialogRequest && (
        <RequestedDialog
          dialog={dialogRequest}
          onClose={closeDialog}
          onSessionOpened={showOpenedSession}
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
