// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { landInPanel } from "./panelLanding";

afterEach(() => {
  vi.useRealTimers();
  document.body.replaceChildren();
});

function card(): HTMLElement {
  const element = document.body.appendChild(document.createElement("section"));
  element.id = "panel-processes";
  return element;
}

it("lands in a control that fills in after the card, within the deadline", async () => {
  vi.useFakeTimers();
  landInPanel("processes");
  const element = card();
  await vi.advanceTimersByTimeAsync(2000);
  const control = element.appendChild(document.createElement("button"));
  await vi.advanceTimersByTimeAsync(0);
  expect(document.activeElement).toBe(control);
});

it("gives up on a control that fills in only after the deadline, whether or not the card was there", async () => {
  vi.useFakeTimers();
  landInPanel("processes");
  const element = card();
  await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(3500);
  element.appendChild(document.createElement("button"));
  await vi.advanceTimersByTimeAsync(0);
  expect(document.activeElement).toBe(document.body);
});

it("stops waiting when the returned canceller is called", async () => {
  vi.useFakeTimers();
  const cancel = landInPanel("processes");
  cancel();
  card().appendChild(document.createElement("button"));
  await vi.advanceTimersByTimeAsync(0);
  expect(document.activeElement).toBe(document.body);
});
