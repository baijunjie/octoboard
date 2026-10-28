import React from "react";

import { StatusBadge } from "../components/StatusBadge";
import { useT } from "../i18n/react";
import type { QuotaWindow } from "../protocol";
import { useNow } from "../useNow";
import { AccountQuotaNotice, NoUsageReading, QuotaStaleMark } from "./AccountQuotaSummary";
import { AccountUsageBar } from "./AccountUsageBar";
import { quotaUsedUpDetail, usageBarSummary, usageBarWindows } from "./quotaFormat";
import type { QuotaReading } from "./useAccountQuota";

/** An account's usage in a table row: the picker's two-layer usage bar (`AccountUsageBar`, the 5-hour window with the
 * weekly one beneath it, else the primary window; model-specific windows are left to the details), with the same words
 * for assistive technology as the picker's option (`usageBarSummary`). With the `exhausted` window of a used-up account
 * (`exhaustedQuotaWindow`) the bar is grey, followed by the "Limit reached" badge, and the words say which window is
 * used up (`quotaUsedUpDetail`) in place of what remains; the badge is hidden from assistive technology, which those
 * words already tell. Windows kept from before a failed read are drawn as any others, followed by the out-of-date mark
 * (`QuotaStaleMark`). Its only notice is the reading still on its way (`AccountQuotaNotice`); with no windows to draw
 * it is a muted dash (`NoUsageReading`), followed by the failure mark when the read failed. */
export function AccountUsageCell({ quota, exhausted }: { quota: QuotaReading; exhausted?: QuotaWindow }): React.ReactElement {
  const t = useT();
  const now = useNow();
  if (quota === "loading") return <AccountQuotaNotice quota={quota} />;
  const bar = quota ? usageBarWindows(quota) : undefined;
  if (!quota || !bar) {
    return <NoUsageReading quota={quota} />;
  }
  return (
    <div className="flex items-center gap-2">
      <AccountUsageBar agent={quota.agent} quota={quota} now={now} className="w-24" edge={false} usedUp={exhausted !== undefined} stale />
      <span className="sr-only">{exhausted ? quotaUsedUpDetail(t, quota, exhausted, now) : usageBarSummary(t, bar, now)}</span>
      {exhausted && <StatusBadge variant="soft" color="warning" separated={false} aria-hidden="true" className="whitespace-nowrap">{t("quota.usedUp")}</StatusBadge>}
      <QuotaStaleMark quota={quota} />
    </div>
  );
}
