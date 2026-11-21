import React from "react";

import { Spinner } from "../../primitives";

/**
 * A pane's loading line: a spinner and what is being read, centred in the space the parent leaves
 * it. Seen only, hidden from assistive technology: the caller speaks `label` from a
 * `StatusAnnouncer` mounted beside it. `sm` is a line of its own in a list or a viewer, `md` a state
 * that takes a whole pane, stacked and larger. `none` leaves the label on its own.
 */
export function LoadingState({ label, size = "sm", indicator = "spinner" }: {
  label: string;
  size?: "sm" | "md";
  indicator?: "spinner" | "none";
}): React.ReactElement {
  const layout = size === "sm" ? "justify-center gap-2" : "flex-col justify-center-safe gap-3";
  return (
    <div aria-hidden="true" className={`flex min-h-0 flex-1 items-center text-sm text-app-muted ${layout}`}>
      {indicator === "spinner" && <Spinner size={size} />}
      {label}
    </div>
  );
}
