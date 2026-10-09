// @vitest-environment jsdom
import { act, createRef } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { ProjectFilterTag } from "./ProjectFilter";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom has no ResizeObserver, which FadeOverflow needs to mount.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

it("keeps the keyword's tag mounted while the keyword is typed", () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const returnFocusTo = createRef<HTMLElement>();
  const render = (keyword: string) =>
    act(() => root.render(<ProjectFilterTag keyword={keyword} onRemove={() => {}} returnFocusTo={returnFocusTo} />));

  render("d");
  // react-aria throws when a collection item's id changes, as one keyed by the keyword would.
  expect(() => render("do")).not.toThrow();
  expect(host.textContent).toContain("do");

  act(() => root.unmount());
  host.remove();
});
