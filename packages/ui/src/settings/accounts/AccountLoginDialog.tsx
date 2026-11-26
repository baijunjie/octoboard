import React, { useEffect, useRef, useState } from "react";

import { defaultAccountName } from "../../agents/agents";
import { DaemonErrorMessage } from "../../components/alerts/DaemonErrorMessage";
import { DetailsDisclosure } from "../../components/alerts/ErrorDetailsDisclosure";
import { errorRemedy, rawErrorDetails } from "../../components/alerts/errorParts";
import { ExternalLink } from "../../components/text/ExternalLink";
import { MarkLineBox } from "../../components/status/MarkLineBox";
import { InlineReference } from "../../i18n/InlineReference";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { CodedError, type CodedMessage } from "../../errors/codedError";
import { daemonMessage, NO_RECORDS } from "../../errors/daemonMessage";
import { RecoveryGuidance } from "../../components/alerts/ErrorGuidance";
import { uncataloguedSummaryKey } from "../../errors/errorPresentation";
import { isUncataloguedFailure } from "../../errors/uncataloguedFailure";
import { classifyError } from "../../errors/errorRecovery";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { Dialog } from "../../components/dialog/Dialog";
import { useDialogAction } from "../../components/dialog/dialogHooks";
import { TextInput } from "../../components/form/TextInput";
import { useIssueReport } from "../../components/alerts/useIssueReport";
import { isolate } from "../../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../../i18n/react";
import type { AccountLogin, Agent, LoginAnswer, LoginQuestion, LoginStatus } from "../../protocol";
import { dismissAccountLogin, trackAccountLogin, useDaemon, useDaemonStore, useProviders } from "../../store";
import { display } from "../../format/hostPath";
import { useHostPathsOrEmpty } from "../../components/hooks/useHostPaths";
import { ProviderName } from "../../components/text/ProviderName";
import { ringForAutomaticFocus } from "../../input/lastInput";
import { accountConfigDir } from "../../accounts/accountDir";
import { accountLoginOf, loginIsActive } from "../../accounts/accountRecords";
import { loginQuestionKeys, secretLoginQuestionKeys } from "../../agents/providers";
import type { MessageKey } from "../../i18n/catalog";
import { useAccountLoginActions } from "./AccountLogin";
import { raiseError } from "../../components/alerts/raiseError";
import { Button, ModalStatusIcon, Spinner } from "../../primitives";
import { knownValues } from "../../values/knownValue";

const isLoginStatus = knownValues<LoginStatus>({ starting: true, waiting: true, succeeded: true, failed: true, cancelled: true, timed_out: true });

/** A question an agent's login asks, in the provider's own words (`id`): each value the daemon sent with the question
 * stands where the wording names it, drawn as a technical literal (a path, a file name), which is all a question's
 * params are. */
function QuestionText({ question, id }: { question: LoginQuestion; id: MessageKey }): React.ReactElement {
  const params = Object.fromEntries(Object.entries(question.params).map(([name, value]) => [name, <InlineReference key={name} dir="ltr" monospace wrap>{value ?? ""}</InlineReference>]));
  // The key is found at run time, from the provider and the question the daemon names, so its placeholders cannot be
  // checked against the params here: a placeholder the daemon sent no value for is left as written.
  return <Message id={id} params={params as never} />;
}

function SecretQuestionDialog({ subject, question, words, url, send, close }: {
  subject: React.ReactNode;
  question: LoginQuestion;
  words: NonNullable<ReturnType<typeof secretLoginQuestionKeys>>;
  url: string | null | undefined;
  send: (answer: LoginAnswer) => Promise<void>;
  close: () => void;
}): React.ReactElement {
  const t = useT();
  const [secret, setSecret] = useState("");
  const { busy, run } = useDialogAction(question.id, t("settings.accounts.login.answerFailedTitle"));
  const submit = () => {
    if (busy || !secret) return;
    // The typed secret stays where sending fails, for the person to send again.
    void run(async () => {
      await send({ secret_text: secret });
      setSecret("");
    });
  };
  return (
    <Dialog
      title={t("settings.accounts.login.questionTitle")}
      onClose={close}
      busy={busy}
      submitLabel={t(words.submit)}
      submitDisabled={!busy && !secret}
      onSubmit={submit}
    >
      <p>{subject}</p>
      <p><QuestionText question={question} id={words.question} /></p>
      {url && <ExternalLink href={url}>{t("settings.accounts.login.reopen")}</ExternalLink>}
      <TextInput type="password" dir={secret ? "ltr" : undefined} label={t(words.field)} placeholder={t("settings.accounts.login.secret.placeholder")} value={secret} onChange={setSecret} autoFocus />
    </Dialog>
  );
}

/** The login of one account (`null` for an agent's default account), in a modal of its own. With `start` it asks the
 * daemon to start the login on opening; without, it takes up one that is already running (after a reload, or started by
 * another window). Its title is the plain "Log in"; the body opens with the sentence naming the agent and the account,
 * then shows where the login is, with the device code and the link to open the authorization page again where there is
 * one, and Cancel login. Closing it by any means ends a login that is still running. While the login waits on a
 * question of the agent's own, a question dialog stands in its place. A choice opens with focus on the dialog
 * itself. A secret question opens on its masked field and can offer the authorization link again. Both name the
 * agent and account, use the provider's wording, and hold their controls while sending the answer. Closing the
 * question ends the login unanswered. A question the window has no words for is not
 * offered for an answer and ends what the dialog is for: a failure alert of its own says that the login is waiting for
 * an answer, that the question cannot be shown, and how to log the account in instead, naming its configuration
 * directory; Cancel login, its only button, and its close button end the login. A login that succeeded closes it:
 * the account's badge and the row's hidden status region say so. One that failed or timed out, and a refusal to start
 * one, replace it with a failure alert dialog of its own, in the form of the error alert dialog (a close button that
 * does what Close does, but no Escape): the title says what failed in plain words, the body opens with the sentence
 * naming the agent and the account, which goes on with the cause where the agent named one, then the main remedy, the
 * further explanation, and the raw text behind an "Error details" disclosure; Close then Log in again, which takes
 * focus; where it is unexpected, Report issue takes Close's place in the footer (a footer holds at most two buttons)
 * and the close button stands for Close. Its mounting is what a screen reader announces. */
export function AccountLoginDialog({ agent, account, start, onClose }: {
  agent: Agent;
  account: string | null;
  start: boolean;
  /** Told whether the dialog closed because the login succeeded and, when closing it cancels a login still running,
   * given that cancel, which settles once the daemon has answered it. */
  onClose: (succeeded: boolean, cancelling?: Promise<void>) => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { store } = useDaemon();
  const actions = useAccountLoginActions();
  const providers = useProviders();
  const paths = useHostPathsOrEmpty();
  const dir = useDaemonStore((s) => accountConfigDir(s.settings.accounts, s.agentAvailability, providers, agent, account));
  const held = useDaemonStore((s) => accountLoginOf(s.accountLogins, agent, account));
  // What the store held for the account when the dialog opened to start a login, if it had ended, is the record of
  // an earlier one (cancelled, succeeded or failed), not of this.
  const [heldAtOpen] = useState(() => (start && held && !loginIsActive(held) ? held : undefined));
  const current = held === heldAtOpen ? undefined : held;
  const name = useDaemonStore((s) => (account && s.settings.accounts.find((stored) => stored.id === account)?.name) || defaultAccountName(t, s.settings.default_account_names, agent));
  const [refusal, setRefusal] = useState<CodedMessage>();
  // The last record of this login seen: until there is one, no record means the login is still starting; after one
  // that was running, a record that is gone (a snapshot after a reconnect dropped it) means the login is over, and
  // after one that had ended, the ended login stays shown.
  const [lastSeen, setLastSeen] = useState<AccountLogin>();
  const login = current ?? (lastSeen && !loginIsActive(lastSeen) ? lastSeen : undefined);
  const primaryRef = useRef<HTMLButtonElement>(null);
  // The request to start the login while it is in flight: the daemon registers the login only once it has the
  // shell's environment, so a cancel sent meanwhile would find none.
  const pendingStart = useRef<Promise<void> | undefined>(undefined);

  const cancel = () => actions.cancel(agent, account).catch((err: unknown) => {
    if ((err as Partial<CodedError>).code !== "login_not_running") raiseError(err, { title: t("settings.accounts.login.cancelFailedTitle") });
  });
  const begin = () => {
    setRefusal(undefined);
    setLastSeen(undefined);
    // The record of an earlier login that ended is dropped, for the new one's to stand in its place.
    const earlier = accountLoginOf(store.getState().accountLogins, agent, account);
    if (earlier && !loginIsActive(earlier)) dismissAccountLogin(store, earlier);
    const pending: Promise<void> = actions.start(agent, account).catch((err: unknown) => {
      const refused = err as Partial<CodedError>;
      // A login of this account is already running: take it up, as the daemon's events bring it in.
      if (refused.code === "login_already_running") return;
      setRefusal({ code: refused.code ?? "", params: refused.params ?? {}, message: refused.message ?? String(err) });
    }).finally(() => {
      if (pendingStart.current === pending) pendingStart.current = undefined;
    });
    pendingStart.current = pending;
  };
  // Once, though React's development remount runs the effect twice: a second start would be refused.
  const started = useRef(false);
  useEffect(() => {
    if (!start || started.current) return;
    started.current = true;
    begin();
  }, []);
  useEffect(() => {
    if (current) setLastSeen(current);
  }, [current]);

  const ended = login?.status === "failed" || login?.status === "timed_out" || refusal !== undefined;
  const active = !refusal && (login === undefined ? lastSeen === undefined : loginIsActive(login));
  // A login that succeeded, or that another window cancelled, or that has vanished, has nothing more to show, and nor
  // has one in a status of a newer daemon, which cannot be followed.
  useEffect(() => {
    if (refusal) return;
    if (login?.status === "succeeded" || login?.status === "cancelled" || (login !== undefined && !isLoginStatus(login.status)) || (current === undefined && loginIsActive(lastSeen))) onClose(login?.status === "succeeded");
  }, [login, current, lastSeen, refusal]);
  // Each form is a dialog of its own, whose own focus handling may leave focus on the dialog or on `<body>`: the
  // form's primary button (Log in again, Cancel login) takes it.
  // Not asked for by the user, so the ring follows the last input, but only where the form changed, not when the
  // dialog first opens.
  // The question dialog takes focus for itself; the running dialog that comes back once it is answered is a form
  // changed in the same way.
  const question = !ended && login?.status === "waiting" ? login.question : undefined;
  const words = question && question.answer_kind === undefined ? loginQuestionKeys(agent, question.id) : undefined;
  const secretWords = question?.answer_kind === "secret_text" ? secretLoginQuestionKeys(agent, question.id) : undefined;
  const asking = words !== undefined || secretWords !== undefined;
  // A question with no words here cannot be answered here: the failure form says so.
  const unworded = question !== undefined && !asking;
  const wasEnded = useRef(ended);
  const wasAsking = useRef(asking);
  const wasUnworded = useRef(unworded);
  useEffect(() => {
    if ((ended && !wasEnded.current) || (!asking && wasAsking.current) || unworded !== wasUnworded.current) ringForAutomaticFocus();
    wasEnded.current = ended;
    wasAsking.current = asking;
    wasUnworded.current = unworded;
    if (!asking) primaryRef.current?.focus();
  }, [ended, asking, unworded]);

  const close = () => {
    const pending = pendingStart.current;
    let cancelling: Promise<void> | undefined;
    if (pending) {
      // The login is not this window's to open the page of any more, and is cancelled once the daemon has it.
      trackAccountLogin(store, agent, account, false);
      cancelling = pending.then(cancel);
    } else if (active) {
      cancelling = cancel();
    } else if (login && (login.status === "failed" || login.status === "timed_out")) {
      dismissAccountLogin(store, login);
    }
    onClose(false, cancelling);
  };

  const who = <InlineReference dir="auto" wrap>{name}</InlineReference>;
  const failure: CodedMessage | undefined = refusal ?? (login?.failure ?? undefined);
  const reason = failure && daemonMessage(language, failure.code, failure.params, failure.message, NO_RECORDS);
  const code = login?.status === "waiting" ? login.device_code : null;
  const progress = t(login?.status === "waiting" ? "settings.accounts.login.waiting" : "settings.accounts.login.starting");
  const timedOut = login?.status === "timed_out" && !refusal;
  const agentChip = <ProviderName agent={agent} />;

  const presentation = failure && reason !== undefined ? new CodedError(reason, failure.code, failure.params) : undefined;
  // A failure the application has no words for is its generic sentence as the cause, and where it is classified
  // unexpected, that sentence is "Something unexpected went wrong" with the request to report it.
  const uncatalogued = !!presentation && !timedOut && isUncataloguedFailure(presentation);
  const classification = uncatalogued ? classifyError(presentation) : undefined;
  const unexpected = classification?.kind === "unexpected";
  // The report is made from the daemon's own text, not `reason`, which is worded for the user.
  const issue = useIssueReport(unexpected && failure ? [{ error: new CodedError(failure.message, failure.code, failure.params), unexpected: true }] : undefined);

  if (ended) {
    const remedy = presentation && errorRemedy(presentation);
    const raw = presentation && rawErrorDetails(presentation);
    // A login that failed with no cause the agent named (`login_failed`) has a summary that only says again that it
    // failed, which the lead has said already; only a failed login has a cause at all.
    // The refusal because the account is being removed is said with a pronoun, as the lead names the account already.
    const cause = presentation && !timedOut && presentation.code !== "login_failed"
      ? presentation.code === "account_being_removed" ? t("settings.accounts.login.failedBeingRemoved")
        : uncatalogued ? t(uncataloguedSummaryKey(presentation)) : <DaemonErrorMessage error={presentation} part="summary" />
      : undefined;
    const lead = timedOut
      ? <Message id="settings.accounts.login.timedOutLead" params={{ agent: agentChip, name: who }} />
      : cause
        ? <Message id="settings.accounts.login.failedLeadBecause" params={{ agent: agentChip, name: who, reason: cause }} />
        : <Message id="settings.accounts.login.failedLead" params={{ agent: agentChip, name: who }} />;
    // The failure form is a dialog of its own, not the running one with its frame changed: the alert dialog is
    // announced as it mounts and takes focus on Log in again, and the dialog's height and toast clearance follow it.
    return (
      <Dialog
        key="ended"
        alert
        keyboardDismissHeld
        headerLeading={<ModalStatusIcon status="danger" />}
        title={t(timedOut ? "settings.accounts.login.timedOutTitle" : "settings.accounts.login.failedTitle")}
        onClose={close}
        footer={(
          <>
            {unexpected ? issue.link : <Button type="button" variant="secondary" onPress={close}>{t("common.close")}</Button>}
            <Button ref={primaryRef} autoFocus type="button" variant="primary" onPress={begin}>{t("settings.accounts.login.again")}</Button>
          </>
        )}
      >
        <p>{lead}</p>
        {remedy && <p>{remedy}</p>}
        {unexpected ? <p>{issue.guidance}</p> : classification?.kind === "actionable" && classification.recovery !== "none" && <p><RecoveryGuidance recovery={classification.recovery} /></p>}
        {presentation && <DaemonErrorMessage error={presentation} part="further" />}
        {raw ? <DetailsDisclosure>{raw}</DetailsDisclosure> : null}
      </Dialog>
    );
  }
  if (question && (words || secretWords)) {
    // An answer the login no longer waits for (another window gave one first, or the login ended) is not a failure:
    // the login's own updates show where it is.
    const send = (answer: LoginAnswer) => actions.answer(agent, account, question.id, answer).catch((err: unknown) => {
      const code = (err as Partial<CodedError>).code;
      if (code !== "login_question_not_open" && code !== "login_not_running") throw err;
    });
    const subject = <Message id="settings.accounts.login.questionLead" params={{ agent: agentChip, name: who }} />;
    if (secretWords) {
      return <SecretQuestionDialog key={question.id} subject={subject} question={question} words={secretWords} url={login?.authorization_url} send={send} close={close} />;
    }
    if (words) return (
      <ConfirmDialog
        key="question"
        unprompted
        hideCancel
        resetKey={question.id}
        title={t("settings.accounts.login.questionTitle")}
        subject={subject}
        message={<p><QuestionText question={question} id={words.question} /></p>}
        confirmLabel={t(words.yes)}
        otherAction={{ label: t(words.no), onPress: () => send("no") }}
        errorTitle={t("settings.accounts.login.answerFailedTitle")}
        onConfirm={() => send("yes")}
        onCancel={close}
      />
    );
  }
  if (unworded) {
    // A dialog of its own in the failure alert's form, as the ended login's is: its mounting is what is announced.
    return (
      <Dialog
        key="unworded"
        alert
        keyboardDismissHeld
        headerLeading={<ModalStatusIcon status="danger" />}
        title={t("settings.accounts.login.questionUnwordedTitle")}
        onClose={close}
        footer={<Button ref={primaryRef} autoFocus type="button" variant="primary" onPress={close}>{t("settings.accounts.login.cancel")}</Button>}
      >
        <p><Message id="settings.accounts.login.questionUnwordedLead" params={{ agent: agentChip, name: who }} /></p>
        <p>{t("settings.accounts.login.questionUnworded")}</p>
        {/* Said only for a directory that is known, which the remedy names. */}
        {dir !== "" && <p><Message id="settings.accounts.login.questionUnwordedRemedy" params={{ dir: <InlineReference dir="ltr" monospace wrap>{display(dir, paths)}</InlineReference> }} /></p>}
      </Dialog>
    );
  }
  return (
    <Dialog
      key="running"
      title={t("settings.accounts.login.title")}
      onClose={close}
      footer={(
        <>
          {login?.status === "waiting" && login.authorization_url && (
            <ExternalLink key="reopen" href={login.authorization_url} appearance="button">{t("settings.accounts.login.reopen")}</ExternalLink>
          )}
          <Button key="cancel" ref={primaryRef} autoFocus type="button" variant="secondary" onPress={close}>{t("settings.accounts.login.cancel")}</Button>
        </>
      )}
    >
      <p><Message id="settings.accounts.login.intro" params={{ agent: agentChip, name: who }} /></p>
      <div className="mt-2 flex items-start gap-2 text-sm">
        <MarkLineBox size="text-sm"><Spinner size="sm" /></MarkLineBox>
        {/* While the login starts, the intro line above already says "Logging in…", so the spinner stands alone. */}
        {login?.status === "waiting" && (
          <span aria-hidden>
            {code
              ? <Message id="settings.accounts.login.waitingCode" params={{ code: <InlineReference monospace>{code}</InlineReference> }} />
              : progress}
          </span>
        )}
        {/* It comes with a focus move — the dialog opening, or Log in again handing focus to Cancel login — so its first
            text waits for that to be announced. */}
        <StatusAnnouncer
          firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
          text={!active ? undefined : code ? t("settings.accounts.login.waitingCode", { code: isolate(code) }) : progress}
        />
      </div>
    </Dialog>
  );
}
