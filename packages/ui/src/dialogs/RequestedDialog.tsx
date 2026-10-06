import { Alert } from "@heroui/react";
import React from "react";

import { AgentIcon } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { StatusIcon } from "../components/StatusIcon";
import { useT } from "../i18n/react";
import { isLive } from "../protocol";
import { compareSessions } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConsoleDialog } from "./ConsoleDialog";
import type { DialogRequest } from "./dialogRequest";
import { ProjectDialog } from "./ProjectDialog";
import { RenameDialog } from "./RenameDialog";
import { SessionDialog } from "./SessionDialog";

/** The dialog a sidebar menu asked for. `onClose` is called once the dialog is done or dismissed;
 * `onSessionOpened` receives the session a "new session" dialog started. */
export function RequestedDialog({
  dialog,
  onClose,
  onSessionOpened,
}: {
  dialog: DialogRequest;
  onClose: () => void;
  onSessionOpened: (sessionId: string) => void;
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
              {running.length > 0 && (
                // HeroUI's own warning tint rather than its default surface, which is the dialog's
                // own fill and leaves the callout unmarked.
                <Alert status="warning" className="bg-warning-soft shadow-none">
                  <Alert.Indicator />
                  <Alert.Content className="min-w-0">
                    <Alert.Title>{t("dialog.removeProject.running", { count: running.length })}</Alert.Title>
                    <Alert.Description className="min-w-0 self-stretch text-foreground">
                      <ul className="mt-1 flex min-w-0 flex-col gap-1">
                        {running.map((session) => (
                          <li key={session.id} className="flex min-w-0 items-center gap-2">
                            <StatusIcon status={session.status} />
                            <AgentIcon agent={session.agent} />
                            <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={session.title}>
                              {session.title}
                            </FadeOverflow>
                          </li>
                        ))}
                      </ul>
                    </Alert.Description>
                  </Alert.Content>
                </Alert>
              )}
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
    case "archive-session":
      return (
        <ConfirmDialog
          title={t("dialog.archiveSession.title", { title: dialog.session.title })}
          message={t("dialog.archiveSession.message")}
          confirmLabel={t("dialog.archiveSession.confirm")}
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "archive_session", session: dialog.session.id });
            onClose();
          }}
        />
      );
    case "delete-session":
      return (
        <ConfirmDialog
          title={t("dialog.deleteSession.title", { title: dialog.session.title })}
          message={t("dialog.deleteSession.message")}
          confirmLabel={t("common.delete")}
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "delete_session", session: dialog.session.id });
            onClose();
          }}
        />
      );
    case "delete-archived":
      return (
        <ConfirmDialog
          title={t("dialog.deleteArchived.title", { count: dialog.count })}
          message={t("dialog.deleteArchived.message", { count: dialog.count })}
          confirmLabel={t("dialog.deleteArchived.confirm")}
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "delete_archived_sessions", console: dialog.console.id, project: dialog.project?.id });
            onClose();
          }}
        />
      );
  }
}
