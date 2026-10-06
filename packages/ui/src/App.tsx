import React, { useEffect, useMemo, useRef, useState } from "react";

import { ConnectionBanner } from "./components/ConnectionBanner";
import { Scrim } from "./components/Scrim";
import { Sidebar } from "./components/Sidebar";
import { Toasts } from "./components/Toasts";
import { DaemonRequestError } from "./daemon-client";
import { ConfirmDialog } from "./dialogs/ConfirmDialog";
import type { DialogRequest } from "./dialogs/dialogRequest";
import { RequestedDialog } from "./dialogs/RequestedDialog";
import { TrustPromptDialog } from "./dialogs/TrustPromptDialog";
import { useIsNarrow } from "./layout";
import { useAppExit } from "./lifecycle/useAppExit";
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { isDormant, type Console, type Session } from "./protocol";
import { ReportPanel } from "./report/ReportPanel";
import { useDaemon, useDaemonStore } from "./store";
import { TerminalPane } from "./terminal/TerminalPane";

/** The code the daemon's `error` carries for a launch asked for while one was already running or
 * starting for that session, or for resuming an archived hub while a live one already exists (see
 * "Daemon to client" in `daemon/PROTOCOL.md`). The in-flight guard below already stops this client
 * from causing the double-click kind, but another path to the same session — the sidebar row and
 * its own "Resume" action-menu item, for instance — can still race it; `runOnce` decides per call
 * whether that race is worth showing. */
const SESSION_ALREADY_RUNNING = "session_already_running";

export function App(): React.ReactElement {
  const { request, toastError, reconnect } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const hosts = useDaemonStore((s) => s.hosts);
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const sessions = useDaemonStore((s) => s.sessions);
  const trustedDirectories = useDaemonStore((s) => s.trustedDirectories);
  // One dialog at a time, oldest prompt first; answering or declining it brings up the next.
  const trustPrompt = useDaemonStore((s) => s.trustPrompts[0]);
  const [selectedSessionId, setSelectedSessionId] = useState<string>();
  const [dialogRequest, setDialogRequest] = useState<DialogRequest>();

  // Below the `docked` breakpoint the sidebar and the report panel are closed-by-default overlays
  // rather than row siblings (their own components' doc comments have the layout). Both states
  // live here, one level above either component, since the terminal pane's header is where their
  // toggles sit and widening the window past the breakpoint has to be able to close either one.
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [reportOpen, setReportOpen] = useState(false);
  const isNarrow = useIsNarrow();

  const consoleList = useMemo(() => Array.from(consoles.values()), [consoles]);
  const projectList = useMemo(() => Array.from(projects.values()), [projects]);
  const sessionList = useMemo(() => Array.from(sessions.values()), [sessions]);
  const selectedSession = selectedSessionId ? sessions.get(selectedSessionId) : undefined;
  // Only a hub session has a report panel at all; computed here (rather than where it is
  // consumed below) because the effect that resets `reportOpen` needs it too.
  const hasReportPanel = selectedSession?.role === "hub";

  // An overlay left open stops meaning anything once the window is wide enough to show its
  // content in the row instead — without this, widening past the breakpoint with a drawer open
  // would leave `docked:hidden`'s sibling, the scrim, also gone (it is hidden the same way), but
  // the drawer's own `open` state would still be true, so reopening it narrow again would show it
  // already open with no toggle press in between.
  useEffect(() => {
    if (isNarrow) return;
    setSidebarOpen(false);
    setReportOpen(false);
  }, [isNarrow]);

  // Selecting a session whose console has no report panel leaves `reportOpen` with nothing to
  // mean: the panel stops rendering (it only exists for a hub session), but a scrim rendered on
  // `reportOpen` alone would still dim the whole viewport with no toggle left to close it.
  useEffect(() => {
    if (!hasReportPanel) setReportOpen(false);
  }, [hasReportPanel]);

  // Opening either drawer closes the other — without this, both scrims can be on screen at once,
  // and since they stack in DOM order, a press only ever reaches the later one.
  const toggleSidebar = () => {
    setSidebarOpen((open) => !open);
    if (!sidebarOpen) setReportOpen(false);
  };
  const toggleReport = () => {
    setReportOpen((open) => !open);
    if (!reportOpen) setSidebarOpen(false);
  };

  // Closes whichever drawer is open — both scrims already do this on a press, Escape needs its
  // own listener to do the same. Registered on the capture phase: xterm.js owns Escape too (it
  // forwards the keystroke to the running agent) and stops it bubbling once it has, so a bubble
  // listener here would never see the key while the terminal holds focus, which is most of the
  // time.
  //
  // Scoped to origins marked `data-escape-scope` (the terminal pane and the drawers themselves)
  // rather than every keydown: a dialog or a menu popover is portalled to `<body>`, outside both,
  // and closes itself from its own bubble-phase `onKeyDown` — stopping propagation unconditionally
  // here reached the key first and ate it before react-aria's own handler ever saw it.
  //
  // A target of `<body>` (or no target at all) is treated as in scope too: that is what a fresh
  // narrow-mode window has focused once a drawer is open but no session has ever been selected, so
  // `term.focus()` has never run. react-aria keeps focus inside an open dialog or menu, so an
  // overlay's Escape normally cannot arrive that way. The one exception is a dialog whose focused
  // control has just unmounted, which drops focus to `<body>`: Escape then closes the drawer under
  // the dialog rather than the dialog. That state already costs the dialog its own Escape, which is
  // what `useRefocusIfLost` in `dialogs/Dialog.tsx` exists to prevent, so it is not worth a second
  // guard here.
  useEffect(() => {
    if (!sidebarOpen && !reportOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      const target = event.target as Element | null;
      if (target && target !== document.body && !target.closest("[data-escape-scope]")) return;
      // Consumed here rather than also reaching the terminal: with a drawer open, Escape closes
      // it, not whatever the running agent would have done with it.
      event.stopPropagation();
      setSidebarOpen(false);
      setReportOpen(false);
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [sidebarOpen, reportOpen]);

  useWaitingNotifications(sessionList, consoles, projects, hosts !== undefined);

  // The exit flow's frontend owner: it registers with the shell on mount, which is what makes the
  // window quittable. Only its confirmation is needed here: the main app has no separate "Quit"
  // control.
  const { exitConfirmOpen, closeExitConfirm, confirmExit } = useAppExit({
    getSessions: () => sessionList,
    requestShutdown: () => request({ type: "shutdown" }),
    toastError,
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
      const isAlreadyRunning = err instanceof DaemonRequestError && err.code === SESSION_ALREADY_RUNNING;
      if (!isAlreadyRunning || !suppressAlreadyRunning) {
        toastError((err as Error).message);
      }
    } finally {
      inFlightRef.current.delete(key);
    }
  };

  const resumeSession = (sessionId: string) => {
    // The daemon also answers `session_already_running` for resuming an archived hub while a live
    // one already exists (it refuses a second live hub outright) — unlike the plain double-click
    // this guard exists for, suppressing that one would make the click look like it did nothing, so
    // a hub resume lets the error through instead.
    const suppressAlreadyRunning = sessions.get(sessionId)?.role !== "hub";
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

  const removeTrustedDirectory = (path: string) =>
    void request({ type: "remove_trusted_directory", path }).catch((err) => toastError((err as Error).message));

  // Nothing to show until the first snapshot. The toasts belong here all the same, and so does the
  // banner with the Retry the client offers once it has given up — otherwise this screen would say
  // it is connecting forever with nothing the user can do about it.
  if (!hosts) {
    return (
      <div className="flex h-full flex-col">
        <Toasts />
        {connectionState === "closed" && <ConnectionBanner state={connectionState} onRetry={reconnect} />}
        <div className="flex flex-1 items-center justify-center text-muted">
          {connectionState === "closed" ? "The daemon is not answering." : "Connecting to the daemon…"}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <Toasts belowPaneHeader />
      <ConnectionBanner state={connectionState} onRetry={reconnect} />
      <div className="flex min-h-0 flex-1">
        {sidebarOpen && <Scrim label="Close sessions" onClose={() => setSidebarOpen(false)} />}
        <Sidebar
          consoles={consoleList}
          projects={projectList}
          sessions={sessionList}
          selectedSessionId={selectedSessionId}
          trustedDirectories={trustedDirectories}
          onRemoveTrustedDirectory={removeTrustedDirectory}
          onSelectSession={selectSession}
          onOpenHub={openHub}
          onOpenDialog={setDialogRequest}
          open={sidebarOpen}
        />
        {/* Below the `docked` breakpoint the terminal is the row's only content and the floor
            drops to 382px (see `TerminalPane.tsx`); `overflow-x-auto` is what makes a viewport
            narrower than that scroll instead of clipping. */}
        <main className="flex min-w-0 flex-1 overflow-x-auto docked:overflow-visible">
          <TerminalPane
            session={selectedSession}
            onResume={resumeSession}
            sidebarOpen={sidebarOpen}
            onToggleSidebar={toggleSidebar}
            hasReportPanel={hasReportPanel}
            reportOpen={reportOpen}
            onToggleReport={toggleReport}
          />
          {/* Only the hub session's console has a report panel — it is that console's panel, not
              the session's. Keyed on the console id so switching hubs mounts a fresh instance. */}
          {/* The `selectedSession &&` is only for narrowing: `hasReportPanel` already implies it. */}
          {hasReportPanel && selectedSession && (
            <ReportPanel
              key={selectedSession.console_id}
              consoleId={selectedSession.console_id}
              hubSessionId={selectedSession.id}
              open={reportOpen}
            />
          )}
        </main>
        {reportOpen && <Scrim label="Close report" onClose={() => setReportOpen(false)} />}
      </div>
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
          title="Quit Octoboard"
          message="Some sessions are still running. Quitting interrupts them; each stays resumable next time."
          confirmLabel="Quit"
          destructive
          onCancel={closeExitConfirm}
          onConfirm={confirmExit}
        />
      )}
    </div>
  );
}
