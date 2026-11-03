/** Runs `then` after `count` animation frames, and returns what cancels it. A focus hand-off waits like this for the
 * frame React Aria's own restore runs in (the one after a dialog unmounts) to pass before it acts. */
export function afterFrames(count: number, then: () => void): () => void {
  let frame = 0;
  const wait = (left: number) => {
    frame = requestAnimationFrame(() => (left <= 1 ? then() : wait(left - 1)));
  };
  wait(count);
  return () => cancelAnimationFrame(frame);
}
