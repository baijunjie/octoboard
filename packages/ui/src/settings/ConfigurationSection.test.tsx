// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf } from "../gallery/fixtures/builders";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConfigurationSection } from "./ConfigurationSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The sidebar's chosen console is read when its module loads, so it is set before the imports run.
vi.hoisted(() => localStorage.setItem("octoboard.sidebarConsole", "c-docs"));

const WITH_CONFIGURATION_FILE: PlatformAdapter = { kind: "browser", configurationFile: { save: vi.fn(), open: vi.fn() } };

function mount(consoles: ReturnType<typeof consoleOf>[], platform: PlatformAdapter = WITH_CONFIGURATION_FILE): HTMLElement {
  const daemon: Daemon = {
    store: createStateStore({ consoles: new Map(consoles.map((c) => [c.id, c])) }),
    request: vi.fn(),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <PlatformProvider value={platform}>
        <DaemonProvider value={daemon}>
          <ConfigurationSection />
        </DaemonProvider>
      </PlatformProvider>,
    ),
  );
  return container;
}

const press = (label: string) =>
  act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === label)!.click());

const main = consoleOf("c-main", "Main");
const docs = consoleOf("c-docs", "Docs");

it("exports the console the sidebar shows when the export is opened", async () => {
  mount([main, docs]);
  await press("Export…");
  expect(document.body.textContent).toContain("Console Docs and its projects");
});

it("falls back to the first console when the one the sidebar chose is gone", async () => {
  mount([main]);
  await press("Export…");
  expect(document.body.textContent).toContain("Console Main and its projects");
});

it("opens the import from its own button", async () => {
  mount([main]);
  await press("Import…");
  expect(document.body.textContent).toContain("Choose an exported configuration file");
});

it("has nothing to press where the platform cannot save or open a file", () => {
  const container = mount([main], { kind: "browser" });
  expect(container.querySelectorAll("button")).toHaveLength(0);
  expect(container.textContent).toContain("not available here");
});
