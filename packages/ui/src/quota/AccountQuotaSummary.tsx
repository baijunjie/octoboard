import React from "react";

import { InlineChip } from "../components/InlineChip";
import { codedMessageNode } from "../components/DaemonErrorMessage";
import { daemonMessage } from "../daemonMessage";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { QuotaUnavailable } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { useNow } from "../useNow";
import { QuotaMeter } from "./QuotaMeter";
import { planLabel } from "./quotaFormat";
import type { QuotaReading } from "./useAccountQuota";

const NO_RECORDS = { consoles: new Map(), projects: new Map(), sessions: new Map() };

/** Why a reading failed, in the current language with the technical literals of its message set
 * apart. None of the quota messages names a record, so none is given. */
function QuotaFailure({ unavailable }: { unavailable: QuotaUnavailable }): React.ReactElement {
  const language = useCurrentLanguage();
  const text = daemonMessage(language, unavailable.code, unavailable.params, unavailable.message, NO_RECORDS);
  return <>{codedMessageNode(unavailable, text)}</>;
}

/** The account's plan, in display case ("Max", "Plus", "SuperGrok"); nothing when the agent does
 * not say. */
export function QuotaPlan({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  if (typeof quota !== "object" || !quota.plan) return null;
  return <InlineChip className="shrink-0">{planLabel(quota.plan)}</InlineChip>;
}

/** One account's quota windows as meters, and what stands in their place or beside them when there
 * is nothing current to show:
 *
 * - `undefined`: nothing at all, the account has no reading to wait for;
 * - `"loading"`: the first reading is still on its way;
 * - an entry with no windows: the reason it is unavailable;
 * - windows and a failure: the last good readings stay, marked as out of date with when they were
 *   taken and why the newer attempt failed. */
export function AccountQuotaSummary({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  if (quota === "loading") return <div className="text-sm text-app-muted">{t("quota.loading")}</div>;
  if (!quota) return null;
  const { windows, unavailable } = quota;
  if (windows.length === 0) {
    return unavailable && (
      <div className="text-sm text-app-muted">
        <div className="font-medium">{t("quota.unavailable")}</div>
        <div><QuotaFailure unavailable={unavailable} /></div>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-3">
      {unavailable && (
        <div className="text-sm text-app-muted">
          <div className="font-medium">{t("quota.stale", { age: formatRelativeTime(language, quota.fetched_at, now) })}</div>
          <div><QuotaFailure unavailable={unavailable} /></div>
        </div>
      )}
      <div className="grid grid-cols-1 gap-x-6 gap-y-3 sm:grid-cols-2">
        {windows.map((window) => (
          <QuotaMeter key={`${window.kind}:${window.label ?? ""}`} agent={quota.agent} window={window} />
        ))}
      </div>
    </div>
  );
}
