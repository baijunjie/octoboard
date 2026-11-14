// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { agentCliStatusesOf, agentCliStatusOf, agentCliUpdateOf } from "../fixtures/builders";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { DaemonUpdateStatus } from "../protocol";
import { DaemonProvider, type State } from "../store";
import { BROWSER_HOST } from "../test/hosts";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { testDaemon } from "../test/testDaemon";
import { reloadPage } from "../platform/reloadPage";
import { answerDaemonUpdateRequest } from "./daemonUpdateTestSupport";
import { UpdateNotice } from "./UpdateNotice";
import { UpdatesProvider } from "./UpdatesProvider";

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
  vi.mocked(reloadPage).mockClear();
});

/** The command the daemon hands over for an upgrade by hand: its own, not one the application makes up. */
const MANUAL_COMMAND = "curl -fsSL https://example.test/install.sh | sh -s -- --bin-dir /opt/octoboard --beta";
const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };
const AVAILABLE = { version: "0.2.0", holder_floor: null, channel: "stable" } as const;

function mount(status: DaemonUpdateStatus, clipboard?: PlatformAdapter["clipboard"], agentCliStatuses?: State["agentCliStatuses"]) {
  const request = vi.fn(answerDaemonUpdateRequest);
  const daemon = testDaemon({ state: { daemon: { version: __OCTOBOARD_VERSION__, generation: 1 }, daemonUpdate: status, ...(agentCliStatuses && { agentCliStatuses }) }, request });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST, clipboard }}>
        <LanguageProvider>
          <DaemonProvider value={daemon}>
            <UpdatesProvider firstCheckDelayMs={null}>
              <UpdateNotice focusTerminal={() => {}} />
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

it("offers an update the daemon found, and asks the daemon to install it on the user's press only", async () => {
  const { container, request } = mount({ ...IDLE, available: AVAILABLE });
  expect(textOf(container)).toContain("Version 0.2.0 is available.");
  expect(request).not.toHaveBeenCalled();
  await act(async () => void button(container, "Download and install")!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "update_daemon" });
});

it("says which file the daemon cannot replace, offers the install command to copy and no download, and can be dismissed", async () => {
  const writeText = vi.fn(async (_text: string) => {});
  const { container } = mount({ ...IDLE, available: AVAILABLE, manual: { reason: "not_writable", path: "/opt/octoboard/octoboardd", command: MANUAL_COMMAND } }, { writeText, readText: async () => "" });
  expect(textOf(container)).toContain("Version 0.2.0 is available.");
  expect(textOf(container)).toContain("cannot replace its own file");
  expect(textOf(container)).not.toContain("/opt/octoboard/octoboardd");
  expect(button(container, "Download and install")).toBeUndefined();
  expect(container.textContent).toContain("What is new");
  await act(async () => void button(container, "Copy install command")!.click());
  expect(writeText).toHaveBeenCalledExactlyOnceWith(MANUAL_COMMAND);
  await act(async () => void container.querySelector<HTMLButtonElement>("button[aria-label=Later]")!.click());
  expect(container.textContent).not.toContain("Version 0.2.0");
});

it("asks for a reload once the daemon runs another version than the page was loaded for, which cannot be dismissed", async () => {
  const { container, daemon } = mount({ ...IDLE, available: AVAILABLE });
  act(() => daemon.store.setState({ daemon: { version: "0.2.0", generation: 1 } }));
  expect(textOf(container)).toContain("now runs version 0.2.0. Reload this page to use it.");
  expect(container.querySelector("button[aria-label=Later]")).toBeNull();
  expect(button(container, "Download and install")).toBeUndefined();
  expect(reloadPage).not.toHaveBeenCalled();
  await act(async () => void button(container, "Reload")!.click());
  expect(reloadPage).toHaveBeenCalledOnce();
});

it("tells an installed update takes effect after the restart, and shows nothing for a daemon with nothing to say", () => {
  expect(textOf(mount({ ...IDLE, phase: "installed", installed: { version: "0.2.0", holder_floor: null } }).container)).toContain("Version 0.2.0 is installed.");
  expect(mount(IDLE).container.textContent).toBe("");
});

const INSTALLED: DaemonUpdateStatus = { ...IDLE, phase: "installed", installed: { version: "0.2.0", holder_floor: null } };
const UPDATING_AGENT = agentCliStatusesOf([agentCliStatusOf("claude"), agentCliStatusOf("codex", { update: agentCliUpdateOf() })]);
/** Found by its accessible name, which stays "Restart now" while the reason sits inside the button for a screen reader. */
const restartNow = (container: HTMLElement) => container.querySelector<HTMLButtonElement>("button[aria-label='Restart now']")!;

it("says the restart waits for an agent's update, leaves Restart now unavailable with the reason, and asks for no restart when it is pressed, which it explains instead", async () => {
  const { container, request } = mount(INSTALLED, undefined, UPDATING_AGENT);
  expect(textOf(container)).toContain("Version 0.2.0 is installed. Restart when the agent update finishes.");
  expect(textOf(container)).not.toContain("takes effect after");
  const restart = restartNow(container);
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

it("keeps Restart now as it was while no agent's update runs", async () => {
  const { container } = mount(INSTALLED, undefined, agentCliStatusesOf([agentCliStatusOf("claude")]));
  expect(textOf(container)).toContain("Version 0.2.0 is installed. It takes effect after");
  expect(textOf(container)).not.toContain("agent update");
  expect(restartNow(container).hasAttribute("aria-disabled")).toBe(false);
  expect(restartNow(container).hasAttribute("aria-describedby")).toBe(false);
  await act(async () => void restartNow(container).click());
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
});

it("follows the agent's update through its start and its end, and leaves Later as it is", async () => {
  const { container, daemon } = mount(INSTALLED);
  expect(restartNow(container).hasAttribute("aria-disabled")).toBe(false);
  act(() => daemon.store.setState({ agentCliStatuses: UPDATING_AGENT }));
  expect(textOf(container)).toContain("Restart when the agent update finishes.");
  expect(restartNow(container).getAttribute("aria-disabled")).toBe("true");
  expect(button(container, "Later")).toBeDefined();
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("codex", { update: agentCliUpdateOf({ state: "succeeded", finished_at: 2 }) })]) }));
  expect(textOf(container)).toContain("It takes effect after");
  expect(restartNow(container).hasAttribute("aria-disabled")).toBe(false);
  act(() => daemon.store.setState({ agentCliStatuses: UPDATING_AGENT }));
  await act(async () => void button(container, "Later")!.click());
  expect(container.textContent).not.toContain("Version 0.2.0");
});

it("says an installed update of no known version waits for the agent's update too, and takes the restart's hold off with it", () => {
  const { container, daemon } = mount({ ...IDLE, phase: "installed", installed: { version: null, holder_floor: null } }, undefined, UPDATING_AGENT);
  expect(textOf(container)).toContain("An update is installed. Restart when the agent update finishes.");
  expect(restartNow(container).getAttribute("aria-disabled")).toBe("true");
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("codex")]) }));
  expect(textOf(container)).toContain("An update is installed. It takes effect after");
  expect(restartNow(container).hasAttribute("aria-disabled")).toBe(false);
});

it("keeps the focus on Restart now, the same button, as the agent's update starts and ends", () => {
  const { container, daemon } = mount(INSTALLED);
  const restart = restartNow(container);
  restart.focus();
  expect(document.activeElement).toBe(restart);
  act(() => daemon.store.setState({ agentCliStatuses: UPDATING_AGENT }));
  expect(restartNow(container)).toBe(restart);
  expect(document.activeElement).toBe(restart);
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("codex", { update: agentCliUpdateOf({ state: "succeeded", finished_at: 2 }) })]) }));
  expect(restartNow(container)).toBe(restart);
  expect(document.activeElement).toBe(restart);
});

it("drops the hover and press fills of Restart now while it is unavailable, and keeps them for Later and for Restart now once it is available", () => {
  const { container, daemon } = mount(INSTALLED);
  // The installed strip's Later is a text button, found by its text (the dismiss icon button of an available update has the label).
  const later = () => button(container, "Later")!;
  expect(restartNow(container).className).toContain("var(--accent-hover)");
  act(() => daemon.store.setState({ agentCliStatuses: UPDATING_AGENT }));
  expect(restartNow(container).className).not.toContain("var(--accent-hover)");
  expect(later().className).toContain("var(--accent-hover)");
  act(() => daemon.store.setState({ agentCliStatuses: agentCliStatusesOf([agentCliStatusOf("codex")]) }));
  expect(restartNow(container).className).toContain("var(--accent-hover)");
});
