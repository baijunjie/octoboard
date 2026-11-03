import { consoleAccount } from "../accounts/accountInheritance";
import { defaultAccountName, listedAgents, servedAgents } from "../agents/agents";
import { providerShortName } from "../agents/providers";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Language } from "../i18n/languages";
import type { StatusItemSubmenuDraft, StatusItemSubmenuDraftItem } from "../platform";
import { joinPhrases } from "../i18n/joinPhrases";
import type { Account, AccountQuota, Agent, AgentAvailability, Console, ProviderInfo, QuotaWindow, Settings } from "../protocol";
import { exhaustedQuotaWindow, isNotLoggedIn, orderedQuotaWindows, planLabel, quotaNaming, quotaResetsIn, quotaWindowLabel, remainingPercentAt, twoLayerWindows, usageBarWindows, type QuotaNaming } from "../accounts/quotaFormat";
import { formatRelativeTime, isJustNow } from "../format/relativeTime";
import { listedQuotaAccounts, type QuotaRow } from "../quota/quotaListing";
import { accountQuotaLabel } from "../quota/sessionQuota";
import { accountQuotaOf } from "../accounts/accountQuotaOf";
import { agentIconKey } from "./statusItemIconKeys";

const MINUTE = 60_000;

/** When the submenus next read differently for the clock alone, at or after `now`: the earliest reset time still
 * ahead among `quotas`' windows (the window is then drawn full) or the next whole minute of a reading's age, which
 * is the finest the ages and countdowns are worded in. `undefined` for no reading. */
export function nextQuotaTick(quotas: readonly AccountQuota[], now: number): number | undefined {
  const ahead = quotas.flatMap((quota) => [
    ...quota.windows.map((window) => window.resets_at).filter((reset): reset is number => reset != null && reset > now),
    ...(quota.windows.length > 0 ? [quota.fetched_at + (Math.max(0, Math.floor((now - quota.fetched_at) / MINUTE)) + 1) * MINUTE] : []),
  ]);
  return ahead.length > 0 ? Math.min(...ahead) : undefined;
}

/** What the quota submenus are built from. */
export interface StatusItemQuotaSource {
  language: Language;
  /** The moment the countdowns, the age of a reading (in its header, or in the note of an out-of-date one) and
   * whether a window's reset has passed (so that it is drawn full) are decided at. */
  now: number;
  accounts: Account[];
  accountOrder?: Settings["account_order"];
  /** The agents the user added, `null` while the daemon has not decided them. */
  addedAgents: Agent[] | null;
  /** The names the user gave the default accounts, by agent. */
  defaultAccountNames?: Settings["default_account_names"];
  /** The agents whose default account is turned off, which the submenus leave out like an owned account that is off
   * (bar the account the selected running session is on, see `buildQuotaSubmenus`). */
  disabledDefaultAccounts?: Settings["disabled_default_accounts"];
  agentAvailability: Map<Agent, AgentAvailability>;
  accountQuotas: AccountQuota[];
  /** The providers the daemon serves, which name the quota windows. */
  providers: readonly ProviderInfo[];
}

/** A window as the shell shows it: its name, its detail ("82% remaining · resets in 1h 57m"), how full its
 * bar is, and the whole as one phrase. */
function windowParts(t: Translate, source: StatusItemQuotaSource, naming: QuotaNaming, window: QuotaWindow) {
  const resets = quotaResetsIn(t, window, source.now);
  const percent = remainingPercentAt(window, source.now);
  const name = quotaWindowLabel(t, naming, window);
  return {
    name,
    detail: resets ? t("statusItem.quota.detail", { percent, resets }) : t("statusItem.quota.detailNoReset", { percent }),
    label: resets ? t("statusItem.quota.window", { window: name, percent, resets }) : t("statusItem.quota.windowNoReset", { window: name, percent }),
    fraction: percent / 100,
  };
}

/** One account's lines: its header with the plan (and, with windows to show, how long ago they were read, at its right),
 * then per window a row (the shell draws its bar), or a single note when there is nothing current to show.
 * The 5-hour window and the account-wide weekly one are one two-layer item, as in Settings. */
function accountLines(t: Translate, source: StatusItemQuotaSource, agent: Agent, { account, disabled }: QuotaRow): StatusItemSubmenuDraftItem[] {
  const name = account ? isolate(account.name) : defaultAccountName(t, source.defaultAccountNames, agent);
  const quota = accountQuotaOf(source.accountQuotas, agent, account?.id ?? null);
  const planned = quota?.plan ? t("statusItem.quota.account", { account: name, plan: planLabel(agent, quota.plan) }) : name;
  // A turned-off account is listed only while the selected session runs on it, and says so last, after its quota
  // state, as the rail's row puts its badge after the rest.
  const marked = (header: string) => disabled ? t("statusItem.quota.accountDisabled", { account: header, disabled: t("accountChoice.disabled.reason") }) : header;
  if (!quota) return [{ kind: "header", title: marked(planned) }, { kind: "note", text: t("quota.loading") }];
  // An entry with no windows is unavailable whether or not it says why, which the menu does not, except for an
  // account that is not logged in: a state to be told in a word, not a failure.
  if (quota.windows.length === 0) return [{ kind: "header", title: marked(planned) }, { kind: "note", text: t(isNotLoggedIn(quota.unavailable) ? "statusItem.quota.notLoggedIn" : "quota.unavailable") }];
  const naming = quotaNaming(quota, source.providers);
  const status = exhaustedQuotaWindow(quota, source.now) ? "used_up"
    : quota.unavailable || !usageBarWindows(naming) ? "unknown" : "available";
  const statusTitle = marked(t(status === "used_up" ? "statusItem.quota.accountUsedUp"
    : status === "unknown" ? "statusItem.quota.accountUnknown" : "statusItem.quota.accountAvailable", { account: planned }));
  const age = formatRelativeTime(source.language, quota.fetched_at, source.now);
  // The header says when the windows were read, at its right, even when the latest read failed; a native menu has
  // no popover for the reason, so the out-of-date note only says that the last read failed.
  const justNow = isJustNow(quota.fetched_at, source.now);
  const lines: StatusItemSubmenuDraftItem[] = [{
    kind: "header",
    title: statusTitle,
    detail: justNow ? t("quota.updatedNow") : t("quota.updated", { age }),
    label: justNow ? t("statusItem.quota.updatedNow", { header: statusTitle }) : t("statusItem.quota.updated", { header: statusTitle, age }),
  }];
  if (quota.unavailable) lines.push({ kind: "note", text: t("statusItem.quota.stale") });
  // The shell draws a used-up account's bars in grey, so that it recedes beside the accounts that can still be used.
  const usedUp = status === "used_up" ? { used_up: true } : {};
  const layers = twoLayerWindows(quota.windows);
  for (const window of orderedQuotaWindows(quota.windows)) {
    if (layers?.weekly && window === layers.weekly) continue;
    if (layers?.weekly && window === layers.session) {
      const session = windowParts(t, source, naming, window);
      const weekly = windowParts(t, source, naming, layers.weekly);
      lines.push({
        kind: "meter",
        session: { name: session.name, detail: session.detail, fraction: session.fraction },
        weekly: { name: weekly.name, detail: weekly.detail, fraction: weekly.fraction },
        label: joinPhrases(source.language, [session.label, weekly.label]),
        agent,
        ...usedUp,
      });
      continue;
    }
    const { name: windowName, detail, label, fraction } = windowParts(t, source, naming, window);
    lines.push({ kind: "window", name: windowName, detail, label, fraction, agent, ...usedUp });
  }
  return lines;
}

/** One submenu per added agent that is available, in the order they were added — "Claude quota ▸" —
 * listing its accounts in the saved account order, each set off by a separator, leaving out an
 * account that is turned off, except the one `inUse` names (the account the selected running session is on), which
 * is listed and marked "Disabled". Nothing for an agent that is not added, not available or not yet determined, or
 * whose accounts are all turned off: unlike the rail's panel, which lists an undetermined agent, a submenu
 * without readings to fill it would be empty. */
export function buildQuotaSubmenus(t: Translate, source: StatusItemQuotaSource, currentConsole?: Console, inUse?: { agent: Agent; account: string | null }): StatusItemSubmenuDraft[] {
  const available = listedAgents(source.addedAgents, servedAgents(source.providers)).filter((agent) => source.agentAvailability.get(agent)?.availability === "available");
  return listedQuotaAccounts(available, source, inUse).flatMap(({ agent, rows }): StatusItemSubmenuDraft[] => {
    const items = rows.flatMap((row, index): StatusItemSubmenuDraftItem[] => [
      ...(index > 0 ? [{ kind: "separator" as const }] : []),
      ...accountLines(t, source, agent, row),
    ]);
    const accountId = currentConsole ? consoleAccount(currentConsole, agent, source.accounts) : undefined;
    // A turned-off account has no lines in the submenu unless it is the one in use, so otherwise its bar and name would
    // head a list that does not hold it.
    const listed = accountId !== undefined && rows.some(({ account }) => (account?.id ?? null) === accountId);
    const reading = listed ? accountQuotaOf(source.accountQuotas, agent, accountId) : undefined;
    const bar = reading && !reading.unavailable ? usageBarWindows(quotaNaming(reading, source.providers)) : undefined;
    const accountName = source.accounts.find((account) => account.id === accountId)?.name;
    return [{
      title: t("statusItem.quota", { agent: isolate(providerShortName(t, source.providers, agent)) }),
      icon: agentIconKey(agent),
      ...(bar ? {
        quota_remaining: remainingPercentAt(bar.main, source.now),
        ...(bar.thin ? { quota_weekly_remaining: remainingPercentAt(bar.thin, source.now) } : {}),
        quota_label: accountQuotaLabel(t, source.providers, agent, accountName, bar, source.defaultAccountNames, source.now),
      } : {}),
      items,
    }];
  });
}
