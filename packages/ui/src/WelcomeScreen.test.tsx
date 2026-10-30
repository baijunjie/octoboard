// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { PlatformAdapter } from "./platform";
import { PlatformProvider } from "./platform/react";
import type { Agent, AgentAvailability } from "./protocol";
import { createStateStore, DaemonProvider, type Daemon, type State } from "./store";
import { WelcomeScreen } from "./WelcomeScreen";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A platform that can save and open a file, which is what puts the import action on the screen. */
const WITH_CONFIGURATION_FILE: PlatformAdapter = { kind: "browser", configurationFile: { save: vi.fn(), open: vi.fn() } };

function mount(onOpenDialog = vi.fn(), state: Partial<State> = {}, platform: PlatformAdapter = { kind: "browser" }): HTMLElement {
  const daemon: Daemon = {
    store: createStateStore(state),
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
          <WelcomeScreen onOpenDialog={onOpenDialog} />
        </DaemonProvider>
      </PlatformProvider>,
    ),
  );
  return container;
}

it("opens the new-console dialog from its action", () => {
  const onOpenDialog = vi.fn();
  const container = mount(onOpenDialog);
  act(() => container.querySelector("button")!.click());
  expect(onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "new-console" });
});

it("is a named region of the window", () => {
  const region = mount().querySelector("[data-region=welcome]");
  expect(region?.getAttribute("aria-label")).toBeTruthy();
});

it("always lists supported tools and keeps the action enabled when none is detected", () => {
  const unavailable = new Map<Agent, AgentAvailability>(
    (["claude", "codex", "grok"] as const).map((agent) => [agent, { agent, availability: "unavailable", default_account_dir: null }]),
  );
  const pending = mount();
  expect(pending.querySelectorAll("li")).toHaveLength(3);
  expect(pending.textContent).toContain("Grok Build");
  expect(pending.textContent).toContain("Checking…");
  const container = mount(vi.fn(), { agentAvailability: unavailable });
  expect(container.querySelectorAll("li")).toHaveLength(3);
  expect(container.textContent).toContain("Not detected");
  // Unlike the sidebar's, the button is not disabled: a console's agents are only defaults.
  expect(container.querySelector("button")!.disabled).toBe(false);
});

it("shows each tool's own detection status when an installed tool is available", () => {
  const agentAvailability = new Map<Agent, AgentAvailability>([
    ["claude", { agent: "claude", availability: "available", default_account_dir: null }],
    ["codex", { agent: "codex", availability: "unavailable", default_account_dir: null }],
    ["grok", { agent: "grok", availability: "available", default_account_dir: null }],
  ]);
  const rows = Array.from(mount(vi.fn(), { agentAvailability }).querySelectorAll("li"));
  expect(rows.map((row) => row.textContent)).toEqual([
    "Claude Code, Installed", "Codex, Not detected", "Grok Build, Installed",
  ]);
  expect(rows.every((row) => row.querySelector("svg[aria-hidden=true]"))).toBe(true);
  expect(rows.map((row) => row.querySelector("[role=status]")?.textContent)).toEqual(rows.map((row) => row.textContent));
});

it("offers importing a configuration only where the platform can open a file", () => {
  const onOpenDialog = vi.fn();
  const buttons = (container: HTMLElement) => Array.from(container.querySelectorAll("button"));
  expect(buttons(mount())).toHaveLength(1);
  const container = mount(onOpenDialog, {}, WITH_CONFIGURATION_FILE);
  expect(buttons(container)).toHaveLength(2);
  act(() => buttons(container)[1].click());
  expect(onOpenDialog).toHaveBeenCalledExactlyOnceWith({ kind: "import-configuration" });
});
