// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { InlinePath, pathText } from "./InlinePath";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function render(node: React.ReactElement): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(node));
  return container;
}

it("draws each step as a reference, in order, with a chevron between neighbours only", () => {
  const container = render(<InlinePath steps={["General", "Sharing", "Remote Login"]} />);
  const chips = [...container.querySelectorAll("[data-inline-reference]")];
  expect(chips.map((chip) => chip.textContent)).toEqual(["General", "Sharing", "Remote Login"]);
  for (const chip of chips) {
    expect(chip.getAttribute("dir")).toBe("auto");
    expect(chip.getAttribute("data-size")).toBe("xs");
  }
  expect(container.querySelectorAll("[data-inline-path-separator]")).toHaveLength(2);
});

it("hides the chevrons from assistive technology and mirrors them in a right-to-left interface", () => {
  const container = render(<InlinePath steps={["a", "b"]} />);
  const chevron = container.querySelector("[data-inline-path-separator]")!;
  expect(chevron.getAttribute("aria-hidden")).toBe("true");
  expect(chevron.getAttribute("class")).toContain("rtl:-scale-x-100");
});

it("reads as its steps joined by a separator, the same as the plain-text form", () => {
  const steps = ["System", "Optional features"];
  const container = render(<InlinePath steps={steps} />);
  expect(container.textContent).toBe("System > Optional features");
  expect(pathText(steps)).toBe("System > Optional features");
  expect(pathText(["Only"])).toBe("Only");
});

it("is one isolate that follows the interface's direction", () => {
  const container = render(<InlinePath steps={["x", "y"]} />);
  const path = container.querySelector<HTMLElement>("[data-inline-path]")!;
  expect(path.style.unicodeBidi).toBe("isolate");
  expect(path.hasAttribute("dir")).toBe(false);
});

it("draws a single step as one reference with no chevron, and nothing for no steps", () => {
  const single = render(<InlinePath steps={["Only"]} />);
  expect([...single.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual(["Only"]);
  expect(single.querySelector("[data-inline-path-separator]")).toBeNull();
  expect(single.textContent).toBe("Only");
  const none = render(<InlinePath steps={[]} />);
  expect(none.querySelector("[data-inline-reference]")).toBeNull();
  expect(none.textContent).toBe("");
});

it("hands the size and the tone to every step", () => {
  const container = render(<InlinePath steps={["a", "b", "c"]} size="md" color="danger" />);
  const chips = [...container.querySelectorAll("[data-inline-reference]")];
  expect(chips).toHaveLength(3);
  for (const chip of chips) {
    expect(chip.getAttribute("data-size")).toBe("md");
    expect(chip.getAttribute("data-color")).toBe("danger");
  }
});

it("keeps the steps in reading order in a right-to-left interface, each name taking its own direction", () => {
  const wrapper = document.body.appendChild(document.createElement("div"));
  wrapper.dir = "rtl";
  onTestFinished(() => wrapper.remove());
  const root = createRoot(wrapper);
  onTestFinished(() => { act(() => root.unmount()); });
  act(() => root.render(<InlinePath steps={["الإعدادات", "General"]} />));
  const chips = [...wrapper.querySelectorAll("[data-inline-reference]")];
  expect(chips.map((chip) => chip.textContent)).toEqual(["الإعدادات", "General"]);
  expect(chips.every((chip) => chip.getAttribute("dir") === "auto")).toBe(true);
  // The path itself sets no direction, so it follows the interface's, and isolates what it holds.
  const path = wrapper.querySelector<HTMLElement>("[data-inline-path]")!;
  expect(path.hasAttribute("dir")).toBe(false);
  expect(path.style.unicodeBidi).toBe("isolate");
  expect(wrapper.querySelectorAll("[data-inline-path-separator]")).toHaveLength(1);
});

it("lets the line break between steps and not inside the separator's text for readers", () => {
  const container = render(<InlinePath steps={["a", "b"]} />);
  // A break opportunity sits before each chevron; the text a reader gets carries the separator once.
  expect(container.querySelectorAll("[data-inline-path] > wbr")).toHaveLength(1);
  expect(container.querySelectorAll(".sr-only")).toHaveLength(1);
  expect(container.textContent).toBe("a > b");
});
