// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import type { DaemonUpdateStatus } from "../protocol";
import { applyDaemonEvent, DaemonProvider } from "../store";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { testDaemon } from "../test/testDaemon";
import { useUpdatedNotice } from "./useUpdatedNotice";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => localStorage.clear());

const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };

function mount(status: DaemonUpdateStatus, version: string) {
  const showNotice = vi.fn();
  const daemon = testDaemon({ state: { daemon: { version, generation: 1 }, daemonUpdate: status }, showNotice });
  const Probe = () => {
    useUpdatedNotice();
    return null;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <LanguageProvider>
        <DaemonProvider value={daemon}>
          <Probe />
        </DaemonProvider>
      </LanguageProvider>,
    ),
  );
  return { daemon, showNotice };
}

const hello = (version: string) => ({ type: "hello", daemon_version: version, generation: 1 }) as never;

it("says the daemon was updated once per version, with a link to what is new, and not again for the same version", () => {
  const first = mount({ ...IDLE, updated_from: "0.1.0" }, "0.2.0");
  expect(first.showNotice).toHaveBeenCalledOnce();
  expect(first.showNotice.mock.calls[0]![0]).toContain("Updated to version");
  const again = mount({ ...IDLE, updated_from: "0.1.0" }, "0.2.0");
  expect(again.showNotice).not.toHaveBeenCalled();
  const later = mount({ ...IDLE, updated_from: "0.2.0" }, "0.3.0");
  expect(later.showNotice).toHaveBeenCalledOnce();
});

it("says nothing for a daemon that replaced no version of itself", () => {
  expect(mount(IDLE, "0.2.0").showNotice).not.toHaveBeenCalled();
});

it("reads what the browser was told from storage when it asks, so another tab's telling counts", () => {
  const { daemon, showNotice } = mount(IDLE, "0.2.0");
  localStorage.setItem(PREFERENCE_KEYS.daemonUpdatedNoticeVersion, "0.3.0");
  act(() => applyDaemonEvent(daemon.store, hello("0.3.0")));
  act(() => applyDaemonEvent(daemon.store, { type: "daemon_update_status_updated", status: { ...IDLE, updated_from: "0.2.0" } }));
  expect(showNotice).not.toHaveBeenCalled();
});

it("says nothing of a status held from the daemon before the one that greeted, until the new one sends its own", () => {
  const { daemon, showNotice } = mount({ ...IDLE, updated_from: "0.1.0" }, "0.2.0");
  expect(showNotice).toHaveBeenCalledOnce();
  // A second update: the new daemon greets, and the status still held is the previous one's.
  act(() => applyDaemonEvent(daemon.store, hello("0.3.0")));
  expect(showNotice).toHaveBeenCalledOnce();
  act(() => applyDaemonEvent(daemon.store, { type: "daemon_update_status_updated", status: { ...IDLE, updated_from: "0.2.0" } }));
  expect(showNotice).toHaveBeenCalledTimes(2);
  // A rollback: the daemon that comes back replaced none, whatever the previous held.
  act(() => applyDaemonEvent(daemon.store, hello("0.1.0")));
  act(() => applyDaemonEvent(daemon.store, { type: "daemon_update_status_updated", status: IDLE }));
  expect(showNotice).toHaveBeenCalledTimes(2);
});

it("says it once per page load where the browser has no storage", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("blocked"); });
  onTestFinished(() => void vi.restoreAllMocks());
  const { daemon, showNotice } = mount({ ...IDLE, updated_from: "0.1.0" }, "0.2.0");
  expect(showNotice).toHaveBeenCalledOnce();
  // The status goes stale and comes back for the same daemon: the effect runs again, and the page has said it.
  act(() => applyDaemonEvent(daemon.store, hello("0.2.0")));
  act(() => applyDaemonEvent(daemon.store, { type: "daemon_update_status_updated", status: { ...IDLE, updated_from: "0.1.0" } }));
  expect(showNotice).toHaveBeenCalledOnce();
});
