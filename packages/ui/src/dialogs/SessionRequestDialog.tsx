import React from "react";

import { AGENT_LABEL } from "../agents";
import { DaemonRequestError } from "../daemon-client";
import { errorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { SESSION_REQUEST_ANSWERED, SESSION_REQUEST_NOT_WAITING } from "../protocol";
import { useDaemon, useDaemonStore, type SessionRequest } from "../store";
import type { ConfirmDialogProps } from "./ConfirmDialog";

/** What the dialog says: which session asks, and where it is, for a session of which agent where —
 * a project and its console, or a console — that would report to the user. It carries no text an
 * agent wrote: the confirmation is the one place the user's approval is enforced, so it says only
 * what Octoboard knows. The requester's place is named because it may belong to another project or
 * console than the one it asks in, which the user has no other way to see; its console is named
 * only where it is not the one the new session would be in, which the sentence names anyway. The names are the
 * user's own, and a long one may have no break opportunity, so the sentence wraps anywhere. Names
 * the store does not know stand in as their ids. */
function SessionRequestMessage({ request }: { request: SessionRequest }): React.ReactElement {
  const t = useT();
  const requesting = useDaemonStore((s) => s.sessions.get(request.session));
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  // Named rather than "this console": the window may be showing another one, and the session can
  // be asked for in any console.
  const console_ = consoles.get(request.console)?.name ?? request.console;
  const project = request.project ? (projects.get(request.project)?.name ?? request.project) : "";
  const session = isolate(requesting?.title ?? request.session);
  let requester: string;
  if (!requesting) requester = t("dialog.sessionRequest.requester.unknown", { session });
  else {
    const elsewhere = requesting.console_id !== request.console;
    const own = { session, console: isolate(consoles.get(requesting.console_id)?.name ?? requesting.console_id) };
    if (requesting.project_id) {
      const project = isolate(projects.get(requesting.project_id)?.name ?? requesting.project_id);
      requester = elsewhere
        ? t("dialog.sessionRequest.requester.projectElsewhere", { ...own, project })
        : t("dialog.sessionRequest.requester.project", { session, project });
    } else {
      requester = elsewhere
        ? t("dialog.sessionRequest.requester.consoleElsewhere", own)
        : t("dialog.sessionRequest.requester.console", { session });
    }
  }
  const params = { requester, console: isolate(console_), agent: AGENT_LABEL[request.agent] };
  return (
    <p className="[overflow-wrap:anywhere]">
      {request.role === "console"
        ? t("dialog.sessionRequest.question.console", params)
        : t("dialog.sessionRequest.question.project", { ...params, project: isolate(project) })}
    </p>
  );
}

/** What the dialog for a session's request to start a session with no owner shows and does, for
 * the request `PendingQuestionDialog` puts forward, or nothing without one. The daemon holds the
 * request, so the dialog moves on when the daemon says it stopped waiting, which an answer from
 * here or from any other client does at once. Dismissing it is a refusal. A hook rather than a
 * component, so the one dialog slot it shares with the trust prompts (`PendingQuestionDialog`)
 * stays mounted between them. */
export function useSessionRequestDialogProps(request: SessionRequest | undefined): ConfirmDialogProps | undefined {
  const t = useT();
  const { request: send, toastError } = useDaemon();
  if (!request) return undefined;

  // Never rejects: the dialog has usually moved on by the time an approval has been carried out,
  // so what the user has to know goes to a toast about the requesting session instead of into the
  // dialog. The window does not follow the session an approval starts: the user's attention stays
  // with the requesting session, which carries on, and the new session is not one the user asked
  // to open.
  const answer = async (approve: boolean): Promise<void> => {
    try {
      await send({ type: "answer_session_request", request_id: request.requestId, approve });
    } catch (err) {
      if (err instanceof DaemonRequestError && err.code === SESSION_REQUEST_ANSWERED) return;
      // Nothing was going to start for a refusal, so the session no longer waiting is no news.
      if (!approve && err instanceof DaemonRequestError && err.code === SESSION_REQUEST_NOT_WAITING) return;
      toastError(errorPresentation(err as Error), request.session);
    }
  };

  return {
    resetKey: `request:${request.requestId}`,
    title: t("dialog.sessionRequest.title"),
    message: <SessionRequestMessage request={request} />,
    confirmLabel: t("dialog.sessionRequest.confirm"),
    cancelLabel: t("dialog.sessionRequest.refuse"),
    onCancel: () => void answer(false),
    onConfirm: () => answer(true),
  };
}
