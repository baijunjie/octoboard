import { useSyncExternalStore } from "react";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { changeLayout, type ChangeLayout } from "./changeLayout";
import { gitMoment } from "./gitRequests";

/** What a project's browser shows: its files on disk, or Git review. */
export type BrowserMode = "files" | "git";

/** The part of a project's browser state that outlives the window: its mode, the Git mode's change
 * layout, whether the Git mode's committed changes are grouped by commit, and directories expanded in its
 * tree (wire paths). File bodies and listings are never stored. */
interface StoredBrowser {
  mode: BrowserMode;
  expanded: string[];
  layouts?: { changes?: ChangeLayout };
  byCommit?: boolean;
  /** When the project's browser was last shown, for dropping the least recently used ones. */
  used: number;
}

/** How many projects' browser state is kept, and how many expanded directories per project; past
 * either, the least recently used project and the earliest expanded directory go first. */
const MAX_PROJECTS = 50;
const MAX_EXPANDED = 200;

export function parseStored(raw: string | null): Record<string, StoredBrowser> {
  try {
    const value = raw ? (JSON.parse(raw) as unknown) : undefined;
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const result: Record<string, StoredBrowser> = {};
    for (const [project, entry] of Object.entries(value as Record<string, Partial<StoredBrowser>>)) {
      if (!entry || typeof entry !== "object") continue;
      const expanded = Array.isArray(entry.expanded) ? entry.expanded.filter((p): p is string => typeof p === "string" && p !== "") : [];
      const mode = entry.mode === "git" ? "git" : "files";
      // Earlier versions kept a layout per tab: the Files tab's is no longer a choice, and a stored
      // one is ignored. `worktree` is the key they kept the Git mode's under, which stands for the
      // Changes tab until that has one of its own.
      const saved = entry.layouts as Partial<Record<"changes" | "worktree", unknown>> | undefined;
      const layout = saved?.changes ?? saved?.worktree;
      result[project] = {
        mode,
        layouts: layout === "flat" || layout === "tree" ? { changes: layout } : {},
        ...(entry.byCommit === true ? { byCommit: true } : {}),
        expanded: expanded.slice(-MAX_EXPANDED),
        used: typeof entry.used === "number" ? entry.used : 0,
      };
    }
    return result;
  } catch {
    return {};
  }
}

const stored = createPersistedPreference<Record<string, StoredBrowser>>(PREFERENCE_KEYS.projectBrowsers, parseStored, (value) =>
  Object.keys(value).length === 0 ? null : JSON.stringify(value),
);

/** What each project's browser has selected, kept for the window's lifetime only: a selection is
 * about what the user is looking at now, and what an earlier run named may well be gone — a file,
 * a branch, a change. `file` is the tree's selected file (the one last opened in the viewer),
 * `branch` the branch whose changes the Git mode shows (a wire path; none for the one checked out
 * where the project's directory is), and `change` the key of the change last opened from them
 * (`ChangeItem.key`). */
type Selection = "file" | "branch" | "change";
const SELECTIONS: readonly Selection[] = ["file", "branch", "change"];
const selections = Object.fromEntries(SELECTIONS.map((kind) => [kind, new Map<string, string>()])) as Record<Selection, Map<string, string>>;
const selectionListeners = new Set<() => void>();
let selectionVersion = 0;

/** The rows of the Git mode's change list that the user folded away — a section's heading
 * (`sectionKey`), a commit (`commitKey`) or, in the tree, a directory (`directoryKey`) — per project and for the window's
 * lifetime, like the selections. Everything starts unfolded. */
const NONE_COLLAPSED: ReadonlySet<string> = new Set();
const collapsedRows = new Map<string, ReadonlySet<string>>();

/** What a session set in a project's Git mode branch selector, kept for the window's lifetime so a
 * pane mounted again under the same selection does not set it again over the user's choice. */
export interface SessionFollow {
  session: string;
  /** The selection (`noteSessionSwitch`) this record is of: a later one makes it a new switch. */
  switchId: number;
  /** When that switch was made (`gitMoment`): only what was read from the daemon after it is
   * weighed. */
  since: number;
  /** The same for the worktree list, moved forward when it is asked again for the worktree the
   * session's branch should be checked out in. */
  sourceSince: number;
  /** Whether the pane has asked again for what was read before the switch. */
  asked: boolean;
  /** Whether the worktree list has been asked again for a worktree no listed one is. */
  retried: boolean;
  /** Set once nothing is left to set, or the user has chosen a branch. */
  settled: boolean;
}
const follows = new Map<string, SessionFollow>();
let switchCount = 0;
let lastSwitchAt = 0;

/** Records that the selected session changed, so each project's selector follows it afresh. */
export function noteSessionSwitch(): void {
  switchCount += 1;
  lastSwitchAt = gitMoment();
}

/** The record of `session` being followed in `project`, new for each selection of it. */
export function sessionFollow(project: string, session: string): SessionFollow {
  const held = follows.get(project);
  if (held && held.session === session && held.switchId === switchCount) return held;
  const made: SessionFollow = { session, switchId: switchCount, since: lastSwitchAt, sourceSince: lastSwitchAt, asked: false, retried: false, settled: false };
  follows.set(project, made);
  return made;
}

function subscribeSelections(listener: () => void): () => void {
  selectionListeners.add(listener);
  return () => selectionListeners.delete(listener);
}

function setSelection(kind: Selection, project: string, value: string | undefined): void {
  const map = selections[kind];
  if (map.get(project) === value) return;
  if (value === undefined) map.delete(project);
  else map.set(project, value);
  selectionVersion += 1;
  for (const listener of selectionListeners) listener();
}

function setCollapsed(project: string, collapsed: ReadonlySet<string>): void {
  if (collapsed.size === 0) collapsedRows.delete(project);
  else collapsedRows.set(project, collapsed);
  selectionVersion += 1;
  for (const listener of selectionListeners) listener();
}

function update(project: string, change: (entry: StoredBrowser) => StoredBrowser): void {
  const current = stored.get();
  const next = { ...current, [project]: change(current[project] ?? { mode: "files", expanded: [], used: 0 }) };
  const projects = Object.keys(next);
  if (projects.length > MAX_PROJECTS) {
    projects.sort((a, b) => next[a].used - next[b].used);
    for (const old of projects.slice(0, projects.length - MAX_PROJECTS)) delete next[old];
  }
  stored.set(next);
}

const NO_EXPANDED: ReadonlySet<string> = new Set();
/** One `Set` per stored array, so a render that did not change it gets the same object back. */
const expandedSets = new WeakMap<string[], ReadonlySet<string>>();

export interface ProjectBrowserState {
  /** How the Git mode's changes are laid out. */
  layout: ChangeLayout;
  setLayout: (layout: ChangeLayout) => void;
  /** Whether the Git mode's committed changes are grouped by commit within the Committed section. */
  byCommit: boolean;
  setByCommit: (byCommit: boolean) => void;
  mode: BrowserMode;
  expanded: ReadonlySet<string>;
  selected?: string;
  /** The branch the Git mode shows the changes of, as a wire path; none for the one checked out in
   * the worktree holding the project's directory. */
  branch?: string;
  /** The key of the change the Git mode's viewer showed last. */
  selectedChange?: string;
  /** The rows of the Git mode's change list that are folded away, as row keys: a section's
   * heading, a commit or a directory of the tree. */
  collapsedChangeRows: ReadonlySet<string>;
  setMode: (mode: BrowserMode) => void;
  setExpanded: (expanded: ReadonlySet<string>) => void;
  setSelected: (path: string | undefined) => void;
  setBranch: (branch: string | undefined) => void;
  setSelectedChange: (key: string | undefined) => void;
  setCollapsedChangeRows: (collapsed: ReadonlySet<string>) => void;
  /** Records the project's browser as just shown, which keeps it from being the first dropped. */
  touch: () => void;
}

export function setProjectBrowserMode(project: string, mode: BrowserMode): void {
  update(project, (entry) => ({ ...entry, mode }));
}

/**
 * A project's browser state, owned by the project rather than by any session: the mode, the
 * expanded directories and what is selected. The first two are kept in `localStorage` per client
 * (`PREFERENCE_KEYS.projectBrowsers`), the selections for the window's lifetime.
 */
export function useProjectBrowserState(project: string): ProjectBrowserState {
  // Kept out of the React Compiler: the selections are read from module state that changes in
  // place, which its memoization would take for constant and keep showing as it first was.
  "use no memo";
  const entry = stored.useValue()[project];
  useSyncExternalStore(subscribeSelections, () => selectionVersion);
  let expanded = NO_EXPANDED;
  if (entry) {
    expanded = expandedSets.get(entry.expanded) ?? new Set(entry.expanded);
    expandedSets.set(entry.expanded, expanded);
  }
  return {
    layout: entry?.layouts?.changes ?? changeLayout.get(),
    setLayout: (layout) => update(project, (e) => ({ ...e, layouts: { changes: layout } })),
    byCommit: entry?.byCommit === true,
    setByCommit: (byCommit) => update(project, (e) => ({ ...e, byCommit })),
    mode: entry?.mode ?? "files",
    expanded,
    selected: selections.file.get(project),
    branch: selections.branch.get(project),
    selectedChange: selections.change.get(project),
    collapsedChangeRows: collapsedRows.get(project) ?? NONE_COLLAPSED,
    setMode: (mode) => setProjectBrowserMode(project, mode),
    setExpanded: (next) => update(project, (e) => ({ ...e, expanded: [...next].slice(-MAX_EXPANDED) })),
    setSelected: (path) => setSelection("file", project, path),
    setBranch: (branch) => setSelection("branch", project, branch),
    setSelectedChange: (key) => setSelection("change", project, key),
    setCollapsedChangeRows: (collapsed) => setCollapsed(project, collapsed),
    touch: () => update(project, (e) => ({ ...e, used: Date.now() })),
  };
}

/** Drops the browser state of every project not among `projects`, once they are known: a removed
 * project's state has nothing left to apply to. */
export function forgetProjectsOtherThan(projects: ReadonlySet<string>): void {
  const current = stored.get();
  const gone = Object.keys(current).filter((project) => !projects.has(project));
  if (gone.length > 0) {
    const next = { ...current };
    for (const project of gone) delete next[project];
    stored.set(next);
  }
  for (const kind of SELECTIONS) {
    for (const project of [...selections[kind].keys()]) if (!projects.has(project)) setSelection(kind, project, undefined);
  }
  for (const project of [...collapsedRows.keys()]) if (!projects.has(project)) setCollapsed(project, NONE_COLLAPSED);
  for (const project of [...follows.keys()]) if (!projects.has(project)) follows.delete(project);
}
