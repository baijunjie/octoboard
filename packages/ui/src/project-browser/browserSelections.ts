import { useSyncExternalStore } from "react";

/**
 * What a project's browser has selected and folded, kept for the window's lifetime only: a
 * selection is about what the user is looking at now, and what an earlier run named may well be
 * gone — a file, a branch, a change.
 */
export interface BrowserSelections {
  /** The tree's selected file (the one last opened in the viewer). */
  selected?: string;
  /** The branch whose changes the Git mode shows, as a wire path; none for the one checked out in
   * the worktree holding the project's directory. */
  branch?: string;
  /** The key of the change the Git mode's viewer showed last (`ChangeItem.key`). */
  selectedChange?: string;
  /** The rows of the Git mode's change list that the user folded away, as row keys: a section's
   * heading (`sectionKey`), a commit (`commitKey`) or, in the tree, a directory (`directoryKey`).
   * Everything starts unfolded. */
  collapsedChangeRows: ReadonlySet<string>;
}

/** The writers of one project's selections. */
export interface BrowserSelectionSetters {
  setSelected: (path: string | undefined) => void;
  setBranch: (branch: string | undefined) => void;
  setSelectedChange: (key: string | undefined) => void;
  setCollapsedChangeRows: (collapsed: ReadonlySet<string>) => void;
}

const NONE_COLLAPSED: ReadonlySet<string> = new Set();
const NOTHING_SELECTED: BrowserSelections = { collapsedChangeRows: NONE_COLLAPSED };

/** Each project's record, replaced whole on every change, so a reader can tell a change by identity. */
const records = new Map<string, BrowserSelections>();
const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function selectionsOf(project: string): BrowserSelections {
  return records.get(project) ?? NOTHING_SELECTED;
}

function change(project: string, next: Partial<BrowserSelections>): void {
  if (next.collapsedChangeRows?.size === 0) next = { ...next, collapsedChangeRows: NONE_COLLAPSED };
  const current = selectionsOf(project);
  if (Object.entries(next).every(([key, value]) => current[key as keyof BrowserSelections] === value)) return;
  const record = { ...current, ...next };
  if (record.selected === undefined && record.branch === undefined && record.selectedChange === undefined && record.collapsedChangeRows.size === 0) {
    records.delete(project);
  } else {
    records.set(project, record);
  }
  for (const listener of listeners) listener();
}

/** `project`'s selections, re-rendering the caller whenever they change. */
export function useBrowserSelections(project: string): BrowserSelections {
  return useSyncExternalStore(subscribe, () => selectionsOf(project));
}

/** The writers of `project`'s selections; the caller keeps them for as long as the project stays the
 * same, so they keep their identity across renders. */
export function browserSelectionSetters(project: string): BrowserSelectionSetters {
  return {
    setSelected: (selected) => change(project, { selected }),
    setBranch: (branch) => change(project, { branch }),
    setSelectedChange: (selectedChange) => change(project, { selectedChange }),
    setCollapsedChangeRows: (collapsedChangeRows) => change(project, { collapsedChangeRows }),
  };
}

/** Drops the selections of every project not among `projects`. */
export function forgetSelectionsOtherThan(projects: ReadonlySet<string>): void {
  const gone = [...records.keys()].filter((project) => !projects.has(project));
  if (gone.length === 0) return;
  for (const project of gone) records.delete(project);
  for (const listener of listeners) listener();
}
