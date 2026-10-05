import React from "react";

import { useDaemon } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConsoleDialog } from "./ConsoleDialog";
import type { DialogRequest } from "./dialogRequest";
import { ProjectDialog } from "./ProjectDialog";
import { RenameSessionDialog } from "./RenameSessionDialog";
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
  const { request } = useDaemon();

  switch (dialog.kind) {
    case "new-console":
      return <ConsoleDialog onClose={onClose} />;
    case "edit-console":
      return <ConsoleDialog console={dialog.console} onClose={onClose} />;
    case "delete-console":
      return (
        <ConfirmDialog
          title="Delete console"
          message={`Delete "${dialog.console.name}"? This only works while it has no live sessions.`}
          confirmLabel="Delete"
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
    case "delete-project":
      return (
        <ConfirmDialog
          title="Remove project"
          message={`Remove "${dialog.project.name}" from its console? This only removes the association and never touches the directory, but it deletes this project's sessions — archived ones included — and only works while none of them is still running.`}
          confirmLabel="Remove"
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "delete_project", project: dialog.project.id });
            onClose();
          }}
        />
      );
    case "new-session":
      return (
        <SessionDialog
          console={dialog.console}
          project={dialog.project}
          onClose={onClose}
          onOpened={onSessionOpened}
        />
      );
    case "rename-session":
      return <RenameSessionDialog session={dialog.session} onClose={onClose} />;
    case "archive-session":
      return (
        <ConfirmDialog
          title="Archive session"
          message={`Archive "${dialog.session.title}"? Its process will end; archived sessions can be reopened later.`}
          confirmLabel="Archive"
          destructive
          onCancel={onClose}
          onConfirm={async () => {
            await request({ type: "archive_session", session: dialog.session.id });
            onClose();
          }}
        />
      );
  }
}
