// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { setLanguage } from "../i18n/language";
import type { PlatformAdapter } from "../platform";
import { reloadPage } from "../platform/reloadPage";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { IncompatibleDaemonScreen } from "./IncompatibleDaemonScreen";
import { BROWSER_HOST } from "../test/hosts";

vi.mock("../platform/reloadPage", () => ({ reloadPage: vi.fn() }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  vi.mocked(reloadPage).mockClear();
  act(() => setLanguage("en"));
});

const desktop = { kind: "tauri", host: BROWSER_HOST, exit: {}, updater: { currentVersion: () => Promise.resolve("1.3.0") } } as unknown as PlatformAdapter;
const button = (container: HTMLElement, name: string) => Array.from(container.querySelectorAll("button")).find((b) => b.textContent === name);

it("names both versions and restarts on the desktop", async () => {
  const onRestart = vi.fn(async () => true);
  const daemon = testDaemon({ state: { connectionState: "incompatible", daemon: { version: "1.4.0", generation: 2 } } });
  const container = mountHost(<IncompatibleDaemonScreen onRestart={onRestart} />, { daemon, platform: desktop });
  await act(async () => {});
  expect(container.querySelector("h1")?.textContent).toBe("The app and its daemon are different versions");
  expect(container.textContent?.replace(/[\u2068\u2069]/g, "")).toContain("Window version: 1.3.0. Daemon version: 1.4.0.");
  expect(button(container, "Reload")).toBeUndefined();
  expect(button(container, "Quit")).toBeUndefined();
  act(() => button(container, "Restart")!.click());
  expect(onRestart).toHaveBeenCalledTimes(1);
});

it("offers Reload in a browser, and says a version it does not know is unknown", async () => {
  const daemon = testDaemon({ state: { connectionState: "incompatible", daemon: null } });
  const container = mountHost(<IncompatibleDaemonScreen onRestart={vi.fn(async () => true)} />, { daemon });
  await act(async () => {});
  expect(container.textContent).toContain("Window version: unknown. Daemon version: unknown.");
  expect(button(container, "Restart")).toBeUndefined();
  expect(button(container, "Reload")).toBeDefined();
});

it("reloads the page through reloadPage when Reload is pressed in a browser", async () => {
  const daemon = testDaemon({ state: { connectionState: "incompatible", daemon: { version: "1.4.0", generation: 2 } } });
  const container = mountHost(<IncompatibleDaemonScreen onRestart={vi.fn(async () => true)} />, { daemon });
  await act(async () => {});
  expect(reloadPage).not.toHaveBeenCalled();
  act(() => button(container, "Reload")!.click());
  expect(reloadPage).toHaveBeenCalledOnce();
});

it("holds Restart pending from the press while the restart is under way", async () => {
  const onRestart = vi.fn(async () => true);
  const daemon = testDaemon({ state: { connectionState: "incompatible", daemon: { version: "1.4.0", generation: 2 } } });
  const container = mountHost(<IncompatibleDaemonScreen onRestart={onRestart} />, { daemon, platform: desktop });
  await act(async () => {});
  expect(button(container, "Restart")!.getAttribute("aria-busy")).not.toBe("true");
  await act(async () => button(container, "Restart")!.click());
  expect(button(container, "Restart")!.getAttribute("aria-busy")).toBe("true");
  await act(async () => button(container, "Restart")!.click());
  expect(onRestart).toHaveBeenCalledTimes(1);
});

it("releases Restart once the restart did not happen, so it can be pressed again", async () => {
  const onRestart = vi.fn(async () => false);
  const daemon = testDaemon({ state: { connectionState: "incompatible", daemon: { version: "1.4.0", generation: 2 } } });
  const container = mountHost(<IncompatibleDaemonScreen onRestart={onRestart} />, { daemon, platform: desktop });
  await act(async () => {});
  await act(async () => button(container, "Restart")!.click());
  expect(button(container, "Restart")!.getAttribute("aria-busy")).not.toBe("true");
  await act(async () => button(container, "Restart")!.click());
  expect(onRestart).toHaveBeenCalledTimes(2);
});
