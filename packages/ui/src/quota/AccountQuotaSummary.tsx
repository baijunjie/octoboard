import React from "react";

import { InlineChip } from "../components/InlineChip";
import { codedMessageNode } from "../components/DaemonErrorMessage";
import { daemonMessage } from "../daemonMessage";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { QuotaUnavailable } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { useNow } from "../useNow";
import { QuotaMeter } from "./QuotaMeter";
import { isLoggedIn, isNotLoggedIn, orderedQuotaWindows, planLabel } from "./quotaFormat";
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

/** The badge of an account's login: logged in, or not logged in yet; nothing while the state is not known (a reading
 * still to come, or one that failed for another reason). It carries its state in words and a dot, not in colour alone. */
function LoginStateBadge({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  const loggedIn = isLoggedIn(quota);
  if (!loggedIn && !(typeof quota === "object" && isNotLoggedIn(quota.unavailable))) return null;
  return (
    <InlineChip color={loggedIn ? "success" : "warning"} className="shrink-0" startContent={<span aria-hidden="true" className="size-1.5 rounded-full bg-current" />}>
      {t(loggedIn ? "settings.accounts.loggedIn" : "settings.accounts.notLoggedIn")}
    </InlineChip>
  );
}

/** The line naming an account: its `name`, its plan chip when the agent names one, with `showLoginState` a badge
 * for an account known to be logged in or not, and a `help` tip that ends the line. */
export function AccountNameLine({ name, quota, help, showLoginState = false, children }: { children?: React.ReactNode; name: React.ReactNode; quota: QuotaReading; help?: React.ReactNode; showLoginState?: boolean }): React.ReactElement {
  return (
    <div className="flex items-center gap-2 text-sm font-medium">
      <div className="min-w-0">{name}</div>
      <QuotaPlan quota={quota} />
      {showLoginState && <LoginStateBadge quota={quota} />}
      {children}
      {help}
    </div>
  );
}

/** One account's quota windows as meters, and what stands in their place or beside them when there
 * is nothing current to show:
 *
 * - `undefined`: nothing at all, the account has no reading to wait for;
 * - `"loading"`: the first reading is still on its way;
 * - an entry with no windows: the reason it is unavailable;
 * - windows and a failure: the last good readings stay, marked as out of date with when they were
 *   taken and why the newer attempt failed. */
export function AccountQuotaSummary({ quota, single }: {
  quota: QuotaReading;
  /** One window to a row whatever the viewport, for a surface narrower than the window. */
  single?: boolean;
}): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  if (quota === "loading") return <div className="text-sm text-app-muted">{t("quota.loading")}</div>;
  if (!quota) return null;
  const { windows, unavailable } = quota;
  if (windows.length === 0) {
    // An account that is not logged in is a state, not a failure: the line says what to do and nothing else.
    if (isNotLoggedIn(unavailable)) {
      return <div className="text-sm text-app-muted"><QuotaFailure unavailable={unavailable} /></div>;
    }
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
      <div className={`grid grid-cols-1 gap-x-6 gap-y-3 ${single ? "" : "sm:grid-cols-2"}`}>
        {orderedQuotaWindows(windows).map((window) => (
          <QuotaMeter key={`${window.kind}:${window.label ?? ""}`} agent={quota.agent} window={window} />
        ))}
      </div>
    </div>
  );
}
