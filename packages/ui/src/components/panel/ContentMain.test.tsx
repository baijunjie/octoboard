// @vitest-environment jsdom
import { createRef } from "react";
import { expect, it } from "vitest";

import { mountHost } from "../../test/mountHost";
import { ContentMain } from "./ContentMain";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("renders the content area's one main landmark, positioned and flexible, casting the panel's shadow while a sidebar is docked", () => {
  const container = mountHost(<ContentMain>page</ContentMain>);
  const main = container.querySelector("main")!;
  expect(main.textContent).toBe("page");
  for (const name of ["relative", "flex", "min-w-0", "flex-1", "pane-docked:shadow-(--panel-shadow)"]) expect(main.classList.contains(name), name).toBe(true);
});

it("adds a page's own classes and passes its attributes and ref on, keeping the shared ones", () => {
  const ref = createRef<HTMLElement>();
  const container = mountHost(<ContentMain ref={ref} className="flex-col overflow-x-auto" data-region="devices" aria-label="Page">page</ContentMain>);
  const main = container.querySelector("main")!;
  expect(ref.current).toBe(main);
  expect(main.getAttribute("data-region")).toBe("devices");
  expect(main.getAttribute("aria-label")).toBe("Page");
  for (const name of ["flex-col", "overflow-x-auto", "relative", "pane-docked:shadow-(--panel-shadow)"]) expect(main.classList.contains(name), name).toBe(true);
});
