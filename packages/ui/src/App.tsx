import React, { useMemo, useRef, useState } from "react";

import { ConnectionBanner } from "./components/ConnectionBanner";
import { PaneResizeHandle } from "./components/PaneResizeHandle";
import { Scrim } from "./components/Scrim";
import { Sidebar } from "./components/Sidebar";
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
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { ALREADY_RUNNING_CODES, isDormant, isLive, type Console, type Session } from "./protocol";
import { ReportPanel } from "./report/ReportPanel";
import { SettingsDialog } from "./settings/SettingsDialog";
import { useSettingsDialog } from "./settings/useSettingsDialog";
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

  const terminalRef = useRef<TerminalPaneHandle>(null);
  const focusTerminal = () => terminalRef.current?.focus();

  const consoleList = useMemo(() => Array.from(consoles.values()), [consoles]);
  const projectList = useMemo(() => Array.from(projects.values()), [projects]);
  const sessionList = useMemo(() => Array.from(sessions.values()), [sessions]);
  const selectedSession = selectedSessionId ? sessions.get(selectedSessionId) : undefined;
  // Only a hub session has a report panel at all; computed here (rather than where it is
  // consumed below) because the pane toggles and the panes' width clamps need it too.
  const hasReportPanel = selectedSession?.role === "hub";

  const panes = usePaneToggles({ hasReportPanel, focusTerminal });

  // A region off screen is skipped by F6: a hidden docked pane, a closed drawer, and a floating pane
  // too, which is only a hover away rather than shown. The top bar is always there.
  const regionCycle = useRegionCycle({
    shown: {
      topbar: true,
      sidebar: panes.sidebarShown,
      terminal: selectedSession !== undefined,
      report: hasReportPanel && panes.reportShown,
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
  // fast clicks on the Hub row otherwise create two hub sessions (only one of which the tree can
  // ever show again, since hub sessions belong to no project node), and two fast clicks on a
  // dormant session fire two `resume_session` calls.
  const inFlightRef = useRef<Set<string>>(new Set());

  const runOnce = async (key: string, action: () => Promise<void>, suppressAlreadyRunning = true) => {
    if (inFlightRef.current.has(key)) return;
    inFlightRef.current.add(key);
    try {
      await action();
    } catch (err) {
      // The in-flight guard above already stops this client from causing the double-click kind of
      // "already running" (see `ALREADY_RUNNING_CODES`), but another path to the same session — the
      // sidebar row and its own "Resume" action-menu item, for instance — can still race it;
      // `suppressAlreadyRunning` decides per call whether that race is worth showing.
      const isAlreadyRunning = err instanceof DaemonRequestError && ALREADY_RUNNING_CODES.includes(err.code);
      if (!isAlreadyRunning || !suppressAlreadyRunning) {
        toastError((err as Error).message);
      }
    } finally {
      inFlightRef.current.delete(key);
    }
  };

  const resumeSession = (sessionId: string) => {
    const session = sessions.get(sessionId);
    // The daemon refuses to resume a hub while its console has another live one
    // (`hub_reopen_blocked`), so check first and say what the user has to do instead of waiting for
    // the refusal; the session itself stays selected, showing its last output.
    const liveHubExists =
      session?.role === "hub" &&
      sessionList.some(
        (other) =>
          other.console_id === session.console_id &&
          other.role === "hub" &&
          other.id !== session.id &&
          isLive(other.status),
      );
    if (liveHubExists) {
      toastError(t("app.hubAlreadyLive"), sessionId);
      return;
    }
    // `hub_reopen_blocked` (one of `ALREADY_RUNNING_CODES`) also reaches here should a race get past
    // the check above — unlike the plain double-click this guard exists for, suppressing it would
    // make the click look like it did nothing, so a hub resume lets the error through instead.
    const suppressAlreadyRunning = session?.role !== "hub";
    void runOnce(
      `resume:${sessionId}`,
      () => request({ type: "resume_session", session: sessionId }).then(() => {}),
      suppressAlreadyRunning,
    );
  };

  const openHub = (console_: Console) =>
    void runOnce(`hub:${console_.id}`, async () => {
      const reply = await request({ type: "open_session", console_id: console_.id });
      if (reply.type === "session_opened") setSelectedSessionId(reply.session.id);
    });

  const selectSession = (session: Session) => {
    setSelectedSessionId(session.id);
    if (isDormant(session.status)) resumeSession(session.id);
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
        {connectionState === "closed" && <ConnectionBanner state={connectionState} onRetry={reconnect} />}
        <div className="flex flex-1 items-center justify-center text-muted">
          {connectionState === "closed" ? t("app.daemonNotAnswering") : t("app.connecting")}
        </div>
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
        onNewConsole={() => setDialogRequest({ kind: "new-console" })}
        selectedSession={selectedSession}
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
      <ConnectionBanner state={connectionState} onRetry={reconnect} />
      <div className="flex min-h-0 flex-1">
        {panes.sidebarOpen && <Scrim label={t("app.closeSessions")} onClose={panes.closeSidebar} />}
        <Sidebar
          consoles={consoleList}
          projects={projectList}
          sessions={sessionList}
          selectedSessionId={selectedSessionId}
          onSelectSession={selectSession}
          onOpenHub={openHub}
          onOpenDialog={setDialogRequest}
          open={panes.sidebarOpen}
          peek={panes.sidebarDocked ? undefined : panes.sidebarPeek}
          sidebarWidth={sidebarWidth}
        />
        {panes.sidebarDocked && <PaneResizeHandle side="sidebar" paneWidth={sidebarWidth} />}
        {/* Below the `docked` breakpoint the terminal is the row's only content and the floor
            drops to 382px (see `TerminalPane.tsx`); `overflow-x-auto` is what makes a viewport
            narrower than that scroll instead of clipping. */}
        <main className="flex min-w-0 flex-1 overflow-x-auto docked:overflow-visible">
          <TerminalPane
            ref={terminalRef}
            session={selectedSession}
            onResume={resumeSession}
            onProblemChange={setTerminalProblem}
          />
          {/* Only the hub session's console has a report panel — it is that console's panel, not
              the session's. Keyed on the console id so switching hubs mounts a fresh instance. */}
          {/* The `selectedSession &&` is only for narrowing: `hasReportPanel` already implies it. */}
          {hasReportPanel && selectedSession && (
            <ReportPanel
              key={selectedSession.console_id}
              consoleId={selectedSession.console_id}
              hubSessionId={selectedSession.id}
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
      {settingsOpen && <SettingsDialog onClose={closeSettings} />}
      {dialogRequest && (
        <RequestedDialog
          dialog={dialogRequest}
          onClose={() => setDialogRequest(undefined)}
          onSessionOpened={setSelectedSessionId}
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
