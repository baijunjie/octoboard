import React, { useEffect, useState } from "react";

/** How long a region sits empty before its first text is written, so WebKit sees the text arrive
 * in a region that already exists. */
const FIRST_WRITE_DELAY_MS = 100;

/** How long a status that comes with a focus move (a dialog opening or closing) waits before it is
 * written. VoiceOver announces the focus move and replaces a status written within about 300 ms of
 * it; a write 1.5 s after a dialog opened is spoken after it (verified). */
export const AFTER_FOCUS_MOVE_DELAY_MS = 1500;

/** How long a status written with `clearAfterRead` stays in its region: long enough for a screen reader
 * to take the sentence up, after which the region is emptied. */
export const READ_TIME_MS = 5000;

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
 * would otherwise replace it. `element` is `span` where the region sits inside a paragraph: a
 * `div` there is not valid HTML, and the announcement is the same either way. `clearAfterRead` empties
 * the region again `READ_TIME_MS` after its text was written, for a sentence that is also on the
 * screen, so a screen reader reading line by line does not meet it a second time; a changed text is
 * written and cleared afresh, including one that returns to a text emptied before.
 */
export function StatusAnnouncer({
  text,
  firstWriteDelayMs = FIRST_WRITE_DELAY_MS,
  element: Element = "div",
  clearAfterRead = false,
}: {
  text: string | undefined;
  firstWriteDelayMs?: number;
  element?: "div" | "span";
  clearAfterRead?: boolean;
}): React.ReactElement {
  const [ready, setReady] = useState(false);
  // Read once: the delay only applies to the first write.
  useEffect(() => {
    const timer = setTimeout(() => setReady(true), firstWriteDelayMs);
    return () => clearTimeout(timer);
  }, []);
  // Whether the current write has been emptied; any change of text is a new write, even back to a
  // text emptied before, so it starts out unread.
  const [cleared, setCleared] = useState(false);
  const [written, setWritten] = useState(text);
  if (written !== text) {
    setWritten(text);
    setCleared(false);
  }
  useEffect(() => {
    if (!clearAfterRead || !ready || text === undefined) return;
    const timer = setTimeout(() => setCleared(true), READ_TIME_MS);
    return () => clearTimeout(timer);
  }, [clearAfterRead, ready, text]);
  return (
    <Element role="status" className="sr-only">
      {ready && !cleared ? text : undefined}
    </Element>
  );
}
