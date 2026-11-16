import React from "react";

import { Tab, TabList, TabListContainer } from "../../primitives";

/** The strip of text-only tabs for the `Tabs` it sits in, one per entry of `tabs`, from the strip's start at their
 * content's width; it scrolls with its edge labels when they do not fit. `label` is the tab list's accessible name.
 *
 * With `underline`, for the `Tabs` of `variant="underline"`, it is the strip of a page of Settings, whose own sections
 * are the filled strip: a row `h-10` with the divider drawn under it as a groove across the whole row, the tabs at their
 * content width from the start, the selected one marked by an underline over the divider.
 */
export function TextTabStrip({ label, tabs, underline }: { label: string; tabs: readonly { id: string; label: React.ReactNode }[]; underline?: boolean }): React.ReactElement {
  return (
    <TabListContainer fit={underline ? undefined : "content"} className={underline ? "h-10 w-full min-w-0 max-w-full groove-b" : undefined}>
      <TabList label={label} fit="content" className={underline ? "relative" : undefined}>
        {tabs.map((tab) => (
          <Tab key={tab.id} id={tab.id} size="content" indicatorClassName={underline ? "-bottom-1" : undefined}>
            {tab.label}
          </Tab>
        ))}
      </TabList>
    </TabListContainer>
  );
}
