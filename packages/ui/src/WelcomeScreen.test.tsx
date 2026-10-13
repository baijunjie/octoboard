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

it("shows the install prompt when no agent is on PATH, and keeps the action enabled", () => {
  const unavailable = new Map<Agent, AgentAvailability>(
    (["claude", "codex", "grok"] as const).map((agent) => [agent, { agent, availability: "unavailable" }]),
  );
  // "Grok Build" is in the install prompt, in every language, and nowhere else on the screen.
  expect(mount().textContent).not.toContain("Grok Build");
  const container = mount(vi.fn(), { agentAvailability: unavailable });
  expect(container.textContent).toContain("Grok Build");
  // Unlike the sidebar's, the button is not disabled: a console's agents are only defaults.
  expect(container.querySelector("button")!.disabled).toBe(false);
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
