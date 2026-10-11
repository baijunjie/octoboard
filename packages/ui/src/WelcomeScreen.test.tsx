// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { Agent, AgentAvailability } from "./protocol";
import { createStateStore, DaemonProvider, type Daemon, type State } from "./store";
import { WelcomeScreen } from "./WelcomeScreen";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(onOpenDialog = vi.fn(), state: Partial<State> = {}): HTMLElement {
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
      <DaemonProvider value={daemon}>
        <WelcomeScreen onOpenDialog={onOpenDialog} />
      </DaemonProvider>,
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
