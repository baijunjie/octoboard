// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { mountForTooltip } from "./components/hint/tooltipTestSupport";
import { consoleOf, snapshotState } from "./fixtures/builders";
import { PlatformProvider } from "./platform/react";
import { DaemonProvider, type Daemon } from "./store";
import { testDaemon } from "./test/testDaemon";
import { ThemeProvider } from "./preferences/theme";
import { matchesWidthQuery } from "./test/viewport";
import { BROWSER_HOST } from "./test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));

// xterm owns the live terminal; the real pane and its empty-state controls stay under test.
vi.mock("./terminal/TerminalController", () => ({
  TerminalController: class {
    mount() {}
    dispose() {}
    fit() {}
    syncSize() {}
    setInputLabel() {}
    detach() {}
  },
}));

it.each(["sidebar", "terminal"] as const)("opens console session settings from the %s before launching", async (entry) => {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: matchesWidthQuery(media, 1440), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--rail-width");
    document.documentElement.style.removeProperty("--breakpoint-tablet");
    document.documentElement.style.removeProperty("--breakpoint-phone");
  });
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  const console_ = consoleOf("c-main", "Main");
  const daemon = testDaemon({
    state: snapshotState({ consoles: [console_] }),
    request: vi.fn(async () => ({ type: "ack", id: null })) as Daemon["request"],
    showNotice: vi.fn(),
  });
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <ThemeProvider>
        <DaemonProvider value={daemon}><App /></DaemonProvider>
      </ThemeProvider>
    </PlatformProvider>,
  );
  const region = container.querySelector(`[data-region="${entry}"]`)!;
  const trigger = Array.from(region.querySelectorAll("button")).find((button) =>
    button.textContent === "New console session" || button.getAttribute("aria-label") === "New console session",
  )!;
  expect(trigger).toBeDefined();
  await act(async () => trigger.click());
  expect(document.body.querySelector('[role="dialog"] h2')?.textContent).toBe("Open console session");
  expect(document.body.querySelector('[role="dialog"]')?.textContent).toContain("The session will be opened in the console Main.");
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "open_session")).toBe(false);
  await act(async () => Array.from(document.body.querySelectorAll("button")).find((button) => button.textContent === "Open")!.click());
  expect(daemon.request).toHaveBeenCalledWith({
    type: "open_session", console_id: console_.id, agent: "claude", account: null, title: "Main session", terminal_appearance: "light",
  });
});

it("replaces the whole app with the key refused screen, with no retry, once the connection is key refused", async () => {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: matchesWidthQuery(media, 1440), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => void vi.unstubAllGlobals());
  const daemon = testDaemon({
    state: snapshotState({ consoles: [consoleOf("c-main", "Main")] }),
    request: vi.fn(async () => ({ type: "ack", id: null })) as Daemon["request"],
    reconnect: vi.fn(),
  });
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <ThemeProvider>
        <DaemonProvider value={daemon}><App /></DaemonProvider>
      </ThemeProvider>
    </PlatformProvider>,
  );
  expect(container.querySelector("h1")?.textContent).not.toBe("This address no longer opens the daemon");
  await act(async () => daemon.store.setState({ connectionState: "keyRefused" }));
  expect(container.querySelector("h1")?.textContent).toBe("This address no longer opens the daemon");
  expect(container.textContent).toContain("Its key has changed.");
  expect(container.querySelector('[data-region="sidebar"]')).toBeNull();
  expect(container.querySelector("button")).toBeNull();
});
