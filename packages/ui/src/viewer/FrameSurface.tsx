import React from "react";

/** The focus ring sits above the scrolling child's fade mask and inside the frame, so it remains
 * visible where the reading area meets the dialog's clipped edges. */
export function FrameSurface({ children, focused, background }: { children: React.ReactNode; focused?: boolean; background?: string }): React.ReactElement {
  return (
    <div style={background ? { backgroundColor: background } : undefined} className={`relative isolate flex min-h-0 min-w-0 flex-1 flex-col ${focused ? "after:pointer-events-none after:absolute after:inset-0 after:z-10 after:ring-2 after:ring-focus after:ring-inset" : ""}`}>
      {children}
    </div>
  );
}
