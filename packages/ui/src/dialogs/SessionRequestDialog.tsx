import React, { useId, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { incompleteAccount } from "../accounts/choiceEntries";
import { CodedError } from "../errors/codedError";
import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { InlineReference } from "../i18n/InlineReference";
import { joinNodes } from "../i18n/joinPhrases";
import type { Language } from "../i18n/languages";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { PlainMessageKey } from "../i18n/catalog";
import { type Account, type SessionRequestAction } from "../protocol";
import { CODE } from "../errors/codes";
import { sessionAccount, sessionDisplayTitle } from "../sessions/sessionLabel";
import { isScheduledTaskRequest, useDaemon, useDaemonStore, type ScheduledTaskRequest, type SessionRequest } from "../store";
import { ProviderName } from "../components/text/ProviderName";
import { raiseError } from "../components/alerts/raiseError";
import { displayWirePath } from "../format/wirePath";
import type { ConfirmDialogProps } from "../components/dialog/ConfirmDialog";
import { dialogAround } from "../components/dialog/dialogElement";
import { useFailureWhenBlocked } from "../components/dialog/useFailureWhenBlocked";
import { RequestPrompt } from "./RequestPrompt";
import { ScheduledTaskRequestMessage } from "./ScheduledTaskRequestMessage";
import { recordAccount, recordReference, requesterOf } from "./sessionRequestReferences";
import { HandoffRequestForm, IncompleteSharingAnnouncer, IncompleteSharingMark, isChangeRequest, SwitchRequestForm, type RequestFormState } from "./SessionRequestForms";

/** The branches an approval would transfer, each as code, joined as the language joins a list.
 * They are not text the agent wrote: the daemon checked each is a branch the requester is
 * responsible for. */
function branchList(language: Language, branches: string[]): React.ReactNode[] {
  return joinNodes(language, branches.map(displayWirePath), (branch, key) => <InlineReference key={key} monospace wrap>{branch}</InlineReference>);
}

/** The session a failed approval's notification is about: for a reopen or resume, the one the error
 * names, else the target, since the error comes from the target's resume; the requesting session
 * for everything else, including the request no longer waiting. */
function failureSubject(request: SessionRequest, err: unknown): string {
  if ((request.action !== "reopen" && request.action !== "resume") || (err instanceof CodedError && err.code === CODE.session_request_not_waiting)) {
    return request.session;
  }
  return (err instanceof CodedError ? err.params.session : undefined) ?? request.target;
}

/** What the dialog says: which session asks, and where it is, for a session of which agent where — a
 * project and its console, or a console — that would report to the user, and the branches it would
 * be handed where the request names any. A request to resume a conversation names instead the
 * session it would bring back (its name, agent and account, and where it is), whom that session
 * reports to — the user, or its owner, who stays — and the archived owners that come back with it;
 * for a conversation held only by an agent's record, the conversation's id and the account that
 * holds the record, and that the session it opens reports to the user. Only project sessions arrive
 * here: the daemon refuses a request to resume a console session's conversation, so every resuming
 * sentence is the project one. A request to switch a session's account or hand it off names that
 * session, the agent and the account asked for; the form below the sentence lets the user change
 * them. It carries no text an agent wrote, which goes in a disclosure of its own beneath
 * (`RequestPrompt`) — the name an archived session would be given is not shown either: the
 * confirmation is the one place the user's approval is enforced, so the sentence says only what
 * Octoboard knows. The requester's place is named because it may belong to another
 * project or console than the one it asks in, which the user has no other way to
 * see; its project and console are named only where they are not the ones the new session would be
 * in, which the sentence names anyway. A session is named by its display title, its current task
 * beside its name, since the name alone is often just its project's. The names are the user's own,
 * and a long one may have no break opportunity, so the sentence wraps anywhere. Names the store
 * does not know stand in as their ids. Where the account the session would run on has a sharing change
 * that did not finish (`incompleteAccount`), a warning mark in the sentence follows the account; a
 * switch or a handoff has it in its form instead, unless the form is not drawn (the window does not
 * know the session, or the console), when the sentence has it. */
function SessionRequestMessage({ request }: { request: Exclude<SessionRequest, ScheduledTaskRequest> }): React.ReactElement {
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const sharing = useDaemonStore((s) => s.accountSharing);
  // The account the sentence marks, kept here with the announcer so that a mark which appears after the
  // dialog opened is said once.
  const launchAccount = useDaemonStore((s) => {
    switch (request.action) {
      case "resume_record":
        return request.account;
      case "reopen":
      case "resume":
        return s.sessions.get(request.target)?.account_id;
      case "switch":
        return s.sessions.has(request.target) ? undefined : request.account;
      case "handoff":
        return s.consoles.has(request.console) ? undefined : request.account;
      default:
        return undefined;
    }
  });
  const incomplete = incompleteAccount(accounts, sharing, launchAccount);
  return (
    <>
      <IncompleteSharingAnnouncer account={incomplete} />
      <SessionRequestSentence request={request} incomplete={incomplete} />
    </>
  );
}

/** The sentence `SessionRequestMessage` describes, with `incomplete` the account it marks, if any. */
function SessionRequestSentence({ request, incomplete }: { request: SessionRequest; incomplete: Account | undefined }): React.ReactElement {
  const language = useCurrentLanguage();
  const t = useT();
  // Only the sessions the sentence names, so an unrelated session changing does not redraw it.
  const bringsBack = request.action === "reopen" || request.action === "resume" ? request : undefined;
  const actsOn = bringsBack ?? (request.action === "switch" || request.action === "handoff" ? request : undefined);
  const [requesting, target, owner, ...reopening] = useDaemonStore(useShallow((s) => (
    [request.session, actsOn?.target, bringsBack?.owner, ...(bringsBack?.reopenedOwners ?? [])].map((id) => (id === undefined ? undefined : s.sessions.get(id)))
  )));
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const markId = useId();
  const sentence = useRef<HTMLParagraphElement>(null);
  const handoffFrom = useHandoffAfterRemoval(() => dialogAround(sentence));
  // The mark after the account the session would run on. When it goes while it or its popover holds
  // focus, focus moves to the dialog.
  const incompleteMark = incomplete && (
    <span className="ms-1">
      {handoffFrom(`[data-status-mark-id="${markId}"]`)}
      <IncompleteSharingMark inline account={incomplete} markId={markId} />
    </span>
  );
  // Named rather than "this console": the window may be showing another one, and the session can
  // be asked for in any console.
  const console_ = recordReference(consoles.get(request.console)?.name, request.console);
  const project = request.project ? recordReference(projects.get(request.project)?.name, request.project) : null;
  const requester = requesterOf(t, request, requesting, consoles, projects);
  const agent = <ProviderName agent={request.agent} />;
  const params = { requester, console: console_, agent };
  if (request.action === "resume_record") {
    const account = <>{recordAccount(t, accounts, defaultNames, request.agent, request.account)}{incompleteMark}</>;
    return (
      <p ref={sentence} className="[overflow-wrap:anywhere]">
        <Message id="dialog.sessionRequest.question.record" params={{ ...params, project, account, conversation: recordReference(undefined, request.conversation) }} />
      </p>
    );
  }
  if (request.action === "switch" || request.action === "handoff") {
    const session = recordReference(target ? sessionDisplayTitle(t, target) : undefined, request.target);
    const account = <>{recordAccount(t, accounts, defaultNames, request.agent, request.account)}{incompleteMark}</>;
    return (
      <p ref={sentence} className="[overflow-wrap:anywhere]">
        <Message id={request.action === "switch" ? "dialog.sessionRequest.question.switchAndContinueLaunch" : "dialog.sessionRequest.question.handoffLaunch"} params={{ requester, agent, session, account }} />
      </p>
    );
  }
  if (request.action === "reopen" || request.action === "resume") {
    const targetName = recordReference(target ? sessionDisplayTitle(t, target) : undefined, request.target);
    // A target the store does not know has no account to name, so a dash stands in for it; a known
    // one that recorded an account the store does not know is shown by that id.
    const targetAccount = target && sessionAccount(t, target, accounts, defaultNames);
    const accountReference = !target
      ? <InlineReference dir="auto">—</InlineReference>
      : targetAccount
        ? <InlineReference monospace={targetAccount.isPath} dir={targetAccount.isPath ? "ltr" : "auto"} wrap>{targetAccount.name}</InlineReference>
        : recordReference(undefined, target.account_id ?? "");
    const account = <>{accountReference}{incompleteMark}</>;
    const reports = request.owner === undefined
      ? <Message id="dialog.sessionRequest.reportsTo.user" />
      : <Message id="dialog.sessionRequest.reportsTo.owner" params={{ owner: recordReference(owner ? sessionDisplayTitle(t, owner) : undefined, request.owner) }} />;
    const question = <Message id={request.action === "reopen" ? "dialog.sessionRequest.question.reopen" : "dialog.sessionRequest.question.resume"} params={{ ...params, project, session: targetName, account, reports }} />;
    const reopened = request.reopenedOwners;
    return (
      <p ref={sentence} className="[overflow-wrap:anywhere]">
        {reopened.length > 0
          ? <Message id="dialog.sessionRequest.withOwners" params={{ question, owners: <Message id="dialog.sessionRequest.reopenedOwners" params={{ count: reopened.length, owners: joinNodes(language, reopened, (id, key) => {
            const reopenedOwner = reopening[reopened.indexOf(id)];
            return recordReference(reopenedOwner ? sessionDisplayTitle(t, reopenedOwner) : undefined, id, key);
          }) }} /> }} />
          : question}
      </p>
    );
  }
  const question = request.kind === "console"
    ? <Message id="dialog.sessionRequest.question.console" params={params} />
    : <Message id="dialog.sessionRequest.question.project" params={{ ...params, project }} />;
  return (
    <p ref={sentence} className="[overflow-wrap:anywhere]">
      {request.branches.length > 0
        ? <Message id="dialog.sessionRequest.withBranches" params={{ question, branches: <Message id="dialog.sessionRequest.branches" params={{ count: request.branches.length, branches: branchList(language, request.branches) }} /> }} />
        : question}
    </p>
  );
}

/** The title and the confirm button for what an approval does. */
const ACTION_TEXT: Record<Exclude<SessionRequestAction, "set_scheduled_task_enabled">, { title: PlainMessageKey; confirm: PlainMessageKey }> = {
  start: { title: "dialog.sessionRequest.title", confirm: "dialog.sessionRequest.confirm" },
  reopen: { title: "dialog.sessionRequest.reopen.title", confirm: "dialog.sessionRequest.reopen.confirm" },
  resume: { title: "dialog.sessionRequest.resume.title", confirm: "dialog.sessionRequest.resume.confirm" },
  resume_record: { title: "dialog.sessionRequest.record.title", confirm: "dialog.sessionRequest.record.confirm" },
  switch: { title: "dialog.sessionRequest.switch.title", confirm: "dialog.sessionRequest.switch.confirm" },
  handoff: { title: "dialog.sessionRequest.handoff.title", confirm: "dialog.sessionRequest.handoff.confirm" },
  create_scheduled_task: { title: "dialog.sessionRequest.createTask.title", confirm: "dialog.sessionRequest.createTask.confirm" },
  update_scheduled_task: { title: "dialog.sessionRequest.updateTask.title", confirm: "dialog.sessionRequest.updateTask.confirm" },
  delete_scheduled_task: { title: "dialog.sessionRequest.deleteTask.title", confirm: "dialog.sessionRequest.deleteTask.confirm" },
  run_scheduled_task: { title: "dialog.sessionRequest.runTask.title", confirm: "dialog.sessionRequest.runTask.confirm" },
};

/** The title and the confirm button of the dialog for `request`; a request to turn a task on or off words itself by
 * which. */
function actionText(request: SessionRequest): { title: PlainMessageKey; confirm: PlainMessageKey } {
  if (request.action !== "set_scheduled_task_enabled") return ACTION_TEXT[request.action];
  return request.enabled
    ? { title: "dialog.sessionRequest.enableTask.title", confirm: "dialog.sessionRequest.enableTask.confirm" }
    : { title: "dialog.sessionRequest.disableTask.title", confirm: "dialog.sessionRequest.disableTask.confirm" };
}

/** What the dialog for a session's request shows and does — to start a session with no owner, to
 * resume a conversation, to switch the account of a session the requester owns or hand it off
 * (each a form the user may change before confirming, which an approval carries to the daemon), or
 * to create, change, turn on or off, delete or run a scheduled task (a plain confirmation) —
 * for the request `PendingQuestionDialog` puts forward, or nothing without one. A request that
 * would launch on an account whose last sharing change did not finish says so in a warning mark at
 * the account, where the user's own resume asks again (`useConfirmIncompleteLaunch`): the dialog is
 * already the confirmation; for a switch or a handoff it is the account the form is on (the
 * sentence's where the form is not drawn). One that appears after the dialog opened is said once.
 * The daemon holds the request, so the dialog moves on when the daemon says it stopped waiting,
 * which an answer from here or from any other client does at once. Dismissing it is a refusal. A hook rather than a
 * component, so the one dialog slot it shares with the trust prompts (`PendingQuestionDialog`)
 * stays mounted between them. */
export function useSessionRequestDialogProps(request: SessionRequest | undefined): ConfirmDialogProps | undefined {
  const t = useT();
  const { request: send } = useDaemon();
  // What the form of a switch or a handoff ended on, which the form reports.
  const [reported, setReported] = useState<RequestFormState>();
  // A switch of a session that has been archived cannot be carried out whatever the user changes: the dialog says
  // so as a failure, and acknowledging it refuses the request.
  const archived = useDaemonStore((s) => request?.action === "switch" && s.sessions.get(request.target)?.status === "archived");
  const failure = useFailureWhenBlocked(archived, t("dialog.sessionRequest.switch.archived"), t("dialog.sessionRequest.switch.archivedTitle"), request?.requestId);
  if (!request) return undefined;
  const form = reported?.requestId === request.requestId ? reported : undefined;

  // Never rejects: the dialog has usually moved on by the time an approval has been carried out,
  // so what the user has to know goes to a notification instead of into the dialog. The
  // notification's subject is `failureSubject`'s. The window does not follow the session an
  // approval starts or brings back: the user's attention stays with the requesting session, which
  // carries on, and the session is not one the user asked to open.
  const answer = async (approve: boolean): Promise<void> => {
    const edits = approve && isChangeRequest(request) ? form?.edits : undefined;
    try {
      await send({ type: "answer_session_request", request_id: request.requestId, approve, ...edits });
    } catch (err) {
      // Answered already, here or from another client: nothing is wrong.
      if (err instanceof CodedError && err.code === CODE.session_request_answered) return;
      // Nothing was going to start for a refusal, so the session no longer waiting is no news.
      if (!approve && err instanceof CodedError && err.code === CODE.session_request_not_waiting) return;
      raiseError(err, { title: t(approve ? "dialog.sessionRequest.approveFailedTitle" : "dialog.sessionRequest.refuseFailedTitle"), keepTitle: true, session: failureSubject(request, err) });
    }
  };

  const text = actionText(request);
  return {
    resetKey: `request:${request.requestId}`,
    failure,
    title: t(text.title),
    // A request for a scheduled task has a sentence and list of its own, which reads none of what a session request's does.
    message: isScheduledTaskRequest(request)
      ? <ScheduledTaskRequestMessage key={request.requestId} request={request} />
      : <SessionRequestMessage key={request.requestId} request={request} />,
    extraControl: isChangeRequest(request) || request.prompt !== undefined
      ? (
        <div key={request.requestId} className="flex flex-col gap-4">
          {isChangeRequest(request) && (request.action === "switch"
            ? <SwitchRequestForm request={request} onReport={setReported} />
            : <HandoffRequestForm request={request} onReport={setReported} />)}
          {request.prompt !== undefined && <RequestPrompt action={request.action} prompt={request.prompt} />}
        </div>
      )
      : undefined,
    confirmDisabled: form?.unavailable === true,
    confirmLabel: t(text.confirm),
    destructive: request.action === "delete_scheduled_task",
    cancelLabel: t("dialog.sessionRequest.refuse"),
    errorTitle: t("dialog.sessionRequest.approveFailedTitle"),
    onCancel: () => void answer(false),
    onConfirm: () => answer(true),
  };
}
