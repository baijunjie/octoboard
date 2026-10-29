import React, { useEffect, useState } from "react";

import { reportExternalLinkFailure } from "../components/ExternalLink";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { useWindowAttended } from "../components/useWindowAttended";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { useOptionalPlatform } from "../platform/react";
import type { AccountLogin, Agent } from "../protocol";
import { accountLoginKey, dismissAccountLogin, trackAccountLogin, useDaemon, useDaemonStore } from "../store";

/** How long a login that succeeded is kept, for its status region to be announced, before it clears itself: unbroken
 * time with the window attended, started over each time the user comes back, so it is not gone before they are back
 * to hear it. Longer than `AFTER_FOCUS_MOVE_DELAY_MS`, which the announcement waits first. */
const SUCCESS_SHOWN_MS = 6000;

/** The login of an account (`null` for an agent's default account), if one is running or has just ended. */
export function accountLoginOf(logins: AccountLogin[], agent: Agent, account: string | null): AccountLogin | undefined {
  return logins.find((login) => login.agent === agent && login.account === account);
}

/** The login of an account (`null` for an agent's default account) when it has succeeded, read from the store as it is
 * now: what the account's `AccountLoginAnnouncer` is given. */
export function useSucceededAccountLogin(agent: Agent, account: string | null): AccountLogin | undefined {
  const login = useDaemonStore((s) => accountLoginOf(s.accountLogins, agent, account));
  return login?.status === "succeeded" ? login : undefined;
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

/** The account row's status region, which says once that a login succeeded: mounted for as long as the row, visually
 * hidden, since the account's badge shows the result. The login dialog closes on that success: until then it hides
 * the row from assistive technology, and closing it moves focus back into Settings, so a status written at that
 * moment would surface already written and be replaced by the announcement of that focus move. The text is written
 * `AFTER_FOCUS_MOVE_DELAY_MS` after the success instead. The user is usually in the browser authorizing when the
 * login succeeds, and a status written while the window is not attended goes unheard, so then the wait starts once the
 * user is back in the window, after VoiceOver has spoken the control focus is on. A success already there when the row
 * mounts (its tab or section shown again) is old news and is not said. `name` is the account's, so the announcement
 * names its subject. */
export function AccountLoginAnnouncer({ login, name }: { login: AccountLogin | undefined; name: string }): React.ReactElement {
  const t = useT();
  const succeeded = login?.status === "succeeded";
  const [heldAtMount, setHeldAtMount] = useState(succeeded);
  const [due, setDue] = useState(false);
  const attended = useWindowAttended();
  // Keyed on the success, not on the record's identity. Leaving the window before the text is due starts the wait
  // again on the way back; once written it stays, and is not said again.
  useEffect(() => {
    if (!succeeded) {
      setDue(false);
      setHeldAtMount(false);
      return;
    }
    if (heldAtMount || !attended) return;
    const timer = setTimeout(() => setDue(true), AFTER_FOCUS_MOVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [succeeded, attended]);
  return <StatusAnnouncer text={succeeded && due ? t("settings.accounts.login.succeeded", { name: isolate(name) }) : undefined} />;
}

/** Clears a login that succeeded from the state once the window has been attended for `SUCCESS_SHOWN_MS` without a
 * break. Keyed on the login itself: a callback made here is new on every render, which would restart the wait. */
export function useClearSucceededLogin(login: AccountLogin | undefined): void {
  const { store } = useDaemon();
  const attended = useWindowAttended();
  useEffect(() => {
    if (login?.status !== "succeeded" || !attended) return;
    const timer = setTimeout(() => dismissAccountLogin(store, login), SUCCESS_SHOWN_MS);
    return () => clearTimeout(timer);
  }, [login, attended]);
}
