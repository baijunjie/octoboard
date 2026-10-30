// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { accountOf, sessionOf } from "../gallery/fixtures/builders";
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

const accounts: Account[] = [accountOf("w", "claude", "Work", "/home/w")];

it("names the agent, then the account as a badge that takes its text's own direction", () => {
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "w" })} accounts={accounts} defaultNames={{}} />);
  expect(container.textContent).toBe("Claude Code, Work");
  const badge = Array.from(container.querySelectorAll("[dir]")).find((element) => element.textContent === "Work");
  expect(badge?.getAttribute("dir")).toBe("auto");
});

it("lets a long recorded path wrap inside the tooltip, read left to right", () => {
  const path = `/Users/someone/.config/${"deeply-nested-directory/".repeat(8)}claude`;
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "gone", config_dir: path })} accounts={accounts} defaultNames={{}} />);
  const badge = container.querySelector<HTMLElement>("code")!.parentElement!;
  expect(badge.textContent).toBe(path);
  expect(badge.getAttribute("dir")).toBe("ltr");
  expect(badge.className).toContain("min-w-0");
  expect(badge.className).toContain("[overflow-wrap:anywhere]");
  expect(badge.className).not.toContain("whitespace-nowrap");
  expect(container.firstElementChild?.className).toContain("flex-wrap");
});

it("shows the agent alone for a removed account that recorded no directory", () => {
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "gone" })} accounts={accounts} defaultNames={{}} />);
  expect(container.textContent).toBe("Claude Code");
});

it("lists the account, model and effort under their labels when given the launch", () => {
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "w" })} accounts={accounts} defaultNames={{}} launch={{ model: "claude-opus-5-5", effort: "high" }} />);
  const rows = Array.from(container.querySelectorAll("dt")).map((dt) => [dt.textContent, dt.nextElementSibling?.textContent]);
  expect(container.firstElementChild?.firstElementChild?.textContent).toBe("Claude Code");
  expect(rows).toEqual([["Account", "Work"], ["Model", "Opus 5.5"], ["Effort", "high"]]);
});

it("lists a recorded path left to right, and each unknown value as the default", () => {
  const path = `/Users/someone/.config/${"deeply-nested-directory/".repeat(8)}claude`;
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "gone", config_dir: path })} accounts={accounts} defaultNames={{}} launch={{ model: null, effort: null }} />);
  const value = container.querySelector<HTMLElement>("code")!.parentElement!;
  expect(value.tagName).toBe("DD");
  expect(value.getAttribute("dir")).toBe("ltr");
  expect(value.className).toContain("[overflow-wrap:anywhere]");
  expect(Array.from(container.querySelectorAll("dd")).slice(1).map((dd) => dd.textContent)).toEqual(["Default", "Default"]);
});

it("leaves the account row out for a removed account that recorded no directory", () => {
  const container = mount(<SessionAgentTooltip session={sessionOf("s", "c", "p", "T", "idle", { account_id: "gone" })} accounts={accounts} defaultNames={{}} launch={{ model: "claude-opus-5-5", effort: "high" }} />);
  expect(Array.from(container.querySelectorAll("dt")).map((dt) => dt.textContent)).toEqual(["Model", "Effort"]);
});
