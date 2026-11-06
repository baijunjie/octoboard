// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { registerDialogLauncher } from "../dialogs/dialogLauncher";
import { consoleOf } from "../fixtures/builders";
import type { PlatformAdapter } from "../platform";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { ConfigurationSection } from "./ConfigurationSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The sidebar's chosen console is read when its module loads, so it is set before the imports run.
vi.hoisted(() => localStorage.setItem("octoboard.sidebarConsole", "c-docs"));

const WITH_CONFIGURATION_FILE: PlatformAdapter = { kind: "browser", configurationFile: { save: vi.fn(), open: vi.fn() } };

function mount(consoles: ReturnType<typeof consoleOf>[], platform: PlatformAdapter = WITH_CONFIGURATION_FILE): HTMLElement {
  return mountHost(<ConfigurationSection />, { daemon: testDaemon({ state: { consoles: new Map(consoles.map((c) => [c.id, c])) } }), platform });
}

const press = (label: string) =>
  act(async () => Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === label)!.click());

const main = consoleOf("c-main", "Main");
const docs = consoleOf("c-docs", "Docs");

it("exports the console the sidebar shows when the export is opened", async () => {
  mount([main, docs]);
  await press("Export");
  expect(document.body.textContent).toContain("Console Docs and its projects");
});

it("falls back to the first console when the one the sidebar chose is gone", async () => {
  mount([main]);
  await press("Export");
  expect(document.body.textContent).toContain("Console Main and its projects");
});

it("asks the window to open the import from its own button, and opens none itself", async () => {
  const open = vi.fn();
  onTestFinished(registerDialogLauncher(open));
  mount([main]);
  await press("Import");
  expect(open).toHaveBeenCalledExactlyOnceWith({ kind: "import-configuration" });
  expect(document.body.textContent).not.toContain("Choose an exported configuration file");
});

it("has nothing to press where the platform cannot save or open a file", () => {
  const container = mount([main], { kind: "browser" });
  expect(container.querySelectorAll("button")).toHaveLength(0);
  expect(container.textContent).toContain("not available here");
});
