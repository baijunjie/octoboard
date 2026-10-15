import { useSyncExternalStore } from "react";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";

/** What a project's browser shows: its files on disk, or Git review. */
export type BrowserMode = "files" | "git";

/** What the Git mode shows: a worktree's uncommitted changes, or the changes between two
 * branches. */
export type GitView = "worktree" | "compare";

/** The part of a project's browser state that outlives the window: its mode and the directories
 * expanded in its tree (wire paths). File bodies and listings are never stored. */
interface StoredBrowser {
  mode: BrowserMode;
  expanded: string[];
  /** When the project's browser was last shown, for dropping the least recently used ones. */
  used: number;
}

/** How many projects' browser state is kept, and how many expanded directories per project; past
 * either, the least recently used project and the earliest expanded directory go first. */
const MAX_PROJECTS = 50;
const MAX_EXPANDED = 200;

function parseStored(raw: string | null): Record<string, StoredBrowser> {
  try {
    const value = raw ? (JSON.parse(raw) as unknown) : undefined;
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const result: Record<string, StoredBrowser> = {};
    for (const [project, entry] of Object.entries(value as Record<string, Partial<StoredBrowser>>)) {
      if (!entry || typeof entry !== "object") continue;
      const expanded = Array.isArray(entry.expanded) ? entry.expanded.filter((p): p is string => typeof p === "string" && p !== "") : [];
      const mode = entry.mode === "git" ? "git" : "files";
      result[project] = { mode, expanded: expanded.slice(-MAX_EXPANDED), used: typeof entry.used === "number" ? entry.used : 0 };
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
 * a worktree, a branch, a change. `file` is the tree's selected file (the one last opened in the
 * viewer), `worktree` the worktree whose changes the Git mode shows (none for the one holding the
 * project's directory), `change` the key of the change last opened from them (`changeKey`);
 * `gitView` which of its views the Git mode shows, `left` and `right` the branches compared (wire
 * paths) and `comparedChange` the key of the change last opened from their comparison. */
type Selection = "file" | "worktree" | "change" | "gitView" | "left" | "right" | "comparedChange";
const SELECTIONS: readonly Selection[] = ["file", "worktree", "change", "gitView", "left", "right", "comparedChange"];
const selections = Object.fromEntries(SELECTIONS.map((kind) => [kind, new Map<string, string>()])) as Record<Selection, Map<string, string>>;
const selectionListeners = new Set<() => void>();
let selectionVersion = 0;

/** The rows of the Git mode's change list that the user folded away — a section's heading
 * (`sectionKey`) or, in the tree, a directory (`directoryKey`) — per project and for the window's
 * lifetime, like the selections: the rows of the two views never share a key, so one set serves
 * both. Everything starts unfolded. */
const NONE_COLLAPSED: ReadonlySet<string> = new Set();
const collapsedRows = new Map<string, ReadonlySet<string>>();

/** What a session set in a project's Git mode selectors, kept for the window's lifetime so a pane
 * mounted again under the same selection does not set them again over the user's choices. */
export interface SessionFollow {
  session: string;
  /** The selection (`noteSessionSwitch`) this record is of: a later one makes it a new switch. */
  switchId: number;
  /** When that switch was made, on the clock `performance.now()` reads: only what was read from the
   * daemon after it is weighed. */
  since: number;
  /** The same for the worktree list, moved forward when it is asked again for the worktree the
   * session's branch should be in. */
  sourceSince: number;
  /** Whether the pane has asked again for what was read before the switch. */
  asked: boolean;
  /** Whether the worktree list has been asked again for a worktree no listed one is. */
  retried: boolean;
  /** Set once nothing is left to set, or the user has chosen a selector. */
  settled: boolean;
}
const follows = new Map<string, SessionFollow>();
let switchCount = 0;
let lastSwitchAt = 0;

/** Records that the selected session changed, so each project's selectors follow it afresh. */
export function noteSessionSwitch(): void {
  switchCount += 1;
  lastSwitchAt = performance.now();
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
  mode: BrowserMode;
  expanded: ReadonlySet<string>;
  selected?: string;
  /** The worktree the Git mode shows the changes of, by its id; none for the one holding the
   * project's directory. */
  worktree?: string;
  /** The key of the change the Git mode's viewer showed last from the worktree's changes. */
  selectedChange?: string;
  gitView: GitView;
  /** The branches compared, old side and new side, as wire paths; none until chosen. */
  left?: string;
  right?: string;
  /** The key of the change the Git mode's viewer showed last from the comparison. */
  selectedComparedChange?: string;
  /** The rows of the Git mode's change list that are folded away, as row keys: a section's
   * heading, or a directory of the tree. */
  collapsedChangeRows: ReadonlySet<string>;
  setMode: (mode: BrowserMode) => void;
  setExpanded: (expanded: ReadonlySet<string>) => void;
  setSelected: (path: string | undefined) => void;
  setWorktree: (worktree: string | undefined) => void;
  setSelectedChange: (key: string | undefined) => void;
  setGitView: (view: GitView) => void;
  setBranches: (left: string | undefined, right: string | undefined) => void;
  setSelectedComparedChange: (key: string | undefined) => void;
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
    mode: entry?.mode ?? "files",
    expanded,
    selected: selections.file.get(project),
    worktree: selections.worktree.get(project),
    selectedChange: selections.change.get(project),
    gitView: selections.gitView.get(project) === "compare" ? "compare" : "worktree",
    left: selections.left.get(project),
    right: selections.right.get(project),
    selectedComparedChange: selections.comparedChange.get(project),
    collapsedChangeRows: collapsedRows.get(project) ?? NONE_COLLAPSED,
    setMode: (mode) => setProjectBrowserMode(project, mode),
    setExpanded: (next) => update(project, (e) => ({ ...e, expanded: [...next].slice(-MAX_EXPANDED) })),
    setSelected: (path) => setSelection("file", project, path),
    setWorktree: (worktree) => setSelection("worktree", project, worktree),
    setSelectedChange: (key) => setSelection("change", project, key),
    setGitView: (view) => setSelection("gitView", project, view),
    setBranches: (left, right) => {
      setSelection("left", project, left);
      setSelection("right", project, right);
    },
    setSelectedComparedChange: (key) => setSelection("comparedChange", project, key),
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
