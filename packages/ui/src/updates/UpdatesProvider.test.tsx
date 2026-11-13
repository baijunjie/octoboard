// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter, UpdaterCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { DaemonUpdateStatus } from "../protocol";
import { DaemonProvider } from "../store";
import { BROWSER_HOST } from "../test/hosts";
import { fakeTimers } from "../test/clock";
import { testDaemon } from "../test/testDaemon";
import { CHECK_INTERVAL_MS, FIRST_CHECK_DELAY_MS, type UpdateStore } from "./updateStore";
import { UpdatesProvider, useUpdateStore } from "./UpdatesProvider";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => localStorage.clear());

const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };

/** The version the desktop application's shell reports, distinct from the daemon's so a state says whose store it is. */
const SHELL_VERSION = "7.7.7";
const DAEMON_VERSION = "0.1.0";

function updaterOf(check: UpdaterCapability["check"] = async () => undefined): UpdaterCapability {
  return { currentVersion: async () => SHELL_VERSION, check, install: async () => {} };
}

/** Mounts the provider the way `AppRoot` nests it: inside the daemon's provider (when `daemonUpdate` is given, the daemon is
 * there with that status; `undefined` mounts none), under a platform with or without an updater. */
function mount({ updater, daemonUpdate, firstCheckDelayMs }: { updater?: UpdaterCapability; daemonUpdate?: DaemonUpdateStatus | null; firstCheckDelayMs?: number | null }) {
  const request = vi.fn(async () => ({ type: "ack" as const, id: null }));
  const daemon = daemonUpdate === undefined ? undefined : testDaemon({ state: { daemon: { version: DAEMON_VERSION, generation: 1 }, daemonUpdate }, request });
  let store: UpdateStore | undefined;
  const Capture = () => {
    store = useUpdateStore();
    return null;
  };
  const platform: PlatformAdapter = updater ? { kind: "tauri", host: BROWSER_HOST, updater } : { kind: "browser", host: BROWSER_HOST };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const providers = (
    <UpdatesProvider firstCheckDelayMs={firstCheckDelayMs}>
      <Capture />
    </UpdatesProvider>
  );
  act(() =>
    root.render(
      <PlatformProvider value={platform}>
        <LanguageProvider>{daemon ? <DaemonProvider value={daemon}>{providers}</DaemonProvider> : providers}</LanguageProvider>
      </PlatformProvider>,
    ),
  );
  return { store: () => store, daemon, request };
}

it("provides the desktop store where the platform has an updater, whatever the daemon says of its own update", async () => {
  const check = vi.fn(async () => undefined);
  const { store, request } = mount({ updater: updaterOf(check), daemonUpdate: IDLE, firstCheckDelayMs: null });
  await act(async () => void (await store()!.check("manual")));
  expect(check).toHaveBeenCalledOnce();
  expect(request).not.toHaveBeenCalled();
  // The version is the shell's, not the connected daemon's.
  expect(store()!.getState().currentVersion).toBe(SHELL_VERSION);
});

it("provides the desktop store for the desktop application connected to a daemon that updates nothing", () => {
  const { store } = mount({ updater: updaterOf(), daemonUpdate: null, firstCheckDelayMs: null });
  expect(store()).toBeDefined();
});

it("provides the daemon's store where there is no updater and the daemon updates itself, and its checks are requests", async () => {
  const { store, request } = mount({ daemonUpdate: IDLE, firstCheckDelayMs: null });
  expect(store()).toBeDefined();
  expect(store()!.getState().currentVersion).toBe(DAEMON_VERSION);
  await act(async () => void (await store()!.check("manual")));
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "check_daemon_update" });
});

it("provides nothing without an updater where the daemon does not update itself, or where there is no daemon", () => {
  expect(mount({ daemonUpdate: null }).store()).toBeUndefined();
  expect(mount({}).store()).toBeUndefined();
});

it("starts offering the daemon's update when the first status of an update arrives, and stops when it goes", () => {
  const { store, daemon } = mount({ daemonUpdate: null });
  expect(store()).toBeUndefined();
  act(() => daemon!.store.setState({ daemonUpdate: IDLE }));
  expect(store()).toBeDefined();
  act(() => daemon!.store.setState({ daemonUpdate: null }));
  expect(store()).toBeUndefined();
});

it("makes no automatic request of its own for the daemon's update: the daemon looks by itself", async () => {
  fakeTimers();
  const { request } = mount({ daemonUpdate: IDLE });
  await act(async () => vi.advanceTimersByTimeAsync(FIRST_CHECK_DELAY_MS + 2 * CHECK_INTERVAL_MS));
  expect(request).not.toHaveBeenCalled();
});
