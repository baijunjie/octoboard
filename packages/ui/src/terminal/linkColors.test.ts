// @vitest-environment jsdom
import type { Terminal } from "@xterm/xterm";
import { expect, it } from "vitest";

import { TerminalLinkColors } from "./linkColors";

/** Enough of xterm to place decorations: a buffer of text lines, markers that follow their line. */
function fakeTerminal(lines: string[]) {
  const writeListeners: (() => void)[] = [];
  const markers: { line: number; isDisposed: boolean; dispose(): void }[] = [];
  const decorations: { marker: (typeof markers)[number]; isDisposed: boolean; dispose(): void }[] = [];
  const term = {
    cols: 24,
    rows: 4,
    onWriteParsed: (listener: () => void) => writeListeners.push(listener),
    onScroll() {},
    onResize() {},
    buffer: {
      onBufferChange() {},
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
  const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
  const update = async () => {
    writeListeners.forEach((listener) => listener());
    await frame();
  };
  return { term: term as unknown as Terminal, lines, markers, decorations, update };
}

it("keeps a decoration for a link whose line moved, instead of placing it again", async () => {
  const { term, lines, markers, decorations, update } = fakeTerminal(["", "see http://a.dev/x", ""]);
  new TerminalLinkColors(term, "dark");
  await update();
  expect(decorations).toHaveLength(1);

  // The buffer is trimmed by one line: the link is on line 0 now, and its marker followed it.
  lines.shift();
  markers[0].line--;
  await update();
  expect(decorations).toHaveLength(1);
  expect(decorations[0].isDisposed).toBe(false);
});
