// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers, runOutTimers } from "../test/clock";
import { mountHost } from "../test/mountHost";
import type { SessionRequestAction } from "../protocol";
import { RequestPrompt } from "./RequestPrompt";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const TEXT = "Fix the **bug**\n  in <b>search</b>\thttps://example.com\n";

function mount(action: SessionRequestAction, prompt = TEXT): HTMLElement {
  // Before anything mounts: the frame the overflow watch reports from is armed on the faked clock.
  fakeTimers();
  return mountHost(<RequestPrompt action={action} prompt={prompt} />);
}

const trigger = (container: HTMLElement) => container.querySelector<HTMLButtonElement>("button[aria-expanded]")!;

it.each<{ action: SessionRequestAction; label: string }>([
  { action: "start", label: "Brief written by the session" },
  { action: "handoff", label: "Note written by the session" },
  { action: "create_scheduled_task", label: "Prompt written by the session" },
  { action: "update_scheduled_task", label: "Prompt written by the session" },
])("labels the text of a $action as the session's own: $label", ({ action, label }) => {
  const container = mount(action);
  expect(trigger(container).textContent).toBe(label);
});

it("is folded to begin with and shows the text verbatim as plain text once opened", async () => {
  const container = mount("create_scheduled_task");
  expect(trigger(container).getAttribute("aria-expanded")).toBe("false");
  await act(async () => trigger(container).click());
  expect(trigger(container).getAttribute("aria-expanded")).toBe("true");
  const block = container.querySelector("pre")!;
  expect(block.textContent).toBe(TEXT);
  expect(block.querySelector("a, b, strong")).toBeNull();
  expect(block.classList.contains("whitespace-pre-wrap")).toBe(true);
  await act(async () => trigger(container).click());
  expect(trigger(container).getAttribute("aria-expanded")).toBe("false");
});

it("keeps a long text in a block of bounded height that scrolls on its own, named and reachable by keyboard", async () => {
  for (const [key, value] of Object.entries({ clientWidth: 100, clientHeight: 100, scrollWidth: 100, scrollHeight: 900 })) {
    const mock = vi.spyOn(HTMLElement.prototype, key as "clientWidth", "get").mockReturnValue(value);
    onTestFinished(() => mock.mockRestore());
  }
  const container = mount("update_scheduled_task", "line\n".repeat(200));
  await act(async () => trigger(container).click());
  await act(async () => {});
  runOutTimers();
  const scroller = container.querySelector<HTMLElement>("[data-slot=scroll-shadow]")!;
  expect(scroller.classList.contains("max-h-[40vh]")).toBe(true);
  expect(scroller.classList.contains("overflow-auto")).toBe(true);
  expect(scroller.getAttribute("role")).toBe("region");
  expect(scroller.getAttribute("tabindex")).toBe("0");
  expect(scroller.getAttribute("aria-label")).toBe("Prompt written by the session");
});
