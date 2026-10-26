import React, { useEffect, useRef, useState } from "react";

import { useStableHandler } from "../components/useLatest";
import { DaemonRequestError } from "../daemon-client";
import { errorPresentation } from "../errorPresentation";
import { useListMotion } from "../components/useListMotion";
import { InlineReference } from "../components/InlineReference";
import { SessionAgentIcon } from "../components/SessionAgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { StatusIcon } from "../components/StatusIcon";
import { ImportConfigurationDialog } from "../configuration/ImportConfigurationDialog";
import { Message, useT } from "../i18n/react";
import { isLive, SESSION_BINDING_CHANGED, SESSION_NOT_BOUND, type Project, type Session, type SessionBranchItem } from "../protocol";
import { sessionAgentLabel, sessionDisplayTitle, sessionShownStatusKind, sessionStatusKind } from "../sessionLabel";
import {
  boundArchivedSessions,
  boundSessionsArchivedWith,
  compareSessions,
  hasBoundSessions,
  isUnusedSession,
  ordersByStatus,
} from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConsoleDialog } from "./ConsoleDialog";
import { HandoffDialog } from "./HandoffDialog";
import type { DialogRequest } from "./dialogRequest";
import { ProjectDialog } from "./ProjectDialog";
import { RenameDialog } from "./RenameDialog";
import { SessionDialog } from "./SessionDialog";
import { SwitchAccountDialog } from "./SwitchAccountDialog";
import { useBranchCleanup, useProjectRemovalCleanup, type ConsequenceLine } from "./BranchCleanupOptions";
import { ConfirmOption } from "./ConfirmOption";
import { KeepFocusInDialog } from "./KeepFocusInDialog";
import { useSessionRecords } from "./useSessionRecords";
import { PROJECT_NAME_LIMIT } from "./textLength";

/** What a confirmation says it is about to take along, as body text under its options: `title` when
 * there is one, over the list of sessions when there is one to show — each with its agent's icon and
 * title, plus a status glyph for live sessions; archived sessions are already identified by the
 * wording of the title — and `lines`: one more consequence for each option the user has ticked.
 * Nothing at all when there is neither a title nor a line. */
function ConfirmConsequences({ title, sessions, lines = [] }: { title?: string; sessions?: Session[]; lines?: ConsequenceLine[] }): React.ReactElement | null {
  const t = useT();
  const sessionsRef = useListMotion<HTMLUListElement>();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const linesRef = useListMotion<HTMLDivElement>();
  if (!title && lines.length === 0) return null;
  return (
    <div className="flex min-w-0 flex-col gap-2 text-sm text-foreground">
      {title && <p>{title}</p>}
      {sessions && (
        <ul ref={sessionsRef} className="relative flex min-w-0 flex-col gap-1">
          {sessions.map((session) => (
            <li key={session.id} data-list-motion={session.id} className="flex min-w-0 items-center gap-2">
              {/* The list items are not pressable, so a tap on a mark opens the popover. */}
              {session.status !== "archived" && <StatusIcon status={sessionShownStatusKind(session)} withTooltip touchPopover />}
              <SessionAgentIcon session={session} accounts={accounts} defaultNames={defaultNames} touchPopover />
              {/* The icon is decorative, and nothing else here names the agent. The comma keeps the name apart from the
                  title when the row is read as one phrase: a hidden element's trailing space is dropped in rendering. */}
              <span className="sr-only">{`${sessionAgentLabel(t, session, accounts, defaultNames)}, `}</span>
              <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={sessionDisplayTitle(t, session)}>
                {sessionDisplayTitle(t, session)}
              </FadeOverflow>
            </li>
          ))}
        </ul>
      )}
      {/* Always mounted, so a line added or removed by an option moves in and out. */}
      <div ref={linesRef} className="relative flex min-w-0 flex-col gap-1">
        {lines.map((line) => <p key={line.key} data-list-motion={line.key}>{line.text}</p>)}
      </div>
    </div>
  );
}

/** Confirms removing a project. The folder stays on disk unless the user opts in to moving it to
 * the Trash, which the daemon does before removing the project and refuses (keeping the project)
 * when it cannot. With the folder go the worktrees outside it that the daemon lists and, where the
 * repository outlives the folder, the branches of the project's sessions, listed once the option is
 * chosen; a locked worktree only when the user also says so, with the branch it has checked out. */
function DeleteProjectDialog({ project, running, onClose }: { project: Project; running: Session[]; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [trash, setTrash] = useState(false);
  const cleanup = useProjectRemovalCleanup(project.id, trash, setTrash);
  return (
    <ConfirmDialog
      title={t("dialog.removeProject.title")}
      subject={<Message id="dialog.removeProject.subject" params={{ name: <InlineReference dir="auto" wrap>{project.name}</InlineReference> }} />}
      message={t(trash ? "dialog.removeProject.messageTrash" : "dialog.removeProject.message")}
      consequences={<ConfirmConsequences title={running.length > 0 ? t("dialog.removeProject.running", { count: running.length }) : undefined} sessions={running.length > 0 ? running : undefined} lines={cleanup.consequences} />}
      confirmLabel={t(running.length > 0 ? "dialog.removeProject.confirmRunning" : "common.remove")}
      destructive
      waiting={cleanup.pending}
      extraControl={cleanup.extraControl}
      onCancel={onClose}
      onConfirm={async () => {
        await request({
          type: "delete_project",
          project: project.id,
          stop_sessions: running.length > 0,
          trash_directory: trash,
          ...cleanup.choices,
        });
        onClose();
      }}
    />
  );
}

/** The option of also moving the agent's own records to the Trash, which a completed inspection
 * can take away. */
function SessionRecordsTrashOption({ visible, ...props }: React.ComponentProps<typeof ConfirmOption> & { visible: boolean }): React.ReactElement {
  return <KeepFocusInDialog>{visible && <ConfirmOption {...props} />}</KeepFocusInDialog>;
}

/** Confirms archiving a session and every unarchived session under it, listing the bound branches
 * and worktrees git refuses to remove with the choice of forcing them. */
function ArchiveSessionDialog({ session, withIt, branches, onClose }: {
  session: Session;
  withIt: Session[];
  /** An inspection of the bound branches already made for this archive. */
  branches?: SessionBranchItem[];
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const cleanup = useBranchCleanup([session.id, ...withIt.map((s) => s.id)], branches);
  return (
    <ConfirmDialog
      title={t("dialog.archiveSession.title")}
      subject={<Message id="dialog.archiveSession.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />}
      message={t(sessionStatusKind(session) === "background" ? "dialog.archiveSession.messageBackground" : "dialog.archiveSession.message")}
      consequences={<ConfirmConsequences title={withIt.length > 0 ? t("dialog.archiveSession.boundAndEnded", { count: withIt.length }) : undefined} sessions={withIt.length > 0 ? withIt : undefined} lines={cleanup.consequences} />}
      confirmLabel={t(cleanup.forcing ? "dialog.archiveSession.confirmDelete" : "dialog.archiveSession.confirm")}
      destructive={cleanup.forcing}
      waiting={cleanup.pending}
      extraControl={cleanup.extraControl}
      errorTitle={t("dialog.archiveSession.failedTitle")}
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "archive_session", session: session.id, ...cleanup.choices });
        onClose();
      }}
    />
  );
}

/** The confirmation of releasing a session from its owner, which only the session itself can undo.
 * Closes without a word when the binding is already gone: the session was deleted or released, or its owner
 * is gone, or the daemon refuses the release as `session_not_bound`. An archived session keeps its binding, but
 * the menu offers no Unbind on an archived row, so the dialog closes for it too. Any other refusal is shown
 * and the dialog stays, `session_binding_changed` included: the session is bound to another owner by then,
 * which the dialog reads live and so names. */
function UnbindSessionDialog({ sessionId, onClose }: { sessionId: string; onClose: () => void }): React.ReactElement | null {
  const t = useT();
  const { request, store } = useDaemon();
  const session = useDaemonStore((s) => s.sessions.get(sessionId));
  const owner = useDaemonStore((s) => (session?.bound_to ? s.sessions.get(session.bound_to) : undefined));
  // The release itself clears the binding, which must not read as the session having gone.
  const [releasing, setReleasing] = useState(false);
  const gone = !releasing && (!session || !owner || session.status === "archived");
  useEffect(() => {
    if (gone) onClose();
  }, [gone]);
  if (!session || !owner) return null;
  const ownerName = <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, owner)}</InlineReference>;
  return (
    <ConfirmDialog
      title={t("dialog.unbindSession.title")}
      message={<p><Message id="dialog.unbindSession.message" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference>, owner: ownerName }} /></p>}
      confirmLabel={t("dialog.unbindSession.confirm")}
      errorTitle={t("dialog.unbindSession.error")}
      onCancel={onClose}
      onConfirm={async () => {
        setReleasing(true);
        try {
          await request({ type: "release_session", session: session.id });
        } catch (err) {
          const code = err instanceof DaemonRequestError ? err.code : undefined;
          // Refused as changed while still bound (to another owner, which the dialog then names): the user
          // decides again. Once the store shows it unbound there is nothing left to decide.
          if (code === SESSION_NOT_BOUND || (code === SESSION_BINDING_CHANGED && !store.getState().sessions.get(sessionId)?.bound_to)) {
            onClose();
            return;
          }
          setReleasing(false);
          throw err;
        }
        onClose();
      }}
    />
  );
}

/** The daemon checks the unused state atomically before deleting and cleaning up native records. */
function DeleteUnusedSession({ sessionId, onClose }: { sessionId: string; onClose: () => void }): null {
  const { request, toastError } = useDaemon();
  const close = useStableHandler(onClose);
  const deletion = useRef<ReturnType<typeof request> | undefined>(undefined);
  useEffect(() => {
    let active = true;
    // Reuse the request when Strict Mode reconnects this effect.
    deletion.current ??= request({ type: "delete_session", session: sessionId, only_if_unused: true });
    void deletion.current.then(() => {
      if (active) close();
    }, (err: unknown) => {
      if (!active) return;
      toastError(errorPresentation(err as Error), sessionId);
      close();
    });
    return () => { active = false; };
  }, [sessionId, request, toastError, close]);
  return null;
}

/** The confirmation of a session's deletion, with the option of also moving the agent's own
 * records of it to the Trash. `withIt` are the archived sessions that go along. */
function DeleteSessionDialog({ session, withIt, onClose }: { session: Session; withIt: Session[]; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [trash, setTrash] = useState(false);
  const scope = [session.id, ...withIt.map((s) => s.id)];
  const presence = useSessionRecords(scope);
  const cleanup = useBranchCleanup(scope);
  const canTrash = presence !== "absent";
  const moveRecords = canTrash && trash;
  return (
    <ConfirmDialog
      title={t("dialog.deleteSession.title")}
      subject={<Message id="dialog.deleteSession.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />}
      message={t("dialog.deleteSession.message")}
      consequences={(
        <ConfirmConsequences
          title={withIt.length > 0 ? t("dialog.deleteSession.bound", { count: withIt.length }) : undefined}
          sessions={withIt.length > 0 ? withIt : undefined}
          lines={[...cleanup.consequences, ...(moveRecords ? [{ key: "records", text: t(withIt.length > 0 ? "dialog.deleteSession.trashRecordsLineBound" : "dialog.deleteSession.trashRecordsLine") }] : [])]}
        />
      )}
      confirmLabel={t(moveRecords || cleanup.forcing ? "dialog.deleteSession.confirmAll" : "common.delete")}
      destructive
      waiting={cleanup.pending}
      extraControl={(busy) => (
        <>
          {cleanup.extraControl(busy)}
          <SessionRecordsTrashOption
            visible={canTrash}
            isSelected={trash}
            onChange={setTrash}
            isReadOnly={busy}
            label={t("dialog.deleteSession.trashRecords")}
            hint={t(withIt.length > 0 ? "dialog.deleteSession.trashRecordsNoteBound" : "dialog.deleteSession.trashRecordsNote")}
          />
        </>
      )}
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "delete_session", session: session.id, trash_agent_records: moveRecords, ...cleanup.choices });
        onClose();
      }}
    />
  );
}

/** The confirmation of deleting many archived sessions at once, with the option of also moving the
 * agent's own records of them to the Trash. `withThem` counts the archived sessions that go along. */
function DeleteArchivedDialog({
  dialog,
  withThem,
  targets,
  onClose,
}: {
  dialog: Extract<DialogRequest, { kind: "delete-archived" }>;
  withThem: number;
  targets: Session[];
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [trash, setTrash] = useState(false);
  const presence = useSessionRecords(targets.map((s) => s.id));
  const cleanup = useBranchCleanup(targets.map((s) => s.id));
  const canTrash = presence !== "absent";
  const moveRecords = canTrash && trash;
  return (
    <ConfirmDialog
      title={t("dialog.deleteArchived.title")}
      message={t("dialog.deleteArchived.message", { count: dialog.count })}
      consequences={(
        <ConfirmConsequences
          title={withThem > 0 ? t("dialog.deleteArchived.bound", { count: withThem }) : undefined}
          lines={[...cleanup.consequences, ...(moveRecords ? [{ key: "records", text: t("dialog.deleteArchived.trashRecordsLine") }] : [])]}
        />
      )}
      confirmLabel={t("dialog.deleteArchived.confirm")}
      destructive
      waiting={cleanup.pending}
      extraControl={(busy) => (
        <>
          {cleanup.extraControl(busy)}
          <SessionRecordsTrashOption
            visible={canTrash}
            isSelected={trash}
            onChange={setTrash}
            isReadOnly={busy}
            label={t("dialog.deleteArchived.trashRecords")}
            hint={t("dialog.deleteArchived.trashRecordsNote")}
          />
        </>
      )}
      onCancel={onClose}
      onConfirm={async () => {
        await request({
          type: "delete_archived_sessions",
          console: dialog.console.id,
          project: dialog.project?.id,
          console_session: dialog.consoleSession?.id,
          trash_agent_records: moveRecords,
          ...cleanup.choices,
        });
        onClose();
      }}
    />
  );
}

/** The dialog a sidebar menu asked for. `onClose` is called once the dialog is done or dismissed;
 * `onSessionOpened` receives the session a "new session" dialog started, or a handoff's receiving session. */
export function RequestedDialog({
  dialog: requestedDialog,
  onClose,
  onSessionOpened,
  onSwitchAccounts,
  onResumeSession,
}: {
  dialog: DialogRequest;
  onClose: () => void;
  onSessionOpened: (sessionId: string) => void;
  /** Starts switching sessions to an account (`null`, the default account; `accountName` its name) in
   * the background, and returns at once; the outcome is reported once the whole batch is done. */
  onSwitchAccounts: (sessionIds: string[], account: string | null, accountName: string) => void;
  /** Resumes a session the session dialog found, an archived one (reopening it) or an interrupted one;
   * rejects with the reason when it did not happen. */
  onResumeSession: (sessionId: string) => Promise<void>;
}): React.ReactElement {
  const t = useT();
  const { request, toastNotice } = useDaemon();
  const sessions = useDaemonStore((s) => s.sessions);
  const byStatus = useDaemonStore((s) => ordersByStatus(s.settings));
  const dialog = "session" in requestedDialog && requestedDialog.kind !== "rename-session"
    ? { ...requestedDialog, session: sessions.get(requestedDialog.session.id) ?? requestedDialog.session }
    : requestedDialog;

  switch (dialog.kind) {
    case "new-console":
      return <ConsoleDialog onClose={onClose} />;
    case "edit-console":
      return <ConsoleDialog console={dialog.console} onClose={onClose} />;
    case "import-configuration":
      return <ImportConfigurationDialog onClose={onClose} />;
    case "delete-console":
      return (
        <ConfirmDialog
          title={t("dialog.deleteConsole.title")}
          subject={<Message id="dialog.deleteConsole.subject" params={{ name: <InlineReference dir="auto" wrap>{dialog.console.name}</InlineReference> }} />}
          message={t("dialog.deleteConsole.message")}
          confirmLabel={t("common.delete")}
          typeToConfirm={t("dialog.typeToConfirm.delete")}
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "delete_console", console: dialog.console.id });
            onClose();
          }}
        />
      );
    case "new-project":
      return <ProjectDialog consoleId={dialog.console.id} onClose={onClose} />;
    case "edit-project":
      return <ProjectDialog consoleId={dialog.project.console_id} project={dialog.project} onClose={onClose} />;
    case "restore-project-changes":
      return (
        <ConfirmDialog
          title={t("dialog.restoreProjectChanges.title")}
          subject={<Message id="dialog.restoreProjectChanges.subject" params={{ name: <InlineReference dir="auto" wrap>{dialog.project.name}</InlineReference> }} />}
          message={t("dialog.restoreProjectChanges.message")}
          confirmLabel={t("dialog.restoreProjectChanges.confirm")}
          pendingLabel={t("dialog.restoreProjectChanges.pending")}
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "restore_project_changes", project: dialog.project.id });
            toastNotice(t("sidebar.project.changesRestored"));
            onClose();
          }}
        />
      );
    case "delete-project": {
      // Read live: a session that starts or stops while the dialog is open joins or leaves the list.
      const running = Array.from(sessions.values())
        .filter((s) => s.project_id === dialog.project.id && isLive(s.status))
        .sort((a, b) => compareSessions(a, b, byStatus));
      return <DeleteProjectDialog project={dialog.project} running={running} onClose={onClose} />;
    }
    case "new-console-session":
      return (
        <SessionDialog
          console={dialog.console}
          sessions={Array.from(sessions.values())}
          onClose={onClose}
          onOpened={onSessionOpened}
          onResume={onResumeSession}
        />
      );
    case "new-session":
      return (
        <SessionDialog
          console={dialog.console}
          project={dialog.project}
          binding={dialog.binding}
          sessions={Array.from(sessions.values())}
          onClose={onClose}
          onOpened={onSessionOpened}
          onResume={onResumeSession}
        />
      );
    case "rename-project":
      return (
        <RenameDialog
          title={t("dialog.renameProject.title")}
          label={t("common.name")}
          initialValue={dialog.project.name}
          placeholder={t("dialog.project.nameExample")}
          requiredMessage={t("dialog.renameProject.nameRequired")}
          characterLimit={PROJECT_NAME_LIMIT}
          tooLongMessage={t("daemon.field_too_long.name", { limit: PROJECT_NAME_LIMIT })}
          onSubmit={async (name) => {
            await request({ type: "update_project", project: dialog.project.id, name });
          }}
          onClose={onClose}
        />
      );
    case "rename-session":
      return (
        <RenameDialog
          title={t("dialog.rename.title")}
          label={t("dialog.rename.field")}
          initialValue={dialog.session.title}
          placeholder={t("dialog.rename.nameExample")}
          requiredMessage={t("dialog.rename.titleRequired")}
          onSubmit={async (title) => {
            await request({ type: "rename_session", session: dialog.session.id, title });
          }}
          onClose={onClose}
        />
      );
    case "archive-session": {
      // Read live: every unarchived session under this owner goes along, with running
      // processes ended too. Sessions opened or bound while the dialog is open join the list, and
      // the message follows background work starting or finishing in the session itself.
      const withIt = boundSessionsArchivedWith(Array.from(sessions.values()), dialog.session.id);
      const session = sessions.get(dialog.session.id) ?? dialog.session;
      return <ArchiveSessionDialog session={session} withIt={withIt} branches={dialog.branches} onClose={onClose} />;
    }
    case "switch-account":
      return (
        <SwitchAccountDialog
          session={dialog.session}
          account={dialog.account}
          accountName={dialog.accountName}
          onClose={onClose}
          onSwitchAccounts={onSwitchAccounts}
        />
      );
    case "hand-off":
      return <HandoffDialog session={dialog.session} onClose={onClose} onHandedOff={onSessionOpened} />;
    case "unbind-session":
      return <UnbindSessionDialog sessionId={dialog.session.id} onClose={onClose} />;
    case "delete-session": {
      // An archived owner takes the archived sessions bound to it, and those bound to those, along.
      const withIt = boundArchivedSessions(Array.from(sessions.values()), dialog.session.id);
      const unused = isUnusedSession(dialog.session, hasBoundSessions(sessions.values(), dialog.session.id));
      return unused
        ? <DeleteUnusedSession key={dialog.session.id} sessionId={dialog.session.id} onClose={onClose} />
        : <DeleteSessionDialog session={dialog.session} withIt={withIt} onClose={onClose} />;
    }
    case "delete-archived": {
      // All of a console's archived console sessions take their archived bound sessions with them;
      // a project's archive, and a console session's own, do not reach any console session.
      const consoleSessions = dialog.project || dialog.consoleSession
        ? []
        : Array.from(sessions.values()).filter((s) => s.console_id === dialog.console.id && s.role === "console" && s.status === "archived");
      const withThem = consoleSessions.flatMap((owner) => boundArchivedSessions(Array.from(sessions.values()), owner.id)).length;
      const all = Array.from(sessions.values());
      const roots = dialog.project
        ? all.filter((s) => s.project_id === dialog.project!.id && s.status === "archived")
        : dialog.consoleSession
          ? boundArchivedSessions(all, dialog.consoleSession.id)
          : consoleSessions;
      const targets = Array.from(new Map(roots.flatMap((s) => [s, ...boundArchivedSessions(all, s.id)]).map((s) => [s.id, s])).values());
      return <DeleteArchivedDialog dialog={dialog} withThem={withThem} targets={targets} onClose={onClose} />;
    }
  }
}

