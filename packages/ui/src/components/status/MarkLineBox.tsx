import React from "react";

/**
 * The box a mark stands in after a line of text, as "How a mark is drawn and opened" in `docs/ui-accessibility.md`
 * sets out: one line of that text's own size and line height, the mark centred in it. `size` is the text's size class
 * (`text-sm`), repeated here so the `1lh` does not depend on a size inherited from somewhere else. The row holding
 * the text and this box needs `items-start`.
 */
export function MarkLineBox({ size, children }: { size: string; children: React.ReactNode }): React.ReactElement {
  return <span className={`flex h-[1lh] items-center ${size}`}>{children}</span>;
}
