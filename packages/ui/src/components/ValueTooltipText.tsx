import React, { useLayoutEffect, useState } from "react";

/**
 * The text of a tooltip that is a value in full — a path, a branch name, a URI, a session id, a name
 * too long for its label — rather than a sentence. A tooltip is portalled out of what it describes,
 * so it inherits neither the label's font nor its direction, and the value takes both from the label
 * again:
 *
 * - the font family of `label`, the element that draws the value, so a value shown in monospace (a
 *   session id, a path, a terminal's text) is monospace in its tooltip too, and one in the UI font
 *   stays in it. Only the family: the tooltip keeps its own size and weight.
 * - with `dir="ltr"`, the left-to-right direction of a value that reads left to right in every
 *   language. It is a block of its own, not a run inside the tooltip's line: under a right-to-left
 *   window a run would sit in a right-to-left paragraph, whose lines all align to the right, so a
 *   value wrapping onto a second line would start that line at the right. As a block its lines align
 *   to its own start, the left, and read as one left-to-right paragraph; a value on one line looks
 *   the same either way, the tooltip being only as wide as its text. Without `dir` the value takes
 *   the window's direction, as a name the user typed does.
 */
export function ValueTooltipText({
  children,
  label,
  dir,
}: {
  children: string;
  label: React.RefObject<Element | null>;
  dir?: "ltr";
}): React.ReactElement {
  const [fontFamily, setFontFamily] = useState<string>();
  // Read on every commit rather than once: the label's font can change while the tooltip is open (a
  // class swapped, the element replaced). An unchanged family leaves the state as it was, so this
  // settles after one pass.
  useLayoutEffect(() => {
    if (label.current) setFontFamily(getComputedStyle(label.current).fontFamily || undefined);
  });
  return <div dir={dir} style={fontFamily ? { fontFamily } : undefined}>{children}</div>;
}
