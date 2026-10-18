// Shared by the project browser's tests: the fixtures, and mounting `children` in the providers
// they render inside.
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished } from "vitest";

import { errorText } from "../errorText";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { isolate } from "../i18n/isolate";
import type { ChangeEntry, LineStats } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import type { ChangeViewState } from "./changeView";

/** A modified file, in the staged or the unstaged section, with the lines the daemon counted for it. */
export const change = (group: "staged" | "unstaged", path: string, stats?: LineStats): ChangeEntry => ({
  group,
  old: { state: "present", path, kind: "file", source: { kind: "index", worktree: "w", blob: "b" } },
  new: { state: "present", path, kind: "file", source: { kind: "live", root_id: "r", version: "v" } },
  ...(stats ? { stats } : {}),
});

/** The file row named `name`, found by its accessible name. */
export const rowOf = (container: HTMLElement, name: string): HTMLElement =>
  [...container.querySelectorAll<HTMLElement>("[role=row]")].find((row) => row.getAttribute("aria-label")?.startsWith(isolate(name)))!;

/** A file a commit changed, in the committed section. */
export const committedChange = (path: string): ChangeEntry => ({
  group: "committed",
  old: { state: "present", path, kind: "file", source: { kind: "commit", commit: "p", branch: null, blob: "b" } },
  new: { state: "present", path, kind: "file", source: { kind: "commit", commit: "c", branch: null, blob: "b2" } },
});

/** A `ChangeViewState` for a list that does nothing of its own, with `view` over it. The toggle the
 * filter field hands keyboard focus to as it goes lives outside the list, so a stand-in for it is
 * put in the document and taken out again when the test ends. */
export function changeViewFixture(view: Partial<ChangeViewState> = {}): ChangeViewState {
  const toggle = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => toggle.remove());
  return {
    layout: "flat",
    collapsed: new Set(),
    onCollapsedChange: () => {},
    filterOpen: false,
    onFilterOpenChange: () => {},
    filter: "",
    onFilterChange: () => {},
    filterToggleRef: { current: toggle },
    ...view,
  };
}

interface ToastSpies {
  notice: (message: string) => void;
  error: (message: string) => void;
}

interface MountRowsOptions {
  request?: Daemon["request"];
  /** The platform the rows render on; without a `clipboard` they offer no Copy path. */
  clipboard?: PlatformAdapter["clipboard"];
  /** Where the daemon's toasts go. */
  toasts?: ToastSpies;
  /** What the daemon's store holds; without it, the state before the first snapshot. */
  state?: Partial<State>;
}

/**
 * Renders `children` into a container of its own and returns that container, inside the platform
 * and daemon a change row needs.
 *
 * The root is unmounted when the test ends, not merely wiped from the document: the unmount takes a
 * row's menu popover (portalled to `<body>`) with it, and it runs the effect cleanups, which is
 * what clears the timers the tree still has running — the list's status region
 * (`StatusAnnouncer`) delays its first write by one. Left mounted, such a timer fires after the
 * test file is done, updates state outside `act`, and React schedules that work into a jsdom that
 * is being torn down: the scheduler then dies on the missing `window`, as an unhandled error the
 * whole run is blamed for.
 */
export function mountRows(children: React.ReactNode, { request = () => Promise.reject(new Error("no daemon")), clipboard, toasts = { notice: () => {}, error: () => {} }, state }: MountRowsOptions = {}): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const daemon: Daemon = {
    store: createStateStore(state),
    request,
    toastError: (message) => toasts.error(errorText(message)),
    toastNotice: toasts.notice,
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  act(() =>
    root.render(
      <PlatformProvider value={{ kind: "browser", clipboard }}>
        <DaemonProvider value={daemon}>{children}</DaemonProvider>
      </PlatformProvider>,
    ),
  );
  return container;
}
