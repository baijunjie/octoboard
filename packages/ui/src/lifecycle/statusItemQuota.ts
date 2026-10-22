import { consoleAccount } from "../accountChoices";
import { AGENT_SHORT_NAME, accountsByAgent, defaultAccountName, listedAgents } from "../agents";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Language } from "../i18n/languages";
import type { StatusItemSubmenu, StatusItemSubmenuItem } from "../platform";
import { joinPhrases } from "../i18n/joinPhrases";
import type { Account, AccountQuota, Agent, AgentAvailability, Console, QuotaWindow, Settings } from "../protocol";
import { isNotLoggedIn, orderedQuotaWindows, planLabel, quotaResetsIn, quotaWindowLabel, remainingPercentAt, twoLayerWindows, usageBarWindows } from "../quota/quotaFormat";
import { formatRelativeTime, isJustNow } from "../relativeTime";
import { accountQuotaLabel } from "../quota/sessionQuota";
import { accountQuotaOf } from "../quota/accountQuotaOf";
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
  /** The agents the user added, `null` while the daemon has not decided them. */
  addedAgents: Agent[] | null;
  /** The names the user gave the default accounts, by agent. */
  defaultAccountNames?: Settings["default_account_names"];
  agentAvailability: Map<Agent, AgentAvailability>;
  accountQuotas: AccountQuota[];
}

/** A window as the shell shows it: its name, its detail ("82% remaining · resets in 1h 57m"), how full its
 * bar is, and the whole as one phrase. */
function windowParts(t: Translate, source: StatusItemQuotaSource, window: QuotaWindow) {
  const resets = quotaResetsIn(t, window, source.now);
  const percent = remainingPercentAt(window, source.now);
  const name = quotaWindowLabel(t, window);
  return {
    name,
    detail: resets ? t("statusItem.quota.detail", { percent, resets }) : t("statusItem.quota.detailNoReset", { percent }),
    label: resets ? t("statusItem.quota.window", { window: name, percent, resets }) : t("statusItem.quota.windowNoReset", { window: name, percent }),
    fraction: percent / 100,
  };
}

/** One account's lines: its header with the plan (and, for a current reading, how long ago it was read, at its right),
 * then per window a row (the shell draws its bar), or a single note when there is nothing current to show.
 * The 5-hour window and the account-wide weekly one are one two-layer item, as in Settings. */
function accountLines(t: Translate, source: StatusItemQuotaSource, agent: Agent, account: Account | null): StatusItemSubmenuItem[] {
  const name = account ? isolate(account.name) : defaultAccountName(t, source.defaultAccountNames, agent);
  const quota = accountQuotaOf(source.accountQuotas, agent, account?.id ?? null);
  const title = quota?.plan ? t("statusItem.quota.account", { account: name, plan: planLabel(quota.plan) }) : name;
  if (!quota) return [{ kind: "header", title }, { kind: "note", text: t("quota.loading") }];
  // An entry with no windows is unavailable whether or not it says why, which the menu does not, except for an
  // account that is not logged in: a state to be told in a word, not a failure.
  if (quota.windows.length === 0) return [{ kind: "header", title }, { kind: "note", text: t(isNotLoggedIn(quota.unavailable) ? "statusItem.quota.notLoggedIn" : "quota.unavailable") }];
  const age = formatRelativeTime(source.language, quota.fetched_at, source.now);
  // The out-of-date note says when the reading was taken, so the header does not.
  const justNow = isJustNow(quota.fetched_at, source.now);
  const header: StatusItemSubmenuItem = quota.unavailable ? { kind: "header", title } : {
    kind: "header",
    title,
    detail: justNow ? t("quota.updatedNow") : t("quota.updated", { age }),
    label: justNow ? t("statusItem.quota.updatedNow", { header: title }) : t("statusItem.quota.updated", { header: title, age }),
  };
  const lines: StatusItemSubmenuItem[] = [header];
  if (quota.unavailable) lines.push({ kind: "note", text: t("quota.stale", { age }) });
  const layers = twoLayerWindows(quota.windows);
  for (const window of orderedQuotaWindows(quota.windows)) {
    if (layers?.weekly && window === layers.weekly) continue;
    if (layers?.weekly && window === layers.session) {
      const session = windowParts(t, source, window);
      const weekly = windowParts(t, source, layers.weekly);
      lines.push({
        kind: "meter",
        session: { name: session.name, detail: session.detail, fraction: session.fraction },
        weekly: { name: weekly.name, detail: weekly.detail, fraction: weekly.fraction },
        label: joinPhrases(source.language, [session.label, weekly.label]),
        agent,
      });
      continue;
    }
    const { name: windowName, detail, label, fraction } = windowParts(t, source, window);
    lines.push({ kind: "window", name: windowName, detail, label, fraction, agent });
  }
  return lines;
}

/** One submenu per added agent that is available, in the order they were added — "Claude quota ▸" —
 * listing its default account, then the accounts the user owns, each set off by a separator. Nothing
 * for an agent that is not added, not available or not yet determined: unlike the rail's panel, which
 * lists an undetermined agent, a submenu without readings to fill it would be empty. */
export function buildQuotaSubmenus(t: Translate, source: StatusItemQuotaSource, currentConsole?: Console): StatusItemSubmenu[] {
  const owned = new Map(accountsByAgent(source.accounts).map(({ agent, accounts }) => [agent, accounts]));
  return listedAgents(source.addedAgents).flatMap((agent): StatusItemSubmenu[] => {
    if (source.agentAvailability.get(agent)?.availability !== "available") return [];
    const accounts: Array<Account | null> = [null, ...(owned.get(agent) ?? [])];
    const items = accounts.flatMap((account, index): StatusItemSubmenuItem[] => [
      ...(index > 0 ? [{ kind: "separator" as const }] : []),
      ...accountLines(t, source, agent, account),
    ]);
    const accountId = currentConsole ? consoleAccount(currentConsole, agent, source.accounts) : undefined;
    const reading = accountId !== undefined ? accountQuotaOf(source.accountQuotas, agent, accountId) : undefined;
    const bar = reading && !reading.unavailable ? usageBarWindows(reading.windows) : undefined;
    const accountName = source.accounts.find((account) => account.id === accountId)?.name;
    return [{
      title: t("statusItem.quota", { agent: AGENT_SHORT_NAME[agent] }),
      icon: agentIconKey(agent),
      ...(bar ? {
        quota_remaining: remainingPercentAt(bar.main, source.now),
        ...(bar.thin ? { quota_weekly_remaining: remainingPercentAt(bar.thin, source.now) } : {}),
        quota_label: accountQuotaLabel(t, agent, accountName, bar, source.defaultAccountNames, source.now),
      } : {}),
      items,
    }];
  });
}
