import { ChevronsDownUp, ChevronsUpDown, FolderTree, Funnel, FunnelX, List, TextSearch } from "lucide-react";
import React from "react";

import { IconToggle } from "../components/controls/IconToggle";
import { ROW_ICON_BUTTON_CLASS } from "../components/controls/RowControls";
import { useT } from "../i18n/react";
import type { ChangeLayout } from "./git/changeLayout";
import type { FileFilterState } from "./FileFilterField";
import { Button } from "../primitives";

/** The header's controls for the tab on screen: the filter field's toggle, then, for the Files tree
 * alone, the button that opens Find in files on the whole project (`findInFiles`) and the button that folds every
 * folder (`collapseAll`) and, for the Git mode's changes
 * alone, the button that folds every folder or, with none open, unfolds them all (`foldDirectories`,
 * in the tree layout only) and the folder grouping (`layout`; the Files tree is always by folder). */
export function BrowserViewControls({
  filter,
  findInFiles,
  findInFilesRef,
  collapseAll,
  foldDirectories,
  layout,
}: {
  filter: FileFilterState;
  findInFiles?: () => void;
  findInFilesRef?: React.Ref<HTMLButtonElement>;
  collapseAll?: { disabled: boolean; onPress: () => void };
  foldDirectories?: { collapse: boolean; disabled: boolean; onPress: () => void };
  layout?: { value: ChangeLayout; onChange: (layout: ChangeLayout) => void };
}): React.ReactElement {
  const t = useT();
  const { filterToggleRef, filterOpen, onFilterOpenChange } = filter;
  const foldLabel = foldDirectories && t(foldDirectories.collapse ? "browser.collapseAll" : "browser.expandAll");
  return (
    <>
      <IconToggle ref={filterToggleRef} label={t(filterOpen ? "browser.filter.hide" : "browser.filter.show")} isSelected={filterOpen} expanded={filterOpen} onChange={onFilterOpenChange}>
        {filterOpen ? <FunnelX className="size-4" /> : <Funnel className="size-4" />}
      </IconToggle>
      {findInFiles && (
        <Button ref={findInFilesRef} tooltip={t("contentSearch.title")} iconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} label={t("contentSearch.title")} focusOnPress={false} onPress={findInFiles}>
          <TextSearch className="size-4" />
        </Button>
      )}
      {collapseAll && (
        <Button tooltip={t("browser.collapseAll")} iconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} label={t("browser.collapseAll")} disabled={collapseAll.disabled} focusOnPress={false} onPress={collapseAll.onPress}>
          <ChevronsDownUp className="size-4" />
        </Button>
      )}
      {foldDirectories && (
        <Button
          tooltip={foldLabel}
          iconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS}
          label={foldLabel}
          disabled={foldDirectories.disabled} focusOnPress={false} onPress={foldDirectories.onPress}
        >
          {foldDirectories.collapse ? <ChevronsDownUp className="size-4" /> : <ChevronsUpDown className="size-4" />}
        </Button>
      )}
      {layout && (
        <IconToggle label={t(layout.value === "tree" ? "git.layout.flat" : "git.layout.tree")} isSelected={layout.value === "tree"} onChange={(grouped) => layout.onChange(grouped ? "tree" : "flat")}>
          {layout.value === "tree" ? <FolderTree className="size-4 rtl:-scale-x-100" /> : <List className="size-4 rtl:-scale-x-100" />}
        </IconToggle>
      )}
    </>
  );
}
