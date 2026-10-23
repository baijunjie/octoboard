import { Alert } from "@heroui/react";
import React from "react";
import { useShallow } from "zustand/react/shallow";

import { incompleteAccount } from "../accountChoices";
import { AGENT_LABEL, defaultAccountName } from "../agents";
import { DaemonRequestError } from "../daemon-client";
import { errorPresentation } from "../errorPresentation";
import { InlineReference } from "../components/InlineReference";
import { TintedAlert } from "../components/TintedAlert";
import { isolate } from "../i18n/isolate";
import { joinNodes } from "../i18n/joinPhrases";
import type { Language } from "../i18n/languages";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { MessageKey, Translate } from "../i18n/catalog";
import { SESSION_REQUEST_ANSWERED, SESSION_REQUEST_NOT_WAITING, type Account, type Agent, type SessionRequestAction, type Settings } from "../protocol";
import { sessionAccount, sessionDisplayTitle } from "../sessionLabel";
import { useDaemon, useDaemonStore, type SessionRequest } from "../store";
import { displayWirePath } from "../wirePath";
import type { ConfirmDialogProps } from "./ConfirmDialog";

/** The branches an approval would hand over, each as code, joined as the language joins a list.
 * They are not text the agent wrote: the daemon checked each is a branch the requester is
 * responsible for. */
function branchList(language: Language, branches: string[]): React.ReactNode[] {
  return joinNodes(language, branches.map(displayWirePath), (branch, key) => <InlineReference key={key} monospace wrap>{branch}</InlineReference>);
}

function recordReference(name: string | undefined, id: string, key?: number): React.ReactElement {
  return <InlineReference key={key} monospace={name === undefined} dir={name === undefined ? "ltr" : "auto"} wrap>{name ?? id}</InlineReference>;
}

/** The account that holds a conversation's record, as the pickers name it: the default account of
 * `agent` for `null`, a stored account by its name, else by its id, as any other id the store does
 * not know. */
function recordAccount(t: Translate, accounts: Account[], defaultNames: Settings["default_account_names"], agent: Agent, account: string | null): React.ReactElement {
  const name = account === null ? defaultAccountName(t, defaultNames, agent) : accounts.find((item) => item.id === account)?.name;
  return recordReference(name, account ?? "");
}

/** The session a failed approval's toast is about: for a reopen or resume, the one the error names,
 * else the target, since the error comes from the target's resume; the requesting session for
 * everything else, including the request no longer waiting. */
function failureSubject(request: SessionRequest, err: unknown): string {
  if ((request.action !== "reopen" && request.action !== "resume") || (err instanceof DaemonRequestError && err.code === SESSION_REQUEST_NOT_WAITING)) {
    return request.session;
  }
  return (err instanceof DaemonRequestError ? err.params.session : undefined) ?? request.target;
}

/** What the dialog says: which session asks, and where it is, for a session of which agent where — a
 * project and its console, or a console — that would report to the user, and the branches it would
 * be handed where the request names any. A request to resume a conversation names instead the
 * session it would bring back (its name, agent and account, and where it is), whom that session
 * reports to — the user, or its owner, who stays — and the archived owners that come back with it;
 * for a conversation held only by an agent's record, the conversation's id and the account that
 * holds the record, and that the session it opens reports to the user. Only project sessions arrive
 * here: the daemon refuses a request to resume a console session's conversation, so every resuming
 * sentence is the project one. It carries no text an agent wrote — the name an archived session
 * would be given is not shown either: the confirmation is the one place the user's approval is
 * enforced, so it says only what Octoboard knows. The requester's place is named because it may
 * belong to another project or console than the one it asks in, which the user has no other way to
 * see; its project and console are named only where they are not the ones the new session would be
 * in, which the sentence names anyway. A session is named by its display title, its current task
 * beside its name, since the name alone is often just its project's. The names are the user's own,
 * and a long one may have no break opportunity, so the sentence wraps anywhere. Names the store
 * does not know stand in as their ids. */
function SessionRequestMessage({ request }: { request: SessionRequest }): React.ReactElement {
  const language = useCurrentLanguage();
  const t = useT();
  // Only the sessions the sentence names, so an unrelated session changing does not redraw it.
  const bringsBack = request.action === "reopen" || request.action === "resume" ? request : undefined;
  const [requesting, target, owner, ...reopening] = useDaemonStore(useShallow((s) => (
    [request.session, bringsBack?.target, bringsBack?.owner, ...(bringsBack?.reopenedOwners ?? [])].map((id) => (id === undefined ? undefined : s.sessions.get(id)))
  )));
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  // Named rather than "this console": the window may be showing another one, and the session can
  // be asked for in any console.
  const console_ = recordReference(consoles.get(request.console)?.name, request.console);
  const project = request.project ? recordReference(projects.get(request.project)?.name, request.project) : null;
  const session = recordReference(requesting ? sessionDisplayTitle(t, requesting) : undefined, request.session);
  let requester: React.ReactElement;
  if (!requesting || (requesting.project_id && requesting.project_id === request.project)) {
    requester = <Message id="dialog.sessionRequest.requester.session" params={{ session }} />;
  } else {
    const elsewhere = requesting.console_id !== request.console;
    const own = { session, console: recordReference(consoles.get(requesting.console_id)?.name, requesting.console_id) };
    if (requesting.project_id) {
      const project = recordReference(projects.get(requesting.project_id)?.name, requesting.project_id);
      requester = elsewhere
        ? <Message id="dialog.sessionRequest.requester.projectElsewhere" params={{ ...own, project }} />
        : <Message id="dialog.sessionRequest.requester.project" params={{ session, project }} />;
    } else {
      requester = elsewhere
        ? <Message id="dialog.sessionRequest.requester.consoleElsewhere" params={own} />
        : <Message id="dialog.sessionRequest.requester.console" params={{ session }} />;
    }
  }
  const agent = <InlineReference dir="auto" wrap>{AGENT_LABEL[request.agent]}</InlineReference>;
  const params = { requester, console: console_, agent };
  if (request.action === "resume_record") {
    const account = recordAccount(t, accounts, defaultNames, request.agent, request.account);
    return (
      <p className="[overflow-wrap:anywhere]">
        <Message id="dialog.sessionRequest.question.record" params={{ ...params, project, account, conversation: recordReference(undefined, request.conversation) }} />
      </p>
    );
  }
  if (request.action === "reopen" || request.action === "resume") {
    const targetName = recordReference(target ? sessionDisplayTitle(t, target) : undefined, request.target);
    // A target the store does not know has no account to name, so a dash stands in for it; a known
    // one that recorded an account the store does not know is shown by that id.
    const targetAccount = target && sessionAccount(t, target, accounts, defaultNames);
    const account = !target
      ? <InlineReference dir="auto">—</InlineReference>
      : targetAccount
        ? <InlineReference monospace={targetAccount.isPath} dir={targetAccount.isPath ? "ltr" : "auto"} wrap>{targetAccount.name}</InlineReference>
        : recordReference(undefined, target.account_id ?? "");
    const reports = request.owner === undefined
      ? <Message id="dialog.sessionRequest.reportsTo.user" />
      : <Message id="dialog.sessionRequest.reportsTo.owner" params={{ owner: recordReference(owner ? sessionDisplayTitle(t, owner) : undefined, request.owner) }} />;
    const question = <Message id={request.action === "reopen" ? "dialog.sessionRequest.question.reopen" : "dialog.sessionRequest.question.resume"} params={{ ...params, project, session: targetName, account, reports }} />;
    const reopened = request.reopenedOwners;
    return (
      <p className="[overflow-wrap:anywhere]">
        {reopened.length > 0
          ? <Message id="dialog.sessionRequest.withOwners" params={{ question, owners: <Message id="dialog.sessionRequest.reopenedOwners" params={{ count: reopened.length, owners: joinNodes(language, reopened, (id, key) => {
            const reopenedOwner = reopening[reopened.indexOf(id)];
            return recordReference(reopenedOwner ? sessionDisplayTitle(t, reopenedOwner) : undefined, id, key);
          }) }} /> }} />
          : question}
      </p>
    );
  }
  const question = request.role === "console"
    ? <Message id="dialog.sessionRequest.question.console" params={params} />
    : <Message id="dialog.sessionRequest.question.project" params={{ ...params, project }} />;
  return (
    <p className="[overflow-wrap:anywhere]">
      {request.branches.length > 0
        ? <Message id="dialog.sessionRequest.withBranches" params={{ question, branches: <Message id="dialog.sessionRequest.branches" params={{ count: request.branches.length, branches: branchList(language, request.branches) }} /> }} />
        : question}
    </p>
  );
}

/** The title and the confirm button for what an approval does. */
const ACTION_TEXT = {
  start: { title: "dialog.sessionRequest.title", confirm: "dialog.sessionRequest.confirm" },
  reopen: { title: "dialog.sessionRequest.reopen.title", confirm: "dialog.sessionRequest.reopen.confirm" },
  resume: { title: "dialog.sessionRequest.resume.title", confirm: "dialog.sessionRequest.resume.confirm" },
  resume_record: { title: "dialog.sessionRequest.record.title", confirm: "dialog.sessionRequest.record.confirm" },
} as const satisfies Record<SessionRequestAction, { title: MessageKey; confirm: MessageKey }>;

/** What the dialog for a session's request shows and does — to start a session with no owner, or
 * to resume a conversation — for the request `PendingQuestionDialog` puts forward, or nothing
 * without one. A resume that would launch on an account whose last sharing change did not finish
 * says so in a warning line, where the user's own resume asks again
 * (`useConfirmIncompleteLaunch`): the dialog is already the confirmation. The daemon holds the
 * request, so the dialog moves on when the daemon says it stopped waiting, which an answer from
 * here or from any other client does at once. Dismissing it is a refusal. A hook rather than a
 * component, so the one dialog slot it shares with the trust prompts (`PendingQuestionDialog`)
 * stays mounted between them. */
export function useSessionRequestDialogProps(request: SessionRequest | undefined): ConfirmDialogProps | undefined {
  const t = useT();
  const { request: send, toastError } = useDaemon();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const sharing = useDaemonStore((s) => s.accountSharing);
  const targetId = request?.action === "reopen" || request?.action === "resume" ? request.target : undefined;
  const targetAccount = useDaemonStore((s) => (targetId === undefined ? undefined : s.sessions.get(targetId)?.account_id));
  if (!request) return undefined;

  // Never rejects: the dialog has usually moved on by the time an approval has been carried out,
  // so what the user has to know goes to a toast instead of into the dialog. The toast's subject is
  // `failureSubject`'s. The window does not follow the session an approval starts or brings back:
  // the user's attention stays with the requesting session, which carries on, and the session is
  // not one the user asked to open.
  const answer = async (approve: boolean): Promise<void> => {
    try {
      await send({ type: "answer_session_request", request_id: request.requestId, approve });
    } catch (err) {
      if (err instanceof DaemonRequestError && err.code === SESSION_REQUEST_ANSWERED) return;
      // Nothing was going to start for a refusal, so the session no longer waiting is no news.
      if (!approve && err instanceof DaemonRequestError && err.code === SESSION_REQUEST_NOT_WAITING) return;
      toastError(errorPresentation(err as Error), failureSubject(request, err));
    }
  };

  const { action } = request;
  // The account the resumed session runs on: the one holding the record, or the session's own,
  // which a store that does not know the session cannot say.
  const launchAccount = request.action === "resume_record" ? request.account : request.action === "start" ? undefined : targetAccount;
  const incomplete = incompleteAccount(accounts, sharing, launchAccount);
  return {
    resetKey: `request:${request.requestId}`,
    title: t(ACTION_TEXT[action].title),
    message: <SessionRequestMessage request={request} />,
    warning: incomplete && (
      <TintedAlert status="warning">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Description>{t("dialog.sessionRequest.incomplete", { account: isolate(incomplete.name) })}</Alert.Description>
        </Alert.Content>
      </TintedAlert>
    ),
    confirmLabel: t(ACTION_TEXT[action].confirm),
    cancelLabel: t("dialog.sessionRequest.refuse"),
    onCancel: () => void answer(false),
    onConfirm: () => answer(true),
  };
}
