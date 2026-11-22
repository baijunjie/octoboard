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
