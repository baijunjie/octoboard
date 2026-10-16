import React from "react";

import { InlineChip } from "../components/InlineChip";
import { STATUS_MARKS, type StatusKey } from "./statusMarks";

/** A soft chip in the colour a change's status is marked with, for the list's letter and the
 * viewer's word alike. Untracked is no HeroUI colour, so it takes the project's own (`.chip-untracked`
 * in `style.css`). */
export function StatusChip({ status, className, ...rest }: { status: StatusKey } & Omit<React.ComponentProps<typeof InlineChip>, "color" | "variant" | "size">): React.ReactElement {
  const { color } = STATUS_MARKS[status];
  return color === "untracked" ? (
    <InlineChip size="sm" variant="soft" className={`chip-untracked ${className ?? ""}`} {...rest} />
  ) : (
    <InlineChip size="sm" variant="soft" color={color} className={className} {...rest} />
  );
}
