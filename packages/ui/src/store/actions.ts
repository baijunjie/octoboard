import type { StoreApi } from "zustand";

import type { AccountLogin, Agent, Event, ShellTerminal } from "../protocol";
import { reducer } from "./reducer";
import { accountKey } from "../accounts/accountRecords";
import type { State } from "./state";

/** Notes that this window is starting the login of an account, so that it opens the authorization page when it
 * comes (`tracked`), or that it is not any more: the page has been opened, or the request was refused. */
export function trackAccountLogin(store: StoreApi<State>, agent: Agent, account: string | null, tracked: boolean): void {
  store.setState((state) => reducer(state, { kind: "track_account_login", key: accountKey(agent, account), tracked }));
}

/** Adds `ids` to the sessions account-switch batches have not finished with, or takes them out
 * (`queued` false): only these ids change, so batches that overlap in time never undo each other.
 * Such a session shows as switching its account (`Session.switching_account`) alongside the ones
 * the daemon says are. */
export function setAccountSwitchesQueued(store: StoreApi<State>, ids: readonly string[], queued: boolean): void {
  store.setState((state) => reducer(state, { kind: "queue_account_switches", ids, queued }));
}

/** Drops an ended login from the state, once the user has seen its outcome or it has cleared itself. Does nothing
 * when the login is not the one held, since the account's next login is a new object. */
export function dismissAccountLogin(store: StoreApi<State>, login: AccountLogin): void {
  store.setState((state) => reducer(state, { kind: "dismiss_account_login", login }));
}

/** Applies a daemon `event` to the store, as the daemon connection does for each one it receives. */
export function applyDaemonEvent(store: StoreApi<State>, event: Event): void {
  store.setState((state) => reducer(state, { kind: "event", event }));
}

/** Holds `terminal` as a `shell_terminal_upserted` for it would, unless the store already holds that
 * id. A request's reply and the broadcast of the same change arrive in no fixed order, so a caller
 * that acts on the reply seeds the record first; a record already held came from a broadcast, which
 * is at least as new as the reply, so it stands, and the broadcast of the same creation arriving
 * later changes nothing. */
export function seedShellTerminal(store: StoreApi<State>, terminal: ShellTerminal): void {
  if (store.getState().shellTerminals.has(terminal.id)) return;
  applyDaemonEvent(store, { type: "shell_terminal_upserted", shell_terminal: terminal });
}
