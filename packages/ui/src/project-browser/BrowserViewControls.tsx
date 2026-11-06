import { ChevronsDownUp, FolderTree, Funnel, FunnelX, List } from "lucide-react";
import React from "react";

import { IconToggle } from "../components/controls/IconToggle";
import { ROW_ICON_BUTTON_CLASS } from "../components/controls/RowControls";
import { TitledControl } from "../components/hint/TitledControl";
import { useT } from "../i18n/react";
import type { ChangeLayout } from "./git/changeLayout";
import type { FileFilterState } from "./FileFilterField";
import { Button } from "../primitives";

/** The header's controls for the tab on screen: the filter field's toggle, then, for the Files tree
 * alone, the button that folds every folder (`collapseAll`) and, for the Git mode's changes
 * alone, the folder grouping (`layout`; the Files tree is always by folder). */
export function BrowserViewControls({
  filter,
  collapseAll,
  layout,
}: {
  filter: FileFilterState;
  collapseAll?: { disabled: boolean; onPress: () => void };
  layout?: { value: ChangeLayout; onChange: (layout: ChangeLayout) => void };
}): React.ReactElement {
  const t = useT();
  const { filterToggleRef, filterOpen, onFilterOpenChange } = filter;
  return (
    <>
      <IconToggle ref={filterToggleRef} label={t(filterOpen ? "browser.filter.hide" : "browser.filter.show")} isSelected={filterOpen} expanded={filterOpen} onChange={onFilterOpenChange}>
        {filterOpen ? <FunnelX aria-hidden="true" className="size-4" /> : <Funnel aria-hidden="true" className="size-4" />}
      </IconToggle>
      {collapseAll && (
        <TitledControl title={t("browser.collapseAll")}>
          <Button iconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} label={t("browser.collapseAll")} disabled={collapseAll.disabled} focusOnPress={false} onPress={collapseAll.onPress}>
            <ChevronsDownUp aria-hidden="true" className="size-4" />
          </Button>
        </TitledControl>
      )}
      {layout && (
        <IconToggle label={t(layout.value === "tree" ? "git.layout.flat" : "git.layout.tree")} isSelected={layout.value === "tree"} onChange={(grouped) => layout.onChange(grouped ? "tree" : "flat")}>
          {layout.value === "tree" ? <FolderTree aria-hidden="true" className="size-4 rtl:-scale-x-100" /> : <List aria-hidden="true" className="size-4 rtl:-scale-x-100" />}
        </IconToggle>
      )}
    </>
  );
}
