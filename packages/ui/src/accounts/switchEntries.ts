import { orderedAccounts } from "./accountOrder";
import { defaultAccountName } from "../agents/agents";
import type { Translate } from "../i18n/catalog";
import type { Account, AccountQuota, AccountSharing, Agent, QuotaWindow, Session, Settings } from "../protocol";
import { accountQuotaOf } from "./accountQuotaOf";
import { accountDisabled, accountSignedOutUnavailable, quotaSignedOut, sharingChangeRuns, type QuotaReadings } from "./choiceEntries";
import { exhaustedQuotaWindow } from "./quotaFormat";
import { sessionAccountName } from "../sessions/sessionLabel";
import type { Reason } from "../format/reason";

/** One account a session can be moved to or is on, as the Switch account submenu lists it. */
export interface SwitchEntry {
  account: string | null;
  name: string;
  current: boolean;
  /** `name` is the directory of an account that has been removed, not a name the user typed. */
  isPath: boolean;
  /** The window that keeps the account from taking the session over (see `exhaustedQuotaWindow`); the
   * entry is listed, but not offered. */
  exhaustedBy: QuotaWindow | undefined;
  /** What the daemon last read of the account's quota, if it has; none for a removed account's directory. */
  quota: AccountQuota | undefined;
  /** A change of the account's sharing runs; the daemon refuses a switch onto it, so it is listed, but not offered. */
  changingSharing: boolean;
  /** The last reading found the account not logged in; the daemon refuses a switch onto it, so it is listed, but not
   * offered. */
  signedOut: boolean;
  /** The user turned the account off; the menu leaves it out unless it is the session's own. */
  disabled: boolean;
}

/** Why a Switch account entry cannot be chosen apart from its quota being used up, or `undefined`
 * when nothing else keeps it: the entry is not logged in, an account switch holds the session (`switching`, see
 * `holdsAccountSwitch`), the entry's own sharing change runs, or the account the session is on is changing its sharing
 * (`ownChanging`, which keeps every other account from taking it). Not being logged in lasts, so it is checked first and comes with a
 * `reason` to draw under the entry's name; the others end on their own, so their `detail` is for a message where a
 * form needs one, and a menu or list disables the entry without a reason. The current entry is never blocked, as it
 * is never offered. The Switch account submenu and the form of a switch request share these rules. */
export function switchEntryBlock(
  t: Translate,
  entry: Pick<SwitchEntry, "current" | "changingSharing" | "signedOut">,
  session: { switching: boolean; ownChanging: boolean },
): { detail: string; reason?: Reason } | undefined {
  if (entry.current) return undefined;
  if (entry.signedOut) return accountSignedOutUnavailable(t);
  if (session.switching) return { detail: t("accountChoice.switching.detail") };
  if (entry.changingSharing) return { detail: t("accountChoice.sharingChange.detail") };
  if (session.ownChanging) return { detail: t("accountChoice.ownSharingChange.detail") };
  return undefined;
}

/** Why a session cannot be handed off, or `undefined` when nothing keeps it: it is locked, it is in the middle of a
 * turn, an account switch holds it (`switching`, see `holdsAccountSwitch`), its own account is changing its sharing
 * (`ownChanging`), or it is still starting, which the daemon refuses. The lasting holds (locked, a turn that can run
 * for long) are checked first and come with a `reason` to draw under a menu item; the others end on their own, come with
 * none, and lose to a lasting hold. The Hand off menu item and the form of a handoff request share these rules. */
export function handoffBlock(
  t: Translate,
  session: Pick<Session, "status" | "locked">,
  { switching, ownChanging }: { switching: boolean; ownChanging: boolean },
): { detail: string; reason?: Reason } | undefined {
  if (session.locked) return { detail: t("accountChoice.handoff.locked"), reason: { state: t("accountChoice.sessionLocked.reason") } };
  if (session.status === "working") return { detail: t("accountChoice.handoff.working"), reason: { state: t("accountChoice.handoff.working.reason") } };
  if (switching) return { detail: t("accountChoice.switching.detail") };
  if (ownChanging) return { detail: t("accountChoice.ownSharingChange.detail") };
  if (session.status === "starting") return { detail: t("accountChoice.handoff.starting") };
  return undefined;
}

/** The accounts of the session's own agent in the saved account order, with the one the session is
 * on marked current. Another agent's accounts never appear: a switch stays within one agent. A
 * session on an account that has since been removed keeps it as its current entry, named as
 * everywhere else a session's account is (`sessionAccountName`: the directory the session
 * recorded), so the menu still says where the session is; with no directory recorded there is
 * nothing to name it by and it is left out. The submenu is offered only when there is somewhere to
 * go, that is with more than one entry, turned-off ones counted: the menu lists only the others that are on, but with
 * every other account off it stays, so the user can go to Settings to turn one back on. Without `readings` no entry is
 * used up, without `sharing` none has a sharing change running, and without `disabledDefaults` no default account is
 * disabled. */
export function switchEntries(
  session: Session,
  accounts: Account[],
  t: Translate,
  defaultNames: Settings["default_account_names"],
  readings?: QuotaReadings,
  sharing?: Map<string, AccountSharing>,
  disabledDefaults?: readonly Agent[],
  order?: Settings["account_order"],
): SwitchEntry[] {
  const owned = accounts.filter((account) => account.agent === session.agent);
  const entries = orderedAccounts(session.agent, accounts, order).map((account) => ({ account: account?.id ?? null, name: account?.name ?? defaultAccountName(t, defaultNames, session.agent), isPath: false }));
  const removed = session.account_id && !owned.some((account) => account.id === session.account_id);
  const removedName = removed ? sessionAccountName(t, session, accounts, defaultNames) : undefined;
  if (session.account_id && removedName !== undefined) {
    entries.push({ account: session.account_id, name: removedName, isPath: true });
  }
  return entries.map((entry) => {
    const quota = entry.isPath || !readings ? undefined : accountQuotaOf(readings.quotas, session.agent, entry.account);
    return {
      ...entry,
      current: entry.account === (session.account_id ?? null),
      exhaustedBy: readings ? exhaustedQuotaWindow(quota, readings.now) : undefined,
      quota,
      changingSharing: !entry.isPath && sharingChangeRuns(sharing, entry.account),
      signedOut: quotaSignedOut(quota),
      disabled: !entry.isPath && accountDisabled(accounts, disabledDefaults, session.agent, entry.account),
    };
  });
}
