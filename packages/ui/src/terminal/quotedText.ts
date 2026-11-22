/**
 * `text` as a Markdown block quote for an agent's input: every line behind `> `, then a line break, so what the user
 * types next starts on a line of its own under the quote. Blank lines around the text are dropped, as a selection
 * dragged past the end of a line carries them; a blank line inside it stays part of the quote, as `>` alone.
 */
export function quotedText(text: string): string {
  const lines = text.split(/\r\n?|\n/).map((line) => line.trimEnd());
  const first = lines.findIndex((line) => line !== "");
  if (first === -1) return "";
  let end = lines.length;
  while (lines[end - 1] === "") end--;
  return `${lines.slice(first, end).map((line) => (line === "" ? ">" : `> ${line}`)).join("\n")}\n`;
}

/** The part of xterm's active buffer `quoteLead` reads: the cursor, and the rows as xterm keeps them. */
export interface QuoteScreen {
  baseY: number;
  cursorX: number;
  cursorY: number;
  getLine(y: number): { translateToString(trimRight?: boolean, startColumn?: number, endColumn?: number): string } | undefined;
}

/**
 * What goes before a quote so that it starts on a line of its own: a line break where the text left of the cursor on
 * its row holds a letter or a digit, or does not end in whitespace; nothing otherwise. The terminal cannot read an
 * agent's input, so this goes by the screen alone, and by nothing that differs between agents. An empty input shows only
 * its prompt mark and a space left of the cursor (`❯ `, `› `, `│ ❯ ` as measured on Claude Code 2.1.330, Codex 0.167.1
 * and Grok Build 1.0.92), and after a quote the cursor already sits at the start of an empty line under it, so neither
 * gets a blank line; typed text does, and so does a collapsed paste placeholder such as `[Pasted text #1 +3 lines]`, and
 * so does a wrapped input whose last row ends in punctuation, which touches the cursor. The row is read untrimmed:
 * xterm's `trimRight` would take the space after the prompt mark off.
 */
export function quoteLead(screen: QuoteScreen): string {
  const before = screen.getLine(screen.baseY + screen.cursorY)?.translateToString(false, 0, screen.cursorX) ?? "";
  if (before === "") return "";
  return /[\p{L}\p{N}]/u.test(before) || !/\s$/u.test(before) ? "\n" : "";
}
