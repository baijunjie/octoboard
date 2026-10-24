import { ChevronsDownUp, FolderTree, Funnel, FunnelX, GitCommitVertical, Layers, List } from "lucide-react";
import React from "react";

import { Button } from "../components/Button";
import { IconToggle } from "../components/IconToggle";
import { ROW_ICON_BUTTON_CLASS } from "../components/RowControls";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";
import type { ChangeLayout } from "./changeLayout";
import type { FileFilterState } from "./FileFilterField";

/** The header's controls for the tab on screen: the filter field's toggle, then, for the Files tree
 * alone, the button that folds every folder (`collapseAll`) and, for the Git mode's changes
 * alone, the folder grouping (`layout`; the Files tree is always by folder) and whether what is
 * committed is grouped by commit (`byCommit`). */
export function BrowserViewControls({
  filter,
  collapseAll,
  layout,
  byCommit,
}: {
  filter: FileFilterState;
  collapseAll?: { disabled: boolean; onPress: () => void };
  layout?: { value: ChangeLayout; onChange: (layout: ChangeLayout) => void };
  byCommit?: { value: boolean; onChange: (byCommit: boolean) => void };
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
          <Button isIconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t("browser.collapseAll")} isDisabled={collapseAll.disabled} preventFocusOnPress onPress={collapseAll.onPress}>
            <ChevronsDownUp aria-hidden="true" className="size-4" />
          </Button>
        </TitledControl>
      )}
      {layout && (
        <IconToggle label={t(layout.value === "tree" ? "git.layout.flat" : "git.layout.tree")} isSelected={layout.value === "tree"} onChange={(grouped) => layout.onChange(grouped ? "tree" : "flat")}>
          {layout.value === "tree" ? <FolderTree aria-hidden="true" className="size-4 rtl:-scale-x-100" /> : <List aria-hidden="true" className="size-4 rtl:-scale-x-100" />}
        </IconToggle>
      )}
      {byCommit && (
        <IconToggle label={t(byCommit.value ? "git.layout.allCommits" : "git.layout.byCommit")} isSelected={byCommit.value} onChange={byCommit.onChange}>
          {byCommit.value ? <GitCommitVertical aria-hidden="true" className="size-4" /> : <Layers aria-hidden="true" className="size-4" />}
        </IconToggle>
      )}
    </>
  );
}
