// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { Agent, AgentAvailability } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { AccountDialog } from "./AccountDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("lists every agent as selectable when adding, naming the ones known to be missing", async () => {
  const daemon: Daemon = {
    store: createStateStore({
      agentAvailability: new Map(
        (["claude", "codex", "grok"] as Agent[]).map((agent): [Agent, AgentAvailability] => [agent, { agent, availability: agent === "codex" ? "unavailable" : "available" }]),
      ),
    }),
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
  act(() => root.render(<DaemonProvider value={daemon}><AccountDialog onClose={() => {}} /></DaemonProvider>));
  await act(async () => document.body.querySelector<HTMLElement>('[data-slot="select-trigger"]')!.click());
  const options = Array.from(document.body.querySelectorAll('[role="option"]'));
  expect(options.map((o) => o.textContent)).toEqual(["Claude Code", "Codex (not installed)", "Grok Build"]);
  expect(options.some((o) => o.hasAttribute("data-disabled") || o.getAttribute("aria-disabled") === "true")).toBe(false);
});
