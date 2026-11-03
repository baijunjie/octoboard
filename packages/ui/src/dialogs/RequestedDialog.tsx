import React from "react";

import { RenameDialog } from "../components/dialog/RenameDialog";
import { PROJECT_NAME_LIMIT } from "../components/form/textLength";
import { ImportConfigurationDialog } from "../configuration/ImportConfigurationDialog";
import { useT } from "../i18n/react";
import type { Session } from "../protocol";
import { useDaemon, useOwnerSessions, useSession } from "../store";
import { ArchiveSessionDialog } from "./confirmations/ArchiveSessionDialog";
import { DeleteArchivedDialog } from "./confirmations/DeleteArchivedDialog";
import { DeleteConsoleDialog } from "./confirmations/DeleteConsoleDialog";
import { DeleteProjectDialog } from "./confirmations/DeleteProjectDialog";
import { DeleteSessionDialog } from "./confirmations/DeleteSessionDialog";
import { RestoreProjectChangesDialog } from "./confirmations/RestoreProjectChangesDialog";
import { UnbindSessionDialog } from "./confirmations/UnbindSessionDialog";
import { ConsoleDialog } from "./ConsoleDialog";
import type { DialogRequest } from "./dialogRequest";
import { HandoffDialog } from "./HandoffDialog";
import { ProjectDialog } from "./ProjectDialog";
import { SessionDialog } from "./SessionDialog";
import { SessionModelDialog, type ModelEdits } from "./SessionModelDialog";
import { SwitchAccountDialog } from "./SwitchAccountDialog";

/** The session dialog over the console's console sessions, read from the store: the owners it offers.
 * The list is the same array until one of them changes. */
function SessionDialogOverOwners(props: Omit<React.ComponentProps<typeof SessionDialog>, "sessions">): React.ReactElement {
  const owners = useOwnerSessions(props.console.id);
  return <SessionDialog {...props} sessions={owners} />;
}

/** Renders `children` with the session `requested` names as the store holds it now, `requested`
 * itself once the store has dropped it. */
function LiveSession({ requested, children }: { requested: Session; children: (session: Session) => React.ReactElement }): React.ReactElement {
  return children(useSession(requested.id) ?? requested);
}

/** The dialog a sidebar menu asked for. `onClose` is called once the dialog is done or dismissed;
 * `onSessionOpened` receives the session a "new session" dialog started, or a handoff's receiving session. */
export function RequestedDialog({
  dialog,
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
  /** Starts changing a session's model and reasoning effort in the background and returns at once; the
   * outcome, a success announced or a failure reported as an error alert, comes once it is known. */
  onChangeSessionModel: (sessionId: string, edits: ModelEdits) => void;
  /** Resumes a session the session dialog found, an archived one (reopening it) or an interrupted one;
   * rejects with the reason when it did not happen. */
  onResumeSession: (sessionId: string) => Promise<void>;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();

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
      return <RestoreProjectChangesDialog project={dialog.project} onClose={onClose} />;
    case "delete-project":
      return <DeleteProjectDialog project={dialog.project} onClose={onClose} />;
    case "new-console-session":
      return <SessionDialogOverOwners console={dialog.console} onClose={onClose} onOpened={onSessionOpened} onResume={onResumeSession} />;
    case "new-session":
      return (
        <SessionDialogOverOwners
          console={dialog.console}
          project={dialog.project}
          binding={dialog.binding}
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
    case "archive-session":
      return <ArchiveSessionDialog requested={dialog.session} inspection={dialog.inspection} onClose={onClose} />;
    case "switch-account":
      return (
        <LiveSession requested={dialog.session}>
          {(session) => (
            <SwitchAccountDialog
              session={session}
              account={dialog.account}
              accountName={dialog.accountName}
              onClose={onClose}
              onSwitchAccounts={onSwitchAccounts}
            />
          )}
        </LiveSession>
      );
    case "session-model":
      return (
        <LiveSession requested={dialog.session}>
          {(session) => <SessionModelDialog session={session} onClose={onClose} onChangeModel={onChangeSessionModel} />}
        </LiveSession>
      );
    case "handoff":
      return (
        <LiveSession requested={dialog.session}>
          {(session) => <HandoffDialog session={session} onClose={onClose} onHandedOff={onSessionOpened} />}
        </LiveSession>
      );
    case "unbind-session":
      return <UnbindSessionDialog sessionId={dialog.session.id} onClose={onClose} />;
    case "delete-session":
      return <DeleteSessionDialog requested={dialog.session} onClose={onClose} />;
    case "delete-archived":
      return <DeleteArchivedDialog dialog={dialog} onClose={onClose} />;
  }
}
