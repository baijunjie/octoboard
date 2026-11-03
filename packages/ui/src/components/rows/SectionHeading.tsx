import React from "react";

import { RowIconGroup } from "../controls/RowControls";
import { useStickyHeadingFocus } from "../scroll/stickyHeadingFocus";

/** A heading over a group of rows: the label with `action` at the row's end, and between them, when
 * given, `inline` (a filter in force) on the same line, taking the room the two leave and sizing and
 * clipping itself within it. Several controls in the action slot are spaced by the heading, so a
 * caller hands them over as a fragment rather than wrapping them itself. */
export function SectionHeading({
  children,
  action,
  inline,
}: {
  children: React.ReactNode;
  action?: React.ReactNode;
  inline?: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="mt-2 mb-0.5 px-(--row-content-inset)">
      <div className="flex items-center gap-2">
        <h3 className={`flex h-7 items-center text-xs font-medium text-app-muted ${inline ? "shrink-0" : "min-w-0 flex-1"}`}>{children}</h3>
        {inline && <div className="flex h-7 min-w-0 flex-1 items-center">{inline}</div>}
        {action && <RowIconGroup className="h-7">{action}</RowIconGroup>}
      </div>
    </div>
  );
}

export const STICKY_HEADING_HEIGHT = 38;

/** Each heading stays inside its section so the next section pushes it out of the way. */
export function StickySectionHeading(props: React.ComponentProps<typeof SectionHeading>): React.ReactElement {
  const pinnedFocus = useStickyHeadingFocus(STICKY_HEADING_HEIGHT);
  return (
    <div {...pinnedFocus} className={`sticky top-0 z-3 flow-root bg-panel-sidebar ${pinnedFocus.className}`}>
      <SectionHeading {...props} />
    </div>
  );
}
