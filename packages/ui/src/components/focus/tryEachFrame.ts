/** Calls `attempt` on the next animation frame, and on each frame after it while it returns `false`, giving up after
 * `maxRetries` further tries; returns what cancels it. For a focus hand-off that has to wait for something to go
 * (a modal in front, a dialog on its way out) and is done once `attempt` says so (`true`). */
export function tryEachFrame(attempt: () => boolean, maxRetries: number): () => void {
  let retries = 0;
  let frame = 0;
  const run = () => {
    if (!attempt() && retries++ < maxRetries) frame = requestAnimationFrame(run);
  };
  frame = requestAnimationFrame(run);
  return () => cancelAnimationFrame(frame);
}
