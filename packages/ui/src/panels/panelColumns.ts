/** The usable width each card keeps before a pair stacks vertically. */
export const MIN_PANEL_COLUMN_WIDTH = 260;

/** Packs panels in reading order, keeping a lone card in the leading column at full height. */
export function panelColumns(kinds: readonly string[], width: number, gap: number): string[][] {
  if (kinds.length === 0) return [];
  if (kinds.length <= 2) {
    return kinds.length === 2 && width >= 2 * MIN_PANEL_COLUMN_WIDTH + gap
      ? kinds.map((kind) => [kind])
      : [[...kinds]];
  }
  const leadingCount = kinds.length % 2 === 0 ? 2 : 1;
  return [kinds.slice(0, leadingCount), ...Array.from({ length: Math.ceil((kinds.length - leadingCount) / 2) }, (_, index) => kinds.slice(leadingCount + index * 2, leadingCount + index * 2 + 2))];
}
