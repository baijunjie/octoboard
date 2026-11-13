// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { PlatformProvider } from "../platform/react";
import type { DaemonUpdateStatus } from "../protocol";
import { DaemonProvider } from "../store";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";
import { BROWSER_HOST } from "../test/hosts";
import { testDaemon } from "../test/testDaemon";
import { UpdateNotice } from "../updates/UpdateNotice";
import { UpdatesProvider } from "../updates/UpdatesProvider";
import { reloadPage } from "../updates/reloadPage";
import { UpdatesSetting } from "./UpdatesSetting";

vi.mock("../updates/reloadPage", () => ({ reloadPage: vi.fn() }));

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
afterEach(() => localStorage.clear());

/** The command the daemon hands over for an upgrade by hand: its own, not one the application makes up. */
const MANUAL_COMMAND = "curl -fsSL https://example.test/install.sh | sh -s -- --bin-dir /opt/octoboard --beta";
const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };
const AVAILABLE = { version: "0.2.0", notes: null, holder_floor: null, channel: "stable" } as const;

function mount(status: DaemonUpdateStatus | null, request = vi.fn(async () => ({ type: "ack" as const, id: null }))) {
  const daemon = testDaemon({
    state: { daemon: { version: "0.1.0", generation: 1 }, daemonUpdate: status, settings: { ...DEFAULT_SETTINGS, versions: { daemon_update_check: 1, daemon_update_channel: 1 } } },
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
              <UpdateNotice focusTerminal={() => {}} />
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
  expect(textOf(container)).toContain("You are using version 0.1.0.");
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
