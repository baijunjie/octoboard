// @vitest-environment jsdom
// The restart a browser tab asks of the daemon it is served by, from the update notice's Restart now: with the control
// connection not open nothing is sent and no reload is waited for; with it open the restart is sent, the tab reconnects
// by itself for a while, and reloads once the daemon is back on the installed version (or speaks another generation).
import { act } from "react";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "./components/alerts/errorAlerts";
import type { ConnectionState } from "./daemon-client";
import { byLabel, deliver, fakeDaemon, mountApp, pressElement, stubWindow } from "./appTestSupport";
import { consoleOf, projectOf, snapshotState } from "./fixtures/builders";
import { PREFERENCE_KEYS } from "./preferences/preferenceKeys";
import type { ClientRequestBody, DaemonUpdateStatus } from "./protocol";
import { fakeTimers, flushPromises } from "./test/clock";
import { UpdatesProvider } from "./updates/UpdatesProvider";
import { RESTART_RECONNECT_BUDGET_MS } from "./updates/useReloadAfterRestart";

// The reload flag as the real module holds it, set by the call that starts the reload.
const page = vi.hoisted(() => ({ reloading: false }));
vi.mock("./platform/reloadPage", () => ({
  reloadPage: vi.fn(() => {
    page.reloading = true;
  }),
  isReloading: () => page.reloading,
}));
const { reloadPage } = await import("./platform/reloadPage");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: () => null }));
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));

afterEach(() => {
  vi.mocked(reloadPage).mockClear();
  page.reloading = false;
});

const console_ = consoleOf("c-main", "Main");
const project = projectOf("p-site", console_.id, "Website");
/** The version of the page's own build, which the daemon runs until it restarts into the installed one. */
const OLD_VERSION = __OCTOBOARD_VERSION__;
const INSTALLED: DaemonUpdateStatus = { phase: "installed", available: null, progress: null, installed: { version: "0.2.0", holder_floor: null }, last_check: null, install: null, manual: null, updated_from: null };

it("raises the failure to restart, sends no restart and waits for no reload, while the connection is not open", async () => {
  stubWindow();
  const daemon = fakeDaemon({
    ...snapshotState({ consoles: [console_], projects: [project], sessions: [] }),
    connectionState: "reconnecting",
    daemon: { version: OLD_VERSION, generation: 1 },
    daemonUpdate: INSTALLED,
  }, (body) => (body.type === "handover_check" ? { type: "handover_check", id: null, known: true, sessions: [], shell_terminals: [], unreached_shell_terminals: [] } : { type: "ack", id: null }));
  onTestFinished(() => act(() => errorAlerts.acknowledge()));
  mountApp(daemon, { wrap: (app) => <UpdatesProvider firstCheckDelayMs={null}>{app}</UpdatesProvider> });
  const restart = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === "Restart now");
  await pressElement(restart);
  await act(async () => new Promise((resolve) => setTimeout(resolve, 100)));
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "restart_daemon")).toBe(false);
  expect(errorAlerts.getSnapshot()[0]?.title).toBe("Cannot restart");
  // The daemon coming back on the version it was meant to go to is no business of a page that sent nothing.
  await deliver(daemon, { type: "hello", daemon_version: "0.2.0", generation: 1 } as never);
  await deliver(daemon, { type: "daemon_update_status_updated", status: { ...INSTALLED, phase: "idle", installed: null } });
  expect(reloadPage).not.toHaveBeenCalled();
  expect(byLabel("Application update")).not.toBeNull();
});

const IDLE: DaemonUpdateStatus = { ...INSTALLED, phase: "idle", installed: null };
const HANDOVER_NOTHING = { type: "handover_check", id: null, known: true, sessions: [], shell_terminals: [], unreached_shell_terminals: [] };

/** The window over a daemon whose connection is open and that has an update installed, waiting for the restart. */
function mountRestartable() {
  stubWindow();
  const daemon = fakeDaemon(
    { ...snapshotState({ consoles: [console_], projects: [project], sessions: [] }), connectionState: "open", daemon: { version: OLD_VERSION, generation: 1 }, daemonUpdate: INSTALLED },
    (body) => (body.type === "handover_check" ? HANDOVER_NOTHING : { type: "ack", id: null }),
  );
  mountApp(daemon, { wrap: (app) => <UpdatesProvider firstCheckDelayMs={null}>{app}</UpdatesProvider> });
  return daemon;
}

const sent = (daemon: ReturnType<typeof fakeDaemon>, type: ClientRequestBody["type"]) => vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === type).length;
const connection = (daemon: ReturnType<typeof fakeDaemon>, state: ConnectionState) => act(() => daemon.store.setState({ connectionState: state }));

/** Presses Restart now, and waits until the restart has been sent. */
async function restartNow(daemon: ReturnType<typeof fakeDaemon>) {
  await pressElement(Array.from(document.querySelectorAll("button")).find((button) => button.textContent === "Restart now"));
  await act(async () => void (await vi.waitFor(() => expect(sent(daemon, "restart_daemon")).toBe(1))));
}

it("sends the restart, and once the daemon is back on the installed version reloads this tab, which says nothing of the update and remembers nothing", async () => {
  const daemon = mountRestartable();
  await restartNow(daemon);
  // Awaited through the gap that follows: the connection drops, and the tab reconnects by itself.
  expect(daemon.store.getState().restartAwaited).toBe(true);
  connection(daemon, "closed");
  expect(daemon.reconnect).toHaveBeenCalledOnce();
  connection(daemon, "connecting");
  connection(daemon, "open");
  expect(reloadPage).not.toHaveBeenCalled();
  // The next daemon: its greeting, then its status, which says it replaced the old one.
  await deliver(daemon, { type: "hello", daemon_version: "0.2.0", generation: 1 } as never);
  expect(daemon.store.getState().restartAwaited).toBe(false);
  expect(reloadPage).not.toHaveBeenCalled();
  await deliver(daemon, { type: "daemon_update_status_updated", status: { ...IDLE, updated_from: OLD_VERSION } });
  expect(reloadPage).toHaveBeenCalledOnce();
  expect(daemon.showNotice).not.toHaveBeenCalled();
  expect(localStorage.getItem(PREFERENCE_KEYS.daemonUpdatedNoticeVersion)).toBeNull();
});

it("reconnects a closed connection while the restart's budget lasts and then leaves the retrying to the connection's Try again", async () => {
  fakeTimers({ toFake: ["Date"] });
  const daemon = mountRestartable();
  await restartNow(daemon);
  connection(daemon, "closed");
  expect(daemon.reconnect).toHaveBeenCalledTimes(1);
  connection(daemon, "reconnecting");
  connection(daemon, "closed");
  expect(daemon.reconnect).toHaveBeenCalledTimes(2);
  connection(daemon, "reconnecting");
  vi.setSystemTime(Date.now() + RESTART_RECONNECT_BUDGET_MS + 1);
  connection(daemon, "closed");
  await flushPromises();
  expect(daemon.reconnect).toHaveBeenCalledTimes(2);
  expect(Array.from(document.querySelectorAll("button")).some((button) => button.textContent === "Try again")).toBe(true);
  expect(reloadPage).not.toHaveBeenCalled();
});

it("reloads this tab when the daemon comes back speaking another protocol generation", async () => {
  const daemon = mountRestartable();
  await restartNow(daemon);
  connection(daemon, "closed");
  connection(daemon, "incompatible");
  expect(reloadPage).toHaveBeenCalledOnce();
});

it("does not reload for an incompatible daemon when this tab asked for no restart", async () => {
  const daemon = mountRestartable();
  connection(daemon, "closed");
  connection(daemon, "incompatible");
  await flushPromises();
  expect(daemon.reconnect).not.toHaveBeenCalled();
  expect(reloadPage).not.toHaveBeenCalled();
});
