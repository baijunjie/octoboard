// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { SessionUsage } from "../protocol";
import { ArchivedSessionMetadata, SessionUsageLabel } from "./SessionMetadata";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactNode) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (node: React.ReactNode) => act(() => root.render(node));
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  render(element);
  return { container, render };
}

const usage: SessionUsage = { input_tokens: 123456, output_tokens: 789, cached_input_tokens: 100000, cache_write_input_tokens: 0, total_tokens: 124999 };

it("uses the recorded total and exposes full localized counts with cache subsets without adding them", () => {
  const { container } = mount(<SessionUsageLabel usage={usage} />);
  expect(container.textContent).toBe("125K");
  const label = container.querySelector('[role="img"]')?.getAttribute("aria-label");
  expect(label).toContain(`${isolate("124,999")} tokens`);
  expect(label).toContain(`Input: ${isolate("123,456")}`);
  expect(label).toContain(`Cached input: ${isolate("100,000")}`);
  expect(label).toContain(`Cache write: ${isolate("0")}`);
  const indicator = container.querySelector<HTMLElement>('[role="img"]')!;
  expect(indicator.tabIndex).toBe(-1);
  expect(indicator.querySelector('[data-inline-chip][data-size="xs"]')?.getAttribute("aria-hidden")).toBe("true");
  expect(container.querySelector("button")).toBeNull();
  expect(indicator.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }))).toBe(false);
});

it("omits unavailable and zero usage until a positive total arrives", () => {
  const { container, render } = mount(<SessionUsageLabel />);
  expect(container.textContent).toBe("");
  render(<SessionUsageLabel usage={{ input_tokens: 0, output_tokens: 0, total_tokens: 0 }} />);
  expect(container.innerHTML).toBe("");
  render(<SessionUsageLabel usage={{ input_tokens: 1, output_tokens: 0, total_tokens: 1 }} />);
  expect(container.textContent).toBe("1");
  expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).toContain(`${isolate("1")} token`);
  expect(container.querySelector('[role="img"]')?.getAttribute("aria-label")).not.toContain("Cached input");
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
