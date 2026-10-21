import type { Reason } from "../components/ReasonText";
import type { Translate } from "../i18n/catalog";
import type { AccountQuota, Agent, QuotaWindow } from "../protocol";
import { AccountUsageBar } from "./AccountUsageBar";
import { quotaUsedUpDetail, quotaUsedUpShort, usageBarSummary, usageBarWindows } from "./quotaFormat";

/** How a list of accounts shows one: its usage bar as the `icon`, and in words what the bar only
 * shows to the eye, for assistive technology. For an account whose quota is used up (`exhaustedBy`)
 * that is the full reason as the `detail` and the short one as the `reason` to draw under its name;
 * for the others, what remains of their quota. A reading that is out of date has no bar and no
 * remainder to state. */
export function accountUsage(
  t: Translate,
  agent: Agent,
  quota: AccountQuota | undefined,
  exhaustedBy: QuotaWindow | undefined,
  now: number,
): { icon: React.ReactElement } & ({ detail?: undefined; reason?: undefined } | { detail: string; reason?: Reason }) {
  const icon = <AccountUsageBar agent={agent} quota={quota} now={now} />;
  if (exhaustedBy) return { icon, detail: quotaUsedUpDetail(t, exhaustedBy, now), reason: quotaUsedUpShort(t, exhaustedBy, now) };
  const bar = quota && !quota.unavailable ? usageBarWindows(quota.windows) : undefined;
  return bar ? { icon, detail: usageBarSummary(t, bar, now) } : { icon };
}
