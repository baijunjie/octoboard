import { Button, Chip } from "@heroui/react";
import React, { useMemo, useRef, useState } from "react";

import { Sidebar } from "./components/Sidebar";
import { DaemonRequestError } from "./daemon-client";
import type { DialogRequest } from "./dialogRequest";
import { useAppExit } from "./lifecycle/useAppExit";
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { isDormant, type Console, type Session } from "./protocol";
import { sessionLocation } from "./sessionLabel";
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
  const { request, toastError, dismissToast, reconnect } = useDaemon();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const hosts = useDaemonStore((s) => s.hosts);
  const toasts = useDaemonStore((s) => s.toasts);
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const sessions = useDaemonStore((s) => s.sessions);
  const trustedDirectories = useDaemonStore((s) => s.trustedDirectories);
  const [selectedSessionId, setSelectedSessionId] = useState<string>();
  // TODO: transitional, see `dialogRequest.ts`.
  const [, setDialogRequest] = useState<DialogRequest>();

  const consoleList = useMemo(() => Array.from(consoles.values()), [consoles]);
  const projectList = useMemo(() => Array.from(projects.values()), [projects]);
  const sessionList = useMemo(() => Array.from(sessions.values()), [sessions]);
  const selectedSession = selectedSessionId ? sessions.get(selectedSessionId) : undefined;

  useWaitingNotifications(sessionList, consoles, projects, hosts !== undefined);

  // The exit flow's frontend owner: it registers with the shell on mount, which is what makes the
  // window quittable. The confirmation it asks for is not rendered yet.
  // TODO: render the quit confirmation (`exitConfirmOpen`, `closeExitConfirm`, `confirmExit`) in
  // the HeroUI UI rewrite plan's milestone 03 (remaining screens).
  useAppExit({
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

  // TODO: a simple stand-in; the HeroUI UI rewrite plan's milestone 03 (remaining screens) replaces
  // it with the real toasts.
  const toastStack = (
    <div className="flex flex-col gap-1 px-3 empty:hidden">
      {toasts.map((toast) => {
        // The daemon's notice text deliberately does not name its session, so it is prefixed here.
        const session = toast.session ? sessions.get(toast.session) : undefined;
        return (
          <div key={toast.id} className="flex items-center gap-2 py-1 text-sm">
            <Chip size="sm" color={toast.kind === "error" ? "danger" : "default"}>
              {toast.kind}
            </Chip>
            <span className="min-w-0 flex-1">
              {session && <strong>{sessionLocation(session, consoles, projects)}: </strong>}
              {toast.message}
            </span>
            <Button size="sm" variant="ghost" preventFocusOnPress onPress={() => dismissToast(toast.id)}>
              Dismiss
            </Button>
          </div>
        );
      })}
    </div>
  );

  // Nothing to show until the first snapshot. The toasts belong here all the same, and so does the
  // Retry the client offers once it has given up — otherwise this screen would say it is connecting
  // forever with nothing the user can do about it.
  if (!hosts) {
    return (
      <div className="flex h-full flex-col">
        {toastStack}
        <div className="flex flex-1 flex-col items-center justify-center gap-3 text-muted">
          {connectionState === "closed" ? (
            <>
              <p>The daemon is not answering.</p>
              <Button onPress={reconnect}>Retry</Button>
            </>
          ) : (
            <p>Connecting to the daemon…</p>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {toastStack}
      {/* TODO: a simple stand-in for the HeroUI UI rewrite plan's milestone 03 (remaining screens)
          connection banner. */}
      {(connectionState === "reconnecting" || connectionState === "closed") && (
        <div className="flex items-center gap-2 bg-danger-soft px-3 py-1 text-sm">
          {connectionState === "closed" ? (
            <>
              <span>Disconnected from the daemon.</span>
              <Button size="sm" preventFocusOnPress onPress={reconnect}>
                Retry
              </Button>
            </>
          ) : (
            <span>Disconnected from the daemon — reconnecting…</span>
          )}
        </div>
      )}
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
          {/* TODO: the hub's report panel (a selected session with `role === "hub"` shows its
              console's panel beside the terminal) belongs to the HeroUI UI rewrite plan's
              milestone 03 (remaining screens). */}
        </main>
      </div>
      {/* TODO: the HeroUI UI rewrite plan's milestone 03 (remaining screens) renders the requested
          dialog here, along with the trust-prompt dialog (`trustPrompts`) and the quit
          confirmation. */}
    </div>
  );
}
