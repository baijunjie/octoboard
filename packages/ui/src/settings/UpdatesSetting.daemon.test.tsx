// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { agentCliStatusesOf, agentCliStatusOf, agentCliUpdateOf } from "../fixtures/builders";
import { FIRST_WRITE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { LanguageProvider } from "../i18n/react";
import { PlatformProvider } from "../platform/react";
import type { DaemonUpdateStatus } from "../protocol";
import { DaemonProvider, type State } from "../store";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { BROWSER_HOST } from "../test/hosts";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { fakeTimers } from "../test/clock";
import { testDaemon } from "../test/testDaemon";
import { UpdateNotice } from "../updates/UpdateNotice";
import { answerDaemonUpdateRequest } from "../updates/daemonUpdateTestSupport";
import { UpdatesProvider } from "../updates/UpdatesProvider";
import { reloadPage } from "../platform/reloadPage";
import { UpdatesSetting } from "./UpdatesSetting";

vi.mock("../platform/reloadPage", () => ({ reloadPage: vi.fn() }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The notice measures itself with one while it is shown, and jsdom has none of its own.
class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

beforeEach(() => {
  onTestFinished(() => void vi.unstubAllGlobals());
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
});
afterEach(() => {
  acknowledgeAllAlerts();
  localStorage.clear();
});

/** The command the daemon hands over for an upgrade by hand: its own, not one the application makes up. */
const MANUAL_COMMAND = "curl -fsSL https://example.test/install.sh | sh -s -- --bin-dir /opt/octoboard --beta";
const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };
const AVAILABLE = { version: "0.2.0", holder_floor: null, channel: "stable" } as const;

function mount(status: DaemonUpdateStatus | null, request = vi.fn(answerDaemonUpdateRequest), { agentCliStatuses, notice = true }: { agentCliStatuses?: State["agentCliStatuses"]; notice?: boolean } = {}) {
  const daemon = testDaemon({
    state: { daemon: { version: __OCTOBOARD_VERSION__, generation: 1 }, daemonUpdate: status, settings: { ...DEFAULT_SETTINGS, versions: { daemon_update_check: 1, daemon_update_channel: 1 } }, ...(agentCliStatuses && { agentCliStatuses }) },
    request,
  });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
        <LanguageProvider>
          <DaemonProvider value={daemon}>
            <UpdatesProvider firstCheckDelayMs={null}>
              {notice && <UpdateNotice focusTerminal={() => {}} />}
              <UpdatesSetting />
            </UpdatesProvider>
          </DaemonProvider>
        </LanguageProvider>
      </PlatformProvider>,
    ),
  );
  return { container, daemon, request };
}

const textOf = (element: Element) => (element.textContent ?? "").replace(/[⁦-⁩]/g, "");
const button = (container: HTMLElement, name: string) => Array.from(container.querySelectorAll("button")).find((candidate) => candidate.textContent === name);

it("shows nothing where the daemon does not update itself", () => {
  const { container } = mount(null);
  expect(container.textContent).toBe("");
});

it("shows the section's rows from the daemon's status, and checks by asking the daemon", async () => {
  const { container, request } = mount(IDLE);
  expect(textOf(container)).toContain(`You are using version ${__OCTOBOARD_VERSION__}.`);
  expect(textOf(container)).toContain("Check for updates automatically");
  await act(async () => void button(container, "Check for updates")!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "check_daemon_update" });
});

it("offers the download of an update found, and asks the daemon for it", async () => {
  const { container, request } = mount({ ...IDLE, available: AVAILABLE });
  expect(textOf(container)).toContain("Version 0.2.0 is available.");
  await act(async () => void button(container, "Download and install")!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_daemon" });
});

it("offers the restart of an installed update through the exit flow", async () => {
  const { container } = mount({ ...IDLE, phase: "installed", installed: { version: "0.2.0", holder_floor: "0.1.0" } });
  expect(textOf(container)).toContain("Version 0.2.0 is installed.");
  expect(button(container, "Restart now")).toBeDefined();
});

it("says the executable cannot be replaced, gives the install command to copy, and offers no download", () => {
  const { container } = mount({ ...IDLE, available: AVAILABLE, manual: { reason: "not_writable", path: "/opt/octoboard/octoboardd", command: MANUAL_COMMAND } });
  const text = textOf(container);
  expect(text).toContain("cannot replace /opt/octoboard/octoboardd");
  expect(text).toContain(MANUAL_COMMAND);
  expect(button(container, "Download and install")).toBeUndefined();
  expect(button(container, "Copy install command")).toBeDefined();
});

it("writes the automatic check and the channel as the daemon's settings", async () => {
  const { container, request } = mount(IDLE);
  await act(async () => void container.querySelector<HTMLInputElement>("input[type=checkbox]")!.click());
  expect(request).toHaveBeenCalledWith({ type: "update_settings", daemon_update_check: false, base_versions: { daemon_update_check: 1 } });
});

it("asks for a reload once the daemon runs another version than the page was loaded for", async () => {
  const { container, daemon } = mount(IDLE);
  expect(button(container, "Reload")).toBeUndefined();
  act(() => daemon.store.setState({ daemon: { version: "0.2.0", generation: 1 } }));
  expect(textOf(container)).toContain("now runs version 0.2.0");
  await act(async () => void button(container, "Reload")!.click());
  expect(reloadPage).toHaveBeenCalled();
});

const INSTALLED: DaemonUpdateStatus = { ...IDLE, phase: "installed", installed: { version: "0.2.0", holder_floor: "0.1.0" } };
const UPDATING_AGENT = agentCliStatusesOf([agentCliStatusOf("claude", { update: agentCliUpdateOf() })]);

/** Found by their accessible name, which stays "Restart now" while the reason sits inside the button for a screen reader. */
const restartButtons = (container: HTMLElement) => Array.from(container.querySelectorAll<HTMLButtonElement>("button[aria-label='Restart now']"));
const rowRestart = (container: HTMLElement) => restartButtons(container)[0]!;

it("says in the row that the restart waits for an agent's update, and leaves Restart now unavailable with the reason, which a press explains instead of asking for the restart", async () => {
  const { container, request } = mount(INSTALLED, undefined, { agentCliStatuses: UPDATING_AGENT, notice: false });
  expect(textOf(container)).toContain("Version 0.2.0 is installed. Restart when the agent update finishes.");
  expect(textOf(container)).not.toContain("takes effect after");
  const restart = rowRestart(container);
  expect(restart.getAttribute("aria-label")).toBe("Restart now");
  expect(restart.getAttribute("aria-disabled")).toBe("true");
  expect(document.getElementById(restart.getAttribute("aria-describedby")!)!.textContent).toBe("An agent is being updated. Restart when the update finishes.");
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  await act(async () => void restart.click());
  expect(request.mock.calls.some(([body]) => body.type === "restart_daemon")).toBe(false);
  const alerts = errorAlerts.getSnapshot();
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.title).toBe("Cannot restart");
  expect((alerts[0]?.error as { code?: string }).code).toBe("agent_update_running");
});

it("keeps the row's Restart now as it was while no agent's update runs", async () => {
  const { container } = mount(INSTALLED, undefined, { agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("claude")]), notice: false });
  expect(textOf(container)).toContain("Version 0.2.0 is installed. It takes effect after");
  expect(textOf(container)).not.toContain("agent update");
  expect(rowRestart(container).hasAttribute("aria-disabled")).toBe(false);
  expect(rowRestart(container).hasAttribute("aria-describedby")).toBe(false);
  await act(async () => void rowRestart(container).click());
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
});

it("follows the agent's update through its start and its end in the row", () => {
  const { container, daemon } = mount(INSTALLED, undefined, { notice: false });
  expect(rowRestart(container).hasAttribute("aria-disabled")).toBe(false);
  act(() => daemon.store.setState({ agentCliStatuses: UPDATING_AGENT }));
  expect(textOf(container)).toContain("Restart when the agent update finishes.");
  expect(rowRestart(container).getAttribute("aria-disabled")).toBe("true");
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("claude", { update: agentCliUpdateOf({ state: "failed", finished_at: 2, error: "agent_update_failed", detail: "bad" }) })]) }));
  expect(textOf(container)).toContain("It takes effect after");
  expect(rowRestart(container).hasAttribute("aria-disabled")).toBe(false);
});

it("holds the strip's and the row's Restart now back together while an agent's update runs", () => {
  const { container } = mount(INSTALLED, undefined, { agentCliStatuses: UPDATING_AGENT });
  const restarts = restartButtons(container);
  expect(restarts).toHaveLength(2);
  for (const restart of restarts) expect(restart.getAttribute("aria-disabled")).toBe("true");
});

it("says in the row that an installed update of no known version waits for the agent's update too", () => {
  const { container, daemon } = mount({ ...IDLE, phase: "installed", installed: { version: null, holder_floor: null } }, undefined, { agentCliStatuses: UPDATING_AGENT, notice: false });
  expect(textOf(container)).toContain("An update is installed. Restart when the agent update finishes.");
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("claude")]) }));
  expect(textOf(container)).toContain("An update is installed. It takes effect after");
});

it("keeps the focus on the row's Restart now, the same button, as the agent's update starts and ends", () => {
  const { container, daemon } = mount(INSTALLED, undefined, { notice: false });
  const restart = rowRestart(container);
  restart.focus();
  expect(document.activeElement).toBe(restart);
  act(() => daemon.store.setState({ agentCliStatuses: UPDATING_AGENT }));
  expect(rowRestart(container)).toBe(restart);
  expect(document.activeElement).toBe(restart);
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("claude")]) }));
  expect(rowRestart(container)).toBe(restart);
  expect(document.activeElement).toBe(restart);
});

it("speaks the row's new sentence when an agent's update holds the restart back and when it lets go, and says nothing of the state it opened with", async () => {
  fakeTimers();
  const { container, daemon } = mount(INSTALLED, undefined, { notice: false });
  const spoken = () => textOf(container.querySelector("[role=status]")!);
  await act(async () => vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(spoken()).toBe("");
  act(() => daemon.store.setState({ agentCliStatuses: UPDATING_AGENT }));
  expect(spoken()).toContain("Version 0.2.0 is installed. Restart when the agent update finishes.");
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("claude")]) }));
  expect(spoken()).toContain("Version 0.2.0 is installed. It takes effect after");
});

it.each([
  ["idle", IDLE],
  ["available", { ...IDLE, available: AVAILABLE }],
] as const)("announces nothing when an agent's update starts and ends while the section shows an %s update", async (_name, status) => {
  fakeTimers();
  const { container, daemon } = mount(status, undefined, { notice: false });
  const spoken = () => textOf(container.querySelector("[role=status]")!);
  await act(async () => vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS));
  expect(spoken()).toBe("");
  act(() => daemon.store.setState({ agentCliStatuses: UPDATING_AGENT }));
  expect(spoken()).toBe("");
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("claude", { update: agentCliUpdateOf({ state: "succeeded", finished_at: 2 }) })]) }));
  expect(spoken()).toBe("");
});
