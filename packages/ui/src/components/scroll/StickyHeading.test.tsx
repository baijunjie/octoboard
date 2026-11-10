// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { StickyHeading } from "./StickyHeading";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(element: React.ReactElement): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(element));
  return container;
}

it("keeps category and subcategory headings at successive accessible levels with the supplied ids", () => {
  const view = mount(<>
    <StickyHeading variant="category" id="category" note="Processes stop with their session.">Session processes</StickyHeading>
    <StickyHeading variant="subcategory" id="session" offset={72}>Fix search</StickyHeading>
  </>);
  expect(view.querySelector("h3#category")?.textContent).toBe("Session processes");
  expect(view.querySelector("p")?.textContent).toBe("Processes stop with their session.");
  const session = view.querySelector("h4#session")!;
  expect(session.textContent).toBe("Fix search");
  expect(session.parentElement!.style.top).toBe("72px");
});

it("accepts a measured CSS offset for a subcategory", () => {
  const view = mount(<StickyHeading variant="subcategory" offset="var(--category-height)">Fix search</StickyHeading>);
  expect(view.querySelector("h4")!.parentElement!.style.top).toBe("var(--category-height)");
});

it("keeps list controls and inline content outside its heading and gives a settings group a heading", () => {
  const view = mount(<>
    <StickyHeading variant="list" id="projects" action={<button>New project</button>} inline={<span>Filtered</span>}>Projects</StickyHeading>
    <StickyHeading variant="group" id="general">General</StickyHeading>
  </>);
  expect(view.querySelector("h3#projects")?.textContent).toBe("Projects");
  expect(view.querySelector("button")?.textContent).toBe("New project");
  expect(view.querySelector("h3#projects")!.contains(view.querySelector("button"))).toBe(false);
  expect(view.textContent).toContain("Filtered");
  expect(view.querySelector("h3#general")?.textContent).toBe("General");
});
