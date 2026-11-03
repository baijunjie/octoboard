import { useMemo } from "react";

import {
  browserPreferenceSetters,
  forgetStoredBrowsersOtherThan,
  useBrowserPreferences,
  type BrowserPreferences,
  type BrowserPreferenceSetters,
} from "./browserPreferences";
import { browserSelectionSetters, forgetSelectionsOtherThan, useBrowserSelections, type BrowserSelections, type BrowserSelectionSetters } from "./browserSelections";
import { forgetGitModeWorktreesOtherThan } from "./gitModeWorktree";
import { forgetSessionFollowsOtherThan } from "./git/sessionFollow";

export type ProjectBrowserState = BrowserPreferences & BrowserPreferenceSetters & BrowserSelections & BrowserSelectionSetters;

/**
 * A project's browser state, owned by the project rather than by any session: the mode, the
 * expanded directories and the Git mode's layout and grouping, kept in `localStorage` per client
 * (`browserPreferences.ts`), and what is selected and folded, kept for the window's lifetime
 * (`browserSelections.ts`). The setters keep their identity for as long as `project` does.
 */
export function useProjectBrowserState(project: string): ProjectBrowserState {
  const preferences = useBrowserPreferences(project);
  const selections = useBrowserSelections(project);
  const setters = useMemo(() => ({ ...browserPreferenceSetters(project), ...browserSelectionSetters(project) }), [project]);
  return { ...preferences, ...selections, ...setters };
}

/** Drops the browser state of every project not among `projects`, once they are known: a removed
 * project's state has nothing left to apply to. */
export function forgetProjectsOtherThan(projects: ReadonlySet<string>): void {
  forgetStoredBrowsersOtherThan(projects);
  forgetSelectionsOtherThan(projects);
  forgetSessionFollowsOtherThan(projects);
  forgetGitModeWorktreesOtherThan(projects);
}
