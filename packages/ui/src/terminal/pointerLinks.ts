import type { Terminal } from "@xterm/xterm";

/** Where the pointer rests over the screen, as its last mouse event there (a move, a press or a
 * release) left it. */
interface RestingPointer {
  clientX: number;
  clientY: number;
  buttons: number;
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}

/**
 * Follows the pointer over `screen`, the terminal's `.xterm-screen` element, and returns what makes
 * xterm ask its link providers again for the cell under it: for a provider whose answer changes
 * while the pointer rests, as a path's does once the daemon has said what it names.
 *
 * xterm has no API for this. Its link detection (`Linkifier`, xterm 6) listens for `mousemove` on the
 * screen element and asks the providers only when the pointer comes onto another row: within a row
 * it reuses the row's replies, never asking again a provider that had no link, and a move to the same
 * cell does nothing. It keeps those replies while the pointer is off the screen, too. So the pointer
 * is moved, in synthetic events, one row over and back to where it rests. The row over is asked about
 * too; every provider registered here answers synchronously, so a link there is hovered and left
 * again in the same task, before anything is drawn.
 *
 * The events are dispatched on the screen element and do not bubble: xterm's mouse reporting listens
 * on the terminal's own element and its selection on the document, both past the screen, so a program
 * that tracks the pointer sees no motion and a selection or a drag is left alone. A move takes no
 * focus. While the pointer is off the screen, a button is held, or the screen is not laid out, the
 * asking waits for the pointer's next move over the screen or the button's release there; on a screen
 * of one row there is no row over, and it is not done. The events carry the modifier keys of the last
 * real mouse event, and xterm shows the column-selection cursor from their Alt key, which may since
 * have been released or pressed (not on macOS, where Option forces a selection here instead); the
 * next real move puts it right. The two renderers draw the same link detection's hover, so it holds for
 * both. It relies on the link detection's behaviour, so recheck it on every `@xterm/xterm` upgrade:
 * if that changes, a link under a resting pointer appears only once the pointer moves to another row.
 */
export function watchPointerForLinks(term: Pick<Terminal, "rows">, screen: HTMLElement, signal: AbortSignal): () => void {
  let pointer: RestingPointer | undefined;
  /** An ask that had to wait for the pointer. */
  let pending = false;
  /** Set while the synthetic moves are dispatched, which are not the pointer's. */
  let moving = false;

  const ask = () => {
    const rest = pointer;
    if (term.rows < 2 || signal.aborted) return;
    const { top, height } = screen.getBoundingClientRect();
    const rowHeight = height / term.rows;
    pending = !rest || rest.buttons !== 0 || !(rowHeight > 0);
    if (!rest || pending) return;
    // Towards the screen's middle, so the row over is on screen.
    const over = rest.clientY - top < height / 2 ? rowHeight : -rowHeight;
    const move = (clientY: number) =>
      screen.dispatchEvent(new MouseEvent("mousemove", { ...rest, clientY, bubbles: false }));
    moving = true;
    try {
      move(rest.clientY + over);
      move(rest.clientY);
    } finally {
      moving = false;
    }
  };
  // The screen exists only once `open()` has put xterm's own listeners on it, so these run after them:
  // for the same event the link detection answers first, from the replies it kept, and the waiting ask
  // then replaces them rather than being undone by them.
  const note = (event: MouseEvent) => {
    if (moving) return;
    const { clientX, clientY, buttons, shiftKey, altKey, ctrlKey, metaKey } = event;
    pointer = { clientX, clientY, buttons, shiftKey, altKey, ctrlKey, metaKey };
    if (pending) ask();
  };
  screen.addEventListener("mousemove", note, { signal });
  screen.addEventListener("mousedown", note, { signal });
  screen.addEventListener("mouseup", note, { signal });
  screen.addEventListener("mouseleave", () => (pointer = undefined), { signal });
  return ask;
}
