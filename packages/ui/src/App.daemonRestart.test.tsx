// @vitest-environment jsdom
// The restart a browser tab asks of the daemon it is served by, from the update notice's Restart now: with the control
// connection not open nothing is sent and no reload is waited for.
import { act } from "react";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "./components/alerts/errorAlerts";
import { byLabel, deliver, fakeDaemon, mountApp, pressElement, stubWindow } from "./appTestSupport";
import { consoleOf, projectOf, snapshotState } from "./fixtures/builders";
import type { DaemonUpdateStatus } from "./protocol";
import { UpdatesProvider } from "./updates/UpdatesProvider";

vi.mock("./updates/reloadPage", () => ({ reloadPage: vi.fn() }));
const { reloadPage } = await import("./updates/reloadPage");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: () => null }));
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));

afterEach(() => vi.mocked(reloadPage).mockClear());

const console_ = consoleOf("c-main", "Main");
const project = projectOf("p-site", console_.id, "Website");
const INSTALLED: DaemonUpdateStatus = { phase: "installed", available: null, progress: null, installed: { version: "0.2.0", holder_floor: null }, last_check: null, install: null, manual: null, updated_from: null };

it("raises the failure to restart, sends no restart and waits for no reload, while the connection is not open", async () => {
  stubWindow();
  const daemon = fakeDaemon({
    ...snapshotState({ consoles: [console_], projects: [project], sessions: [] }),
    connectionState: "reconnecting",
    daemon: { version: "0.1.0", generation: 1 },
    daemonUpdate: INSTALLED,
  }, (body) => (body.type === "handover_check" ? { type: "handover_check", id: null, known: true, sessions: [], shell_terminals: [], unreached_shell_terminals: [] } : { type: "ack", id: null }));
  onTestFinished(() => act(() => errorAlerts.acknowledge()));
  mountApp(daemon, { wrap: (app) => <UpdatesProvider firstCheckDelayMs={null}>{app}</UpdatesProvider> });
  const restart = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === "Restart now");
  await pressElement(restart);
  await act(async () => new Promise((resolve) => setTimeout(resolve, 100)));
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "restart_daemon")).toBe(false);
  expect(errorAlerts.getSnapshot()[0]?.title).toBe("Could not restart");
  // The daemon coming back on the version it was meant to go to is no business of a page that sent nothing.
  await deliver(daemon, { type: "hello", daemon_version: "0.2.0", generation: 1 } as never);
  await deliver(daemon, { type: "daemon_update_status_updated", status: { ...INSTALLED, phase: "idle", installed: null } });
  expect(reloadPage).not.toHaveBeenCalled();
  expect(byLabel("Application update")).not.toBeNull();
});
