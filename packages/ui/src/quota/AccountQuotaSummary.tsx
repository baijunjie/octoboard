import React from "react";

import { StatusBadge } from "../components/StatusBadge";
import { codedMessageNode, CodedFailureBody } from "../components/errorParts";
import { StatusMark } from "../components/StatusMark";
import { daemonMessage } from "../daemonMessage";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { QuotaUnavailable } from "../protocol";
import { formatRelativeTime, isJustNow } from "../relativeTime";
import { useNow } from "../useNow";
import { QuotaMeter, QuotaMeterPair } from "./QuotaMeter";
import { isNotLoggedIn, loginStateOf, orderedQuotaWindows, planLabel, twoLayerWindows } from "./quotaFormat";
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

/** The badge of an account's login state: "Logged in" or "Not logged in" as its reading says (`loginStateOf`); nothing
 * while that is not known. It carries its state in words and a dot, not in colour alone. */
export function LoginStateBadge({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  const state = loginStateOf(quota);
  if (!state) return null;
  return (
    <StatusBadge variant="soft" color={state === "in" ? "success" : "warning"} className="inline-flex items-center gap-1.5">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-current" />
      {t(state === "in" ? "settings.accounts.loggedIn" : "settings.accounts.notLoggedIn")}
    </StatusBadge>
  );
}

/** How a reading failed, when it did: `"failed"` with no windows to show, `"stale"` with the last good windows still
 * held. Neither for a reading still to come, a good one, or an account that is simply not logged in. */
function readingFailure(quota: QuotaReading | undefined): "failed" | "stale" | undefined {
  if (typeof quota !== "object" || !quota.unavailable || isNotLoggedIn(quota.unavailable)) return undefined;
  return quota.windows.length > 0 ? "stale" : "failed";
}

/** The mark of a reading that failed with no windows to show, placed beside the account's name or, in a table row, in
 * its usage cell: its name says so and its popover holds why. Nothing for an account with windows (an out-of-date
 * one is marked by `QuotaStaleMark`), one not yet known, or one that is simply not logged in. */
export function QuotaFailureMark({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  if (typeof quota !== "object" || !quota.unavailable || readingFailure(quota) !== "failed") return null;
  return <StatusMark label={t("quota.unavailable")}><QuotaFailure inMark unavailable={quota.unavailable} /></StatusMark>;
}

/** The warning mark of a reading whose windows are the last good ones because the latest read failed, naming them out
 * of date with when they were read and holding why in its popover. Nothing for any other reading. */
export function QuotaStaleMark({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  if (typeof quota !== "object" || !quota.unavailable || readingFailure(quota) !== "stale") return null;
  return (
    <StatusMark tone="warning" label={t("quota.stale", { age: formatRelativeTime(language, quota.fetched_at, now) })}>
      <QuotaFailure inMark unavailable={quota.unavailable} />
    </StatusMark>
  );
}

/** When the reading with windows was taken ("Updated 3 minutes ago"), followed by the out-of-date mark when the latest
 * read failed (`QuotaStaleMark`). Nothing for an account with no windows. */
export function QuotaUpdated({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  if (typeof quota !== "object" || quota.windows.length === 0) return null;
  const age = formatRelativeTime(language, quota.fetched_at, now);
  return (
    <span className="ms-auto flex shrink-0 items-center gap-1 text-xs font-normal text-app-muted">
      <span>{isJustNow(quota.fetched_at, now) ? t("quota.updatedNow") : t("quota.updated", { age })}</span>
      <QuotaStaleMark quota={quota} />
    </span>
  );
}

/** A muted dash for an account with no usage to draw, that assistive technology reads as "No usage reading". With
 * `quota` and a failed reading it is followed by the failure mark, or by the out-of-date mark when the windows held are
 * the last good ones; the mark's name ("Usage unavailable", "Out of date: last read …") then says what the dash stands
 * for in place of that sentence. */
export function NoUsageReading({ quota }: { quota?: QuotaReading }): React.ReactElement {
  const t = useT();
  const failure = readingFailure(quota);
  return (
    <span className="inline-flex items-center gap-1 text-app-muted">
      <span aria-hidden="true">—</span>
      {failure === "failed" ? <QuotaFailureMark quota={quota!} /> : failure === "stale" ? <QuotaStaleMark quota={quota!} /> : <span className="sr-only">{t("quota.noReading")}</span>}
    </span>
  );
}

/** The line naming an account: its `name`, its plan badge when the agent names one, with `usedUp` the name in the muted
 * colour and a "Limit reached" badge, the mark of a reading with nothing to show, and at its end when the reading was
 * taken and whether it is out of date. `showUsedUp`, `showFailure` and `showUpdated` are off for a surface that places
 * the badge, the failure mark or the reading's age elsewhere (the table's usage cell holds the first two). The name
 * and plan keep to one line, a name too long for it fading before the plan; what follows them wraps rather than
 * squeezing the name, the reading's age first moving to a line of its own. */
export function AccountNameLine({ name, quota, usedUp = false, showUpdated = true, showFailure = true, showUsedUp = true }: { name: React.ReactNode; quota: QuotaReading; usedUp?: boolean; showUpdated?: boolean; showFailure?: boolean; showUsedUp?: boolean }): React.ReactElement {
  const t = useT();
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm font-medium">
      <div className="flex min-w-0 items-center gap-2">
        <div className={`min-w-0 ${usedUp ? "text-app-muted" : ""}`}>{name}</div>
        <QuotaPlan quota={quota} />
      </div>
      {usedUp && showUsedUp && <StatusBadge variant="soft">{t("quota.usedUp")}</StatusBadge>}
      {showFailure && <QuotaFailureMark quota={quota} />}
      {showUpdated && <QuotaUpdated quota={quota} />}
    </div>
  );
}

/** What stands in place of an account's quota windows when there is nothing current to show: the line that the first
 * reading is still on its way, or, for an account that is not logged in, the line saying what to do. Nothing otherwise. */
export function AccountQuotaNotice({ quota }: { quota: QuotaReading }): React.ReactElement | null {
  const t = useT();
  if (quota === "loading") return <div className="text-sm text-app-muted">{t("quota.loading")}</div>;
  // An account that is not logged in is a state, not a failure: the line says what to do and nothing else. Any other
  // failure with no windows is the mark on the account's name line (`AccountNameLine`).
  if (quota && quota.windows.length === 0 && isNotLoggedIn(quota.unavailable)) {
    return <div className="text-sm text-app-muted"><QuotaFailure unavailable={quota.unavailable} /></div>;
  }
  return null;
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
  if (quota === "loading" || !quota || quota.windows.length === 0) return <AccountQuotaNotice quota={quota} />;
  const { windows } = quota;
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
