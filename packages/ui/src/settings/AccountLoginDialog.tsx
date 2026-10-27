import { AlertDialog, Spinner } from "@heroui/react";
import React, { useEffect, useRef, useState } from "react";

import { Button } from "../components/Button";
import { AGENT_LABEL, defaultAccountName } from "../agents";
import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { DetailsDisclosure } from "../components/ErrorDetailsDisclosure";
import { errorRemedy, rawErrorDetails } from "../components/errorParts";
import { ExternalLink } from "../components/ExternalLink";
import { InlineReference } from "../components/InlineReference";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../daemonMessage";
import { RecoveryGuidance } from "../components/ErrorGuidance";
import { isUncataloguedFailure, uncataloguedSummaryKey } from "../errorPresentation";
import { classifyError } from "../errorRecovery";
import { Dialog } from "../dialogs/Dialog";
import { useIssueReport } from "../dialogs/ErrorAlerts";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { AccountLogin, Agent, MessageParams } from "../protocol";
import { dismissAccountLogin, trackAccountLogin, useDaemon, useDaemonStore } from "../store";
import { ringForAutomaticFocus } from "../touchInput";
import { accountLoginOf, loginIsActive, useAccountLoginActions } from "./AccountLogin";

const NO_RECORDS = { consoles: new Map(), projects: new Map(), sessions: new Map() };

/** A coded message saying why a login did not go through: the daemon's refusal to start one, or the failure of one that ran. */
interface Failure {
  code: string;
  params: MessageParams;
  message: string;
}

/** The login of one account (`null` for an agent's default account), in a modal of its own. With `start` it asks
 * the daemon to start the login on opening; without, it takes up one that is already running (after a reload, or
 * started by another window). Its title is the plain "Log in"; the body opens with the sentence naming the agent
 * and the account, then shows where the login is, with the device code and the link to open the authorization page
 * again where there is one, and Cancel login. Closing it by any means ends a login that is
 * still running. A login that succeeded closes it: the account's badge and the row's hidden status region say so.
 * One that failed or timed out, and a refusal to start one, replace it with a failure alert dialog of its own, in
 * the form of the error alert dialog (no close button, no Escape): the title says what failed in plain words, the
 * body opens with the sentence naming the agent and the account, which goes on with the cause where the agent named
 * one, then the main remedy, the further explanation, and the raw text behind an "Error details" disclosure;
 * Close, then Log in again, which takes focus. Its mounting is what a screen reader announces. */
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
  const { store, toastError } = useDaemon();
  const actions = useAccountLoginActions();
  const held = useDaemonStore((s) => accountLoginOf(s.accountLogins, agent, account));
  // What the store held for the account when the dialog opened to start a login, if it had ended, is the record of
  // an earlier one (cancelled, succeeded or failed), not of this.
  const [heldAtOpen] = useState(() => (start && held && !loginIsActive(held) ? held : undefined));
  const current = held === heldAtOpen ? undefined : held;
  const name = useDaemonStore((s) => (account && s.settings.accounts.find((stored) => stored.id === account)?.name) || defaultAccountName(t, s.settings.default_account_names, agent));
  const [refusal, setRefusal] = useState<Failure>();
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
    if ((err as Partial<DaemonRequestError>).code !== "login_not_running") toastError(err as Error, undefined, t("settings.accounts.login.cancelFailedTitle"));
  });
  const begin = () => {
    setRefusal(undefined);
    setLastSeen(undefined);
    // The record of an earlier login that ended is dropped, for the new one's to stand in its place.
    const earlier = accountLoginOf(store.getState().accountLogins, agent, account);
    if (earlier && !loginIsActive(earlier)) dismissAccountLogin(store, earlier);
    const pending: Promise<void> = actions.start(agent, account).catch((err: unknown) => {
      const refused = err as Partial<DaemonRequestError>;
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
  // A login that succeeded, or that another window cancelled, or that has vanished, has nothing more to show.
  useEffect(() => {
    if (refusal) return;
    if (login?.status === "succeeded" || login?.status === "cancelled" || (current === undefined && loginIsActive(lastSeen))) onClose(login?.status === "succeeded");
  }, [login, current, lastSeen, refusal]);
  // Each form is a dialog of its own, whose own focus handling may leave focus on the dialog or on `<body>`: the
  // form's primary button (Log in again, Cancel login) takes it.
  // Not asked for by the user, so its ring shows, but only where the form changed, not when the dialog first opens.
  const wasEnded = useRef(ended);
  useEffect(() => {
    if (ended && !wasEnded.current) ringForAutomaticFocus();
    wasEnded.current = ended;
    primaryRef.current?.focus();
  }, [ended]);

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
  const failure: Failure | undefined = refusal ?? (login?.failure ?? undefined);
  const reason = failure && daemonMessage(language, failure.code, failure.params, failure.message, NO_RECORDS);
  const code = login?.status === "waiting" ? login.device_code : null;
  const progress = t(login?.status === "waiting" ? "settings.accounts.login.waiting" : "settings.accounts.login.starting");
  const timedOut = login?.status === "timed_out" && !refusal;
  const agentChip = <InlineReference dir="auto" wrap>{AGENT_LABEL[agent]}</InlineReference>;

  const presentation = failure && reason !== undefined ? new DaemonRequestError(reason, failure.code, failure.params) : undefined;
  // A failure the application has no words for is its generic sentence as the cause, and where it is classified
  // unexpected, that sentence is "Something unexpected went wrong" with the request to report it.
  const uncatalogued = !!presentation && !timedOut && isUncataloguedFailure(presentation);
  const classification = uncatalogued ? classifyError(presentation) : undefined;
  const unexpected = classification?.kind === "unexpected";
  // The report is made from the daemon's own text, not `reason`, which is worded for the user.
  const issue = useIssueReport(unexpected && failure ? [{ error: new DaemonRequestError(failure.message, failure.code, failure.params), unexpected: true }] : undefined);

  if (ended) {
    const remedy = presentation && errorRemedy(presentation);
    const raw = presentation && rawErrorDetails(presentation);
    // A login that failed with no cause the agent named (`login_failed`) has a summary that only says again that it
    // failed, which the lead has said already; only a failed login has a cause at all.
    const cause = presentation && !timedOut && presentation.code !== "login_failed"
      ? uncatalogued ? t(uncataloguedSummaryKey(presentation)) : <DaemonErrorMessage error={presentation} part="summary" />
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
        dismissable={false}
        headerLeading={<AlertDialog.Icon status="danger" />}
        title={t(timedOut ? "settings.accounts.login.timedOutTitle" : "settings.accounts.login.failedTitle")}
        onClose={close}
        footer={(
          <>
            {unexpected && issue.link}
            <Button type="button" variant="secondary" onPress={close}>{t("common.close")}</Button>
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
      <div className="mt-2 flex items-center gap-2 text-sm">
        <Spinner size="sm" aria-hidden />
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
