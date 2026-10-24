// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { READ_TIME_MS, StatusAnnouncer } from "./StatusAnnouncer";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("writes a text again that comes back after it was emptied", () => {
  vi.useFakeTimers();
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  onTestFinished(() => {
    act(() => root.unmount());
    host.remove();
    vi.useRealTimers();
  });
  const region = () => host.querySelector('[role="status"]')?.textContent;
  const render = (text: string | undefined) => act(() => root.render(<StatusAnnouncer text={text} firstWriteDelayMs={0} clearAfterRead />));

  render("Loading");
  act(() => vi.advanceTimersByTime(0));
  expect(region()).toBe("Loading");
  act(() => vi.advanceTimersByTime(READ_TIME_MS));
  expect(region()).toBe("");

  render(undefined);
  render("Loading");
  expect(region()).toBe("Loading");
  act(() => vi.advanceTimersByTime(READ_TIME_MS));
  expect(region()).toBe("");
});
