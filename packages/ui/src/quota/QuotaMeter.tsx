import React from "react";

import { Message, useT } from "../i18n/react";
import type { Agent, QuotaWindow } from "../protocol";
import { useNow } from "../format/useNow";
import { PROVIDER_FILL_CLASS, providerFillVars } from "./providerFill";
import { QuotaWindowName } from "./quotaCopy";
import { quotaResetsIn, quotaWindowLabel, remainingPercentAt, type QuotaNaming } from "../accounts/quotaFormat";
import { Meter } from "../primitives";

/** The line above or beneath a quota bar: the window's headline ("5h 82% remaining") with the
 * countdown to its reset ("resets in 1h 57m") at its end. It is hidden from assistive technology,
 * since the bar carries the same facts as its name and value. */
function QuotaHeadline({ naming, window, resets, percent, muted }: { naming: QuotaNaming; window: QuotaWindow; resets: string | null; percent: number; muted?: boolean }): React.ReactElement {
  const t = useT();
  return (
    <div aria-hidden="true" className={`flex items-baseline justify-between gap-x-3 text-sm ${muted ? "text-app-muted" : ""}`}>
      <span className="min-w-0 font-medium [overflow-wrap:anywhere]">{<Message id="quota.remaining" params={{ window: <QuotaWindowName naming={naming} window={window} t={t} />, percent }} />}</span>
      {resets && <span className="shrink-0 text-app-muted">{resets}</span>}
    </div>
  );
}

/** The bar of what remains of a window, in the provider's colour, or in a neutral grey when `muted`, named by
 * the window and valued by the remaining percent and the countdown. `thin` is the slim bar of a window that
 * sits beneath another. */
function MeterBar({ agent, naming, window, resets, percent, thin, muted, className }: {
  agent: Agent;
  naming: QuotaNaming;
  window: QuotaWindow;
  resets: string | null;
  percent: number;
  thin?: boolean;
  muted?: boolean;
  className: string;
}): React.ReactElement {
  const t = useT();
  return (
    <Meter
      label={quotaWindowLabel(t, naming, window)}
      value={percent}
      valueLabel={resets ? t("quota.meterValueResets", { percent, resets }) : t("quota.meterValue", { percent })}
      className={`gap-0 ${muted ? "quota-meter--used-up" : PROVIDER_FILL_CLASS} ${className}`}
      style={muted ? undefined : providerFillVars(agent)}
      trackClassName={thin ? "h-1.5" : undefined}
    />
  );
}

/** One quota window of an account: its headline with the countdown to its reset at the end of the
 * same line, and a bar of what remains beneath. The drawn text is hidden from assistive technology
 * and the meter carries the same facts as its name and value, so they are read once. `usedUp` draws
 * it receded, its text in the muted colour and its bar in grey, for an account that cannot be used. */
export function QuotaMeter({ agent, naming, window, usedUp }: { agent: Agent; naming: QuotaNaming; window: QuotaWindow; usedUp?: boolean }): React.ReactElement {
  const t = useT();
  const now = useNow();
  const resets = quotaResetsIn(t, window, now);
  const percent = remainingPercentAt(window, now);
  return (
    <div className="min-w-0">
      <QuotaHeadline naming={naming} window={window} resets={resets} percent={percent} muted={usedUp} />
      <MeterBar agent={agent} naming={naming} window={window} resets={resets} percent={percent} muted={usedUp} className="mt-1.5" />
    </div>
  );
}

/** The 5-hour window and the account-wide weekly one as one meter: the 5-hour headline over a bar,
 * and directly beneath it the weekly window's slim bar with its own headline under that. Each bar
 * keeps its own accessible name and value, as a lone `QuotaMeter` does, and `usedUp` recedes it as it
 * does a lone one. */
export function QuotaMeterPair({ agent, naming, session, weekly, usedUp }: { agent: Agent; naming: QuotaNaming; session: QuotaWindow; weekly: QuotaWindow; usedUp?: boolean }): React.ReactElement {
  const t = useT();
  const now = useNow();
  const sessionResets = quotaResetsIn(t, session, now);
  const weeklyResets = quotaResetsIn(t, weekly, now);
  const sessionPercent = remainingPercentAt(session, now);
  const weeklyPercent = remainingPercentAt(weekly, now);
  return (
    <div className="min-w-0">
      <QuotaHeadline naming={naming} window={session} resets={sessionResets} percent={sessionPercent} muted={usedUp} />
      <MeterBar agent={agent} naming={naming} window={session} resets={sessionResets} percent={sessionPercent} muted={usedUp} className="mt-1.5" />
      <MeterBar agent={agent} naming={naming} window={weekly} resets={weeklyResets} percent={weeklyPercent} thin muted={usedUp} className="mt-1" />
      <div className="mt-1 text-app-muted">
        <QuotaHeadline naming={naming} window={weekly} resets={weeklyResets} percent={weeklyPercent} />
      </div>
    </div>
  );
}
