import React from "react";

/** The content of a tooltip that has a heading and something under it: the heading on a line of its own, a groove
 * line, then `children`, which the caller lays out (the rail avatar's status rows, an action's reason or note). The
 * heading reads in its own direction, as a name the user typed does. A control with such a tooltip names itself in its
 * own `label` and may keep the tooltip as its description, which says the rest (see `ControlHint`, which drops it where
 * the tooltip only repeats the name). */
export function HeadedTooltip({ heading, children }: { heading: React.ReactNode; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <div dir="auto" className="min-w-0 font-medium [overflow-wrap:anywhere]">{heading}</div>
      <hr className="groove-t" />
      {children}
    </div>
  );
}
