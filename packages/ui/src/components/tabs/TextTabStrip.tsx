import { Tabs } from "@heroui/react";
import React from "react";

import { LocalizedTabListContainer } from "./LocalizedTabListContainer";
import { TEXT_TAB_CLASS, TEXT_TAB_CONTAINER_CLASS, TEXT_TAB_LIST_CLASS } from "./textTabClasses";

/** The strip of text-only tabs for the `Tabs` it sits in, one per entry of `tabs`, from the strip's start at their
 * content's width; it scrolls with its edge labels when they do not fit. `label` is the tab list's accessible name. */
export function TextTabStrip({ label, tabs }: { label: string; tabs: readonly { id: string; label: React.ReactNode }[] }): React.ReactElement {
  return (
    <LocalizedTabListContainer className={TEXT_TAB_CONTAINER_CLASS}>
      <Tabs.List aria-label={label} className={TEXT_TAB_LIST_CLASS}>
        {tabs.map((tab) => (
          <Tabs.Tab key={tab.id} id={tab.id} className={TEXT_TAB_CLASS}>
            {tab.label}
            <Tabs.Indicator />
          </Tabs.Tab>
        ))}
      </Tabs.List>
    </LocalizedTabListContainer>
  );
}
