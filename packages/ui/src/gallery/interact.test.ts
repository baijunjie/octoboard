// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { createUi, stuckExit, type Matcher } from "./interact";
import { format } from "../i18n/catalog";

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

it("press takes the control inside a band of the same name, not the band", async () => {
  document.body.innerHTML = `<div role="row" id="band"><button id="fold">Staged15</button></div>`;
  const pressed: string[] = [];
  document.addEventListener("click", (event) => pressed.push((event.target as Element).id));
  const done = createUi(document, "en").press("Staged15");
  await vi.advanceTimersByTimeAsync(500);
  await done;
  expect(pressed).toEqual(["fold"]);
});

it("changeRow names a change row by its file and status in the window's language, whatever follows", () => {
  const english = createUi(document, "en").changeRow("new-name.ts", "renamed");
  const chinese = createUi(document, "zh-Hans").changeRow("new-name.ts", "renamed");
  const translatedFrom = format("zh-Hans", "git.row.detail", {
    name: "new-name.ts", status: format("zh-Hans", "git.status.renamed"),
    detail: format("zh-Hans", "git.row.from", { path: "src/old-name.ts" }),
  });
  const translatedSelected = format("zh-Hans", "git.row.selected", { name: "new-name.ts", status: format("zh-Hans", "git.status.renamed") });
  const translatedFromSelected = format("zh-Hans", "git.row.detail.selected", {
    name: "new-name.ts", status: format("zh-Hans", "git.status.renamed"),
    detail: format("zh-Hans", "git.row.from", { path: "src/old-name.ts" }),
  });
  const matches = (matcher: Matcher, name: string) => (typeof matcher === "string" ? matcher === name : matcher(name));
  expect(matches(english, "new-name.ts, renamed, from src/old-name.ts")).toBe(true);
  expect(matches(english, "new-name.ts, renamed")).toBe(true);
  expect(matches(english, "new-name.ts, renamed, selected")).toBe(true);
  expect(matches(english, "new-name.ts, renamed, from src/old-name.ts, selected")).toBe(true);
  expect(matches(english, "new-name.ts, modified")).toBe(false);
  expect(matches(english, translatedFrom)).toBe(false);
  expect(matches(chinese, translatedFrom)).toBe(true);
  expect(matches(chinese, translatedSelected)).toBe(true);
  expect(matches(chinese, translatedFromSelected)).toBe(true);
  expect(matches(chinese, "new-name.ts, renamed, from src/old-name.ts")).toBe(false);
});

it("a matcher from the helpers says what it looks for when a step fails", () => {
  const ui = createUi(document, "en");
  expect(String(ui.changeRow("new-name.ts", "renamed"))).toBe("changeRow(new-name.ts, renamed)");
  expect(String(ui.session("Hub"))).toBe("session(Hub)");
});
