import { ChevronDown, ChevronRight, Folder, FolderOpen, FolderSymlink } from "lucide-react";
import React from "react";

import { Button } from "../components/Button";
import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";

/** Every row, of the Files tree and of the Git mode's changes tree, is one line of this
 * height, which is what lets the tree be virtualized and a row be scrolled into view from its
 * position alone. It includes `RowContent`'s `h-7` and the row's `my-0.5` margins. */
export const ROW_HEIGHT = 32;

/** The control that opens or closes a row's children, kept separate from the row's icon. */
export function ExpandChevron({ isExpanded }: { isExpanded: boolean }): React.ReactElement {
  const t = useT();
  const toggleTitle = isExpanded ? t("browser.tree.collapse") : t("browser.tree.expand");
  return (
    <TitledControl title={toggleTitle}>
      {/* React Aria appends the row's name through aria-labelledby. Its default translation
          of Expand differs from the tooltip, so both use the app's catalog. */}
      <Button slot="chevron" aria-label={toggleTitle} size="xs" variant="ghost" isIconOnly className="shrink-0">
        {isExpanded
          ? <ChevronDown aria-hidden="true" className="size-3.5" />
          : <ChevronRight aria-hidden="true" className="size-3.5 rtl:-scale-x-100" />}
      </Button>
    </TitledControl>
  );
}

/** A directory row; a symlinked folder retains its link marker in either expanded state. The name takes the
 * row's spare room, unless `nameGrows` is off for a mark that follows it right after (the name still shrinks
 * and fades when it is long); `children` then supply the spacer before the end-of-row actions. */
export function DirectoryRow({ name, isExpanded, link = false, nameGrows = true, children }: { name: string; isExpanded: boolean; link?: boolean; nameGrows?: boolean; children?: React.ReactNode }): React.ReactElement {
  const Icon = link ? FolderSymlink : isExpanded ? FolderOpen : Folder;
  return (
    <RowContent chevron={<ExpandChevron isExpanded={isExpanded} />}>
      <Icon aria-hidden="true" className="size-4 shrink-0 text-app-muted" />
      <RowName name={name} grow={nameGrows} />
      {children}
    </RowContent>
  );
}

/** A row: hovered and selected tints, and a visible ring under keyboard focus. It is also the
 * `group` its end-of-row controls key their visibility off, so a row laid out without this class
 * would hold an actions button that never appears. */
export const ROW_CLASS = [
  "group mx-(--row-fill-inset) my-0.5 cursor-default rounded-md text-sm outline-none select-none hover:bg-panel-hover data-[current]:bg-panel-selected",
  "data-[focus-visible]:ring-2 data-[focus-visible]:ring-focus data-[focus-visible]:ring-inset",
].join(" ");

// A flat row below the first level adds a chevron slot and its gap (1.375rem) once, whatever its depth.
const NESTED = "ps-[calc((var(--tree-item-level)-1)*0.875rem+var(--row-content-inset))]";
const FLAT_NESTED = "ps-[calc((var(--tree-item-level)-1)*0.875rem+min(1,var(--tree-item-level)-1)*1.375rem+var(--row-content-inset))]";

/** What a row lays out, inside a `TreeItem`: indented by its level, with the room of a chevron
 * before its content, or, for a row of a flat list (`flat`), without that room: a flat row has no
 * chevron to carry part of the separation, so it sits a little looser, and below the first level
 * (under a commit) is indented by a chevron's width more, so its content starts clear of the
 * parent's label, which the parent's own chevron pushes in. */
export function RowContent({ children, flat = false, chevron }: { children: React.ReactNode; flat?: boolean; chevron?: React.ReactNode }): React.ReactElement {
  return (
    <div className={`flex h-7 min-w-0 items-center ${flat ? `gap-2 ${FLAT_NESTED}` : `gap-1.5 ${NESTED}`} pe-(--row-content-inset)`}>
      {!flat && <span className="flex size-4 shrink-0 items-center justify-center">{chevron}</span>}
      {children}
    </div>
  );
}

export function RowName({ name, className = "", grow = true }: { name: string; className?: string; grow?: boolean }): React.ReactElement {
  return (
    <FadeOverflow as="span" dir="auto" className={`min-w-0 ${grow ? "flex-1" : "flex-initial"} ${className}`} titleWhenClipped={name}>
      {name}
    </FadeOverflow>
  );
}
