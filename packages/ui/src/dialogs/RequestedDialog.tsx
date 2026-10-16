import { Alert } from "@heroui/react";
import React, { useRef, useState } from "react";

import { useFocusHandoff } from "../components/useFocusHandoff";
import { AgentIcon } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { StatusIcon } from "../components/StatusIcon";
import { TintedAlert } from "../components/TintedAlert";
import { ImportConfigurationDialog } from "../configuration/ImportConfigurationDialog";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { isLive, type Project, type Session } from "../protocol";
import { sessionAccountTooltip, sessionAgentLabel, sessionDisplayTitle } from "../sessionLabel";
import { boundArchivedSessions, boundSessionsArchivedWith, compareSessions } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConsoleDialog } from "./ConsoleDialog";
import type { DialogRequest } from "./dialogRequest";
import { ProjectDialog } from "./ProjectDialog";
import { RenameDialog } from "./RenameDialog";
import { SessionDialog } from "./SessionDialog";
import { SwitchAccountDialog } from "./SwitchAccountDialog";
import { TrashOption } from "./TrashOption";
import { useSessionRecords } from "./useSessionRecords";

/** A warning callout for what a confirmation is about to take along, over the list of sessions when
 * there is one to show: each with its agent's icon and title, plus a status glyph for live sessions.
 * Archived sessions are already identified by the warning's wording. HeroUI's own
 * warning tint rather than its default surface, which is the dialog's own fill and leaves the
 * callout unmarked. */
function SessionCallout({ title, sessions }: { title: string; sessions?: Session[] }): React.ReactElement {
  const t = useT();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  return (
    <TintedAlert status="warning" className="shadow-none">
      <Alert.Indicator />
      <Alert.Content className="min-w-0">
        <Alert.Title>{title}</Alert.Title>
        {/* The list is a sibling of the description rather than inside it: the description is a
            `<span>`, which cannot hold a list. */}
        {sessions && (
          <ul className="mt-1 flex min-w-0 flex-col gap-1 self-stretch text-sm text-foreground">
            {sessions.map((session) => (
              <li key={session.id} className="flex min-w-0 items-center gap-2">
                {session.status !== "archived" && <StatusIcon status={session.status} withTooltip />}
                <AgentIcon agent={session.agent} tooltip={sessionAccountTooltip(t, session, accounts)} />
                {/* The icon is decorative, and nothing else here names the agent. */}
                <span className="sr-only">{sessionAgentLabel(t, session, accounts)}</span>
                <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={sessionDisplayTitle(t, session)}>
                  {sessionDisplayTitle(t, session)}
                </FadeOverflow>
              </li>
            ))}
          </ul>
        )}
      </Alert.Content>
    </TintedAlert>
  );
}

/** Confirms removing a project. The folder stays on disk unless the user opts in to moving it to
 * the Trash, which the daemon does before removing the project and refuses (keeping the project)
 * when it cannot. */
function DeleteProjectDialog({ project, running, onClose }: { project: Project; running: Session[]; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [trash, setTrash] = useState(false);
  const confirmKey =
    running.length > 0
      ? trash
        ? "dialog.removeProject.confirmRunningTrash"
        : "dialog.removeProject.confirmRunning"
      : trash
        ? "dialog.removeProject.confirmTrash"
        : "common.remove";
  return (
    <ConfirmDialog
      title={t("dialog.removeProject.title", { name: isolate(project.name) })}
      message={
        <>
          <p>{t(trash ? "dialog.removeProject.messageTrash" : "dialog.removeProject.message")}</p>
          {running.length > 0 && <SessionCallout title={t("dialog.removeProject.running", { count: running.length })} sessions={running} />}
        </>
      }
      confirmLabel={t(confirmKey)}
      pendingLabel={trash ? t("dialog.removeProject.pendingTrash") : undefined}
      destructive
      extraControl={(busy) => (
        <TrashOption isSelected={trash} onChange={setTrash} isReadOnly={busy} label={t("dialog.removeProject.trash")} hint={t("dialog.removeProject.trashHint")} />
      )}
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "delete_project", project: project.id, stop_sessions: running.length > 0, trash_directory: trash });
        onClose();
      }}
    />
  );
}

/** Keeps focus inside the confirmation when a completed inspection removes its cleanup option. */
function SessionRecordsTrashOption({ visible, ...props }: React.ComponentProps<typeof TrashOption> & { visible: boolean }): React.ReactElement {
  const ref = useRef<HTMLDivElement>(null);
  const handoff = useFocusHandoff(() => ref.current?.closest<HTMLElement>("[role=alertdialog]")?.focus());
  return <div ref={ref} className="contents" {...handoff}>{visible && <TrashOption {...props} />}</div>;
}

/** The confirmation of a session's deletion, with the option of also moving the agent's own
 * records of it to the Trash. `withIt` are the archived sessions that go along. */
function DeleteSessionDialog({ session, withIt, unused, onClose }: { session: Session; withIt: Session[]; unused: boolean; onClose: () => void }): React.ReactElement | null {
  const t = useT();
  const { request } = useDaemon();
  const [trash, setTrash] = useState(false);
  const { pending, presence } = useSessionRecords([session.id, ...withIt.map((s) => s.id)], unused ? session.id : undefined, onClose);
  const canTrash = presence !== "absent";
  const moveRecords = canTrash && trash;
  if (unused && pending) return null;
  return (
    <ConfirmDialog
      title={t("dialog.deleteSession.title", { title: isolate(sessionDisplayTitle(t, session)) })}
      message={
        <>
          <p>{t("dialog.deleteSession.message")}</p>
          {withIt.length > 0 && <SessionCallout title={t("dialog.deleteSession.bound", { count: withIt.length })} sessions={withIt} />}
        </>
      }
      confirmLabel={t("common.delete")}
      pendingLabel={moveRecords ? t("dialog.removeProject.pendingTrash") : undefined}
      destructive
      extraControl={(busy) => (
        <SessionRecordsTrashOption
          visible={canTrash}
          isSelected={trash}
          onChange={setTrash}
          isReadOnly={busy}
          label={t("dialog.deleteSession.trashRecords")}
          hint={t(withIt.length > 0 ? "dialog.deleteSession.trashRecordsHintBound" : "dialog.deleteSession.trashRecordsHint")}
        />
      )}
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "delete_session", session: session.id, trash_agent_records: moveRecords });
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
  const { presence } = useSessionRecords(targets.map((s) => s.id), undefined, onClose);
  const canTrash = presence !== "absent";
  const moveRecords = canTrash && trash;
  return (
    <ConfirmDialog
      title={t("dialog.deleteArchived.title", { count: dialog.count })}
      message={
        <>
          <p>{t("dialog.deleteArchived.message", { count: dialog.count })}</p>
          {withThem > 0 && <SessionCallout title={t("dialog.deleteArchived.bound", { count: withThem })} />}
        </>
      }
      confirmLabel={t("dialog.deleteArchived.confirm")}
      pendingLabel={moveRecords ? t("dialog.removeProject.pendingTrash") : undefined}
      destructive
      extraControl={(busy) => (
        <SessionRecordsTrashOption
          visible={canTrash}
          isSelected={trash}
          onChange={setTrash}
          isReadOnly={busy}
          label={t("dialog.deleteArchived.trashRecords")}
          hint={t("dialog.deleteArchived.trashRecordsHint")}
        />
      )}
      onCancel={onClose}
      onConfirm={async () => {
        await request({
          type: "delete_archived_sessions",
          console: dialog.console.id,
          project: dialog.project?.id,
          console_session: dialog.consoleSession?.id,
          trash_agent_records: moveRecords,
        });
        onClose();
      }}
    />
  );
}

/** The dialog a sidebar menu asked for. `onClose` is called once the dialog is done or dismissed;
 * `onSessionOpened` receives the session a "new session" dialog started. */
export function RequestedDialog({
  dialog: requestedDialog,
  onClose,
  onSessionOpened,
  onSwitchAccount,
}: {
  dialog: DialogRequest;
  onClose: () => void;
  onSessionOpened: (sessionId: string) => void;
  /** Switches a session's account (`null`, the default account); resolves once the daemon has
   * the result, and rejects with the reason when the switch did not happen. */
  onSwitchAccount: (sessionId: string, account: string | null) => Promise<void>;
}): React.ReactElement {
  const t = useT();
  const { request, toastNotice } = useDaemon();
  const sessions = useDaemonStore((s) => s.sessions);
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
          title={t("dialog.deleteConsole.title", { name: isolate(dialog.console.name) })}
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
          title={t("dialog.restoreProjectChanges.title", { name: isolate(dialog.project.name) })}
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
        .sort(compareSessions);
      return <DeleteProjectDialog project={dialog.project} running={running} onClose={onClose} />;
    }
    case "new-console-session":
      return (
        <SessionDialog
          console={dialog.console}
          sessions={Array.from(sessions.values())}
          onClose={onClose}
          onOpened={onSessionOpened}
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
        />
      );
    case "rename-project":
      return (
        <RenameDialog
          title={t("dialog.renameProject.title")}
          label={t("common.name")}
          initialValue={dialog.project.name}
          requiredMessage={t("dialog.renameProject.nameRequired")}
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
          requiredMessage={t("dialog.rename.titleRequired")}
          onSubmit={async (title) => {
            await request({ type: "rename_session", session: dialog.session.id, title });
          }}
          onClose={onClose}
        />
      );
    case "archive-session": {
      // The sessions under an owner (bound to it, or to one bound to it) that go into the
      // archive with it. Read live, like the running ones above. One that has a process running
      // is not among them: the daemon refuses the archive while there is such a session, and the
      // dialog shows that refusal.
      const withIt = boundSessionsArchivedWith(Array.from(sessions.values()), dialog.session.id);
      return (
        <ConfirmDialog
          title={t("dialog.archiveSession.title", { title: isolate(sessionDisplayTitle(t, dialog.session)) })}
          message={
            <>
              <p>{t("dialog.archiveSession.message")}</p>
              {withIt.length > 0 && <SessionCallout title={t("dialog.archiveSession.bound", { count: withIt.length })} sessions={withIt} />}
            </>
          }
          confirmLabel={t("dialog.archiveSession.confirm")}
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "archive_session", session: dialog.session.id });
            onClose();
          }}
        />
      );
    }
    case "switch-account":
      return (
        <SwitchAccountDialog
          session={dialog.session}
          account={dialog.account}
          accountName={dialog.accountName}
          onClose={onClose}
          onSwitchAccount={onSwitchAccount}
        />
      );
    case "delete-session": {
      // An archived owner takes the archived sessions bound to it, and those bound to those, along.
      const withIt = boundArchivedSessions(Array.from(sessions.values()), dialog.session.id);
      const unused = !dialog.session.has_conversation && !dialog.session.bound_to
        && !Array.from(sessions.values()).some((s) => s.bound_to === dialog.session.id);
      return <DeleteSessionDialog session={dialog.session} withIt={withIt} unused={unused} onClose={onClose} />;
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

