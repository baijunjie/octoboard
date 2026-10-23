// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { SessionContext } from "../protocol";
import { ArchivedSessionMetadata, SessionContextLabel } from "./SessionMetadata";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactNode) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (node: React.ReactNode) => act(() => root.render(node));
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  render(element);
  return { container, render };
}

const context: SessionContext = { tokens: 190123, window: 475000, used_percent: 40 };

it("shows the compact context size, and its exact count with the limit and share used in the name and tooltip", () => {
  const { container } = mount(<SessionContextLabel context={context} />);
  expect(container.textContent).toBe("190.1K");
  const badge = container.querySelector('[aria-hidden="true"] > span:not(.sr-only)')!;
  expect(badge.textContent).toBe("190.1K");
  expect(badge.getAttribute("data-variant")).toBe("bordered");
  expect(container.querySelector(".sr-only")).toBeNull();
  const indicator = container.querySelector<HTMLElement>('[role="img"]')!;
  expect(indicator.getAttribute("aria-label")).toBe(`Context ${isolate("190,123")} tokens, Limit: ${isolate("475,000")} (${isolate("40%")} used)`);
  expect(indicator.tabIndex).toBe(-1);
  expect(indicator.querySelector(":scope > [aria-hidden]")?.getAttribute("aria-hidden")).toBe("true");
  expect(container.querySelector("button")).toBeNull();
  expect(indicator.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }))).toBe(false);
});

it("names the context size alone when the agent records no limit", () => {
  const { container } = mount(<SessionContextLabel context={{ tokens: 190123 }} />);
  expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe(`Context ${isolate("190,123")} tokens`);
});

it("omits an unknown or zero context size until a positive one arrives", () => {
  const { container, render } = mount(<SessionContextLabel />);
  expect(container.textContent).toBe("");
  render(<SessionContextLabel context={{ tokens: 0 }} />);
  expect(container.innerHTML).toBe("");
  render(<SessionContextLabel context={{ tokens: 1 }} />);
  expect(container.textContent).toBe("1");
  expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).toBe(`Context ${isolate("1")} token`);
});

it("shows both absolute timestamps and never invents a missing archive timestamp", () => {
  const session = sessionOf("old", "console", "project", "A session", "archived", { started_at: 1700000000000, ended_at: 1700000600000 });
  const { container, render } = mount(<ArchivedSessionMetadata session={session} />);
  expect(Array.from(container.querySelectorAll("time")).map((time) => time.dateTime)).toEqual(["2023-11-14T22:23:20.000Z", "2023-11-14T22:13:20.000Z"]);
  expect(container.textContent).toContain("Created");
  expect(container.textContent).toContain("Archived");
  render(<ArchivedSessionMetadata session={{ ...session, ended_at: null }} />);
  expect(container.querySelectorAll("time")).toHaveLength(1);
  expect(container.textContent).not.toContain("Archived");
});
