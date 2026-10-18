// The line of muted text the project pane puts above or below a list to say something about it:
// that it was cut short, what a comparison covers, what could not be refreshed.
import React from "react";

/** A note on the pane's list, at the pane's inset. `divided` draws a divider above it, for a
 * note that closes a list rather than heading it. `role` is passed through for a note that has to
 * be announced when it appears. */
export function PaneNote({ divided = false, role, children }: { divided?: boolean; role?: "alert"; children: React.ReactNode }): React.ReactElement {
  return (
    <p role={role} className={`shrink-0 px-(--pane-inset) py-2 text-xs text-app-muted ${divided ? "groove-t" : ""}`}>
      {children}
    </p>
  );
}
