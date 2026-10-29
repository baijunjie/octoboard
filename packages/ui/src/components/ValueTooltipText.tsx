import type React from "react";

/**
 * The text of a tooltip that is a left-to-right value in full — a path, a branch name, a URI — rather
 * than a sentence. A tooltip is portalled out of what it describes and takes the window's direction,
 * so the value states its own. It is a block of its own, not a run inside the tooltip's line: under a
 * right-to-left window a run would sit in a right-to-left paragraph, whose lines all align to the
 * right, so a value wrapping onto a second line would start that line at the right. As a block its
 * lines align to its own start, the left, and read as one left-to-right paragraph; a value on one line
 * looks the same either way, the tooltip being only as wide as its text.
 */
export function ValueTooltipText({ children }: { children: string }): React.ReactElement {
  return <div dir="ltr">{children}</div>;
}
