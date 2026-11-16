import React from "react";

import { useListMotion } from "../components/motion/useListMotion";
import { TabPanel } from "../primitives";

/** The panel of a tab strip inside a Settings section, holding `SettingRow`s. Switching tabs swaps the panels, which is
 * no change of a collection: the rows appear and go at once. Left alone, the rows
 * (`data-list-motion`) would be descendants of the section's own list motion (`SectionPanel`), which would fade the
 * ones that leave and the ones that arrive with every switch. The rows are a list of their own here, which makes
 * them its to move: not at all as the panel mounts, and still with list motion when a search hides one of them. */
export function SettingsTabPanel({ id, children }: { id: string; children: React.ReactNode }): React.ReactElement {
  const listRef = useListMotion<HTMLDivElement>();
  return (
    <TabPanel id={id} className="mt-0 p-0">
      <div ref={listRef} className="relative">{children}</div>
    </TabPanel>
  );
}
