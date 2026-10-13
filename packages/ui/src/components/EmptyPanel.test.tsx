// @vitest-environment jsdom
import { FolderOpen } from "lucide-react";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";

import { EmptyPanel } from "./EmptyPanel";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const unmounts: Array<() => void> = [];

afterEach(() => {
  for (const unmount of unmounts.splice(0)) act(unmount);
  document.body.replaceChildren();
});

function mount(element: React.ReactElement): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  unmounts.push(() => root.unmount());
  act(() => root.render(element));
  return container;
}

/** Mounts a compact row that is itself the button filling the list, and opens its tooltip the way a
 * keyboard user does, by focusing it after a key press. */
function mountCompactAction(): HTMLButtonElement {
  const container = mount(
    <EmptyPanel compact icon={FolderOpen} message="No projects yet" action={{ label: "Add project", onPress: () => {} }} />,
  );
  const button = container.querySelector("button");
  if (!button) throw new Error("no button rendered");
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    button.focus();
  });
  return button;
}

const nameOf = (el: HTMLElement) =>
  (el.getAttribute("aria-labelledby") ?? "")
    .split(" ")
    .map((id) => document.getElementById(id)?.textContent)
    .join(" ");

it("names a compact row by its visible message and then by its action", () => {
  expect(nameOf(mountCompactAction())).toBe("No projects yet Add project");
});

it("does not describe a compact row by the tooltip its name already ends with", () => {
  const button = mountCompactAction();
  expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("Add project");
  expect(button.getAttribute("aria-describedby")).toBeFalsy();
});

it("hides a silent panel from assistive technology", () => {
  const container = mount(<EmptyPanel icon={FolderOpen} message="Gone" silent />);
  expect(container.firstElementChild?.getAttribute("aria-hidden")).toBe("true");
});

it("leaves a panel visible to assistive technology unless it is silent", () => {
  const container = mount(<EmptyPanel icon={FolderOpen} message="Here" />);
  expect(container.firstElementChild?.hasAttribute("aria-hidden")).toBe(false);
});

it("renders a node as the description", () => {
  const container = mount(<EmptyPanel icon={FolderOpen} message="Binary" description={<span dir="ltr"><bdi>1 KB</bdi> → <bdi>2 KB</bdi></span>} />);
  expect(container.querySelector("h2")?.textContent).toBe("Binary");
  expect(container.querySelector("p [dir=ltr]")?.textContent).toBe("1 KB → 2 KB");
});

it("renders the message as a heading when there is no description", () => {
  const container = mount(<EmptyPanel icon={FolderOpen} message="Nothing here" />);
  expect(container.querySelector("h2")?.textContent).toBe("Nothing here");
});

it("renders the heading at the level asked for", () => {
  const container = mount(<EmptyPanel icon={FolderOpen} message="Nothing here" headingLevel={3} />);
  expect(container.querySelector("h3")?.textContent).toBe("Nothing here");
  expect(container.querySelector("h2")).toBeNull();
});
