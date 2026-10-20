import { Meter } from "@heroui/react";
import React from "react";

import { Message, useT } from "../i18n/react";
import type { Agent, QuotaWindow } from "../protocol";
import { useNow } from "../useNow";
import { QuotaWindowName } from "./quotaCopy";
import { quotaResetsIn, quotaWindowLabel, remainingPercent } from "./quotaFormat";

/** The line above or beneath a quota bar: the window's headline ("5-hour 82% remaining") with the
 * countdown to its reset ("resets in 1h 57m") at its end. It is hidden from assistive technology,
 * since the bar carries the same facts as its name and value. */
function QuotaHeadline({ window, resets }: { window: QuotaWindow; resets: string | null }): React.ReactElement {
  const t = useT();
  return (
    <div aria-hidden="true" className="flex items-baseline justify-between gap-x-3 text-sm">
      <span className="min-w-0 font-medium [overflow-wrap:anywhere]">{<Message id="quota.remaining" params={{ window: <QuotaWindowName window={window} t={t} />, percent: remainingPercent(window) }} />}</span>
      {resets && <span className="shrink-0 text-app-muted">{resets}</span>}
    </div>
  );
}

/** The bar of what remains of a window, in the agent's colour, named by the window and valued by the
 * remaining percent and the countdown. `thin` is the slim bar of a window that sits beneath another. */
function QuotaBar({ agent, window, resets, thin, className }: {
  agent: Agent;
  window: QuotaWindow;
  resets: string | null;
  thin?: boolean;
  className: string;
}): React.ReactElement {
  const t = useT();
  const percent = remainingPercent(window);
  return (
    <Meter
      aria-label={quotaWindowLabel(t, window)}
      value={percent}
      valueLabel={resets ? t("quota.meterValueResets", { percent, resets }) : t("quota.meterValue", { percent })}
      size={thin ? "sm" : undefined}
      className={`gap-0 quota-meter--${agent} ${className}`}
    >
      <Meter.Track>
        <Meter.Fill />
      </Meter.Track>
    </Meter>
  );
}

/** One quota window of an account: its headline with the countdown to its reset at the end of the
 * same line, and a bar of what remains beneath. The drawn text is hidden from assistive technology
 * and the meter carries the same facts as its name and value, so they are read once. */
export function QuotaMeter({ agent, window }: { agent: Agent; window: QuotaWindow }): React.ReactElement {
  const t = useT();
  const now = useNow();
  const resets = quotaResetsIn(t, window, now);
  return (
    <div className="min-w-0">
      <QuotaHeadline window={window} resets={resets} />
      <QuotaBar agent={agent} window={window} resets={resets} className="mt-1.5" />
    </div>
  );
}

/** The 5-hour window and the account-wide weekly one as one meter: the 5-hour headline over a bar,
 * and directly beneath it the weekly window's slim bar with its own headline under that. Each bar
 * keeps its own accessible name and value, as a lone `QuotaMeter` does. */
export function QuotaMeterPair({ agent, session, weekly }: { agent: Agent; session: QuotaWindow; weekly: QuotaWindow }): React.ReactElement {
  const t = useT();
  const now = useNow();
  const sessionResets = quotaResetsIn(t, session, now);
  const weeklyResets = quotaResetsIn(t, weekly, now);
  return (
    <div className="min-w-0">
      <QuotaHeadline window={session} resets={sessionResets} />
      <QuotaBar agent={agent} window={session} resets={sessionResets} className="mt-1.5" />
      <QuotaBar agent={agent} window={weekly} resets={weeklyResets} thin className="mt-1" />
      <div className="mt-1 text-app-muted">
        <QuotaHeadline window={weekly} resets={weeklyResets} />
      </div>
    </div>
  );
}
