import React from "react";

import { DaemonRequestError } from "../daemon-client";
import { useT } from "../i18n/react";
import { CONSOLE_REQUEST_ANSWERED, CONSOLE_REQUEST_NOT_WAITING, type Session } from "../protocol";
import { useDaemon, useDaemonStore, type ConsoleRequest } from "../store";
import type { ConfirmDialogProps } from "./ConfirmDialog";

/** What the dialog says: which session asks, in which project, for a console session in which
 * console. What the session wrote is not shown: such a request is normally made at the user's own
 * instruction, and it goes to the console session as its first report. */
function ConsoleRequestMessage({ request }: { request: ConsoleRequest }): React.ReactElement {
  const t = useT();
  const session = useDaemonStore((s) => s.sessions.get(request.session)?.title) ?? request.session;
  const project =
    useDaemonStore((s) => (request.project ? s.projects.get(request.project)?.name : undefined)) ?? request.project ?? "";
  // Named rather than "this console": the window may be showing another one.
  const console_ = useDaemonStore((s) => s.consoles.get(request.console)?.name) ?? request.console;
  return <p>{t("dialog.consoleRequest.question", { session, project, console: console_ })}</p>;
}

/** What the dialog for an unbound project session's request for a console session shows and does,
 * for the oldest waiting one, or nothing without one. The daemon holds the request, so the dialog
 * moves on when the daemon says it stopped waiting, which an answer from here or from any other
 * client does at once. Dismissing it is a refusal. A hook rather than a component, so the one
 * dialog slot it shares with the trust prompts (`PendingQuestionDialog`) stays mounted between
 * them. `onStarted` is given the console session an approval started and the requesting
 * session's project. */
export function useConsoleRequestDialogProps(
  request: ConsoleRequest | undefined,
  onStarted: (consoleSession: Session, requestingProject: string | null) => void,
): ConfirmDialogProps | undefined {
  const t = useT();
  const { request: send, toastError } = useDaemon();
  if (!request) return undefined;

  // Never rejects: the dialog has usually moved on by the time an approval has been carried out,
  // so what the user has to know goes to a toast about the session instead of into the dialog.
  const answer = async (approve: boolean) => {
    try {
      const reply = await send({ type: "answer_console_session_request", request_id: request.requestId, approve });
      if (reply.type === "session_opened") onStarted(reply.session, request.project);
    } catch (err) {
      if (err instanceof DaemonRequestError && err.code === CONSOLE_REQUEST_ANSWERED) return;
      // Nothing was going to start for a refusal, so the session no longer waiting is no news.
      if (!approve && err instanceof DaemonRequestError && err.code === CONSOLE_REQUEST_NOT_WAITING) return;
      toastError((err as Error).message, request.session);
    }
  };

  return {
    resetKey: `request:${request.requestId}`,
    title: t("dialog.consoleRequest.title"),
    message: <ConsoleRequestMessage request={request} />,
    confirmLabel: t("dialog.consoleRequest.confirm"),
    cancelLabel: t("dialog.consoleRequest.refuse"),
    onCancel: () => void answer(false),
    onConfirm: () => answer(true),
  };
}
