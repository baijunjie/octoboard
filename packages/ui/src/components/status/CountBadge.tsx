import React from "react";

import { StatusBadge } from "./StatusBadge";

/** A decorative count in the flow of a row, whose accessible name includes the count. */
export function CountBadge({ count, color = "default" }: {
  count: number;
  color?: "default" | "success";
}): React.ReactElement {
  return (
    <StatusBadge color={color} separated={false} aria-hidden="true">
      {count}
    </StatusBadge>
  );
}
