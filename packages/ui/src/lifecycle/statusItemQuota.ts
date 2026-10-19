import { consoleAccount } from "../accountChoices";
import { AGENT_OPTIONS, AGENT_SHORT_NAME, accountsByAgent } from "../agents";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Language } from "../i18n/languages";
import type { StatusItemSubmenu, StatusItemSubmenuItem } from "../platform";
import type { Account, AccountQuota, Agent, AgentAvailability, Console } from "../protocol";
import { orderedQuotaWindows, primaryQuotaWindow, planLabel, quotaResetsIn, quotaWindowLabel, remainingPercent } from "../quota/quotaFormat";
import { formatRelativeTime } from "../relativeTime";
import { accountQuotaLabel } from "../quota/sessionQuota";
import { accountQuotaOf } from "../quota/accountQuotaOf";
import { agentIconKey } from "./statusItemIconKeys";

/** What the quota submenus are built from. */
export interface StatusItemQuotaSource {
  language: Language;
  /** The moment the countdowns and the age of an out-of-date reading are counted from. */
  now: number;
  accounts: Account[];
  agentAvailability: Map<Agent, AgentAvailability>;
  accountQuotas: AccountQuota[];
}

/** One account's lines: its header with the plan, then per window a row (the shell draws its bar),
 * or a single note when there is nothing current to show. */
function accountLines(t: Translate, source: StatusItemQuotaSource, agent: Agent, account: Account | null): StatusItemSubmenuItem[] {
  const name = account ? isolate(account.name) : t("settings.accounts.defaultName");
  const quota = accountQuotaOf(source.accountQuotas, agent, account?.id ?? null);
  const title = quota?.plan ? t("statusItem.quota.account", { account: name, plan: planLabel(quota.plan) }) : name;
  const lines: StatusItemSubmenuItem[] = [{ kind: "header", title }];
  if (!quota) return [...lines, { kind: "note", text: t("quota.loading") }];
  // An entry with no windows is unavailable whether or not it says why, which the menu does not.
  if (quota.windows.length === 0) return [...lines, { kind: "note", text: t("quota.unavailable") }];
  if (quota.unavailable) lines.push({ kind: "note", text: t("quota.stale", { age: formatRelativeTime(source.language, quota.fetched_at, source.now) }) });
  for (const window of orderedQuotaWindows(quota.windows)) {
    const resets = quotaResetsIn(t, window, source.now);
    const percent = remainingPercent(window);
    const windowName = quotaWindowLabel(t, window);
    lines.push({
      kind: "window",
      name: windowName,
      detail: resets ? t("statusItem.quota.detail", { percent, resets }) : t("statusItem.quota.detailNoReset", { percent }),
      label: resets
        ? t("statusItem.quota.window", { window: windowName, percent, resets })
        : t("statusItem.quota.windowNoReset", { window: windowName, percent }),
      fraction: percent / 100,
      agent,
    });
  }
  return lines;
}

/** One submenu per available agent — "Claude quota ▸" — listing its default account, then the
 * accounts the user owns, each set off by a separator. Nothing for an agent that is not available
 * or not yet determined. */
export function buildQuotaSubmenus(t: Translate, source: StatusItemQuotaSource, currentConsole?: Console): StatusItemSubmenu[] {
  const owned = new Map(accountsByAgent(source.accounts).map(({ agent, accounts }) => [agent, accounts]));
  return AGENT_OPTIONS.flatMap(({ value: agent }): StatusItemSubmenu[] => {
    if (source.agentAvailability.get(agent)?.availability !== "available") return [];
    const accounts: Array<Account | null> = [null, ...(owned.get(agent) ?? [])];
    const items = accounts.flatMap((account, index): StatusItemSubmenuItem[] => [
      ...(index > 0 ? [{ kind: "separator" as const }] : []),
      ...accountLines(t, source, agent, account),
    ]);
    const accountId = currentConsole ? consoleAccount(currentConsole, agent, source.accounts) : undefined;
    const reading = accountId !== undefined ? accountQuotaOf(source.accountQuotas, agent, accountId) : undefined;
    const window = reading && !reading.unavailable ? primaryQuotaWindow(reading.windows) : undefined;
    const accountName = source.accounts.find((account) => account.id === accountId)?.name;
    return [{
      title: t("statusItem.quota", { agent: AGENT_SHORT_NAME[agent] }),
      icon: agentIconKey(agent),
      ...(window ? {
        quota_remaining: remainingPercent(window),
        quota_label: accountQuotaLabel(t, agent, accountName, window),
      } : {}),
      items,
    }];
  });
}
