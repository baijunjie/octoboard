import { useSyncExternalStore } from "react";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";

/** What a project's browser shows. Files is the first mode; the Git modes join it here. */
export type BrowserMode = "files";

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
      result[project] = { mode: "files", expanded: expanded.slice(-MAX_EXPANDED), used: typeof entry.used === "number" ? entry.used : 0 };
    }
    return result;
  } catch {
    return {};
  }
}

const stored = createPersistedPreference<Record<string, StoredBrowser>>(PREFERENCE_KEYS.projectBrowsers, parseStored, (value) =>
  Object.keys(value).length === 0 ? null : JSON.stringify(value),
);

/** The file each project's tree has selected — the one last opened in the viewer. Kept for the
 * window's lifetime only: a selection is about what the user is looking at now, and a file named
 * from an earlier run may well be gone. */
const selections = new Map<string, string>();
const selectionListeners = new Set<() => void>();
let selectionVersion = 0;

function subscribeSelections(listener: () => void): () => void {
  selectionListeners.add(listener);
  return () => selectionListeners.delete(listener);
}

function setSelection(project: string, path: string | undefined): void {
  if (selections.get(project) === path) return;
  if (path === undefined) selections.delete(project);
  else selections.set(project, path);
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
  setExpanded: (expanded: ReadonlySet<string>) => void;
  setSelected: (path: string | undefined) => void;
  /** Records the project's browser as just shown, which keeps it from being the first dropped. */
  touch: () => void;
}

/**
 * A project's browser state, owned by the project rather than by any session: the mode, the
 * expanded directories and the selected file. The first two are kept in `localStorage` per client
 * (`PREFERENCE_KEYS.projectBrowsers`), the selection for the window's lifetime.
 */
export function useProjectBrowserState(project: string): ProjectBrowserState {
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
    selected: selections.get(project),
    setExpanded: (next) => update(project, (e) => ({ ...e, expanded: [...next].slice(-MAX_EXPANDED) })),
    setSelected: (path) => setSelection(project, path),
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
  for (const project of [...selections.keys()]) if (!projects.has(project)) setSelection(project, undefined);
}
