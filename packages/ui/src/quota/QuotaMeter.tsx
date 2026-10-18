import { Meter } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";
import type { Agent, QuotaWindow } from "../protocol";
import { useNow } from "../useNow";
import { quotaHeadline, quotaResetsIn, quotaWindowLabel, remainingPercent } from "./quotaFormat";

/** One quota window of an account: its headline ("5-hour 82% remaining") with the countdown to its
 * reset ("resets in 1h 57m") at the end of the same line, and a bar of what remains beneath, in
 * the agent's colour. The drawn text is hidden from assistive technology and the meter carries the
 * same facts as its name and value, so they are read once. */
export function QuotaMeter({ agent, window }: { agent: Agent; window: QuotaWindow }): React.ReactElement {
  const t = useT();
  const now = useNow();
  const percent = remainingPercent(window);
  const resets = quotaResetsIn(t, window, now);
  return (
    <div className="min-w-0">
      <div aria-hidden="true" className="flex items-baseline justify-between gap-x-3 text-sm">
        <span className="min-w-0 font-medium [overflow-wrap:anywhere]">{quotaHeadline(t, window)}</span>
        {resets && <span className="shrink-0 text-app-muted">{resets}</span>}
      </div>
      <Meter
        aria-label={quotaWindowLabel(t, window)}
        value={percent}
        valueLabel={resets ? t("quota.meterValueResets", { percent, resets }) : t("quota.meterValue", { percent })}
        className={`mt-1.5 gap-0 quota-meter--${agent}`}
      >
        <Meter.Track>
          <Meter.Fill />
        </Meter.Track>
      </Meter>
    </div>
  );
}
