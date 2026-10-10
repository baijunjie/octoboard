import React, { useEffect, useState } from "react";

/** How long a region sits empty before its first text is written, so WebKit sees the text arrive
 * in a region that already exists. */
const FIRST_WRITE_DELAY_MS = 100;

/**
 * Speaks `text` to assistive technology without showing it: the status region for a state whose
 * visible form comes and goes (a loading placeholder). WebKit and VoiceOver stay silent for a
 * region inserted already holding its text, so the region is rendered empty and the first text is
 * written only after a short delay, which also covers a state that is there from the first render.
 * Later changes are written at once. Mount this where the state can arise, for as long as the
 * surface lives, and pass `undefined` while there is nothing to say; text that is cleared before
 * the delay has passed is never spoken. The visible placeholder is `aria-hidden` and has no role of
 * its own, so nothing is said twice. `firstWriteDelayMs` stretches the wait for a surface that
 * takes focus as it opens, so the status comes after the announcement of that focus move, which
 * would otherwise replace it.
 */
export function StatusAnnouncer({
  text,
  firstWriteDelayMs = FIRST_WRITE_DELAY_MS,
}: {
  text: string | undefined;
  firstWriteDelayMs?: number;
}): React.ReactElement {
  const [ready, setReady] = useState(false);
  // Read once: the delay only applies to the first write.
  useEffect(() => {
    const timer = setTimeout(() => setReady(true), firstWriteDelayMs);
    return () => clearTimeout(timer);
  }, []);
  return (
    <div role="status" className="sr-only">
      {ready ? text : undefined}
    </div>
  );
}
