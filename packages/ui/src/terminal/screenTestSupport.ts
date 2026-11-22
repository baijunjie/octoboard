// The one imitation of xterm's buffer rows that the tests reading the screen share (`quotedText.test.ts`, the fake
// terminal of `TerminalController.test.ts`), so they stay in step.

/** The part of xterm's `IBufferLine` the code under test reads. */
export interface ScreenRow {
  translateToString(trimRight?: boolean, startColumn?: number, endColumn?: number): string;
}

/** The row `text` as xterm's `translateToString` reads it: cut to the columns asked for, then, with `trimRight`, without
 * the blanks at its end. */
export function screenRow(text: string): ScreenRow {
  return {
    translateToString: (trimRight = false, startColumn = 0, endColumn = text.length) => {
      const cut = text.slice(startColumn, endColumn);
      return trimRight ? cut.trimEnd() : cut;
    },
  };
}

/** `getLine` of a buffer holding `rows`: `undefined` past the last row, as xterm's is. */
export function rowsReader(rows: readonly string[]): (y: number) => ScreenRow | undefined {
  return (y) => (rows[y] === undefined ? undefined : screenRow(rows[y]));
}
