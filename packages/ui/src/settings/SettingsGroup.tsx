import React, { useId } from "react";

import { StickyHeading } from "../components/scroll/StickyHeading";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { useSettingsSearchFilter } from "./settingsSearchFilter";

/** A titled run of rows inside a settings section, named by its heading (the section's own title is the `<h2>`). The
 * heading pins to the top of the section's scroller, so the section registers `STICKY_HEADING_HEIGHT.group` as its
 * `stickyHeadingHeight`. Its bottom divider sits under the heading itself; a group's last row, which `SettingRow`
 * draws without a divider, ends with the gap above the next heading instead of a line. `title` is also the `group` of
 * its rows' entries in `SETTINGS_SEARCH_INDEX`: while a search in Settings finds none of them, the heading is hidden
 * with them and the group, holding nothing shown, is no longer named, so it is no region of its own. The group itself
 * stays shown: its heading and rows leave and come back as items of the section's list motion (marked `group:` and the
 * title), and its own lists play out their rows' leaving, neither of which can be seen inside a hidden element. */
export function SettingsGroup({ title, children }: { title: PlainMessageKey; children: React.ReactNode }): React.ReactElement {
  const t = useT();
  const id = useId();
  const shown = useSettingsSearchFilter().showsGroup(title);
  return (
    <section aria-labelledby={shown ? id : undefined} data-settings-search-group={title}>
      <StickyHeading variant="group" id={id} hidden={!shown} motionKey={`group:${title}`}>{t(title)}</StickyHeading>
      {children}
    </section>
  );
}
