import React from "react";

import type { AccountQuota, Agent } from "../protocol";
import { remainingPercentAt, usageBarWindows } from "./quotaFormat";

/** A bar of what remains of a window, in the agent's colour (the fill reads `--meter-fill`, which the
 * `quota-meter--<agent>` class sets). Plain spans rather than HeroUI's `Meter` or `ProgressBar`: the bar
 * sits inside a list option, where a `role="meter"` or `progressbar` with a name and a value would be a
 * second, nested control inside the option, whose own description already states the numbers
 * (`usageBarSummary`); this one is purely decorative and carries no role or name. The track is
 * `.quota-bar-track`, a step darker than `--default` so it shows on a select trigger filled with that
 * colour (see its measures and its edges in `style.css`; the thin bar's edge lies outside it, so it is
 * inset a pixel on each side to end where the main bar's does); the fill measures at least 3.07:1 against it, in light and
 * in dark. */
function Track({ percent, thin }: { percent?: number; thin?: boolean }): React.ReactElement {
  return (
    <span className={`block overflow-hidden rounded-full quota-bar-track ${thin ? "quota-bar-track--outer-edge mx-px h-0.5" : "quota-bar-track--edge h-1.5"}`}>
      {percent !== undefined && <span className="block h-full rounded-full bg-(--meter-fill)" style={{ width: `${percent}%` }} />}
    </span>
  );
}

/** A compact, wordless usage bar for a list of accounts: the 5-hour window's bar with the weekly
 * window's thin one directly beneath it, or one bar for an account without a 5-hour window. An
 * account with nothing to draw (no reading yet, or one out of date, as on the rail) shows one empty
 * track, so names line up. It is decorative: the list's option states the same numbers in words
 * (`usageBarSummary`). */
export function AccountUsageBar({ agent, quota, now }: { agent: Agent; quota: AccountQuota | undefined; now: number }): React.ReactElement {
  const bar = quota && !quota.unavailable ? usageBarWindows(quota.windows) : undefined;
  return (
    <span aria-hidden="true" className={`flex w-8 shrink-0 flex-col gap-0.5 quota-meter--${agent}`}>
      <Track percent={bar && remainingPercentAt(bar.main, now)} />
      {bar?.thin && <Track thin percent={remainingPercentAt(bar.thin, now)} />}
    </span>
  );
}
