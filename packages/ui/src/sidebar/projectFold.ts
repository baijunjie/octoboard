/** Which action the Projects heading's one fold button offers. */
export type ProjectFoldControl = "collapse" | "expand";

/**
 * The button is not a readout of how many projects are open. It offers one action at a time, and
 * keeps offering it while the list is mixed.
 *
 * `expandPinned` is set when the user asks the button to collapse every listed project, and cleared
 * when they ask it to expand them. A console whose projects the user has brought entirely one way
 * does the same on its own: every project expanded clears the pin, every project collapsed sets it.
 * Opening or closing a single project, while any project is still the other way, leaves the pin
 * as it was. With the pin clear, any project that is still open makes the button offer collapse,
 * which is how it starts — projects begin expanded. It offers expand with the pin clear only when
 * every project is collapsed.
 *
 * `projectIds` is every project of one console. A filter decides which of them a press changes; it
 * does not decide this. An empty list has nothing to judge, so the pin is what shows.
 */
export function projectFoldControl(
  expandPinned: boolean,
  projectIds: readonly string[],
  collapsed: ReadonlySet<string>,
): ProjectFoldControl {
  if (projectIds.length === 0) return expandPinned ? "expand" : "collapse";
  if (projectIds.every((id) => !collapsed.has(id))) return "collapse";
  if (expandPinned || projectIds.every((id) => collapsed.has(id))) return "expand";
  return "collapse";
}

/** Collapsing pins the button to expand; expanding clears that pin. */
export function pinAfterFoldAction(control: ProjectFoldControl): boolean {
  return control === "collapse";
}

/**
 * The pin that follows from one console's projects, leaving a mixed list untouched. An empty list
 * has nothing to judge, so the pin stays.
 */
export function reconcileExpandPin(
  expandPinned: boolean,
  projectIds: readonly string[],
  collapsed: ReadonlySet<string>,
): boolean {
  if (projectIds.length === 0) return expandPinned;
  if (projectIds.every((id) => !collapsed.has(id))) return false;
  if (projectIds.every((id) => collapsed.has(id))) return true;
  return expandPinned;
}

/** Applies `reconcileExpandPin` to each console. Returns a new map when a pin changes, or
 * `undefined` when nothing does, so a caller can keep its previous state. */
export function reconcileExpandPins(
  prev: ReadonlyMap<string, boolean>,
  projects: readonly { id: string; console_id: string }[],
  collapsed: ReadonlySet<string>,
): Map<string, boolean> | undefined {
  const byConsole = new Map<string, string[]>();
  for (const project of projects) {
    const ids = byConsole.get(project.console_id);
    if (ids) ids.push(project.id);
    else byConsole.set(project.console_id, [project.id]);
  }
  let next: Map<string, boolean> | undefined;
  for (const [consoleId, ids] of byConsole) {
    const pin = prev.get(consoleId) ?? false;
    const reconciled = reconcileExpandPin(pin, ids, collapsed);
    if (reconciled !== pin) {
      next ??= new Map(prev);
      next.set(consoleId, reconciled);
    }
  }
  return next;
}
