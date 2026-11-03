// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { appConfig } from "../appConfig";
import { setLanguage } from "../i18n/language";
import type { PlatformAdapter } from "../platform";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { IncompatibleDaemonScreen } from "./IncompatibleDaemonScreen";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => act(() => setLanguage("en")));

const desktop = { kind: "tauri", exit: {}, updater: { currentVersion: () => Promise.resolve("1.3.0") } } as unknown as PlatformAdapter;
const button = (container: HTMLElement, name: string) => Array.from(container.querySelectorAll("button")).find((b) => b.textContent === name);

it("names both versions and quits on the desktop", async () => {
  const onQuit = vi.fn();
  const daemon = testDaemon({ state: { connectionState: "incompatible", daemon: { version: "1.4.0", generation: 2 } } });
  const container = mountHost(<IncompatibleDaemonScreen onQuit={onQuit} />, { daemon, platform: desktop });
  await act(async () => {});
  expect(container.querySelector("h1")?.textContent).toBe(`${appConfig.name} and its daemon are different versions`);
  expect(container.textContent?.replace(/[\u2068\u2069]/g, "")).toContain("Window version: 1.3.0. Daemon version: 1.4.0.");
  expect(button(container, "Reload")).toBeUndefined();
  act(() => button(container, `Quit ${appConfig.name}`)!.click());
  expect(onQuit).toHaveBeenCalledTimes(1);
});

it("offers Reload in a browser, and says a version it does not know is unknown", async () => {
  const daemon = testDaemon({ state: { connectionState: "incompatible", daemon: null } });
  const container = mountHost(<IncompatibleDaemonScreen onQuit={vi.fn()} />, { daemon });
  await act(async () => {});
  expect(container.textContent).toContain("Window version: unknown. Daemon version: unknown.");
  expect(button(container, `Quit ${appConfig.name}`)).toBeUndefined();
  expect(button(container, "Reload")).toBeDefined();
});
