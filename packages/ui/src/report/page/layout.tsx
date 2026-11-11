// The components of a page that only arrange it: columns side by side, tabs, and the rule between blocks. Tabs hold no
// input field or submitter (the daemon refuses a form part in a tab), so a tab's panel mounting only while it is selected
// loses no value.
import React from "react";

import { useT } from "../../i18n/react";
import { Tab, TabList, TabListContainer, TabPanel, Tabs } from "../../primitives";
import { usePageForm } from "./PageFormContext";
import { type ComponentProps, Components } from "./registry";

/** The columns' layout by their count: side by side from the width of the page that gives each column room, one under
 * the other below it. The page is the container (`@container` on its form), as the panel may be narrow or floated
 * whatever the window is. The classes are written out, as Tailwind finds them by their text. */
const COLUMNS_CLASS: Record<number, string> = {
  1: "",
  2: "@md:grid-cols-2",
  3: "@xl:grid-cols-3",
  4: "@3xl:grid-cols-4",
};

/** The columns that are shown: a column whose content is hidden leaves no empty cell behind, and the rest share the
 * width. */
export function ColumnsView({ component }: ComponentProps<"columns">): React.ReactElement {
  const form = usePageForm();
  // Each column keeps the key of its place among all the columns, so hiding one does not hand its state to the next.
  const shown = component.children.map((child, place) => ({ child, place })).filter(({ child }) => form.isShown(child));
  return (
    <div className={`grid grid-cols-1 gap-4 ${COLUMNS_CLASS[shown.length] ?? COLUMNS_CLASS[2]}`}>
      {shown.map(({ child, place }) => (
        <div key={place} className="flex min-w-0 flex-col gap-4">
          <Components list={[child]} />
        </div>
      ))}
    </div>
  );
}

export function TabsView({ component }: ComponentProps<"tabs">): React.ReactElement {
  const t = useT();
  return (
    <Tabs variant="underline" defaultSelectedKey="0">
      <TabListContainer fit="content">
        {/* The list pads its tabs for their focus rings, so the indicator moves down by that padding to sit on the
          strip's line, as the agent tabs' does. */}
        <TabList label={t("report.tabsLabel")} fit="content">
          {component.tabs.map((tab, index) => <Tab key={index} id={String(index)} size="content" indicatorClassName="-bottom-1"><span dir="auto">{tab.label}</span></Tab>)}
        </TabList>
      </TabListContainer>
      {component.tabs.map((tab, index) => (
        <TabPanel key={index} id={String(index)} className="flex flex-col gap-4 px-0 pt-4 pb-0">
          <Components list={tab.children} />
        </TabPanel>
      ))}
    </Tabs>
  );
}

/** A thin rule between blocks, with its label, if it has one, on the rule at its start. A page's blocks are `gap-6` apart
 * (`gap-4` in a section), which on both sides of a rule would read as a wide gap, so the rule pulls each side in by `2`
 * (the label's own height keeps a labelled rule from being squeezed). A labelled rule is a `separator` named by its
 * label (its children are presentational, so the name is on the element, not in its text). */
export function DividerView({ component }: ComponentProps<"divider">): React.ReactElement {
  if (component.label === undefined) return <hr className="-my-2 border-0 border-t border-separator" />;
  return (
    <div role="separator" aria-label={component.label} className="-my-1 flex items-center gap-3">
      <span aria-hidden="true" className="min-w-0 text-xs text-app-muted align-match-parent [overflow-wrap:anywhere]" dir="auto">{component.label}</span>
      <span aria-hidden="true" className="h-px min-w-8 flex-1 bg-separator" />
    </div>
  );
}
