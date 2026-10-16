/** Window-wide lane shared by recursive file searches and Git list requests. Its two places,
 * including requests completing after a mode switch, fit beside directory listings and viewer
 * reads under the daemon's outstanding-request bound (see useDirectoryListings.ts). */
const MAX_WINDOW_LIST_IN_FLIGHT = 2;

let inFlight = 0;
const waiting: (() => void)[] = [];
/** How many requests are out in each slot. */
const slots = new Map<string, number>();

function release(slot: string): void {
  inFlight -= 1;
  const left = (slots.get(slot) ?? 1) - 1;
  if (left === 0) slots.delete(slot);
  else slots.set(slot, left);
  if (inFlight < MAX_WINDOW_LIST_IN_FLIGHT) waiting.shift()?.();
}

/** Runs `send` once fewer than the window's bound of search and Git list requests are out, holding a place
 * until the promise it returns settles. `send` returns nothing to skip the request: its caller has
 * gone, or no longer wants it, by the time its turn comes. A request in a `slot` that already has
 * one out is sent at once, past the bound: the daemon gives the older one up as it arrives, so
 * the two hold one place between them for no longer than that, and a request stuck on a slow
 * repository never holds back the one that replaces it. */
export function sendBrowseListRequest<T>(slot: string, send: () => Promise<T> | undefined): Promise<T | undefined> {
  return new Promise((resolve, reject) => {
    const start = () => {
      inFlight += 1;
      slots.set(slot, (slots.get(slot) ?? 0) + 1);
      const sent = send();
      if (!sent) {
        release(slot);
        resolve(undefined);
        return;
      }
      sent.then(resolve, reject).finally(() => release(slot));
    };
    if (inFlight < MAX_WINDOW_LIST_IN_FLIGHT || slots.has(slot)) start();
    else waiting.push(start);
  });
}

