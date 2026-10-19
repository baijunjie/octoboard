import React, { useEffect } from "react";

import { reportExternalLinkFailure } from "../components/ExternalLink";
import { InlineChip } from "../components/InlineChip";
import { Message, useT } from "../i18n/react";
import { useOptionalPlatform } from "../platform/react";
import type { AccountLogin, Agent } from "../protocol";
import { accountLoginKey, dismissAccountLogin, trackAccountLogin, useDaemon, useDaemonStore } from "../store";

/** How long a login that succeeded is kept, for its status region to be announced, before it clears itself. */
const SUCCESS_SHOWN_MS = 6000;

/** The login of an account (`null` for an agent's default account), if one is running or has just ended. */
export function accountLoginOf(logins: AccountLogin[], agent: Agent, account: string | null): AccountLogin | undefined {
  return logins.find((login) => login.agent === agent && login.account === account);
}

/** Whether the login has not ended: the CLI is running and the user may cancel it. */
export function loginIsActive(login: AccountLogin | undefined): boolean {
  return login !== undefined && (login.status === "starting" || login.status === "waiting");
}

/** Starts and cancels an account's login (`null` for an agent's default account); each rejects with the daemon's
 * refusal, which the caller presents. Starting notes that this window asked for the login, which is what has it
 * open the authorization page (see `useOpenLoginPages`). */
export function useAccountLoginActions(): {
  start: (agent: Agent, account: string | null) => Promise<void>;
  cancel: (agent: Agent, account: string | null) => Promise<void>;
} {
  const { request, store } = useDaemon();
  return {
    start: async (agent, account) => {
      trackAccountLogin(store, agent, account, true);
      try {
        await request({ type: "start_account_login", agent, account });
      } catch (err) {
        trackAccountLogin(store, agent, account, false);
        throw err;
      }
    },
    cancel: async (agent, account) => {
      await request({ type: "cancel_account_login", agent, account });
    },
  };
}

/** Opens the authorization page of a login this window started the moment the daemon has it, once, unless the
 * agent's own CLI opens it (Grok Build). A login another window started is left to that window, and one already
 * running when this window connected has had its page opened or is not this window's to open. Mounted for as long
 * as the window lives, not as long as Settings is open: the page opens whichever section is in view, and whether or
 * not Settings is still there. */
export function useOpenLoginPages(): void {
  const t = useT();
  const { store } = useDaemon();
  const platform = useOptionalPlatform();
  const logins = useDaemonStore((s) => s.accountLogins);
  useEffect(() => {
    for (const login of logins) {
      const url = login.authorization_url;
      if (!url || login.status !== "waiting") continue;
      // Read from the store, not from the render: the effect may run twice for one state.
      if (!store.getState().loginsStartedHere.includes(accountLoginKey(login.agent, login.account))) continue;
      trackAccountLogin(store, login.agent, login.account, false);
      if (login.opened_by_agent) continue;
      void platform?.links?.open(url).catch((cause: unknown) => reportExternalLinkFailure(t, url, cause));
    }
  }, [logins]);
}

/** `useOpenLoginPages` as a component of its own that renders nothing, so that the logins changing renders this and
 * not the component that mounts it. */
export function LoginPageOpener(): null {
  useOpenLoginPages();
  return null;
}

/** What the row's status region says of a login that succeeded: the region stays mounted empty, so that the text
 * arriving in it is announced, and is visually hidden, since the account's badge shows the result. The one live
 * region, so what it holds is plain text and no alert of its own. `name` is the account's, so the announcement names
 * its subject. */
export function AccountLoginAnnouncement({ login, name }: { login: AccountLogin | undefined; name: string }): React.ReactElement | null {
  if (login?.status !== "succeeded") return null;
  return <Message id="settings.accounts.login.succeeded" params={{ name: <InlineChip dir="auto" wrap size="inherit">{name}</InlineChip> }} />;
}

/** Clears a login that succeeded from the state after `SUCCESS_SHOWN_MS`. Keyed on the login itself: a callback made
 * here is new on every render, which would restart the wait. */
export function useClearSucceededLogin(login: AccountLogin | undefined): void {
  const { store } = useDaemon();
  useEffect(() => {
    if (login?.status !== "succeeded") return;
    const timer = setTimeout(() => dismissAccountLogin(store, login), SUCCESS_SHOWN_MS);
    return () => clearTimeout(timer);
  }, [login]);
}
