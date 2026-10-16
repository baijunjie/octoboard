import { FolderTree, Funnel, FunnelX, List } from "lucide-react";
import React from "react";

import { IconToggle } from "../components/IconToggle";
import { useT } from "../i18n/react";
import type { ChangeLayout } from "./changeLayout";
import type { FileFilterState } from "./FileFilterField";

export function BrowserViewControls({ filter, layout, onLayoutChange }: { filter: FileFilterState; layout: ChangeLayout; onLayoutChange: (layout: ChangeLayout) => void }): React.ReactElement {
  const t = useT();
  const { filterToggleRef, filterOpen, onFilterOpenChange } = filter;
  return (
    <>
      <IconToggle ref={filterToggleRef} label={t(filterOpen ? "browser.filter.hide" : "browser.filter.show")} isSelected={filterOpen} expanded={filterOpen} onChange={onFilterOpenChange}>
        {filterOpen ? <FunnelX aria-hidden="true" className="size-4" /> : <Funnel aria-hidden="true" className="size-4" />}
      </IconToggle>
      <IconToggle label={t(layout === "tree" ? "git.layout.flat" : "git.layout.tree")} isSelected={layout === "tree"} onChange={(grouped) => onLayoutChange(grouped ? "tree" : "flat")}>
        {layout === "tree" ? <FolderTree aria-hidden="true" className="size-4 rtl:-scale-x-100" /> : <List aria-hidden="true" className="size-4 rtl:-scale-x-100" />}
      </IconToggle>
    </>
  );
}
