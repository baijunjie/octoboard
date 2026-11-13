// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import type { DaemonUpdateStatus } from "../protocol";
import { applyDaemonEvent, DaemonProvider } from "../store";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { testDaemon } from "../test/testDaemon";
import { reloadPage } from "../platform/reloadPage";
import { useReloadAfterRestart } from "./useReloadAfterRestart";
import { useUpdatedNotice } from "./useUpdatedNotice";

// The reload flag as the real module holds it, set by the call that starts the reload.
const page = vi.hoisted(() => ({ reloading: false }));
vi.mock("../platform/reloadPage", () => ({
  reloadPage: vi.fn(() => {
    page.reloading = true;
  }),
  isReloading: () => page.reloading,
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  localStorage.clear();
  page.reloading = false;
  vi.mocked(reloadPage).mockClear();
});

const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };

function mount(status: DaemonUpdateStatus, version: string) {
  const showNotice = vi.fn();
  const daemon = testDaemon({ state: { daemon: { version, generation: 1 }, daemonUpdate: status }, showNotice });
  let restart!: ReturnType<typeof useReloadAfterRestart>["restart"];
  // As `App` calls them: the reload first, so that on the status that sets it off the notice already sees it.
  const Probe = () => {
    ({ restart } = useReloadAfterRestart());
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
  return { daemon, showNotice, restart: (target: string | null) => act(async () => void (await restart(target, async () => {}))) };
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

it("says nothing while the page is reloading, and does not remember the version, so the reloaded page says it", () => {
  page.reloading = true;
  const reloading = mount({ ...IDLE, updated_from: "0.1.0" }, "0.2.0");
  expect(reloading.showNotice).not.toHaveBeenCalled();
  expect(localStorage.getItem(PREFERENCE_KEYS.daemonUpdatedNoticeVersion)).toBeNull();
  // The reloaded page starts over, with nothing reloading.
  page.reloading = false;
  const reloaded = mount({ ...IDLE, updated_from: "0.1.0" }, "0.2.0");
  expect(reloaded.showNotice).toHaveBeenCalledOnce();
  expect(localStorage.getItem(PREFERENCE_KEYS.daemonUpdatedNoticeVersion)).toBe("0.2.0");
});

it("leaves the tab that asked for the restart without the notice on the status that reloads it, and remembers nothing", async () => {
  const { daemon, showNotice, restart } = mount(IDLE, "0.1.0");
  await restart("0.2.0");
  act(() => applyDaemonEvent(daemon.store, hello("0.2.0")));
  act(() => applyDaemonEvent(daemon.store, { type: "daemon_update_status_updated", status: { ...IDLE, updated_from: "0.1.0" } }));
  expect(reloadPage).toHaveBeenCalledOnce();
  expect(showNotice).not.toHaveBeenCalled();
  expect(localStorage.getItem(PREFERENCE_KEYS.daemonUpdatedNoticeVersion)).toBeNull();
});

it("tells another tab of the browser, which asked for no restart, and remembers the version", () => {
  const { daemon, showNotice } = mount(IDLE, "0.1.0");
  act(() => applyDaemonEvent(daemon.store, hello("0.2.0")));
  act(() => applyDaemonEvent(daemon.store, { type: "daemon_update_status_updated", status: { ...IDLE, updated_from: "0.1.0" } }));
  expect(reloadPage).not.toHaveBeenCalled();
  expect(showNotice).toHaveBeenCalledOnce();
  expect(localStorage.getItem(PREFERENCE_KEYS.daemonUpdatedNoticeVersion)).toBe("0.2.0");
});
