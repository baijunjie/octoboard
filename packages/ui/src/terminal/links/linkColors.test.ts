// @vitest-environment jsdom
import type { Terminal } from "@xterm/xterm";
import { expect, it, vi } from "vitest";

import { fakeTimers, FAKE_FRAME_MS } from "../../test/clock";
import { TerminalLinkColors } from "./linkColors";

/** Enough of xterm to place decorations: a buffer of text lines, markers that follow their line. */
function fakeTerminal(lines: string[]) {
  const writeListeners: (() => void)[] = [];
  const markers: { line: number; isDisposed: boolean; dispose(): void }[] = [];
  const decorations: { marker: (typeof markers)[number]; isDisposed: boolean; dispose(): void }[] = [];
  const bufferListeners: (() => void)[] = [];
  const listeners = (list: (() => void)[]) => (listener: () => void) => {
    list.push(listener);
    return { dispose: () => list.splice(list.indexOf(listener), 1) };
  };
  const term = {
    cols: 24,
    rows: 4,
    onWriteParsed: listeners(writeListeners),
    onScroll: () => ({ dispose() {} }),
    onResize: () => ({ dispose() {} }),
    buffer: {
      onBufferChange: listeners(bufferListeners),
      active: {
        type: "normal",
        viewportY: 0,
        baseY: 0,
        cursorY: 0,
        getNullCell: () => ({}),
        getLine: (y: number) =>
          lines[y] === undefined
            ? undefined
            : {
              isWrapped: false,
              getCell: (x: number) => ({ getWidth: () => 1, getChars: () => lines[y][x] ?? "" }),
            },
      },
    },
    registerMarker: (offset: number) => {
      const marker = { line: offset, isDisposed: false, dispose: () => (marker.isDisposed = true) };
      markers.push(marker);
      return marker;
    },
    registerDecoration: ({ marker }: { marker: (typeof markers)[number] }) => {
      const decoration = { marker, isDisposed: false, dispose: () => (decoration.isDisposed = true) };
      decorations.push(decoration);
      return decoration;
    },
  };
  const update = () => {
    writeListeners.forEach((listener) => listener());
    vi.advanceTimersByTime(FAKE_FRAME_MS);
  };
  return { term: term as unknown as Terminal, lines, markers, decorations, update, bufferListeners };
}

it("keeps a decoration for a link whose line moved, instead of placing it again", (context) => {
  fakeTimers();
  const { term, lines, markers, decorations, update } = fakeTerminal(["", "see http://a.dev/x", ""]);
  const colors = new TerminalLinkColors(term);
  context.onTestFinished(() => colors.dispose());
  update();
  expect(decorations).toHaveLength(1);

  // The buffer is trimmed by one line: the link is on line 0 now, and its marker followed it.
  lines.shift();
  markers[0].line--;
  update();
  expect(decorations).toHaveLength(1);
  expect(decorations[0].isDisposed).toBe(false);
});


it("rebuilds decorations on buffer switches and removes them after repaint or disposal", (context) => {
  fakeTimers();
  const { term, lines, markers, decorations, update, bufferListeners } = fakeTerminal(["http://a.dev/x"]);
  const colors = new TerminalLinkColors(term);
  context.onTestFinished(() => colors.dispose());
  update();
  Object.assign(term.buffer.active, { type: "alternate" });
  bufferListeners.forEach((listener) => listener());
  update();
  expect(decorations).toHaveLength(2);
  expect(decorations[0].isDisposed).toBe(true);
  expect(markers[0].isDisposed).toBe(true);
  expect(decorations[1].isDisposed).toBe(false);
  // Alternate decorations use owned markers and do not register with xterm's buffer.
  expect(markers).toHaveLength(1);
  const alternateMarker = decorations[1].marker;
  lines[0] = "plain text";
  update();
  expect(decorations[1].isDisposed).toBe(true);
  lines[0] = "http://a.dev/x";
  update();
  colors.dispose();
  expect(decorations[2].isDisposed).toBe(true);
  expect(alternateMarker.isDisposed).toBe(true);
  const request = vi.spyOn(window, "requestAnimationFrame");
  update();
  // The disposed colourizer no longer listens for output.
  expect(request).not.toHaveBeenCalled();
  request.mockRestore();
});
