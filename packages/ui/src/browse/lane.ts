/** What a lane is asked to send. */
export interface LaneOptions {
  /** The daemon slot the request claims. In a `replacing` lane, a request in a slot that already has one out is sent at
   * once, past the lane's places (see `createBrowseLane`). Not given in a lane made for one slot of its own, whose
   * requests all claim it. */
  slot?: string;
  /** Aborting it drops the request while it waits, and rejects the promise with the signal's reason. While the request
   * is out, a lane made for one slot of its own also gives up its place at once, to the next request; any other lane
   * keeps the place until the request settles (see `createBrowseLane`). */
  signal?: AbortSignal;
}

export interface BrowseLane {
  /** How many requests the lane has out at once (`replacing` lanes aside). */
  readonly places: number;
  /**
   * Runs `send` once the lane has a free place, holding the place until the promise it returns settles, and settles
   * as that promise does. Requests wait their turn in the order they were asked for. `send` returns nothing to skip
   * the request, its caller having gone or no longer wanting it by the time its turn comes; the promise then
   * resolves to `undefined`.
   */
  send<T>(send: () => Promise<T>, options?: LaneOptions): Promise<T>;
  send<T>(send: () => Promise<T> | undefined, options?: LaneOptions): Promise<T | undefined>;
}

/**
 * A window-wide lane of browse requests with `places` requests out at once, the rest waiting their turn. Module state
 * shared by everything that sends through it, so that several mounted components together never have more out than
 * their share of the daemon's bound (see `browse/budget.ts`).
 *
 * In a `replacing` lane, a request whose `slot` already has one out is sent at once, past the bound: the daemon gives
 * the older one up as it arrives, so the two hold one place between them for no longer than that, and a request stuck
 * on a slow repository never holds back the one that replaces it. The budget reserves a unit beside each place for it.
 *
 * A lane made for one `slot` of its own has every request in it claim that slot, so the next request cancels the one
 * before it in the daemon: a request let go of while out (its `signal` aborted) gives up its place at once, as the
 * next one is about to replace it. In any other lane the requests do not share a slot, so a request that is let go of
 * is still counted by the daemon until it returns, and keeps its place until then.
 */
export function createBrowseLane(places: number, options: { replacing?: boolean } | { slot: string } = {}): BrowseLane {
  const replacing = "replacing" in options && options.replacing === true;
  const laneSlot = "slot" in options ? options.slot : undefined;
  let inFlight = 0;
  const waiting: (() => void)[] = [];
  /** How many requests are out in each slot. */
  const slots = new Map<string, number>();

  const send = <T>(send: () => Promise<T> | undefined, { slot, signal }: LaneOptions = {}): Promise<T | undefined> =>
    new Promise((resolve, reject) => {
      if (laneSlot !== undefined && slot !== undefined) return reject(new Error(`a request in the lane of slot "${laneSlot}" names no slot of its own`));
      if (signal?.aborted) return reject(signal.reason);
      let holding = false;
      const release = () => {
        if (!holding) return;
        holding = false;
        inFlight -= 1;
        if (slot !== undefined) {
          const left = (slots.get(slot) ?? 1) - 1;
          if (left === 0) slots.delete(slot);
          else slots.set(slot, left);
        }
        if (inFlight < places) waiting.shift()?.();
      };
      const onAbort = () => {
        const queued = waiting.indexOf(start);
        if (queued !== -1) waiting.splice(queued, 1);
        if (laneSlot !== undefined) release();
        reject(signal!.reason);
      };
      const finish = () => {
        signal?.removeEventListener("abort", onAbort);
        release();
      };
      function start() {
        holding = true;
        inFlight += 1;
        if (slot !== undefined) slots.set(slot, (slots.get(slot) ?? 0) + 1);
        let sent: Promise<T> | undefined;
        try {
          sent = send();
        } catch (err) {
          finish();
          reject(err);
          return;
        }
        if (!sent) {
          finish();
          resolve(undefined);
          return;
        }
        sent.then(resolve, reject).finally(finish);
      }
      signal?.addEventListener("abort", onAbort, { once: true });
      if (inFlight < places || (replacing && slot !== undefined && slots.has(slot))) start();
      else waiting.push(start);
    });

  return { places, send: send as BrowseLane["send"] };
}
