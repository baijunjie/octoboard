/**
 * Makes a queue for best-effort pushes to the native shell, so that they reach it one at a time, in the order they were
 * made: a push starts when the one before it has settled, so two in flight cannot settle out of order and leave the
 * shell on the older value. The function it returns queues a push and resolves to whether that push succeeded. A push
 * that fails, whether by rejecting or by throwing as `send` is called, is dropped without surfacing and does not hold
 * up the ones after it.
 */
export function createSerialQueue(): (send: () => Promise<unknown>) => Promise<boolean> {
  let pending: Promise<unknown> = Promise.resolve();
  return (send) => {
    const result = pending.then(send).then(
      () => true,
      () => false,
    );
    pending = result;
    return result;
  };
}
