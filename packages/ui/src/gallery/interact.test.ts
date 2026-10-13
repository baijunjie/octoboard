// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { createUi, stuckExit } from "./interact";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  document.body.replaceChildren();
});

function exiting(text: string): HTMLElement {
  const element = document.createElement("div");
  element.dataset.exiting = "true";
  element.textContent = text;
  document.body.append(element);
  return element;
}

it("stuckExit names what is still closing once the wait is spent", async () => {
  exiting("General Appearance");
  const result = stuckExit(document);
  await vi.advanceTimersByTimeAsync(6000);
  expect(await result).toBe("<div> beginning “General Appearance”");
});

it("stuckExit reports nothing for an exit that ends within the wait", async () => {
  const element = exiting("General");
  const result = stuckExit(document);
  await vi.advanceTimersByTimeAsync(200);
  element.remove();
  await vi.advanceTimersByTimeAsync(100);
  expect(await result).toBeUndefined();
});

it("tabToRegion sends Tab before it focuses the region its dialog's title names", async () => {
  document.body.innerHTML = `<h2 id="title">Import configuration</h2><div role="region" tabindex="0" aria-labelledby="title"></div>`;
  const region = document.querySelector<HTMLElement>("[role=region]")!;
  const seen: string[] = [];
  document.addEventListener("keydown", (event) => seen.push(`${event.key}:${document.activeElement === region}`), { once: true });
  const done = createUi(document, "en").tabToRegion("Import configuration");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(seen).toEqual(["Tab:false"]);
  expect(document.activeElement).toBe(region);
});

it("tabToRegion fails where no region has the name, as in a window tall enough for the dialog", async () => {
  document.body.innerHTML = `<h2 id="title">Import configuration</h2><div tabindex="-1" aria-labelledby="title"></div>`;
  const done = expect(createUi(document, "en").tabToRegion("Import configuration")).rejects.toThrow(/No scroll region/);
  await vi.advanceTimersByTimeAsync(6000);
  await done;
});
