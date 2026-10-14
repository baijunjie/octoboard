// @vitest-environment jsdom
import { act, createRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { ProjectFilterButton, ProjectFilterTag } from "./ProjectFilter";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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

it("clears keyword and tags through the active heading button without opening a popover", () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const holderRef = createRef<HTMLDivElement>();
  function Filter() {
    const [filter, setFilter] = useState({ keyword: "site", tags: ["frontend"] });
    return <ProjectFilterButton filter={filter} vocabulary={["frontend"]} onChange={setFilter} holderRef={holderRef} />;
  }
  try {
    act(() => root.render(<Filter />));
    const clear = host.querySelector<HTMLButtonElement>("button")!;
    expect(clear.getAttribute("aria-label")).toBe("Clear filter");
    expect(clear.getAttribute("aria-haspopup")).toBe("false");
    clear.focus();
    act(() => clear.click());
    expect(host.querySelector("button")?.getAttribute("aria-label")).toBe("Filter projects");
    expect(host.querySelector("button")).toBe(clear);
    expect(document.activeElement).toBe(clear);
    expect(document.querySelector("[role=dialog]")).toBeNull();
  } finally {
    act(() => root.unmount());
    host.remove();
  }
});
