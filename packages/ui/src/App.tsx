import React, { useMemo, useRef, useState } from "react";

import { ConnectionBanner } from "./components/ConnectionBanner";
import { Sidebar } from "./components/Sidebar";
import { Toasts } from "./components/Toasts";
import { DaemonRequestError } from "./daemon-client";
import { ConfirmDialog } from "./dialogs/ConfirmDialog";
import type { DialogRequest } from "./dialogs/dialogRequest";
import { RequestedDialog } from "./dialogs/RequestedDialog";
import { TrustPromptDialog } from "./dialogs/TrustPromptDialog";
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

  const consoleList = useMemo(() => Array.from(consoles.values()), [consoles]);
  const projectList = useMemo(() => Array.from(projects.values()), [projects]);
  const sessionList = useMemo(() => Array.from(sessions.values()), [sessions]);
  const selectedSession = selectedSessionId ? sessions.get(selectedSessionId) : undefined;

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
      <Toasts />
      <ConnectionBanner state={connectionState} onRetry={reconnect} />
      <div className="flex min-h-0 flex-1">
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
        />
        <main className="flex min-w-0 flex-1">
          <TerminalPane session={selectedSession} onResume={resumeSession} />
          {/* Only the hub session's console has a report panel — it is that console's panel, not
              the session's. Keyed on the console id so switching hubs mounts a fresh instance. */}
          {selectedSession?.role === "hub" && (
            <ReportPanel key={selectedSession.console_id} consoleId={selectedSession.console_id} />
          )}
        </main>
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
