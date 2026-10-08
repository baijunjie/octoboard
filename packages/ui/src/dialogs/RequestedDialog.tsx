import { Alert } from "@heroui/react";
import React from "react";

import { AgentIcon } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { StatusIcon } from "../components/StatusIcon";
import { useT } from "../i18n/react";
import { isLive, type Session } from "../protocol";
import { sessionAgentLabel } from "../sessionLabel";
import { boundArchivedSessions, boundSessions, compareSessions } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConsoleDialog } from "./ConsoleDialog";
import type { DialogRequest } from "./dialogRequest";
import { ProjectDialog } from "./ProjectDialog";
import { RenameDialog } from "./RenameDialog";
import { SessionDialog } from "./SessionDialog";

/** A warning callout for what a confirmation is about to take along, over the list of sessions when
 * there is one to show: each with its status glyph, its agent's icon and its title. HeroUI's own
 * warning tint rather than its default surface, which is the dialog's own fill and leaves the
 * callout unmarked. */
function SessionCallout({ title, sessions }: { title: string; sessions?: Session[] }): React.ReactElement {
  const t = useT();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  return (
    <Alert status="warning" className="bg-warning-soft shadow-none">
      <Alert.Indicator />
      <Alert.Content className="min-w-0">
        <Alert.Title>{title}</Alert.Title>
        {/* The list is a sibling of the description rather than inside it: the description is a
            `<span>`, which cannot hold a list. */}
        {sessions && (
          <ul className="mt-1 flex min-w-0 flex-col gap-1 self-stretch text-sm text-foreground">
            {sessions.map((session) => (
              <li key={session.id} className="flex min-w-0 items-center gap-2">
                <StatusIcon status={session.status} />
                <AgentIcon agent={session.agent} />
                {/* The icon is decorative, and nothing else here names the agent. */}
                <span className="sr-only">{sessionAgentLabel(t, session, accounts)}</span>
                <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={session.title}>
                  {session.title}
                </FadeOverflow>
              </li>
            ))}
          </ul>
        )}
      </Alert.Content>
    </Alert>
  );
}

/** The dialog a sidebar menu asked for. `onClose` is called once the dialog is done or dismissed;
 * `onSessionOpened` receives the session a "new session" dialog started. */
export function RequestedDialog({
  dialog,
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
  const { request } = useDaemon();
  const sessions = useDaemonStore((s) => s.sessions);

  switch (dialog.kind) {
    case "new-console":
      return <ConsoleDialog onClose={onClose} />;
    case "edit-console":
      return <ConsoleDialog console={dialog.console} onClose={onClose} />;
    case "delete-console":
      return (
        <ConfirmDialog
          title={t("dialog.deleteConsole.title", { name: dialog.console.name })}
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
    case "delete-project": {
      const project = dialog.project;
      // Read live: a session that starts or stops while the dialog is open joins or leaves the list.
      const running = Array.from(sessions.values())
        .filter((s) => s.project_id === project.id && isLive(s.status))
        .sort(compareSessions);
      return (
        <ConfirmDialog
          title={t("dialog.removeProject.title", { name: project.name })}
          message={
            <>
              <p>{t("dialog.removeProject.message")}</p>
              {running.length > 0 && <SessionCallout title={t("dialog.removeProject.running", { count: running.length })} sessions={running} />}
            </>
          }
          confirmLabel={t(running.length > 0 ? "dialog.removeProject.confirmRunning" : "common.remove")}
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "delete_project", project: project.id, stop_sessions: running.length > 0 });
            onClose();
          }}
        />
      );
    }
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
      // The sessions bound to a console session that go into the archive with it. Read live, like
      // the running ones above. One that has a process running is not among them: the daemon
      // refuses the archive while there is such a session, and the dialog shows that refusal.
      const withIt = dialog.session.role === "console" ? boundSessions(Array.from(sessions.values()), dialog.session.id).filter((s) => s.status === "interrupted") : [];
      return (
        <ConfirmDialog
          title={t("dialog.archiveSession.title", { title: dialog.session.title })}
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
        <ConfirmDialog
          title={t("dialog.switchAccount.title", { title: dialog.session.title, account: dialog.accountName })}
          message={t(dialog.session.has_conversation ? "dialog.switchAccount.message" : "dialog.switchAccount.messageFresh")}
          confirmLabel={t("dialog.switchAccount.confirm")}
          pendingLabel={t("dialog.switchAccount.pending")}
          onCancel={onClose}
          onConfirm={async () => {
            await onSwitchAccount(dialog.session.id, dialog.account);
            onClose();
          }}
        />
      );
    case "delete-session": {
      // An archived console session takes the archived sessions bound to it along.
      const withIt = dialog.session.role === "console" ? boundArchivedSessions(Array.from(sessions.values()), dialog.session.id) : [];
      return (
        <ConfirmDialog
          title={t("dialog.deleteSession.title", { title: dialog.session.title })}
          message={
            <>
              <p>{t("dialog.deleteSession.message")}</p>
              {withIt.length > 0 && <SessionCallout title={t("dialog.deleteSession.bound", { count: withIt.length })} sessions={withIt} />}
            </>
          }
          confirmLabel={t("common.delete")}
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "delete_session", session: dialog.session.id });
            onClose();
          }}
        />
      );
    }
    case "delete-archived": {
      // All of a console's archived console sessions take their archived bound sessions with them;
      // a project's archive, and a console session's own, do not reach any console session.
      const consoleSessions = dialog.project || dialog.consoleSession
        ? []
        : Array.from(sessions.values()).filter((s) => s.console_id === dialog.console.id && s.role === "console" && s.status === "archived");
      const withThem = consoleSessions.flatMap((owner) => boundArchivedSessions(Array.from(sessions.values()), owner.id)).length;
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
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({
              type: "delete_archived_sessions",
              console: dialog.console.id,
              project: dialog.project?.id,
              console_session: dialog.consoleSession?.id,
            });
            onClose();
          }}
        />
      );
    }
  }
}

