import type { Terminal } from "@xterm/xterm";

/**
 * Calls `onUserInput` right before each `onData` that xterm marks as the user's: a key, a paste, an
 * input method's text, a wheel turned into arrow keys. xterm's own answers to queries in the output
 * (device attributes, cursor position, colours, focus and window reports) are not marked. Mouse
 * reports are marked too when they come through `onData` (the SGR and SGR-pixels encodings), as is
 * an Alt+click that moves the cursor; the default encoding comes through `onBinary`
 * unmarked. The controller itself tells the mouse reports apart (see `MOUSE_REPORT`, `onBinary`),
 * since they are not typing. The public API does not expose the mark, so this reaches
 * `coreService.onUserInput` through the private `_core`; recheck it on every `@xterm/xterm`
 * upgrade. Without it nothing is ever marked, so only the user's input typed while saved output is
 * still being parsed is lost, and xterm's answers go as input rather than as reply frames (see
 * `sendReply`). Returns whether the mark is watched.
 */
export function watchUserInput(term: Terminal, onUserInput: () => void): boolean {
  const core = (
    term as unknown as {
      _core?: { coreService?: { onUserInput?: (listener: () => void) => unknown } };
    }
  )._core;
  if (!core?.coreService?.onUserInput) return false;
  core.coreService.onUserInput(onUserInput);
  return true;
}

/**
 * Whether xterm's link detection has a link under the pointer: the one a press now would open on a release over it
 * (`Linkifier.currentLink`, xterm 6, which it takes as the pressed link on `mousedown`). The public API exposes only
 * each link's own hover, so this reaches the link detection through the private `_core`; recheck it on every
 * `@xterm/xterm` upgrade. Without it no link is ever found under the pointer, so a click on a link reaches the program
 * in the terminal too (see `LinkClickReports`).
 */
export function hasLinkUnderPointer(term: Terminal): boolean {
  const core = (term as unknown as { _core?: { linkifier?: { currentLink?: unknown } } })._core;
  return core?.linkifier?.currentLink !== undefined;
}
