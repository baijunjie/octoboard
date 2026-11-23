import { beforeEach, onTestFinished } from "vitest";

import { acknowledgeAllAlerts } from "./alertQueue";

// jsdom has no ResizeObserver, which HeroUI's ScrollShadow and `FadeOverflow` need to mount, and
// `Dialog` puts a ScrollShadow around every dialog's body. A test that has to measure installs its
// own with `vi.stubGlobal`, which replaces this one.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

// jsdom has no Web Animations API, which react-aria's tab indicator asks for as the chosen tab changes.
if (typeof Element !== "undefined") Element.prototype.getAnimations ??= () => [];

// jsdom lays nothing out and reports no client rects for any element, which `useListMotion` reads as a list that is not
// rendered. An element is taken as rendered unless it is disconnected, or it or an ancestor is `hidden` or has a
// computed `display: none`.
const notRendered = (start: Element | null): boolean => {
  for (let element = start; element; element = element.parentElement) {
    if (!element.isConnected || element.hasAttribute("hidden") || getComputedStyle(element).display === "none") return true;
  }
  return false;
};
if (typeof Element !== "undefined") {
  Element.prototype.getClientRects = function getClientRects(this: Element) {
    return (notRendered(this) ? [] : [{}]) as unknown as DOMRectList;
  };
}

// Register first so this runs after every afterEach and every later onTestFinished, including unmount and timer drain.
// The drain empties the queue only once the file itself has loaded it, and never loads it: in this hook its hundred-odd
// modules would be the first test's cost in every file, counted against that test's timeout; at the top of this file
// they would be loaded before the test file's mocks apply.
if (typeof document !== "undefined") {
  beforeEach(() => {
    onTestFinished(acknowledgeAllAlerts);
  });
}
