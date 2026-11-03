import { Tabs } from "@heroui/react";
import React from "react";

import { LocalizedTabListContainer } from "./LocalizedTabListContainer";

/** The strip of text-only tabs for the `Tabs` it sits in, one per entry of `tabs`, from the strip's start at their
 * content's width; it scrolls with its edge labels when they do not fit. `label` is the tab list's accessible name. */
export function TextTabStrip({ label, tabs }: { label: string; tabs: readonly { id: string; label: React.ReactNode }[] }): React.ReactElement {
  return (
    <LocalizedTabListContainer className="w-max max-w-full self-start">
      <Tabs.List aria-label={label} className="w-max min-w-0 p-1.5">
        {/* Hovered labels take the foreground colour for clear feedback. */}
        {tabs.map((tab) => (
          <Tabs.Tab key={tab.id} id={tab.id} className="w-auto flex-none scroll-mx-1.5 whitespace-nowrap hover:text-foreground">
            {tab.label}
            <Tabs.Indicator />
          </Tabs.Tab>
        ))}
      </Tabs.List>
    </LocalizedTabListContainer>
  );
}
