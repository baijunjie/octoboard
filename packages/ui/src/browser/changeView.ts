import type React from "react";

import type { ChangeLayout } from "./changeLayout";

/** How a view shows its changes and what it remembers of that: the layout, in the tree the
 * directories folded away (their row keys, `directoryKey`), whether the filter field
 * (`ChangeFilterField`) is on screen (`filterOpen`) and the file name text it narrows the rows by
 * (`filter`, empty for none). */
export interface ChangeViewState {
  layout: ChangeLayout;
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
  filterOpen: boolean;
  onFilterOpenChange: (open: boolean) => void;
  filter: string;
  onFilterChange: (filter: string) => void;
  /** The toggle that opens the field, for the field to hand keyboard focus to as it goes. */
  filterToggleRef: React.RefObject<HTMLButtonElement | null>;
}
