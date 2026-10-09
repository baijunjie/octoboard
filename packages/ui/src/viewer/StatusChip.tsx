import { Chip } from "@heroui/react";
import React from "react";

import { STATUS_MARKS, type StatusKey } from "./statusMarks";

/** A soft chip in the colour a change's status is marked with, for the list's letter and the
 * viewer's word alike. Untracked is no HeroUI colour, so it takes the project's own (`.chip-untracked`
 * in `style.css`). */
export function StatusChip({ status, className, ...rest }: { status: StatusKey } & Omit<React.ComponentProps<typeof Chip>, "color" | "variant" | "size">): React.ReactElement {
  const { color } = STATUS_MARKS[status];
  return color === "untracked" ? (
    <Chip size="sm" variant="soft" className={`chip-untracked ${className ?? ""}`} {...rest} />
  ) : (
    <Chip size="sm" variant="soft" color={color} className={className} {...rest} />
  );
}
