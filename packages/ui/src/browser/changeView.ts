import type { ChangeLayout } from "./changeLayout";
import type { FileFilterState } from "./FileFilterField";

/** How a view shows its changes and what it remembers of that: the layout, the rows folded away —
 * a section's heading (`sectionKey`) and, in the tree, a directory (`directoryKey`) — whether the
 * filter field (`FileFilterField`) is on screen (`filterOpen`) and the file name text it narrows
 * the rows by (`filter`, empty for none). */
export interface ChangeViewState extends FileFilterState {
  layout: ChangeLayout;
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
}
