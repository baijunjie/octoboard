import type { StoreApi } from "zustand";

import { reportSwitchFailures, type SwitchFailure } from "./accountSwitchFailureAlert";
import { errorPresentation } from "./errorPresentation";
import type { Session } from "./protocol";
import { setAccountSwitchesQueued, type State } from "./store";

/** How many switches of a batch run at once; the rest wait their turn. */
export const ACCOUNT_SWITCH_CONCURRENCY = 4;

/** Whether `session` is one a switch to `account` (`null`, the default account) still applies to. */
export function canSwitchTo(session: Session, account: string | null): boolean {
  return session.status !== "archived" && (session.account_id ?? null) !== account;
}

/** What a batch of switches needs besides the sessions and the account. */
export interface AccountSwitchBatchOptions {
  /** The target account's name, for the alert and the announcement. */
  accountName: string;
  /** Switches one session; rejects with the reason when the switch did not happen. */
  switchOne: (sessionId: string, account: string | null) => Promise<void>;
  /** Called with the outcome of this batch. */
  onFinished?: (result: { switched: number; failed: number }) => void;
}

/**
 * Switches the sessions `ids` to `account`, at most `ACCOUNT_SWITCH_CONCURRENCY` at a time, in the
 * background: it is app-level, not tied to the dialog that asked, which closes at once. The set is
 * fixed here. A session shows as switching its account from now until its own turn is over, queued
 * ones included (`setAccountSwitchesQueued`); one that was deleted, archived or put on `account`
 * before its turn is skipped, and a failure does not stop the others. One error alert lists the
 * failures once the batch is done. A session already in a batch of this window is not added again,
 * so batches that overlap never touch each other's sessions. Resolves once the batch is done.
 */
export async function runAccountSwitchBatch(
  store: StoreApi<State>,
  ids: readonly string[],
  account: string | null,
  options: AccountSwitchBatchOptions,
): Promise<void> {
  const held = store.getState().queuedAccountSwitches;
  const queue = ids.filter((id, index) => !held.has(id) && ids.indexOf(id) === index);
  if (queue.length === 0) {
    options.onFinished?.({ switched: 0, failed: 0 });
    return;
  }
  setAccountSwitchesQueued(store, queue, true);
  const failures: SwitchFailure[] = [];
  let switched = 0;
  const next = [...queue];
  const worker = async () => {
    for (let id = next.shift(); id !== undefined; id = next.shift()) {
      const session = store.getState().sessions.get(id);
      if (session && canSwitchTo(session, account)) {
        try {
          await options.switchOne(id, account);
          switched += 1;
        } catch (error) {
          failures.push({ session: store.getState().sessions.get(id) ?? session, error: errorPresentation(error) });
        }
      }
      setAccountSwitchesQueued(store, [id], false);
    }
  };
  await Promise.all(Array.from({ length: Math.min(ACCOUNT_SWITCH_CONCURRENCY, queue.length) }, worker));
  if (failures.length) reportSwitchFailures(failures, options.accountName);
  options.onFinished?.({ switched, failed: failures.length });
}
