/**
 * Reconnect backoff after a dropped-but-still-live terminal's socket closes: doubles each attempt,
 * capped, and stops after `MAX_AUTO_RECONNECT_ATTEMPTS` so a terminal that keeps failing to connect
 * does not retry forever — the cover's Reconnect covers that case instead. The attempt counter only
 * resets once a connection has *stayed* open for `STABLE_CONNECTION_MS` — resetting it on every
 * `open` would mean a connection that opens and drops again within a few seconds (a backpressure
 * drop recurs roughly that often under sustained output) always sees attempt 0, so the delay would
 * never grow past the base value and the cap would never engage.
 */
export const MAX_AUTO_RECONNECT_ATTEMPTS = 5;
export const STABLE_CONNECTION_MS = 10000;
const BASE_RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 8000;

/** The wait before automatic attempt number `attempt` (counting from 0); `override` is the daemon's own schedule, when it has one. */
export function reconnectDelay(attempt: number, override?: (attempt: number) => number): number {
  return override?.(attempt) ?? Math.min(BASE_RECONNECT_DELAY_MS * 2 ** attempt, MAX_RECONNECT_DELAY_MS);
}
