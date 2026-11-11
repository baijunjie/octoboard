// Shared by the viewer's tests: the providers the header needs around a `FileViewer`, and the steps and reads of
// its right-click menu.
import React, { act } from "react";
import { onTestFinished, vi } from "vitest";

import type { RequestHandler } from "../requests";
import { forwardAlerts } from "../components/alerts/alertTestSupport";
import type { PlatformAdapter } from "../platform";
import type { Daemon } from "../store";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { BROWSER_HOST } from "../test/hosts";

const NO_NOTIFICATIONS = { notice: () => {}, error: () => {} };

/** The platform and daemon a `FileViewer` renders inside. `notifications` is where the daemon's notifications go, and
 * the error alerts too. */
export function ViewerHost({
  clipboard,
  links,
  notifications = NO_NOTIFICATIONS,
  request,
  children,
}: {
  clipboard?: PlatformAdapter["clipboard"];
  links?: PlatformAdapter["links"];
  notifications?: { notice: (message: string) => void; error: (message: string) => void };
  request?: RequestHandler;
  children: React.ReactNode;
}): React.ReactElement {
  const daemon = React.useMemo<Daemon>(
    () => testDaemon({ request, showNotice: notifications.notice }),
    [notifications.notice, request],
  );
  // A layout effect, so it is installed before the children's passive effects (where an alert raised as they mount
  // would be missed otherwise) and is replaced when `notifications.error` changes.
  React.useLayoutEffect(() => forwardAlerts(notifications.error), [notifications.error]);
  return <TestHost daemon={daemon} platform={{ kind: "browser", host: BROWSER_HOST, clipboard, links }}>{children}</TestHost>;
}

/** The accessible names of the status marks under `root`, in document order: a popover mark is a
 * button, a note is the named trigger of a tooltip, and both carry `data-status-mark`. */
export function markNames(root: ParentNode = document): string[] {
  return [...root.querySelectorAll("[data-status-mark]")].map((mark) => mark.getAttribute("aria-label") ?? "");
}

/** The entries of the open menu in order: an item's name, and `"-"` for a separator. */
export const menuEntries = (): (string | null)[] => [...document.querySelectorAll<HTMLElement>("[role=menuitem], [role=separator]")]
  .map((entry) => (entry.getAttribute("role") === "separator" ? "-" : entry.textContent));

/** The open menu's item named `name`. */
export const menuItem = (name: string): HTMLElement => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === name)!;

/** Closes the open menu with Escape, from where the menu holds focus. */
export const closeMenu = (): Promise<void> => act(async () => { document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); });

/** Raises a context menu event on `target`; the event comes back to tell whether the page took it over. */
export async function rightClick(target: Element): Promise<MouseEvent> {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 5, clientY: 5 });
  await act(async () => { target.dispatchEvent(event); });
  return event;
}

/** Chooses the open menu's item named `name` with the keyboard. */
export async function chooseItem(name: string): Promise<void> {
  await act(async () => {
    const entry = menuItem(name);
    entry.focus();
    entry.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    entry.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
  });
}

/** What every selection that is set (`selectAllChildren`, `setBaseAndExtent`) selects, as text (or as `read` reads it
 * as it is set), in order, until the test ends. jsdom drops the selection on any `focus()`, and a menu hands focus back
 * as it closes, so what a Select All selected is read as it was set. */
export function recordSelections(): string[];
export function recordSelections<T>(read: (selection: Selection) => T): T[];
export function recordSelections<T>(read: (selection: Selection) => T | string = (selection) => selection.toString()): (T | string)[] {
  const seen: (T | string)[] = [];
  for (const method of ["selectAllChildren", "setBaseAndExtent"] as const) {
    const original = Selection.prototype[method] as (...args: unknown[]) => void;
    const spy = vi.spyOn(Selection.prototype, method as "selectAllChildren").mockImplementation(function (this: Selection, ...args: unknown[]) {
      original.apply(this, args);
      seen.push(read(this));
    } as never);
    onTestFinished(() => spy.mockRestore());
  }
  return seen;
}
