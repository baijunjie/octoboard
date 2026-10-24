import React from "react";

import { useT } from "../i18n/react";
import { useNow } from "../useNow";
import { AccountQuotaNotice, NoUsageReading } from "./AccountQuotaSummary";
import { AccountUsageBar } from "./AccountUsageBar";
import { usageBarSummary, usageBarWindows } from "./quotaFormat";
import type { QuotaReading } from "./useAccountQuota";

/** An account's usage in a table row: the picker's two-layer usage bar (`AccountUsageBar`, the 5-hour window with the
 * weekly one beneath it, else the primary window; model-specific windows are left to the details), with the same words
 * for assistive technology as the picker's option (`usageBarSummary`). Its only notice is the reading still on its way
 * (`AccountQuotaNotice`); without a current reading it is a muted dash (`NoUsageReading`), followed by the failure mark
 * when the read failed or the out-of-date mark when the windows held are the last good ones. */
export function AccountUsageCell({ quota }: { quota: QuotaReading }): React.ReactElement {
  const t = useT();
  const now = useNow();
  if (quota === "loading") return <AccountQuotaNotice quota={quota} />;
  const bar = quota && !quota.unavailable ? usageBarWindows(quota) : undefined;
  if (!quota || !bar) {
    return <NoUsageReading quota={quota} />;
  }
  return (
    <>
      <AccountUsageBar agent={quota.agent} quota={quota} now={now} className="w-24" edge={false} />
      <span className="sr-only">{usageBarSummary(t, bar, now)}</span>
    </>
  );
}
