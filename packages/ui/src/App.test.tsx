// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { mountForTooltip } from "./components/tooltipTestSupport";
import { consoleOf, snapshotState } from "./gallery/fixtures/builders";
import { PlatformProvider } from "./platform/react";
import { createStateStore, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./theme";

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
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--rail-width");
    document.documentElement.style.removeProperty("--breakpoint-docked");
  });
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  const console_ = consoleOf("c-main", "Main");
  const daemon: Daemon = {
    store: createStateStore(snapshotState({ consoles: [console_] })),
    request: vi.fn(async () => ({ type: "ack" })) as Daemon["request"],
    toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {},
    dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser" }}>
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
  expect(document.body.querySelector('[role="dialog"] h2')?.textContent).toBe("Open console session in Main");
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "open_session")).toBe(false);
  await act(async () => Array.from(document.body.querySelectorAll("button")).find((button) => button.textContent === "Open")!.click());
  expect(daemon.request).toHaveBeenCalledWith({
    type: "open_session", console_id: console_.id, agent: "claude", account: null, title: "Main session", terminal_appearance: "light",
  });
});
