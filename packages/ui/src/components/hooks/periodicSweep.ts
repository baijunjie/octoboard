/** Starts window-wide upkeep at a fixed cadence, including while the document is hidden.
 * An omitted first delay runs the first sweep immediately. A sweep starts its own asynchronous work. */
export function startPeriodicSweep(sweep: () => void, pace: number, firstDelay?: number): () => void {
  let interval: ReturnType<typeof setInterval> | undefined;
  const start = () => {
    sweep();
    interval = setInterval(sweep, pace);
  };
  const first = firstDelay === undefined ? undefined : setTimeout(start, firstDelay);
  if (firstDelay === undefined) start();
  return () => {
    clearTimeout(first);
    clearInterval(interval);
  };
}
