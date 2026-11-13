// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { DaemonUpdateStatus } from "../protocol";
import { DaemonProvider } from "../store";
import { BROWSER_HOST } from "../test/hosts";
import { testDaemon } from "../test/testDaemon";
import { reloadPage } from "./reloadPage";
import { UpdateNotice } from "./UpdateNotice";
import { UpdatesProvider } from "./UpdatesProvider";

vi.mock("./reloadPage", () => ({ reloadPage: vi.fn() }));

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
  localStorage.clear();
  vi.mocked(reloadPage).mockClear();
});

/** The command the daemon hands over for an upgrade by hand: its own, not one the application makes up. */
const MANUAL_COMMAND = "curl -fsSL https://example.test/install.sh | sh -s -- --bin-dir /opt/octoboard --beta";
const IDLE: DaemonUpdateStatus = { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null };
const AVAILABLE = { version: "0.2.0", notes: null, holder_floor: null, channel: "stable" } as const;

function mount(status: DaemonUpdateStatus, clipboard?: PlatformAdapter["clipboard"]) {
  const request = vi.fn(async () => ({ type: "ack" as const, id: null }));
  const daemon = testDaemon({ state: { daemon: { version: "0.1.0", generation: 1 }, daemonUpdate: status }, request });
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
