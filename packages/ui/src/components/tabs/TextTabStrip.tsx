import React from "react";

import { Tab, TabList, TabListContainer } from "../../primitives";

/** The strip of text-only tabs for the `Tabs` it sits in, one per entry of `tabs`, from the strip's start at their
 * content's width; it scrolls with its edge labels when they do not fit. `label` is the tab list's accessible name. */
export function TextTabStrip({ label, tabs }: { label: string; tabs: readonly { id: string; label: React.ReactNode }[] }): React.ReactElement {
  return (
    <TabListContainer fit="content">
      <TabList label={label} fit="content">
        {tabs.map((tab) => (
          <Tab key={tab.id} id={tab.id} size="content">
            {tab.label}
          </Tab>
        ))}
      </TabList>
    </TabListContainer>
  );
}
