// Shared by the change list's tests: the fixtures, and mounting a row tree in the providers it
// renders inside.
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished } from "vitest";

import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { ChangeEntry } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";

/** jsdom has no ResizeObserver, which the rows' names (`FadeOverflow`) need. */
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

/** A modified file, in the staged or the unstaged section. */
export const change = (group: "staged" | "unstaged", path: string): ChangeEntry => ({
  group,
  old: { state: "present", path, kind: "file", source: { kind: "index", worktree: "w", blob: "b" } },
  new: { state: "present", path, kind: "file", source: { kind: "live", root_id: "r", version: "v" } },
});

interface ToastSpies {
  notice: (message: string) => void;
  error: (message: string) => void;
}

interface MountRowsOptions {
  /** The platform the rows render on; without a `clipboard` they offer no Copy path. */
  clipboard?: PlatformAdapter["clipboard"];
  /** Where the daemon's toasts go. */
  toasts?: ToastSpies;
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
export function mountRows(children: React.ReactNode, { clipboard, toasts = { notice: () => {}, error: () => {} } }: MountRowsOptions = {}): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const daemon: Daemon = {
    store: createStateStore({}),
    request: () => Promise.reject(new Error("no daemon")),
    toastError: toasts.error,
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
