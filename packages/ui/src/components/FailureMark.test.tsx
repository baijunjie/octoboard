// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { FailureMark } from "./FailureMark";
import { enter, leave, mountForTooltip, openTooltip, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function press(element: HTMLElement) {
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      element.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

const dialog = () => document.querySelector('[role="dialog"]');

it("is a button named for what failed, with no sentence of its own", () => {
  const button = mountForTooltip(<FailureMark label="Failed">Path not found.</FailureMark>).querySelector("button")!;
  expect(button.getAttribute("aria-label")).toBe("Failed");
  expect(button.textContent).toBe("");
  expect(button.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  expect(dialog()).toBeNull();
});

it("opens the reason and details in a popover named like the mark when pressed", () => {
  const button = mountForTooltip(
    <FailureMark label="Failed">
      <span>Path not found.</span>
      <pre>os error 2</pre>
    </FailureMark>,
  ).querySelector("button")!;
  press(button);
  expect(dialog()?.getAttribute("aria-label")).toBe("Failed");
  expect(dialog()?.textContent).toBe("Path not found.os error 2");
  expect(dialog()?.closest(".control-fills")).not.toBeNull();
  expect(button.getAttribute("aria-expanded")).toBe("true");
});

it("closes the popover on Escape and gives focus back to the mark", () => {
  const button = mountForTooltip(<FailureMark label="Failed">Path not found.</FailureMark>).querySelector("button")!;
  act(() => button.focus());
  press(button);
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(button);
});

it("takes no tab stop and is marked for the heading's walk when it stands in a collection's section header", () => {
  const plain = mountForTooltip(<FailureMark label="Failed">Path not found.</FailureMark>).querySelector("button")!;
  expect(plain.tabIndex).toBe(0);
  expect(plain.hasAttribute("data-section-heading-control")).toBe(false);
  const header = mountForTooltip(<FailureMark label="Failed" inHeader>Path not found.</FailureMark>).querySelector("button")!;
  expect(header.tabIndex).toBe(-1);
  expect(header.hasAttribute("data-section-heading-control")).toBe(true);
});

it("is the named glyph alone, with no control, when there is nothing to open", () => {
  const container = mountForTooltip(<FailureMark label="Failed" />);
  expect(container.querySelector("button")).toBeNull();
  const mark = container.querySelector('[role="img"]')!;
  expect(mark.getAttribute("aria-label")).toBe("Failed");
  expect(mark.textContent).toBe("");
});

it("carries its name in a hover tooltip when it is the glyph alone, with no control and nothing for the keyboard", () => {
  const settle = withFakeTimers();
  const container = mountForTooltip(<p><FailureMark label="Failed" /></p>);
  const mark = container.querySelector('[role="img"]')!;
  expect(mark.closest("p")).not.toBeNull();
  expect(mark.tagName).toBe("SPAN");
  expect(mark.hasAttribute("tabindex")).toBe(false);
  expect(openTooltip()).toBeNull();
  enter(mark);
  settle();
  expect(openTooltip()?.textContent).toBe("Failed");
  leave(mark);
  expect(openTooltip()).toBeNull();
});

it("draws the circled exclamation for danger and the triangle for a warning", () => {
  const danger = mountForTooltip(<FailureMark label="Failed" />);
  expect(danger.querySelector("svg.lucide-circle-alert")).not.toBeNull();
  const warning = mountForTooltip(<FailureMark label="Stale" status="warning" />);
  expect(warning.querySelector("svg.lucide-triangle-alert")).not.toBeNull();
});

it("takes the size and baseline of the text only when it is the glyph alone and inline", () => {
  const glyph = (mark: React.ReactElement) => mountForTooltip(mark).querySelector("svg")!.getAttribute("class") ?? "";
  expect(glyph(<FailureMark inline label="Failed" />)).toContain("size-3.5 translate-y-0.5");
  expect(glyph(<FailureMark label="Failed" />)).toContain("size-4");
  expect(glyph(<FailureMark inline label="Failed">Path not found.</FailureMark>)).toContain("size-4");
});

it("draws a visible word after the glyph inside the trigger, named by the label all the same", () => {
  const button = mountForTooltip(<FailureMark label="Website failed" text="Failed">Path not found.</FailureMark>).querySelector("button")!;
  expect(button.getAttribute("aria-label")).toBe("Website failed");
  expect(button.textContent).toBe("Failed");
  expect(button.firstElementChild?.getAttribute("class")).toContain("size-4");
  press(button);
  expect(dialog()?.getAttribute("aria-label")).toBe("Website failed");
  expect(dialog()?.textContent).toBe("Path not found.");
});

it("is the glyph and the word as plain text, with no control and no tooltip, when there is nothing to open", () => {
  const container = mountForTooltip(<FailureMark label="Website failed" text="Failed" />);
  expect(container.querySelector("button")).toBeNull();
  const mark = container.querySelector('[role="img"]')!;
  expect(mark.getAttribute("aria-label")).toBe("Website failed");
  expect(mark.textContent).toBe("Failed");
  expect(mark.querySelector("svg")).not.toBeNull();
});

it("ends the popover with the actions, which close it as they are pressed", () => {
  let pressed = 0;
  const button = mountForTooltip(
    <FailureMark label="Failed" actions={(close) => <button onClick={() => { pressed += 1; close(); }}>Try again</button>}>
      Path not found.
    </FailureMark>,
  ).querySelector("button")!;
  press(button);
  const retry = [...dialog()!.querySelectorAll("button")].find((candidate) => candidate.textContent === "Try again")!;
  expect(dialog()!.lastElementChild?.contains(retry)).toBe(true);
  // At the trailing edge: `justify-end` follows the writing direction, so it is the left under RTL.
  expect(dialog()!.lastElementChild?.className).toContain("justify-end");
  press(retry);
  expect(pressed).toBe(1);
  expect(dialog()).toBeNull();
});

const frames = () => act(async () => { await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve(undefined)))); });

function mountHeader() {
  const container = mountForTooltip(
    <>
      <div role="row">
        <div data-section-heading tabIndex={-1}>Changes</div>
        <FailureMark label="Failed" inHeader>Path not found.</FailureMark>
      </div>
      <button data-outside>Elsewhere</button>
    </>,
  );
  return {
    band: container.querySelector<HTMLElement>("[data-section-heading]")!,
    mark: container.querySelector<HTMLButtonElement>("[role=row] button")!,
    outside: container.querySelector<HTMLButtonElement>("[data-outside]")!,
  };
}

it("gives focus to the heading's band when a header mark's popover closes by Escape", async () => {
  const { band, mark } = mountHeader();
  press(mark);
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await frames();
  expect(document.activeElement).toBe(band);
});

it("leaves focus on another control that a press outside the popover gave it", async () => {
  const { band, mark, outside } = mountHeader();
  press(mark);
  expect(dialog()).not.toBeNull();
  act(() => outside.focus());
  press(outside);
  expect(dialog()).toBeNull();
  await frames();
  expect(document.activeElement).toBe(outside);
  expect(document.activeElement).not.toBe(band);
});
