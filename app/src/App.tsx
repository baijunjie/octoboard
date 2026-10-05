import React, { useRef, useState } from "react";

import { ConfirmDialog } from "./components/ConfirmDialog";
import { ConsoleDialog } from "./components/ConsoleDialog";
import { ProjectDialog } from "./components/ProjectDialog";
import { RenameSessionDialog } from "./components/RenameSessionDialog";
import { ReportPanel } from "./components/ReportPanel";
import { SessionDialog } from "./components/SessionDialog";
import { Sidebar } from "./components/Sidebar";
import { Toasts } from "./components/Toasts";
import { DaemonRequestError } from "./daemon-client";
import { useAppExit } from "./lifecycle/useAppExit";
import { useWaitingNotifications } from "./lifecycle/useWaitingNotifications";
import { isDormant, type Console, type Project, type Session } from "./protocol";
import { useDaemon, type TrustPrompt } from "./store";
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
 * starting for that session, or for resuming an archived hub while a live one already exists (see
 * "Daemon to client" in `daemon/PROTOCOL.md`). The in-flight guard below already stops this client
 * from causing the double-click kind, but another path to the same session — the sidebar row and
 * its own "Resume" action-menu item, for instance — can still race it; `runOnce` decides per call
 * whether that race is worth showing. */
const SESSION_ALREADY_RUNNING = "session_already_running";

/** The code the daemon's `error` carries when the parent directory to trust is the filesystem root,
 * the home directory or one containing it. The trust dialog stays open on it, with the error shown. */
const TRUST_DIRECTORY_TOO_BROAD = "trust_directory_too_broad";

/** The code the daemon's `error` carries for a go-ahead to a trust screen that is no longer waiting:
 * answered already, or its session gone. There is nothing to tell the user then. */
const CLAUDE_TRUST_NOT_WAITING = "claude_trust_not_waiting";

/** What the trust dialog says. The folder-wide choice is spelled out because it reaches beyond the
 * project being asked about: every project under the folder, including repositories the hub clones
 * or adds there later, is trusted without a question, and the permissions and hooks in their
 * `.claude/settings.json` then apply without asking. */
function trustPromptMessage(prompt: TrustPrompt, sessionLabel: string): string {
  const folderWide = prompt.trustDir
    ? ` "Trust all projects in ${shortDirectory(prompt.trustDir)}" trusts every project under ${prompt.trustDir} — those already there and any added there later, repositories the hub clones or adds into it included — without asking again. You can stop that again under "Trusted folders" in the sidebar.`
    : "";
  return `Claude Code is asking whether to trust ${prompt.path}${sessionLabel}. Octoboard can answer for you: "Trust and continue" trusts this project's sessions from now on.${folderWide} A folder's .claude/settings.json may pre-approve tool permissions, and trusting it applies them without asking. "Not now" leaves the question in the terminal for you to answer.`;
}

/** A directory shortened for a button: its last two components. The full path is in the message. */
function shortDirectory(path: string): string {
  const parts = path.split("/").filter((part) => part !== "");
  return parts.length <= 2 ? path : `…/${parts.slice(-2).join("/")}`;
}

export function App({ port }: { port: number }): React.ReactElement {
  const {
    connectionState,
    hosts,
    consoles,
    projects,
    sessions,
    trustPrompts,
    trustedDirectories,
    request,
    toastError,
    dismissTrustPrompt,
    reconnect,
  } = useDaemon();
  const [selectedSessionId, setSelectedSessionId] = useState<string>();
  const [dialog, setDialog] = useState<Dialog>();

  const sessionList = Array.from(sessions.values());
  // One dialog at a time, oldest prompt first; answering or declining it brings up the next.
  const trustPrompt = trustPrompts[0];
  const selectedSession = selectedSessionId ? sessions.get(selectedSessionId) : undefined;

  useWaitingNotifications(sessionList, consoles, projects, hosts !== undefined);

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

  const answerTrustPrompt = async (prompt: TrustPrompt, trustParentDir: boolean) => {
    // Dismissed whether or not the request worked: the daemon answers a screen once, so retrying
    // from this dialog can never succeed. The error is toasted rather than shown inline for the
    // same reason. A failure to answer also reaches the user as the daemon's own session notice,
    // which covers a dialog closed meanwhile. The one exception is a parent directory that is too
    // broad: nothing was answered, so the error stays in the dialog and the user picks again.
    try {
      await request({
        type: "confirm_claude_trust",
        session: prompt.session,
        remember: true,
        trust_parent_dir: trustParentDir,
      });
    } catch (err) {
      if (err instanceof DaemonRequestError && err.code === TRUST_DIRECTORY_TOO_BROAD) throw err;
      if (err instanceof DaemonRequestError && err.code === CLAUDE_TRUST_NOT_WAITING) {
        // Nothing to tell for the plain go-ahead; but a folder the user asked to trust was not.
        if (trustParentDir) {
          toastError(
            "The trust screen was already answered, so the folder was not trusted. Use the dialog again for the next screen.",
          );
        }
      } else {
        toastError((err as Error).message);
      }
    }
    dismissTrustPrompt(prompt.session);
  };

  const removeTrustedDirectory = (path: string) =>
    void request({ type: "remove_trusted_directory", path }).catch((err) => toastError((err as Error).message));

  const trustSessionLabel = (sessionId: string) => {
    const title = sessions.get(sessionId)?.title;
    return title ? ` for session "${title}"` : "";
  };

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

  // `useAppExit` wires `requestQuit` into the window-close/Cmd+Q/app-menu/Dock-Quit/system-terminate
  // listeners on its own; the main app has no separate manual "Quit" control, so only the
  // confirmation dialog below is needed from what it returns.
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
        <Toasts />
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
      <Toasts />
      {(connectionState === "reconnecting" || connectionState === "closed") && (
        <ConnectionBanner state={connectionState} onRetry={reconnect} />
      )}
      <div className="app-body">
        <Sidebar
          consoles={Array.from(consoles.values())}
          projects={Array.from(projects.values())}
          sessions={sessionList}
          selectedSessionId={selectedSessionId}
          trustedDirectories={trustedDirectories}
          onRemoveTrustedDirectory={removeTrustedDirectory}
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
          {/* Only the hub session's console has a report panel — it is that console's panel, not
              the session's. Keyed on the console id so switching hubs mounts a fresh instance:
              the panel's position is an anchor page id looked up fresh in the new console's page
              list, and a carried-over anchor simply misses and falls back to the newest page, so
              the key is defence in depth rather than a fix for a live defect. */}
          {selectedSession?.role === "hub" && (
            <ReportPanel key={selectedSession.console_id} consoleId={selectedSession.console_id} />
          )}
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
      {trustPrompt && (
        <ConfirmDialog
          key={trustPrompt.session}
          title="Trust this folder?"
          message={trustPromptMessage(trustPrompt, trustSessionLabel(trustPrompt.session))}
          confirmLabel="Trust and continue"
          cancelLabel="Not now"
          onCancel={() => dismissTrustPrompt(trustPrompt.session)}
          onConfirm={() => answerTrustPrompt(trustPrompt, false)}
          extraAction={
            trustPrompt.trustDir
              ? {
                  label: `Trust all projects in ${shortDirectory(trustPrompt.trustDir)}`,
                  title: trustPrompt.trustDir,
                  onClick: () => answerTrustPrompt(trustPrompt, true),
                }
              : undefined
          }
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
