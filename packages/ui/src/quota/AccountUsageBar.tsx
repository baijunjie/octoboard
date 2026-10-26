import React from "react";

import type { AccountQuota, Agent } from "../protocol";
import { remainingPercentAt, usageBarWindows } from "./quotaFormat";

/** A bar of what remains of a window, in the agent's colour, or in grey when the account is used up (the fill reads
 * `--meter-fill`, which the `quota-meter--<agent>` or `quota-meter--used-up` class sets). Plain spans rather than
 * HeroUI's `Meter` or `ProgressBar`: the bar sits inside a list option, where a `role="meter"` or `progressbar` with a
 * name and a value would be a second, nested control inside the option, whose own description already states the
 * numbers (`usageBarSummary`); this one is purely decorative and carries no role or name. The track is
 * `.quota-bar-track`, the rail's own track; with `edge` it has a hairline edge, the same on both bars (see `style.css`).
 * The thin bar is 3px against the main bar's 4px, the rail button's capsules; the rail panel's and the details' meters
 * are 6px against 8px, the same 4:3. */
function Track({ percent, thin, edge }: { percent?: number; thin?: boolean; edge: boolean }): React.ReactElement {
  return (
    <span className={`block overflow-hidden rounded-full quota-bar-track ${thin ? "h-[3px]" : "h-1"} ${edge ? "quota-bar-track--edge" : ""}`}>
      {percent !== undefined && <span className="block h-full rounded-full bg-(--meter-fill)" style={{ width: `${percent}%` }} />}
    </span>
  );
}

/** A compact, wordless usage bar for a list of accounts: the 5-hour window's bar with the weekly window's thin one
 * directly beneath it, or one bar for an account without a 5-hour window. An account with nothing to draw (no reading
 * yet, or one out of date, as on the rail) shows one empty track, so names line up. It is decorative: the list's option
 * states the same numbers in words (`usageBarSummary`). `className` sets its width, 2rem by default. `usedUp` draws the
 * bars grey, as the rail panel's meters. `stale` draws the last good windows of a reading whose latest read failed
 * rather than the empty track, for a surface that marks them out of date beside the bar. `edge` (on by default) draws
 * the tracks' hairline edges, which a popover, a menu and a select trigger need to show an empty track and a bar on the
 * page's own surface (a table cell) does not. */
export function AccountUsageBar({ agent, quota, now, className = "w-8", edge = true, usedUp = false, stale = false }: { agent: Agent; quota: AccountQuota | undefined; now: number; className?: string; edge?: boolean; usedUp?: boolean; stale?: boolean }): React.ReactElement {
  const bar = quota && (stale || !quota.unavailable) ? usageBarWindows(quota) : undefined;
  return (
    <span aria-hidden="true" className={`flex ${className} shrink-0 flex-col gap-0.5 ${usedUp ? "quota-meter--used-up" : `quota-meter--${agent}`}`}>
      <Track edge={edge} percent={bar && remainingPercentAt(bar.main, now)} />
      {bar?.thin && <Track thin edge={edge} percent={remainingPercentAt(bar.thin, now)} />}
    </span>
  );
}
