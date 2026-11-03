import { Spinner } from "@heroui/react";
import React from "react";

/**
 * A pane's loading line: a spinner and what is being read, centred in the space the parent leaves
 * it. Seen only, hidden from assistive technology: the caller speaks `label` from a
 * `StatusAnnouncer` mounted beside it. `sm` is a line of its own in a list or a viewer, `md` a state
 * that takes a whole pane, stacked and larger. `spinner` off leaves the label alone, for a cover
 * that only waits on a sentence.
 */
export function LoadingState({ label, size = "sm", spinner = true }: { label: string; size?: "sm" | "md"; spinner?: boolean }): React.ReactElement {
  return size === "sm" ? (
    <div aria-hidden="true" className="flex min-h-0 flex-1 items-center justify-center gap-2 text-sm text-app-muted">
      {spinner && <Spinner size="sm" aria-hidden="true" />}
      {label}
    </div>
  ) : (
    <div aria-hidden="true" className="flex min-h-0 flex-1 flex-col items-center justify-center-safe gap-3 text-sm text-app-muted">
      {spinner && <Spinner size="md" color="current" aria-hidden="true" />}
      {label}
    </div>
  );
}
