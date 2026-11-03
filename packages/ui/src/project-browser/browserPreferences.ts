import { useSyncExternalStore } from "react";

import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import type { ChangeLayout } from "./git/changeLayout";

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
      const layout = (entry.layouts as { changes?: unknown } | undefined)?.changes;
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

export function setProjectBrowserMode(project: string, mode: BrowserMode): void {
  update(project, (entry) => ({ ...entry, mode }));
}

/** What a project's browser keeps across windows, as its browser reads it. */
export interface BrowserPreferences {
  mode: BrowserMode;
  /** How the Git mode's changes are laid out. */
  layout: ChangeLayout;
  /** Whether the Git mode's committed changes are grouped by commit within the Committed section. */
  byCommit: boolean;
  expanded: ReadonlySet<string>;
}

/** The writers of one project's stored browser state. */
export interface BrowserPreferenceSetters {
  setMode: (mode: BrowserMode) => void;
  setLayout: (layout: ChangeLayout) => void;
  setByCommit: (byCommit: boolean) => void;
  setExpanded: (expanded: ReadonlySet<string>) => void;
  /** Records the project's browser as just shown, which keeps it from being the first dropped. */
  touch: () => void;
}

const NO_EXPANDED: ReadonlySet<string> = new Set();
/** One `Set` per stored array, so every reader of an unchanged array gets the same object back. */
const expandedSets = new WeakMap<string[], ReadonlySet<string>>();

function expandedSet(expanded: string[]): ReadonlySet<string> {
  let set = expandedSets.get(expanded);
  if (set === undefined) {
    set = new Set(expanded);
    expandedSets.set(expanded, set);
  }
  return set;
}

/** `project`'s stored browser state, re-rendering the caller whenever the stored record changes. */
export function useBrowserPreferences(project: string): BrowserPreferences {
  const entry = useSyncExternalStore(stored.subscribe, stored.get)[project];
  return {
    mode: entry?.mode ?? "files",
    layout: entry?.layouts?.changes ?? "flat",
    byCommit: entry?.byCommit === true,
    expanded: entry ? expandedSet(entry.expanded) : NO_EXPANDED,
  };
}

/** The writers of `project`'s stored browser state; the caller keeps them for as long as the project
 * stays the same, so they keep their identity across renders. */
export function browserPreferenceSetters(project: string): BrowserPreferenceSetters {
  return {
    setMode: (mode) => setProjectBrowserMode(project, mode),
    setLayout: (layout) => update(project, (e) => ({ ...e, layouts: { changes: layout } })),
    setByCommit: (byCommit) => update(project, (e) => ({ ...e, byCommit })),
    setExpanded: (next) => update(project, (e) => ({ ...e, expanded: [...next].slice(-MAX_EXPANDED) })),
    touch: () => update(project, (e) => ({ ...e, used: Date.now() })),
  };
}

/** Drops the stored browser state of every project not among `projects`. */
export function forgetStoredBrowsersOtherThan(projects: ReadonlySet<string>): void {
  const current = stored.get();
  const gone = Object.keys(current).filter((project) => !projects.has(project));
  if (gone.length === 0) return;
  const next = { ...current };
  for (const project of gone) delete next[project];
  stored.set(next);
}
