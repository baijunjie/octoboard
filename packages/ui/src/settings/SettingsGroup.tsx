import React, { useId } from "react";

/** The height of a group's heading, which pins to the top of the section's scroller while its rows scroll under it. */
export const SETTINGS_GROUP_HEADING_HEIGHT = 40;

/** A titled run of rows inside a settings section, named by an `<h3>` (the section's own title is the `<h2>`). The
 * heading pins to the top of the section's scroller, so the section registers `SETTINGS_GROUP_HEADING_HEIGHT` as its
 * `stickyHeadingHeight`. Its bottom divider sits under the heading itself; a group's last row, which `SettingRow`
 * draws without a divider, ends with the gap above the next heading instead of a line. */
export function SettingsGroup({ title, children }: { title: string; children: React.ReactNode }): React.ReactElement {
  const id = useId();
  return (
    <section aria-labelledby={id}>
      <h3
        id={id}
        data-sticky-heading=""
        className="sticky top-[calc(-1*var(--section-padding))] z-3 mt-4 flex h-[40px] items-center groove-b bg-overlay text-base font-semibold"
      >
        {title}
      </h3>
      {children}
    </section>
  );
}
