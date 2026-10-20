import { Spinner } from "@heroui/react";
import React, { useEffect, useRef, useState } from "react";

import { Button } from "../components/Button";
import { AGENT_LABEL, defaultAccountName } from "../agents";
import { codedMessageNode } from "../components/DaemonErrorMessage";
import { ExternalLink } from "../components/ExternalLink";
import { InlineChip } from "../components/InlineChip";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { DaemonRequestError } from "../daemon-client";
import { daemonMessage } from "../daemonMessage";
import { Dialog, useRefocusIfLost } from "../dialogs/Dialog";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { AccountLogin, Agent, MessageParams } from "../protocol";
import { dismissAccountLogin, trackAccountLogin, useDaemon, useDaemonStore } from "../store";
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
 * started by another window). It shows where the login is, with the device code and the link to open the
 * authorization page again where there is one, and Cancel login. Closing it by any means ends a login that is
 * still running. A login that succeeded closes it: the account's badge and the row's hidden status region say so.
 * One that failed or timed out, and a refusal to start one, stay in it with the reason and Log in again. */
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
    if ((err as Partial<DaemonRequestError>).code !== "login_not_running") toastError(err as Error);
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
  useRefocusIfLost(() => primaryRef.current, [ended]);

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

  const who = <InlineChip dir="auto" wrap size="inherit">{name}</InlineChip>;
  const failure: Failure | undefined = refusal ?? (login?.failure ?? undefined);
  const reason = failure && daemonMessage(language, failure.code, failure.params, failure.message, NO_RECORDS);
  const code = login?.status === "waiting" ? login.device_code : null;
  const progress = t(login?.status === "waiting" ? "settings.accounts.login.waiting" : "settings.accounts.login.starting");
  return (
    <Dialog
      title={<Message id="settings.accounts.login.dialogTitle" params={{ name: who, agent: AGENT_LABEL[agent] }} />}
      onClose={close}
      footer={ended ? (
        <>
          <Button key="again" ref={primaryRef} autoFocus type="button" variant="primary" onPress={begin}>{t("settings.accounts.login.again")}</Button>
          <Button key="close" type="button" variant="secondary" onPress={close}>{t("common.close")}</Button>
        </>
      ) : (
        <>
          {login?.status === "waiting" && login.authorization_url && (
            <ExternalLink key="reopen" href={login.authorization_url} appearance="button">{t("settings.accounts.login.reopen")}</ExternalLink>
          )}
          <Button key="cancel" ref={primaryRef} autoFocus type="button" variant="secondary" onPress={close}>{t("settings.accounts.login.cancel")}</Button>
        </>
      )}
    >
      {ended ? (
        <div className="flex flex-col gap-2">
          <p>
            {login?.status === "timed_out" && !refusal
              ? <Message id="settings.accounts.login.timedOut" params={{ name: who }} />
              : <Message id="settings.accounts.login.failed" params={{ name: who }} />}
          </p>
          {failure && reason !== undefined && <><p>{codedMessageNode(failure, reason, "summary")}</p>{codedMessageNode(failure, reason, "details")}</>}
        </div>
      ) : (
        <div className="flex items-center gap-2 text-sm">
          <Spinner size="sm" aria-hidden />
          <span aria-hidden>
            {code
              ? <Message id="settings.accounts.login.waitingCode" params={{ code: <InlineChip monospace size="inherit">{code}</InlineChip> }} />
              : progress}
          </span>
          {/* It comes with a focus move — the dialog opening, or Log in again handing focus to Cancel login — so its first
              text waits for that to be announced. */}
          <StatusAnnouncer
            firstWriteDelayMs={AFTER_FOCUS_MOVE_DELAY_MS}
            text={!active ? undefined : code ? t("settings.accounts.login.waitingCode", { code: isolate(code) }) : progress}
          />
        </div>
      )}
    </Dialog>
  );
}
