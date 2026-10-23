import { Spinner } from "@heroui/react";
import React from "react";

/** The Git mode's loading line: what is being read. Seen only; a `StatusAnnouncer` mounted beside
 * it says it. */
export function GitLoading({ label }: { label: string }): React.ReactElement {
  return (
    <div aria-hidden="true" className="flex flex-1 items-center justify-center gap-2 text-sm text-app-muted">
      <Spinner size="sm" aria-hidden="true" />
      {label}
    </div>
  );
}
