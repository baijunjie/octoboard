// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { agentCliStatusesOf, agentCliStatusOf, agentCliUpdateOf } from "../fixtures/builders";
import { FIRST_WRITE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { accessibleName } from "../components/dialog/dialogTestSupport";
import { LanguageProvider } from "../i18n/react";
import type { UpdaterCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { DaemonUpdateStatus } from "../protocol";
import { DaemonProvider, type State } from "../store";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
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

it("says in the row that the restart waits for an agent's update, and leaves Restart now unavailable with the reason, and a press does nothing", async () => {
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
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
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

/** Makes this window the application's window of a daemon elsewhere, as the desktop shell opens it. */
function inRemoteWindow() {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  window.history.replaceState({}, "", "/?remote=server&daemon=me%40server");
  onTestFinished(() => window.history.replaceState({}, "", "/"));
}

/** The section in the application's window, with an updater of its own (which a remote window must not use), over a
 * daemon that updates itself or not. */
function mountDesktop(status: DaemonUpdateStatus | null, settings: Partial<State["settings"]> = {}) {
  const request = vi.fn(answerDaemonUpdateRequest);
  const check = vi.fn(async () => undefined);
  const updater: UpdaterCapability = { currentVersion: async () => "7.7.7", check, install: async () => {} };
  const daemon = testDaemon({
    state: { daemon: { version: __OCTOBOARD_VERSION__, generation: 1 }, daemonUpdate: status, settings: { ...DEFAULT_SETTINGS, versions: { daemon_update_check: 1, daemon_update_channel: 2 }, ...settings } },
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
      <PlatformProvider value={{ kind: "tauri", host: MAC_HOST, updater }}>
        <LanguageProvider>
          <DaemonProvider value={daemon}>
            <UpdatesProvider firstCheckDelayMs={null}>
              <UpdatesSetting />
            </UpdatesProvider>
          </DaemonProvider>
        </LanguageProvider>
      </PlatformProvider>,
    ),
  );
  return { container, request, check };
}

const rowLabels = (container: HTMLElement) => ["Version", "Check for updates automatically", "Update channel"].filter((label) => textOf(container).includes(label));
const channelButton = (container: HTMLElement) => container.querySelector<HTMLElement>("button[aria-label='Update channel'], [aria-label='Update channel'] button")!;

it("holds only the automatic check and the update channel in the window of a daemon elsewhere that updates itself, with no version, check, download or restart", () => {
  inRemoteWindow();
  const { container } = mountDesktop(IDLE);
  expect(rowLabels(container)).toEqual(["Check for updates automatically", "Update channel"]);
  expect(container.querySelectorAll("input[type=checkbox]")).toHaveLength(1);
  expect(textOf(container)).not.toContain("version");
  for (const name of ["Check for updates", "Download and install", "Restart now", "Reload", "What is new"]) expect(button(container, name)).toBeUndefined();
  expect(container.querySelector("[role=status]")).toBeNull();
});

it("shows the daemon's own values in the window of a daemon elsewhere", () => {
  inRemoteWindow();
  const { container } = mountDesktop(IDLE, { daemon_update_check: false, daemon_update_channel: "beta" });
  expect(container.querySelector<HTMLInputElement>("input[type=checkbox]")!.checked).toBe(false);
  expect(channelButton(container).textContent).toContain("Beta");
});

it("shows an update channel this build does not know as an empty choice, and replaces it by choosing a known one", async () => {
  inRemoteWindow();
  const { container, request } = mountDesktop(IDLE, { daemon_update_channel: "nightly" });
  expect(channelButton(container).textContent).not.toMatch(/nightly|Stable|Beta/);
  await act(async () => channelButton(container).click());
  await act(async () => Array.from(document.body.querySelectorAll<HTMLElement>("[role=option]")).find((option) => option.textContent?.includes("Stable"))!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", daemon_update_channel: "stable", base_versions: { daemon_update_channel: 2 } });
});

it("offers no download, restart or manual command for a status of a phase this build does not know", () => {
  const { container } = mount({ ...IDLE, phase: "from_a_newer_daemon", available: AVAILABLE, installed: { version: "0.2.0", holder_floor: null }, manual: { reason: "not_writable", path: "/x", command: MANUAL_COMMAND } });
  for (const name of ["Download and install", "Restart now", "Copy install command"]) expect(button(container, name)).toBeUndefined();
  expect(button(container, "Check for updates")).toBeDefined();
});

it("words the automatic check as the daemon's own in the window of a daemon elsewhere, and as the application's in the local window", async () => {
  inRemoteWindow();
  const remote = mountDesktop(IDLE);
  expect(textOf(remote.container)).toContain("The daemon checks for updates in the background.");
  expect(textOf(remote.container)).not.toContain("The app checks");
  window.history.replaceState({}, "", "/");
  vi.unstubAllGlobals();
  const local = mountDesktop(IDLE);
  await act(async () => {});
  expect(textOf(local.container)).toContain("The app checks for updates in the background.");
  expect(textOf(local.container)).not.toContain("The daemon checks");
});

it("writes the automatic check through the normal settings write, based on the version held, and never asks the daemon or the application to check", async () => {
  inRemoteWindow();
  const { container, request, check } = mountDesktop(IDLE);
  await act(async () => void container.querySelector<HTMLInputElement>("input[type=checkbox]")!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", daemon_update_check: false, base_versions: { daemon_update_check: 1 } });
  expect(check).not.toHaveBeenCalled();
});

it("writes the update channel through the normal settings write, based on the version held, and does not look again", async () => {
  inRemoteWindow();
  const { container, request, check } = mountDesktop(IDLE);
  await act(async () => channelButton(container).click());
  await act(async () => Array.from(document.body.querySelectorAll<HTMLElement>("[role=option]")).find((option) => option.textContent?.includes("Beta"))!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_settings", daemon_update_channel: "beta", base_versions: { daemon_update_channel: 2 } });
  expect(check).not.toHaveBeenCalled();
});

it("keeps the full section, with its version and buttons, in the application's local window", async () => {
  const { container } = mountDesktop(IDLE);
  // The application's own version is asked of the shell.
  await act(async () => {});
  expect(textOf(container)).toContain("You are using version 7.7.7.");
  expect(button(container, "Check for updates")).toBeDefined();
  expect(rowLabels(container)).toEqual(expect.arrayContaining(["Check for updates automatically", "Update channel"]));
});

const NOW = 1_700_000_000_000;
const CHECKED = (at: number, outcome: "up_to_date" | "failed" = "up_to_date") => ({ outcome, at, error: outcome === "failed" ? "daemon_update_check_failed" : null, detail: outcome === "failed" ? "offline" : null });
const versionDescription = (container: HTMLElement) => container.querySelector<HTMLElement>("[data-settings-search-id='updates.version'] [data-setting-description]")!;
const progressBar = (container: HTMLElement) => container.querySelector<HTMLElement>("[role=progressbar]");

it("keeps a second line under the Version row's sentence, empty before any check has ended, so the row keeps its height", () => {
  fakeTimers({ now: NOW });
  const { container } = mount(IDLE, undefined, { notice: false });
  expect(versionDescription(container).children).toHaveLength(2);
  expect(textOf(versionDescription(container).children[1]!)).toBe("");
});

it.each([
  ["idle", { phase: "idle" }],
  ["checking", { phase: "checking" }],
  ["a check that did not finish", { phase: "idle", last_check: CHECKED(NOW - 5 * 60_000, "failed") }],
] as const)("words when the last check ended on the second line, kept through %s", (_name, change) => {
  fakeTimers({ now: NOW });
  const { container } = mount({ ...IDLE, last_check: CHECKED(NOW - 5 * 60_000), ...change }, undefined, { notice: false });
  expect(versionDescription(container).children).toHaveLength(2);
  expect(textOf(versionDescription(container).children[1]!)).toBe("Last checked 5 minutes ago");
});

it("says the last check was just now while it is under a minute back, as a check starts and ends", () => {
  fakeTimers({ now: NOW });
  const { container, daemon } = mount({ ...IDLE, last_check: CHECKED(NOW - 10_000) }, undefined, { notice: false });
  expect(textOf(versionDescription(container).children[1]!)).toBe("Last checked just now");
  act(() => daemon.store.setState({ daemonUpdate: { ...IDLE, phase: "checking", last_check: CHECKED(NOW - 10_000) } }));
  expect(textOf(versionDescription(container).children[1]!)).toBe("Last checked just now");
  act(() => daemon.store.setState({ daemonUpdate: { ...IDLE, last_check: CHECKED(NOW) } }));
  expect(textOf(versionDescription(container).children[1]!)).toBe("Last checked just now");
});

it("shows the daemon's download as a progress bar in place of the last check's line, named by the row's sentence, with its percentage", () => {
  fakeTimers({ now: NOW });
  const { container } = mount({ ...IDLE, phase: "downloading", available: AVAILABLE, progress: { downloaded: 25, total: 100 }, last_check: CHECKED(NOW - 60_000) }, undefined, { notice: false });
  const bar = progressBar(container)!;
  expect(bar).not.toBeNull();
  expect(accessibleName(bar)).toBe("Downloading version \u{2068}0.2.0\u{2069}…");
  expect(bar.getAttribute("aria-valuenow")).toBe("25");
  expect(bar.getAttribute("aria-valuetext")).toBe("25%");
  expect(textOf(versionDescription(container))).toContain("25%");
  expect(textOf(versionDescription(container))).not.toContain("Last checked");
});

it("shows an indeterminate progress bar without a percentage while the daemon's download has no known size", () => {
  fakeTimers({ now: NOW });
  const { container, daemon } = mount({ ...IDLE, phase: "downloading", available: AVAILABLE, progress: { downloaded: 25, total: null }, last_check: CHECKED(NOW - 60_000) }, undefined, { notice: false });
  const bar = progressBar(container)!;
  expect(bar).not.toBeNull();
  expect(bar.hasAttribute("aria-valuenow")).toBe(false);
  expect(textOf(versionDescription(container))).not.toContain("%");
  expect(textOf(versionDescription(container))).not.toContain("Last checked");
  // Once the daemon says how big the archive is, the bar turns determinate.
  act(() => daemon.store.setState({ daemonUpdate: { ...IDLE, phase: "downloading", available: AVAILABLE, progress: { downloaded: 25, total: 50 }, last_check: CHECKED(NOW - 60_000) } }));
  expect(progressBar(container)!.getAttribute("aria-valuetext")).toBe("50%");
});

it("puts the last check's line back, and no progress bar, once the daemon's download is done", () => {
  fakeTimers({ now: NOW });
  const { container, daemon } = mount({ ...IDLE, phase: "downloading", available: AVAILABLE, progress: { downloaded: 25, total: 100 }, last_check: CHECKED(NOW - 60_000) }, undefined, { notice: false });
  expect(progressBar(container)).not.toBeNull();
  act(() => daemon.store.setState({ daemonUpdate: { ...INSTALLED, last_check: CHECKED(NOW - 60_000) } }));
  expect(progressBar(container)).toBeNull();
  expect(textOf(versionDescription(container).children[1]!)).toBe("Last checked 1 minute ago");
});
