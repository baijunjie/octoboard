/** How long a poll waits after an answer before it asks again: `live` while the answer is on screen,
 * `background` while it only has to be known (a phone-width pane, where the panel floats in on demand, or a count
 * on a button). The daemon reads the process table (and, for a list, each tree's ports) when it is asked, so the
 * answer is polled at a modest pace rather than followed. */
export const POLL_MS = { live: 3000, background: 15000 };

/** How often a poll asks: not at all, now and then, or as the answer is watched. */
export type ProcessPolling = "off" | "background" | "live";

/** The pace `usePolled` takes for `polling`. */
export function pollPace(polling: ProcessPolling): number | undefined {
  return polling === "off" ? undefined : POLL_MS[polling];
}
