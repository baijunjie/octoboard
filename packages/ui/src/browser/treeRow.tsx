import { ChevronDown, ChevronRight, Folder, FolderOpen, FolderSymlink } from "lucide-react";
import React from "react";
import { Button } from "react-aria-components";

import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";

/** Every row, of the Files tree and of the Git mode's change list and tree, is one line of this
 * height, which is what lets the tree be virtualized and a row be scrolled into view from its
 * position alone. `RowContent`'s `h-7` is this height, whichever shape of row it lays out. */
export const ROW_HEIGHT = 28;

/** The content of a directory's row: a chevron, a folder icon and the name. */
export function DirectoryRow({ name, isExpanded, link = false }: { name: string; isExpanded: boolean; link?: boolean }): React.ReactElement {
  const t = useT();
  const chevronTitle = isExpanded ? t("browser.tree.collapse") : t("browser.tree.expand");
  const Icon = link ? FolderSymlink : isExpanded ? FolderOpen : Folder;
  return (
    <RowContent>
      {/* React Aria keeps the chevron out of the tab order; the row does the same on a press,
          so the chevron is for the pointer alone. React Aria's own label is a translation of
          "Expand" that disagrees with the tooltip's wording (zh-CN "扩展"), so the label is the
          tooltip's word from the catalog; React Aria appends the row's name through
          aria-labelledby, which gives "Expand src". */}
      <TitledControl title={chevronTitle}>
        <Button slot="chevron" aria-label={chevronTitle} className="flex size-4 shrink-0 items-center justify-center text-muted outline-none">
          {isExpanded ? <ChevronDown aria-hidden="true" className="size-3.5" /> : <ChevronRight aria-hidden="true" className="size-3.5 rtl:-scale-x-100" />}
        </Button>
      </TitledControl>
      <Icon aria-hidden="true" className="size-4 shrink-0 text-muted" />
      <RowName name={name} />
    </RowContent>
  );
}

/** A row: hovered and selected tints, and a visible ring under keyboard focus. It is also the
 * `group` its end-of-row controls key their visibility off, so a row laid out without this class
 * would hold an actions button that never appears. */
export const ROW_CLASS = [
  "group cursor-default rounded-md text-sm outline-none select-none hover:bg-panel-hover data-[current]:bg-panel-selected",
  "data-[focus-visible]:ring-2 data-[focus-visible]:ring-focus data-[focus-visible]:ring-inset",
].join(" ");

/** What a row lays out, in one line of `ROW_HEIGHT`: indented by its level inside a `TreeItem`, or,
 * for a row of a flat list (`flat`), which has no level to read, inset by a fixed padding. A flat
 * row also sits a little looser, having no indent to carry part of the separation. */
export function RowContent({ children, flat = false }: { children: React.ReactNode; flat?: boolean }): React.ReactElement {
  return (
    <div className={`flex h-7 min-w-0 items-center ${flat ? "gap-2 px-2" : "gap-1.5 pe-2 ps-[calc((var(--tree-item-level)-1)*0.875rem+0.5rem)]"}`}>
      {children}
    </div>
  );
}

export function RowName({ name, className = "" }: { name: string; className?: string }): React.ReactElement {
  return (
    <FadeOverflow as="span" dir="auto" className={`min-w-0 flex-1 ${className}`} titleWhenClipped={name}>
      {name}
    </FadeOverflow>
  );
}
