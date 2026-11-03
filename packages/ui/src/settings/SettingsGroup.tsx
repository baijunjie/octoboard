import React, { useId } from "react";

import { StickyHeading } from "../components/scroll/StickyHeading";

/** A titled run of rows inside a settings section, named by its heading (the section's own title is the `<h2>`). The
 * heading pins to the top of the section's scroller, so the section registers `STICKY_HEADING_HEIGHT.group` as its
 * `stickyHeadingHeight`. Its bottom divider sits under the heading itself; a group's last row, which `SettingRow`
 * draws without a divider, ends with the gap above the next heading instead of a line. */
export function SettingsGroup({ title, children }: { title: string; children: React.ReactNode }): React.ReactElement {
  const id = useId();
  return (
    <section aria-labelledby={id}>
      <StickyHeading variant="group" id={id}>{title}</StickyHeading>
      {children}
    </section>
  );
}
