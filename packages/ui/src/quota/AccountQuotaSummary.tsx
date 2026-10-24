import React from "react";

import { StatusBadge } from "../components/StatusBadge";
import { codedMessageNode, CodedFailureBody } from "../components/DaemonErrorMessage";
import { StatusMark } from "../components/StatusMark";
import { daemonMessage } from "../daemonMessage";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { QuotaUnavailable } from "../protocol";
import { formatRelativeTime, isJustNow } from "../relativeTime";
import { useNow } from "../useNow";
import { QuotaMeter, QuotaMeterPair } from "./QuotaMeter";
import { isLoggedIn, isNotLoggedIn, orderedQuotaWindows, planLabel, twoLayerWindows } from "./quotaFormat";
import type { QuotaReading } from "./useAccountQuota";

const NO_RECORDS = { consoles: new Map(), projects: new Map(), sessions: new Map() };

/** Why a reading failed, in the current language with the technical literals of its message set
 * apart. None of the quota messages names a record, so none is given. `inMark` is for a popover: the
 * sentence, then the remedy, then the raw text of a failure the application has no words for. */
function QuotaFailure({ unavailable, inMark = false }: { unavailable: QuotaUnavailable; inMark?: boolean }): React.ReactElement {
  const language = useCurrentLanguage();
  const text = daemonMessage(language, unavailable.code, unavailable.params, unavailable.message, NO_RECORDS);
  return inMark ? <CodedFailureBody failure={unavailable} text={text} /> : <>{codedMessageNode(unavailable, text)}</>;
}

/** The account's plan, in display case ("Max", "Plus", "SuperGrok"); nothing when the agent does
 * not say. */
export function QuotaPlan({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  if (typeof quota !== "object" || !quota.plan) return null;
  return <StatusBadge>{planLabel(quota.plan)}</StatusBadge>;
}

/** The badge of an account's login: logged in, or not logged in yet; nothing while the state is not known (a reading
 * still to come, or one that failed for another reason). It carries its state in words and a dot, not in colour alone. */
function LoginStateBadge({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  const loggedIn = isLoggedIn(quota);
  if (!loggedIn && !(typeof quota === "object" && isNotLoggedIn(quota.unavailable))) return null;
  return (
    <StatusBadge variant="soft" color={loggedIn ? "success" : "warning"} className="inline-flex items-center gap-1.5">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-current" />
      {t(loggedIn ? "settings.accounts.loggedIn" : "settings.accounts.notLoggedIn")}
    </StatusBadge>
  );
}

/** The mark of a reading that failed with no windows to show, beside the account's name: its name says
 * so and its popover holds why. Nothing for an account with windows (an out-of-date one is marked beside
 * when it was read, by `QuotaUpdated`), one not yet known, or one that is simply not logged in. */
function QuotaFailureMark({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  if (typeof quota !== "object" || !quota.unavailable || isNotLoggedIn(quota.unavailable) || quota.windows.length > 0) return null;
  return <StatusMark label={t("quota.unavailable")}><QuotaFailure inMark unavailable={quota.unavailable} /></StatusMark>;
}

/** When the reading with windows was taken ("Updated 3 minutes ago"). When the latest read failed and those
 * windows are the last good ones, the time stays and a warning mark follows it, naming them out of date and
 * holding why in its popover. Nothing for an account with no windows. */
export function QuotaUpdated({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  if (typeof quota !== "object" || quota.windows.length === 0) return null;
  const age = formatRelativeTime(language, quota.fetched_at, now);
  const stale = quota.unavailable && !isNotLoggedIn(quota.unavailable) ? quota.unavailable : undefined;
  return (
    <span className="ms-auto flex shrink-0 items-center gap-1 text-xs font-normal text-app-muted">
      <span>{isJustNow(quota.fetched_at, now) ? t("quota.updatedNow") : t("quota.updated", { age })}</span>
      {stale && (
        <StatusMark tone="warning" label={t("quota.stale", { age })}>
          <QuotaFailure inMark unavailable={stale} />
        </StatusMark>
      )}
    </span>
  );
}

/** The line naming an account: its `name`, its plan badge when the agent names one, with `usedUp` a "Used up"
 * badge and the name in the muted colour, with `showLoginState` a badge for an account known to be logged in or not,
 * the mark of a reading with nothing to show, a `help` tip, and at its end, unless `showUpdated` is off for a
 * surface that places it elsewhere, when the reading was taken and whether it is out of date. */
export function AccountNameLine({ name, quota, help, usedUp = false, showLoginState = false, showUpdated = true, children }: { children?: React.ReactNode; name: React.ReactNode; quota: QuotaReading; help?: React.ReactNode; usedUp?: boolean; showLoginState?: boolean; showUpdated?: boolean }): React.ReactElement {
  const t = useT();
  return (
    <div className="flex items-center gap-2 text-sm font-medium">
      <div className={`min-w-0 ${usedUp ? "text-app-muted" : ""}`}>{name}</div>
      <QuotaPlan quota={quota} />
      {usedUp && <StatusBadge variant="soft">{t("quota.usedUp")}</StatusBadge>}
      {showLoginState && <LoginStateBadge quota={quota} />}
      {children}
      <QuotaFailureMark quota={quota} />
      {help}
      {showUpdated && <QuotaUpdated quota={quota} />}
    </div>
  );
}

/** One account's quota windows as meters, and what stands in their place when there is nothing
 * current to show:
 *
 * - `undefined`: nothing at all, the account has no reading to wait for;
 * - `"loading"`: the first reading is still on its way;
 * - an account that is not logged in: the line saying what to do;
 * - any other failure: no windows, or the last good ones, as the meters say; the failure itself is
 *   the mark on the account's name line (`AccountNameLine`).
 *
 * When a current reading was taken is shown beside the account's name (`QuotaUpdated`), not here.
 *
 * The 5-hour and weekly windows share one two-layer meter. */
export function AccountQuotaSummary({ quota, single, usedUp }: {
  quota: QuotaReading;
  /** One window to a row whatever the viewport, for a surface narrower than the window. */
  single?: boolean;
  /** The account is used up: its meters recede (`QuotaMeter`). */
  usedUp?: boolean;
}): React.ReactElement | null {
  const t = useT();
  if (quota === "loading") return <div className="text-sm text-app-muted">{t("quota.loading")}</div>;
  if (!quota) return null;
  const { windows, unavailable } = quota;
  if (windows.length === 0) {
    // An account that is not logged in is a state, not a failure: the line says what to do and nothing else.
    if (isNotLoggedIn(unavailable)) {
      return <div className="text-sm text-app-muted"><QuotaFailure unavailable={unavailable} /></div>;
    }
    return null;
  }
  // An account with a 5-hour window shows it with the weekly window as one two-layer meter; the
  // other windows (a model's weekly limit) keep a meter of their own.
  const layers = twoLayerWindows(windows);
  return (
    <div className={`grid grid-cols-1 gap-x-6 gap-y-3 ${single ? "" : "sm:grid-cols-2"}`}>
      {orderedQuotaWindows(windows).map((window) => {
        if (layers?.weekly && window === layers.weekly) return null;
        const key = `${window.kind}:${window.label ?? ""}`;
        return layers?.weekly && window === layers.session
          ? <QuotaMeterPair key={key} naming={quota} session={window} weekly={layers.weekly} usedUp={usedUp} />
          : <QuotaMeter key={key} naming={quota} window={window} usedUp={usedUp} />;
      })}
    </div>
  );
}
