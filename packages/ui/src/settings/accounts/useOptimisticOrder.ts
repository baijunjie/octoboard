import { useEffect, useState } from "react";

/** An order the user has just asked for, shown at once and not once the daemon's broadcast has come back: until that
 * arrives, or the request is refused and the order falls back to `settled`. A proposed order holds only while `base`
 * (what it was proposed against: the list or the saved order the daemon held) is the one still held, so a newer saved
 * order or a changed set of items supersedes it. `base` is compared by `Object.is`.
 *
 * `propose` shows an order and returns what takes it back, which does nothing once a later proposal has replaced it. */
export function useOptimisticOrder<Order>(base: unknown, settled: Order): {
  order: Order;
  propose: (order: Order) => () => void;
} {
  const [pending, setPending] = useState<{ base: unknown; order: Order }>();
  useEffect(() => {
    setPending((held) => held && !Object.is(held.base, base) ? undefined : held);
  }, [base]);
  const order = pending && Object.is(pending.base, base) ? pending.order : settled;
  const propose = (next: Order) => {
    const change = { base, order: next };
    setPending(change);
    return () => setPending((held) => held === change ? undefined : held);
  };
  return { order, propose };
}
