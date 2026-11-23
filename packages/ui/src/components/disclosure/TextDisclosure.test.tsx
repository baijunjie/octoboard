// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { fakeTimers } from "../../test/clock";
import { mountHost } from "../../test/mountHost";
import { DisclosureTriangle, EXTENDED_TARGET, TextDisclosure } from "./TextDisclosure";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom loads no stylesheet and runs no layout, so what is asserted is the markup the stylesheet and the screen reader
// read: the button's name and state, the triangle's hiding, and the markers on the body.
function mount(ui: React.ReactElement): ReturnType<typeof mountHost> {
  // Before anything mounts: the frame HeroUI's overflow watch reports from is armed on the faked clock.
  fakeTimers();
  return mountHost(ui);
}

const trigger = (container: HTMLElement) => container.querySelector<HTMLButtonElement>("button[aria-expanded]")!;
const bodyOf = (container: HTMLElement) => container.querySelector<HTMLElement>(".disclosure__body")!;
const press = (container: HTMLElement) => act(async () => trigger(container).click());

it("is a button named by the label, its triangle drawn before the label and hidden from assistive technology", () => {
  const container = mount(<TextDisclosure label="More about this"><p>folded away</p></TextDisclosure>);
  const button = trigger(container);
  expect(button).not.toBeNull();
  expect(button.tagName).toBe("BUTTON");
  expect(button.textContent).toBe("More about this");
  const triangle = button.querySelector("svg");
  expect(triangle).not.toBeNull();
  expect(triangle!.getAttribute("aria-hidden")).toBe("true");
  expect(container.querySelectorAll("svg")).toHaveLength(1);
  // The triangle comes first in the button, ahead of the label.
  expect(button.firstElementChild).toBe(triangle);
  expect(button.lastElementChild?.textContent).toBe("More about this");
});

it("takes a node as its label", () => {
  const container = mount(<TextDisclosure label={<>Details of <b>one</b></>}><p>folded away</p></TextDisclosure>);
  expect(trigger(container).textContent).toBe("Details of one");
  expect(trigger(container).querySelector("b")?.textContent).toBe("one");
});

it("is collapsed to begin with, and opens and closes with the trigger", async () => {
  const container = mount(<TextDisclosure label="More"><p>folded away</p></TextDisclosure>);
  expect(trigger(container).getAttribute("aria-expanded")).toBe("false");
  await press(container);
  expect(trigger(container).getAttribute("aria-expanded")).toBe("true");
  await press(container);
  expect(trigger(container).getAttribute("aria-expanded")).toBe("false");
});

const contentOf = (container: HTMLElement) => container.querySelector<HTMLElement>("[data-slot=disclosure-content]")!;

it("keeps its content hidden until it is opened, and shows it once it is", async () => {
  const container = mount(<TextDisclosure label="More"><p>folded away</p></TextDisclosure>);
  expect(contentOf(container).hasAttribute("hidden")).toBe(true);
  expect(contentOf(container).getAttribute("aria-hidden")).toBe("true");
  await press(container);
  expect(contentOf(container).hasAttribute("hidden")).toBe(false);
  expect(contentOf(container).textContent).toBe("folded away");
  await press(container);
  expect(contentOf(container).hasAttribute("hidden")).toBe(true);
});

it("carries the expanded state the caller gives it, and tells it of each press", async () => {
  const onExpandedChange = vi.fn();
  const container = mount(<TextDisclosure label="More" expanded={false} onExpandedChange={onExpandedChange}><p>folded away</p></TextDisclosure>);
  expect(trigger(container).getAttribute("aria-expanded")).toBe("false");
  await press(container);
  expect(onExpandedChange).toHaveBeenCalledExactlyOnceWith(true);
  // Controlled: it stays closed until the caller says otherwise.
  expect(trigger(container).getAttribute("aria-expanded")).toBe("false");
  container.render(<TextDisclosure label="More" expanded onExpandedChange={onExpandedChange}><p>folded away</p></TextDisclosure>);
  expect(trigger(container).getAttribute("aria-expanded")).toBe("true");
  await press(container);
  expect(onExpandedChange).toHaveBeenLastCalledWith(false);
});

it("can begin expanded when the caller says so", () => {
  const container = mount(<TextDisclosure label="More" expanded><p>folded away</p></TextDisclosure>);
  expect(trigger(container).getAttribute("aria-expanded")).toBe("true");
});

it("indents its content past the triangle by default, and sets it flush with what stands above for a code block", () => {
  const text = mount(<TextDisclosure label="More" expanded><p>folded away</p></TextDisclosure>);
  expect(bodyOf(text).hasAttribute("data-indent")).toBe(true);
  expect(bodyOf(text).hasAttribute("data-flush")).toBe(true);
  const code = mount(<TextDisclosure label="More" code expanded><pre>folded away</pre></TextDisclosure>);
  expect(bodyOf(code).hasAttribute("data-indent")).toBe(false);
  expect(bodyOf(code).hasAttribute("data-flush")).toBe(true);
});

it("puts the trigger in a heading only when asked to", () => {
  const plain = mount(<TextDisclosure label="More"><p>folded away</p></TextDisclosure>);
  expect(trigger(plain).closest("h1, h2, h3, h4, h5, h6, [role=heading]")).toBeNull();
  const heading = mount(<TextDisclosure heading label="More"><p>folded away</p></TextDisclosure>);
  const around = trigger(heading).closest("h1, h2, h3, h4, h5, h6, [role=heading]");
  expect(around).not.toBeNull();
  expect(around!.textContent).toBe("More");
});

it("gives its trigger the id it is given, which names the content's group", () => {
  const container = mount(<TextDisclosure label="More" id="disclosure-trigger"><p>folded away</p></TextDisclosure>);
  expect(trigger(container).id).toBe("disclosure-trigger");
  expect(container.querySelectorAll("#disclosure-trigger")).toHaveLength(1);
  expect(contentOf(container).getAttribute("aria-labelledby")).toBe("disclosure-trigger");
});

it("has a trigger that is described by nothing unless a description is named", () => {
  const container = mount(<TextDisclosure label="More"><p>folded away</p></TextDisclosure>);
  expect(trigger(container).getAttribute("aria-describedby")).toBeNull();
});

it("has its trigger described by the element it names", () => {
  const container = mount(
    <>
      <span id="option-name">The option</span>
      <TextDisclosure label="More" describedBy="option-name"><p>folded away</p></TextDisclosure>
    </>,
  );
  expect(trigger(container).getAttribute("aria-describedby")).toBe("option-name");
  // The name stays the label's own; the description is not folded into it.
  expect(trigger(container).textContent).toBe("More");
});

it("puts the content's class on the body's inner element", () => {
  const container = mount(<TextDisclosure label="More" expanded contentClassName="inner-class"><p>folded away</p></TextDisclosure>);
  expect(bodyOf(container).querySelector(".inner-class")).not.toBeNull();
  expect(bodyOf(container).querySelector(".inner-class")!.textContent).toBe("folded away");
});

it("draws the triangle as two shapes, one shown while collapsed and the other while expanded, mirrored in a right-to-left layout", () => {
  const container = mount(<TextDisclosure label="More"><p>folded away</p></TextDisclosure>);
  const triangle = trigger(container).querySelector("svg")!;
  expect(triangle.classList.contains("rtl:-scale-x-100")).toBe(true);
  // Switched between, not one shape turned.
  expect(triangle.getAttribute("class")).not.toMatch(/rotate/);
  const [collapsed, expanded] = Array.from(triangle.querySelectorAll("path"));
  expect(triangle.querySelectorAll("path")).toHaveLength(2);
  expect(collapsed.getAttribute("d")).not.toBe(expanded.getAttribute("d"));
  // The collapsed shape goes away with the trigger's expanded state, and the expanded one appears with it.
  expect(collapsed.classList.contains("group-aria-expanded/disclosure:hidden")).toBe(true);
  expect(collapsed.classList.contains("group-open/disclosure:hidden")).toBe(true);
  expect(collapsed.classList.contains("hidden")).toBe(false);
  expect(expanded.classList.contains("hidden")).toBe(true);
  expect(expanded.classList.contains("group-aria-expanded/disclosure:block")).toBe(true);
  expect(expanded.classList.contains("group-open/disclosure:block")).toBe(true);
});

it("takes the class it is given on the triangle's svg, which stays hidden from assistive technology", () => {
  const container = mount(<DisclosureTriangle className="mt-1 self-start" />);
  const triangle = container.querySelector("svg")!;
  expect(triangle.classList.contains("mt-1")).toBe(true);
  expect(triangle.classList.contains("self-start")).toBe(true);
  expect(triangle.getAttribute("aria-hidden")).toBe("true");
});

it("carries the group the triangle's shapes switch on and an extended pointer target on its trigger", () => {
  const container = mount(<TextDisclosure label="More"><p>folded away</p></TextDisclosure>);
  for (const name of ["group/disclosure", ...EXTENDED_TARGET.split(" ")]) {
    expect(trigger(container).classList.contains(name), name).toBe(true);
  }
});
