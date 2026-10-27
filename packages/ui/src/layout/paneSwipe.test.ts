import { expect, it } from "vitest";

import { resolveSwipe, type SwipeScene } from "./paneSwipe";

const PROJECT = ["terminal", "files"] as const;

function scene(overrides: Partial<SwipeScene> = {}): SwipeScene {
  return { hasDrawer: true, drawerOpen: false, panelKinds: PROJECT, panel: undefined, lastPanel: undefined, ...overrides };
}

it("opens the drawer on a swipe towards the end when nothing is open", () => {
  expect(resolveSwipe("end", scene())).toEqual({ type: "open-drawer" });
});

it("closes the floating panel, rather than opening the drawer, on a swipe towards the end", () => {
  expect(resolveSwipe("end", scene({ panel: "files" }))).toEqual({ type: "close-panel" });
});

it("does nothing on a swipe towards the end with the drawer open or absent", () => {
  expect(resolveSwipe("end", scene({ drawerOpen: true }))).toBeUndefined();
  expect(resolveSwipe("end", scene({ hasDrawer: false }))).toBeUndefined();
});

it("closes the drawer on a swipe towards the start", () => {
  expect(resolveSwipe("start", scene({ drawerOpen: true }))).toEqual({ type: "close-drawer" });
});

it("opens the owner's first panel on a swipe towards the start when it has opened none", () => {
  expect(resolveSwipe("start", scene())).toEqual({ type: "open-panel", kind: "terminal" });
});

it("opens the panel the owner had open last, if it is one of its toggles", () => {
  expect(resolveSwipe("start", scene({ lastPanel: "files" }))).toEqual({ type: "open-panel", kind: "files" });
  expect(resolveSwipe("start", scene({ lastPanel: "report" }))).toEqual({ type: "open-panel", kind: "terminal" });
});

it("switches to the next panel in the order of the toggles, the last wrapping to the first", () => {
  expect(resolveSwipe("start", scene({ panel: "terminal" }))).toEqual({ type: "open-panel", kind: "files" });
  expect(resolveSwipe("start", scene({ panel: "files" }))).toEqual({ type: "open-panel", kind: "terminal" });
});

it("leaves the only panel of an owner as it is", () => {
  expect(resolveSwipe("start", scene({ panelKinds: ["report"], panel: "report" }))).toBeUndefined();
});

it("does nothing on a swipe towards the start with no owner", () => {
  expect(resolveSwipe("start", scene({ panelKinds: [] }))).toBeUndefined();
});
