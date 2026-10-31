import React from "react";

import { StatusBadge, type StatusBadgeColor } from "./StatusBadge";

/** A soft `StatusBadge` for a state, with a dot before its words so the state is not carried by colour alone. `color`
 * and `separated` are `StatusBadge`'s. */
export function DotStatusBadge({ color, separated, children }: { color?: StatusBadgeColor; separated?: boolean; children: React.ReactNode }): React.ReactElement {
  return (
    <StatusBadge variant="soft" color={color} separated={separated} className="inline-flex items-center gap-1.5">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-current" />
      {children}
    </StatusBadge>
  );
}
