import { useEffect, useState } from "react";

import type { QuotaBar } from "../accounts/quotaFormat";

/** The longest the clock goes unread, so a reset that passes while the system sleeps (when timers may
 * pause) is noticed within a minute of waking. */
const MAX_CLOCK_WAIT_MS = 60_000;

/** The time the button's bars are drawn as of: re-read when a reset time among the windows they draw
 * passes, which is when their fill goes full (and at least once a minute). Not `useNow`, whose ticker
 * would run for as long as the rail is mounted, quota or not. */
export function useResetClock(bar: QuotaBar | undefined): number {
  const [now, setNow] = useState(() => Date.now());
  // The resets rather than `bar`, which is a new object whenever the session or the quotas update.
  const mainReset = bar?.main.resets_at;
  const thinReset = bar?.thin?.resets_at;
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const arm = () => {
      const at = Date.now();
      setNow(at);
      const ahead = [mainReset, thinReset].filter((reset): reset is number => reset != null && reset > at);
      if (ahead.length > 0) timer = setTimeout(arm, Math.min(Math.min(...ahead) - at, MAX_CLOCK_WAIT_MS));
    };
    arm();
    return () => clearTimeout(timer);
  }, [mainReset, thinReset]);
  return now;
}
