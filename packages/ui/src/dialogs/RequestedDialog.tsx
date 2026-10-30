import React, { useEffect, useId, useRef, useState } from "react";

import { ErrorAlert } from "../components/ErrorAlert";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { errorDetails } from "../components/errorParts";
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
import { isLive, SESSION_BINDING_CHANGED, SESSION_NOT_BOUND, type Console as ConsoleRecord, type Project, type Session } from "../protocol";
import { sessionAgentLabel, sessionDisplayTitle, sessionShownStatusKind, sessionStatusKind } from "../sessionLabel";
import {
  boundArchivedSessions,
  boundSessionsArchivedWith,
  compareSessions,
  hasBoundSessions,
  isUnusedSession,
  ordersByStatus,
} from "../sidebar/order";
import { useProjectProcesses } from "../process/useProjectProcesses";
import { useDaemon, useDaemonStore } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConsoleDialog } from "./ConsoleDialog";
import { HandoffDialog } from "./HandoffDialog";
import type { DialogRequest } from "./dialogRequest";
import { ProjectDialog } from "./ProjectDialog";
import { RenameDialog } from "./RenameDialog";
import { SessionDialog } from "./SessionDialog";
import { SessionModelDialog, type ModelEdits } from "./SessionModelDialog";
import { SwitchAccountDialog } from "./SwitchAccountDialog";
import { useBranchCleanup, useProjectRemovalCleanup, type ConsequenceLine } from "./BranchCleanupOptions";
import { ConfirmOption } from "./ConfirmOption";
import { KeepFocusInDialog } from "./KeepFocusInDialog";
import { useSessionRecords } from "./useSessionRecords";
import { StoppedRow, useProcessStopping } from "./ProcessStopOptions";
import { isRemovalRefusal, useArchiveInspection, useSessionBranches, type SessionInspection } from "./useSessionBranches";
import { PROJECT_NAME_LIMIT } from "./textLength";

/** The sessions a confirmation names, each with its agent's icon and title, plus a status glyph for live
 * sessions; archived sessions are already identified by the wording of what heads the list. */
function SessionList({ sessions }: { sessions: Session[] }): React.ReactElement {
  const t = useT();
  const sessionsRef = useListMotion<HTMLUListElement>();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  return (
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
  );
}

/** The consequence lines of what is ticked, one paragraph each. Always mounted, so a line added or
 * removed by an option moves in and out. */
function ConsequenceLines({ lines }: { lines: ConsequenceLine[] }): React.ReactElement {
  const linesRef = useListMotion<HTMLDivElement>();
  return (
    <div ref={linesRef} className="relative flex min-w-0 flex-col gap-1">
      {lines.map((line) => <p key={line.key} data-list-motion={line.key}>{line.text}</p>)}
    </div>
  );
}

/** What a confirmation says it is about to take along or take away, as body text under its options:
 * `title` when there is one, over the list of sessions when there is one to show (`SessionList`),
 * and `lines`: further consequences, such as one for each option the user has ticked.
 * Nothing at all when there is neither a title nor a line. */
function ConfirmConsequences({ title, sessions, lines = [] }: { title?: string; sessions?: Session[]; lines?: ConsequenceLine[] }): React.ReactElement | null {
  if (!title && lines.length === 0) return null;
  return (
    <div className="flex min-w-0 flex-col gap-2 text-sm">
      {title && <p>{title}</p>}
      {sessions && <SessionList sessions={sessions} />}
      <ConsequenceLines lines={lines} />
    </div>
  );
}

/** One topic of a sectioned confirmation (the archive's, the project removal's): a labelled group whose `<h3>` heading names it (the
 * dialog's own title is the `<h2>`), under a groove divider when `divided`, i.e. when another section is
 * shown above it. `shown` false renders nothing but keeps the wrapper mounted, so focus that was
 * inside when the section went away goes back to the dialog. */
function ConfirmSection({ heading, shown, divided, children }: {
  heading: string;
  shown: boolean;
  divided: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  const headingId = useId();
  return (
    <KeepFocusInDialog>
      {shown && (
        <div role="group" aria-labelledby={headingId} className={`flex min-w-0 flex-col gap-2 text-sm${divided ? " groove-t pt-3" : ""}`}>
          <h3 id={headingId} className="font-semibold">{heading}</h3>
          {children}
        </div>
      )}
    </KeepFocusInDialog>
  );
}

/** The confirmation section of the project processes a removal stops, one `StoppedRow` each and the count. It is
 * shown when there are any, or when a list could not be read: then a warning Alert in its place says so, since the
 * processes are stopped all the same. */
function ProjectProcessesSection({ processes, divided }: { processes: ReturnType<typeof useProjectProcesses>; divided: boolean }): React.ReactElement {
  const t = useT();
  const { trees, error } = processes;
  return (
    <ConfirmSection heading={t("dialog.projectProcesses.heading")} shown={trees.length > 0 || error !== undefined} divided={divided}>
      {error && (
        <ErrorAlert status="warning" title={t(trees.length > 0 ? "dialog.projectProcesses.listPartlyFailedTitle" : "dialog.projectProcesses.listFailedTitle")} details={errorDetails(error)} announce announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}>
          {t("dialog.projectProcesses.listFailed")}
        </ErrorAlert>
      )}
      {trees.length > 0 && (
        <>
          <div className="flex min-w-0 flex-col gap-2">
            {trees.map((tree) => <StoppedRow key={tree.id} tree={tree} />)}
          </div>
          {/* After a failed list the count would read as the total, while the warning says unlisted ones are stopped too. */}
          {error === undefined && <ConsequenceLines lines={[{ key: "projectProcesses", text: t("dialog.projectProcesses.stopped", { count: trees.length }) }]} />}
        </>
      )}
    </ConfirmSection>
  );
}

/** Confirms deleting a console, which the daemon refuses while a session of it runs, so the one thing it lists is
 * what the deletion stops besides: the project processes of every project under the console. */
function DeleteConsoleDialog({ console: target, onClose }: { console: ConsoleRecord; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const projects = useDaemonStore((s) => s.projects);
  const projectProcesses = useProjectProcesses(Array.from(projects.values()).filter((project) => project.console_id === target.id).map((project) => project.id));
  return (
    <ConfirmDialog
      title={t("dialog.deleteConsole.title")}
      errorTitle={t("dialog.deleteConsole.failedTitle")}
      subject={<Message id="dialog.deleteConsole.subject" params={{ name: <InlineReference dir="auto" wrap>{target.name}</InlineReference> }} />}
      message={t("dialog.deleteConsole.message")}
      extraControl={<ProjectProcessesSection processes={projectProcesses} divided={false} />}
      confirmLabel={t("common.delete")}
      waiting={projectProcesses.pending}
      typeToConfirm={t("dialog.typeToConfirm.delete")}
      destructive
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "delete_console", console: target.id });
        onClose();
      }}
    />
  );
}

/** Confirms removing a project. Its project processes are stopped with it, listed in a section of their own.
 * The folder stays on disk unless the user opts in to moving it to
 * the Trash, which the daemon does before removing the project and refuses (keeping the project)
 * when it cannot. With the folder go the worktrees outside it that the daemon lists and, where the
 * repository outlives the folder, the branches of the project's sessions, listed once the option is
 * chosen; a locked worktree only when the user also says so, with the branch it has checked out. */
function DeleteProjectDialog({ project, running, onClose }: { project: Project; running: Session[]; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [trash, setTrash] = useState(false);
  const cleanup = useProjectRemovalCleanup(project.id, trash, setTrash);
  const projectProcesses = useProjectProcesses([project.id]);
  const showRunning = running.length > 0;
  return (
    <ConfirmDialog
      title={t("dialog.removeProject.title")}
      errorTitle={t("dialog.removeProject.failedTitle")}
      subject={<Message id="dialog.removeProject.subject" params={{ name: <InlineReference dir="auto" wrap>{project.name}</InlineReference> }} />}
      message={t(trash ? "dialog.removeProject.messageTrash" : "dialog.removeProject.message")}
      confirmLabel={t(running.length > 0 ? "dialog.removeProject.confirmRunning" : "common.remove")}
      destructive
      waiting={cleanup.pending || projectProcesses.pending}
      extraControl={(busy) => (
        <>
          <ConfirmSection heading={t("dialog.removeProject.runningHeading", { count: running.length })} shown={showRunning} divided={false}>
            <SessionList sessions={running} />
          </ConfirmSection>
          <ConfirmSection heading={t("dialog.removeProject.folderHeading")} shown divided={showRunning}>
            {cleanup.extraControl(busy)}
            <ConsequenceLines lines={cleanup.consequences} />
          </ConfirmSection>
          <ProjectProcessesSection processes={projectProcesses} divided />
        </>
      )}
      onCancel={onClose}
      onConfirm={async () => {
        try {
          await request({
            type: "delete_project",
            project: project.id,
            stop_sessions: running.length > 0,
            trash_directory: trash,
            ...cleanup.choices,
          });
        } catch (err) {
          // What goes along is asked again, and Confirm waits on it, so a retry carries out what is shown then.
          if (trash && isRemovalRefusal(err)) cleanup.reinspect();
          throw err;
        }
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
 * and worktrees git refuses to remove with the choice of forcing them, and the processes the
 * archive stops with the choice of keeping any of them running, each as a section of its own with
 * the consequences of its options right under them. */
function ArchiveSessionDialog({ session, withIt, inspection, onClose }: {
  session: Session;
  withIt: Session[];
  /** An inspection already made for this archive. */
  inspection?: SessionInspection;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const inspected = useArchiveInspection([session.id, ...withIt.map((s) => s.id)], inspection);
  const cleanup = useBranchCleanup(inspected);
  const processes = useProcessStopping(inspected.processes);
  const showBound = withIt.length > 0;
  const showCleanup = cleanup.offered;
  const showProcesses = inspected.processes.length > 0;
  return (
    <ConfirmDialog
      title={t("dialog.archiveSession.title")}
      subject={<Message id="dialog.archiveSession.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />}
      message={t(sessionStatusKind(session) === "background" ? "dialog.archiveSession.messageBackground" : "dialog.archiveSession.message")}
      // Nothing in it: the sections are in `extraControl`, bound sessions first. It stays defined for the warning icon.
      consequences={null}
      confirmLabel={t(cleanup.forcing ? "dialog.archiveSession.confirmDelete" : "dialog.archiveSession.confirm")}
      destructive={cleanup.forcing}
      waiting={cleanup.pending}
      extraControl={(busy) => (
        <>
          <ConfirmSection heading={t("dialog.archiveSession.boundHeading", { count: withIt.length })} shown={showBound} divided={false}>
            <SessionList sessions={withIt} />
          </ConfirmSection>
          <ConfirmSection heading={t("dialog.archiveSession.branchesHeading")} shown={showCleanup} divided={showBound}>
            {cleanup.extraControl(busy)}
            <ConsequenceLines lines={cleanup.consequences} />
          </ConfirmSection>
          <ConfirmSection heading={t("dialog.archiveSession.processesHeading")} shown={showProcesses} divided={showBound || showCleanup}>
            {processes.extraControl(busy)}
            <ConsequenceLines lines={processes.consequences} />
          </ConfirmSection>
        </>
      )}
      errorTitle={t("dialog.archiveSession.failedTitle")}
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "archive_session", session: session.id, ...cleanup.choices, ...processes.choices });
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
      subject={<Message id="dialog.unbindSession.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference>, owner: ownerName }} />}
      message={t("dialog.unbindSession.answersToYou")}
      consequences={<ConfirmConsequences lines={[{ key: "owner", text: t("dialog.unbindSession.consequence") }]} />}
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
  const t = useT();
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
      toastError(errorPresentation(err as Error), sessionId, t("dialog.deleteSession.failedTitle"));
      close();
    });
    return () => { active = false; };
  }, [sessionId, request, toastError, close, t]);
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
  const cleanup = useBranchCleanup(useSessionBranches(scope));
  const canTrash = presence !== "absent";
  const moveRecords = canTrash && trash;
  return (
    <ConfirmDialog
      title={t("dialog.deleteSession.title")}
      errorTitle={t("dialog.deleteSession.failedTitle")}
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
  const cleanup = useBranchCleanup(useSessionBranches(targets.map((s) => s.id)));
  const canTrash = presence !== "absent";
  const moveRecords = canTrash && trash;
  return (
    <ConfirmDialog
      title={t("dialog.deleteArchived.title")}
      errorTitle={t("dialog.deleteArchived.failedTitle")}
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
  onChangeSessionModel,
  onResumeSession,
}: {
  dialog: DialogRequest;
  onClose: () => void;
  onSessionOpened: (sessionId: string) => void;
  /** Starts switching sessions to an account (`null`, the default account; `accountName` its name) in
   * the background, and returns at once; the outcome is reported once the whole batch is done. */
  onSwitchAccounts: (sessionIds: string[], account: string | null, accountName: string) => void;
  /** Starts changing a session's model and reasoning effort in the background and returns at once; a
   * failure is reported once it comes. */
  onChangeSessionModel: (sessionId: string, edits: ModelEdits) => void;
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
      return <DeleteConsoleDialog console={dialog.console} onClose={onClose} />;
    case "new-project":
      return <ProjectDialog consoleId={dialog.console.id} onClose={onClose} />;
    case "edit-project":
      return <ProjectDialog consoleId={dialog.project.console_id} project={dialog.project} onClose={onClose} />;
    case "restore-project-changes":
      return (
        <ConfirmDialog
          title={t("dialog.restoreProjectChanges.title")}
          errorTitle={t("dialog.restoreProjectChanges.failedTitle")}
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
          errorTitle={t("dialog.renameProject.failedTitle")}
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
          errorTitle={t("dialog.rename.failedTitle")}
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
      return <ArchiveSessionDialog session={session} withIt={withIt} inspection={dialog.inspection} onClose={onClose} />;
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
    case "session-model":
      return <SessionModelDialog session={dialog.session} onClose={onClose} onChangeModel={onChangeSessionModel} />;
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

