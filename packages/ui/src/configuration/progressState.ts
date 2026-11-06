import type { ImportFailure, ImportItem } from "../protocol";
import type { ImportProgressEvent } from "../store";

/** Where one item of a running import stands, as the last progress event for it said (the event's `item` and `import_id`
 * are what the maps below are keyed by). */
export type ItemProgress = { state: "waiting" | "creating" | "cloning" | "done" | "not_attempted" } | { state: "failed"; error: ImportFailure };

/** The items of a running import, by `itemKey`. Only the items the daemon announced are in it: one it leaves out (a
 * project with no resolution) never is. */
export type ImportProgress = ReadonlyMap<string, ItemProgress>;

export const NO_PROGRESS: ImportProgress = new Map();

/** The key an item goes by: its place in the document, as the daemon names it. */
export function itemKey(item: ImportItem): string {
  switch (item.kind) {
    case "console":
      return `console:${item.console}`;
    case "project":
      return `project:${item.console}:${item.project}`;
    case "account":
      return `account:${item.agent}:${item.account}`;
    case "settings":
      return "settings";
  }
}

/** `progress` with `event` applied. */
export function withProgress(progress: ImportProgress, event: ImportProgressEvent): ImportProgress {
  const { type: _type, import_id: _import, item, ...state } = event;
  return new Map(progress).set(itemKey(item), state);
}

/** Whether an item has reached its end (done, failed or never attempted), so nothing more will be reported of it. */
function isSettled(item: ItemProgress): boolean {
  return item.state === "done" || item.state === "failed" || item.state === "not_attempted";
}

/** How many items have settled, out of how many were announced. */
export function progressCount(progress: ImportProgress): { settled: number; total: number } {
  let settled = 0;
  for (const item of progress.values()) if (isSettled(item)) settled += 1;
  return { settled, total: progress.size };
}

/** A token for one import: this window's request names its progress events by it, and events of another window's import
 * (broadcast to every window) never carry it. */
export function newImportId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
