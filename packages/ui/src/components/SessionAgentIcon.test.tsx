// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import type { Account } from "../protocol";
import { SessionAgentTooltip } from "./SessionAgentIcon";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactNode) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(element));
  return container;
}

const accounts: Account[] = [{ id: "w", agent: "claude", name: "Work", config_dir: "/home/w" }];

it("names the agent, then the account as a badge that takes its text's own direction", () => {
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "w" })} accounts={accounts} defaultNames={undefined} />);
  expect(container.textContent).toBe("Claude Code, Work");
  expect(container.querySelector("[dir]")?.getAttribute("dir")).toBe("auto");
});

it("lets a long recorded path wrap inside the tooltip, read left to right", () => {
  const path = `/Users/someone/.config/${"deeply-nested-directory/".repeat(8)}claude`;
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "gone", config_dir: path })} accounts={accounts} defaultNames={undefined} />);
  const badge = container.querySelector<HTMLElement>("code")!.parentElement!;
  expect(badge.textContent).toBe(path);
  expect(badge.getAttribute("dir")).toBe("ltr");
  expect(badge.className).toContain("min-w-0");
  expect(badge.className).toContain("[overflow-wrap:anywhere]");
  expect(badge.className).not.toContain("whitespace-nowrap");
  expect(container.firstElementChild?.className).toContain("flex-wrap");
});

it("shows the agent alone for a removed account that recorded no directory", () => {
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "gone" })} accounts={accounts} defaultNames={undefined} />);
  expect(container.textContent).toBe("Claude Code");
});
