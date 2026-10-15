// The silence watchdog of a run of the page: `seen` takes every line the page posted, in order, and
// `onStall` is called with a message once nothing but heartbeats has arrived for STALL_MS. The heartbeats
// (instrument.js posts a frame count every 5 s) let the message say what kind of stall it is.
// Longer than the longest stretch the driver can spend on one thing before it reports its own timeout
// (30 s for the fixtures, then per step up to 20 s idle, 30 s for the terminal and 20 s idle again).
export const STALL_MS = 150_000;
const FROZEN_MS = 15_000;

export function stallWatch(label, onStall) {
  let timer;
  let beat;
  const why = () => {
    if (!beat) return "the page never loaded: no heartbeat from instrument.js";
    const frozen = Date.now() - beat.changedAt;
    return frozen > FROZEN_MS ? `the page loaded but ran no frames for ${Math.round(frozen / 1000)} s` : "the page ran frames but the driver posted nothing";
  };
  const arm = () => {
    clearTimeout(timer);
    timer = setTimeout(() => onStall(`${label}: nothing posted for ${STALL_MS / 1000} s (${why()}); stopping it`), STALL_MS);
  };
  arm();
  return {
    /** Returns whether `line` was a heartbeat, which the caller drops; any other line is progress. */
    seen(line) {
      let heartbeat;
      try { heartbeat = JSON.parse(line).heartbeat; } catch { /* not JSON: a result or noise */ }
      if (heartbeat === undefined) {
        arm();
        return false;
      }
      if (heartbeat !== beat?.frames) beat = { frames: heartbeat, changedAt: Date.now() };
      return true;
    },
    stop: () => clearTimeout(timer),
  };
}
