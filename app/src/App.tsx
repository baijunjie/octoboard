import React, { useRef, useState } from "react";

import { ConfirmDialog } from "./components/ConfirmDialog";
import { ConsoleDialog } from "./components/ConsoleDialog";
import { ErrorToasts } from "./components/ErrorToasts";
import { ProjectDialog } from "./components/ProjectDialog";
import { RenameSessionDialog } from "./components/RenameSessionDialog";
import { SessionDialog } from "./components/SessionDialog";
import { Sidebar } from "./components/Sidebar";
import { DaemonRequestError } from "./daemon-client";
import { useAppExit } from "./lifecycle/useAppExit";
import { isDormant, type Console, type Project, type Session } from "./protocol";
import { useDaemon } from "./store";
import { TerminalPane } from "./terminal/TerminalPane";

type Dialog =
  | { kind: "new-console" }
  | { kind: "edit-console"; console: Console }
  | { kind: "delete-console"; console: Console }
  | { kind: "new-project"; console: Console }
  | { kind: "edit-project"; project: Project }
  | { kind: "delete-project"; project: Project }
  | { kind: "new-session"; console: Console; project: Project }
  | { kind: "rename-session"; session: Session }
  | { kind: "archive-session"; session: Session };

/** The code the daemon's `error` carries for a launch asked for while one was already running or
 * starting for that session (see "Daemon to client" in `daemon/PROTOCOL.md`). The in-flight guard below already stops this
 * client from causing one, but another path to the same session — the sidebar row and its own
 * "Resume" action-menu item, for instance — can still race it; these are not failures worth
 * showing. */
const SESSION_ALREADY_RUNNING = "session_already_running";

export function App({ port }: { port: number }): React.ReactElement {
  const { connectionState, hosts, consoles, projects, sessions, request, toastError, reconnect } =
    useDaemon();
  const [selectedSessionId, setSelectedSessionId] = useState<string>();
  const [dialog, setDialog] = useState<Dialog>();

  const sessionList = Array.from(sessions.values());
  const selectedSession = selectedSessionId ? sessions.get(selectedSessionId) : undefined;

  // Every click that could resume or create a session is guarded against its own double-click: two
  // fast clicks on the Hub row otherwise create two hub sessions (only one of which the tree can
  // ever show again, since hub sessions belong to no project node), and two fast clicks on a
  // dormant session fire two `resume_session` calls.
  const inFlightRef = useRef<Set<string>>(new Set());

  const runOnce = async (key: string, action: () => Promise<void>) => {
    if (inFlightRef.current.has(key)) return;
    inFlightRef.current.add(key);
    try {
      await action();
    } catch (err) {
      if (!(err instanceof DaemonRequestError) || err.code !== SESSION_ALREADY_RUNNING) {
        toastError((err as Error).message);
      }
    } finally {
      inFlightRef.current.delete(key);
    }
  };

  const resumeSession = (sessionId: string) =>
    void runOnce(`resume:${sessionId}`, () => request({ type: "resume_session", session: sessionId }).then(() => {}));

  const openHub = (console_: Console) =>
    void runOnce(`hub:${console_.id}`, async () => {
      const reply = await request({ type: "open_session", console_id: console_.id });
      if (reply.type === "session_opened") setSelectedSessionId(reply.session.id);
    });

  // --- session selection -----------------------------------------------------------------
  const selectSession = (session: Session) => {
    setSelectedSessionId(session.id);
    if (isDormant(session.status)) resumeSession(session.id);
  };

  // `useAppExit` wires `requestQuit` into the window-close/Cmd+Q/app-menu listeners on its own; the
  // main app has no separate manual "Quit" control, so only the confirmation dialog below is needed
  // from what it returns.
  const { exitConfirmOpen, closeExitConfirm, confirmExit } = useAppExit({
    getSessions: () => sessionList,
    requestShutdown: () => request({ type: "shutdown" }),
    toastError,
  });

  if (!hosts) {
    // Nothing to show until the first snapshot. The toasts belong here all the same, and so does
    // the Retry the banner carries once the client has given up — otherwise this screen would say
    // it is connecting forever with nothing the user can do about it.
    return (
      <div className="app-shell">
        <ErrorToasts />
        {connectionState === "closed" && <ConnectionBanner state={connectionState} onRetry={reconnect} />}
        <div className="message-screen">
          {connectionState === "closed"
            ? "The daemon is not answering."
            : "Connecting to the daemon…"}
        </div>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <ErrorToasts />
      {(connectionState === "reconnecting" || connectionState === "closed") && (
        <ConnectionBanner state={connectionState} onRetry={reconnect} />
      )}
      <div className="app-body">
        <Sidebar
          consoles={Array.from(consoles.values())}
          projects={Array.from(projects.values())}
          sessions={sessionList}
          selectedSessionId={selectedSessionId}
          onSelectSession={selectSession}
          onNewConsole={() => setDialog({ kind: "new-console" })}
          onEditConsole={(c) => setDialog({ kind: "edit-console", console: c })}
          onDeleteConsole={(c) => setDialog({ kind: "delete-console", console: c })}
          onOpenHub={openHub}
          onNewProject={(c) => setDialog({ kind: "new-project", console: c })}
          onEditProject={(p) => setDialog({ kind: "edit-project", project: p })}
          onDeleteProject={(p) => setDialog({ kind: "delete-project", project: p })}
          onNewSession={(c, p) => setDialog({ kind: "new-session", console: c, project: p })}
          onRenameSession={(s) => setDialog({ kind: "rename-session", session: s })}
          onArchiveSession={(s) => setDialog({ kind: "archive-session", session: s })}
        />
        <main className="main-pane">
          <TerminalPane port={port} session={selectedSession} onResume={resumeSession} />
        </main>
      </div>

      {dialog?.kind === "new-console" && <ConsoleDialog onClose={() => setDialog(undefined)} />}
      {dialog?.kind === "edit-console" && <ConsoleDialog console={dialog.console} onClose={() => setDialog(undefined)} />}
      {dialog?.kind === "delete-console" && (
        <ConfirmDialog
          title="Delete console"
          message={`Delete "${dialog.console.name}"? This only works while it has no live sessions.`}
          confirmLabel="Delete"
          destructive
          onCancel={() => setDialog(undefined)}
          onConfirm={async () => {
            await request({ type: "delete_console", console: dialog.console.id });
            setDialog(undefined);
          }}
        />
      )}
      {dialog?.kind === "new-project" && (
        <ProjectDialog consoleId={dialog.console.id} onClose={() => setDialog(undefined)} />
      )}
      {dialog?.kind === "edit-project" && (
        <ProjectDialog consoleId={dialog.project.console_id} project={dialog.project} onClose={() => setDialog(undefined)} />
      )}
      {dialog?.kind === "delete-project" && (
        <ConfirmDialog
          title="Remove project"
          message={`Remove "${dialog.project.name}" from its console? This only removes the association and never touches the directory, but it deletes this project's sessions — archived ones included — and only works while none of them is still running.`}
          confirmLabel="Remove"
          destructive
          onCancel={() => setDialog(undefined)}
          onConfirm={async () => {
            await request({ type: "delete_project", project: dialog.project.id });
            setDialog(undefined);
          }}
        />
      )}
      {dialog?.kind === "new-session" && (
        <SessionDialog
          console={dialog.console}
          project={dialog.project}
          onClose={() => setDialog(undefined)}
          onOpened={setSelectedSessionId}
        />
      )}
      {dialog?.kind === "rename-session" && (
        <RenameSessionDialog session={dialog.session} onClose={() => setDialog(undefined)} />
      )}
      {dialog?.kind === "archive-session" && (
        <ConfirmDialog
          title="Archive session"
          message={`Archive "${dialog.session.title}"? Its process will end; archived sessions can be reopened later.`}
          confirmLabel="Archive"
          destructive
          onCancel={() => setDialog(undefined)}
          onConfirm={async () => {
            await request({ type: "archive_session", session: dialog.session.id });
            setDialog(undefined);
          }}
        />
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

function ConnectionBanner({
  state,
  onRetry,
}: {
  state: "connecting" | "reconnecting" | "closed";
  onRetry: () => void;
}): React.ReactElement {
  return (
    <div className="connection-banner">
      {state !== "closed" ? (
        <span>Disconnected from the daemon — reconnecting…</span>
      ) : (
        <>
          <span>Disconnected from the daemon.</span>
          <button type="button" onClick={onRetry}>
            Retry
          </button>
        </>
      )}
    </div>
  );
}
