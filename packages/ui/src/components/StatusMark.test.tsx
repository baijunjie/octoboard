// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { StatusMark } from "./StatusMark";
import { HOVER_CLOSE_DELAY_MS, HOVER_OPEN_DELAY_MS } from "./Hint";
import { enter, leave, mountForTooltip, openTooltip, pointer, press as pointerPress, tap, withFakeTimers } from "./tooltipTestSupport";

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
  const button = mountForTooltip(<StatusMark label="Failed">Path not found.</StatusMark>).querySelector("button")!;
  expect(button.getAttribute("aria-label")).toBe("Failed");
  expect(button.textContent).toBe("");
  expect(button.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  expect(dialog()).toBeNull();
});

it("opens the reason and details in a popover named like the mark when pressed", () => {
  const button = mountForTooltip(
    <StatusMark label="Failed">
      <span>Path not found.</span>
      <pre>os error 2</pre>
    </StatusMark>,
  ).querySelector("button")!;
  press(button);
  expect(dialog()?.getAttribute("aria-label")).toBe("Failed");
  expect(dialog()?.textContent).toBe("Path not found.os error 2");
  expect(dialog()?.closest(".control-fills")).not.toBeNull();
  expect(button.getAttribute("aria-expanded")).toBe("true");
});

it("closes the popover on Escape and gives focus back to the mark", () => {
  const button = mountForTooltip(<StatusMark label="Failed">Path not found.</StatusMark>).querySelector("button")!;
  act(() => button.focus());
  press(button);
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(button);
});

it("takes no tab stop and is marked for the heading's walk when it stands in a collection's section header", () => {
  const plain = mountForTooltip(<StatusMark label="Failed">Path not found.</StatusMark>).querySelector("button")!;
  expect(plain.tabIndex).toBe(0);
  expect(plain.hasAttribute("data-section-heading-control")).toBe(false);
  const header = mountForTooltip(<StatusMark label="Failed" inHeader>Path not found.</StatusMark>).querySelector("button")!;
  expect(header.tabIndex).toBe(-1);
  expect(header.hasAttribute("data-section-heading-control")).toBe(true);
});

it("draws the circled exclamation for danger and the triangle for a warning", () => {
  const danger = mountForTooltip(<StatusMark label="Failed" />);
  expect(danger.querySelector("svg.lucide-circle-alert")).not.toBeNull();
  const warning = mountForTooltip(<StatusMark label="Stale" tone="warning" />);
  expect(warning.querySelector("svg.lucide-triangle-alert")).not.toBeNull();
  const note = mountForTooltip(<StatusMark label="Large" tone="note" />);
  expect(note.querySelector("svg.lucide-info")).not.toBeNull();
});

it("takes the size and baseline of the text only when it is inline", () => {
  const glyph = (mark: React.ReactElement) => mountForTooltip(mark).querySelector("svg")!.getAttribute("class") ?? "";
  expect(glyph(<StatusMark inline label="Failed" />)).toContain("size-3.5");
  expect(glyph(<StatusMark inline tone="note" label="Large" />)).toContain("size-3.5 translate-y-0.5");
  expect(glyph(<StatusMark label="Failed" />)).toContain("size-4");
  expect(glyph(<StatusMark inline label="Failed">Path not found.</StatusMark>)).toContain("size-3.5");
});

it("draws a visible word after the glyph inside the trigger, named by the label all the same", () => {
  const button = mountForTooltip(<StatusMark label="Website failed" text="Failed">Path not found.</StatusMark>).querySelector("button")!;
  expect(button.getAttribute("aria-label")).toBe("Website failed");
  expect(button.textContent).toBe("Failed");
  expect(button.firstElementChild?.getAttribute("class")).toContain("size-4");
  press(button);
  expect(dialog()?.getAttribute("aria-label")).toBe("Website failed");
  expect(dialog()?.textContent).toBe("Path not found.");
});

it("is the glyph and the word as plain text, with no control and no popover, when there is a word and nothing to open", () => {
  withFakeTimers();
  const container = mountForTooltip(<StatusMark label="Website failed" text="Failed" />);
  expect(container.querySelector("button")).toBeNull();
  const mark = container.querySelector('[role="img"]')!;
  expect(mark.getAttribute("aria-label")).toBe("Website failed");
  expect(mark.textContent).toBe("Failed");
  expect(mark.querySelector("svg")).not.toBeNull();
  enter(mark);
  hoverDelay();
  expect(dialog()).toBeNull();
  expect(openTooltip()).toBeNull();
});

it("ends the popover with the actions, which close it as they are pressed", () => {
  let pressed = 0;
  const button = mountForTooltip(
    <StatusMark label="Failed" actions={(close) => <button onClick={() => { pressed += 1; close(); }}>Try again</button>}>
      Path not found.
    </StatusMark>,
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
        <StatusMark label="Failed" inHeader>Path not found.</StatusMark>
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

const hoverDelay = () => act(() => void vi.advanceTimersByTime(HOVER_OPEN_DELAY_MS));
const graceDelay = () => act(() => void vi.advanceTimersByTime(HOVER_CLOSE_DELAY_MS));
const mark = (container: HTMLElement) => container.querySelector("button")!;

function mountFocused(element: React.ReactElement) {
  const container = mountForTooltip(<>{element}<button data-elsewhere>Elsewhere</button></>);
  const elsewhere = container.querySelector<HTMLButtonElement>("[data-elsewhere]")!;
  act(() => elsewhere.focus());
  return { container, elsewhere };
}

it("opens on a mouse hover after a delay without moving focus, as a non-modal dialog that takes part in the topmost Escape rule", () => {
  withFakeTimers();
  const { container, elsewhere } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  expect(dialog()).toBeNull();
  hoverDelay();
  expect(dialog()?.textContent).toBe("Path not found.");
  expect(dialog()?.getAttribute("aria-modal")).toBe("false");
  expect(dialog()?.getAttribute("aria-label")).toBe("Failed");
  expect(dialog()?.closest("[data-escape-topmost]")).not.toBeNull();
  expect(document.activeElement).toBe(elsewhere);
});

it("carries no tooltip on the mark, hovered or focused", () => {
  withFakeTimers();
  const { container } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  act(() => mark(container).focus());
  expect(openTooltip()).toBeNull();
});

it("does not open for a touch or pen hover", () => {
  withFakeTimers();
  const container = mountForTooltip(<StatusMark label="Failed">Path not found.</StatusMark>);
  for (const pointerType of ["touch", "pen"]) {
    const over = new MouseEvent("pointerover", { bubbles: true });
    Object.defineProperty(over, "pointerType", { value: pointerType });
    act(() => void mark(container).dispatchEvent(over));
  }
  hoverDelay();
  expect(dialog()).toBeNull();
});

it("closes a grace after the pointer leaves the mark, and stays open while it is inside the popover", () => {
  withFakeTimers();
  const { container, elsewhere } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  leave(mark(container));
  // Into the popover before the grace runs out.
  enter(dialog()!);
  graceDelay();
  expect(dialog()).not.toBeNull();
  leave(dialog()!);
  graceDelay();
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(elsewhere);
});

it("does not open if the pointer leaves before the delay", () => {
  withFakeTimers();
  const { container } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  leave(mark(container));
  hoverDelay();
  expect(dialog()).toBeNull();
});

it("closes a hover-opened popover on Escape without moving focus at any point", () => {
  withFakeTimers();
  const { container, elsewhere } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  const moves: string[] = [];
  const watch = (event: FocusEvent) => void moves.push(event.type);
  document.addEventListener("focusin", watch, true);
  document.addEventListener("focusout", watch, true);
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  act(() => void elsewhere.dispatchEvent(event));
  expect(event.defaultPrevented).toBe(true);
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(elsewhere);
  // Nor through the popover's exit and unmount.
  act(() => void vi.advanceTimersByTime(500));
  document.removeEventListener("focusin", watch, true);
  document.removeEventListener("focusout", watch, true);
  expect(document.activeElement).toBe(elsewhere);
  expect(moves).toEqual([]);
});

it("closes a hover-opened popover on a press outside it", () => {
  withFakeTimers();
  const { container, elsewhere } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  pointerPress(elsewhere, "mouse");
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(elsewhere);
});

it("turns a hover-opened popover into the press-opened one, modal and with focus inside, when the mark is tapped", () => {
  withFakeTimers();
  const { container } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  pointerPress(mark(container), "touch");
  expect(dialog()).not.toBeNull();
  expect(dialog()?.getAttribute("aria-modal")).not.toBe("false");
  expect(dialog()?.contains(document.activeElement)).toBe(true);
  expect(dialog()?.closest("[data-escape-topmost]")).toBeNull();
  // It no longer closes as the pointer moves off.
  leave(mark(container));
  graceDelay();
  expect(dialog()).not.toBeNull();
});

it("does nothing on a mouse click: a closed mark stays closed, a hover-opened popover stays as it is, a pending hover-open still happens", () => {
  withFakeTimers();
  const { container, elsewhere } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  pointerPress(mark(container), "mouse");
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(elsewhere);
  enter(mark(container));
  pointerPress(mark(container), "mouse");
  hoverDelay();
  expect(dialog()?.getAttribute("aria-modal")).toBe("false");
  pointerPress(mark(container), "mouse");
  expect(dialog()?.getAttribute("aria-modal")).toBe("false");
  expect(document.activeElement).toBe(elsewhere);
});

it.each(["touch", "pen"] as const)("opens on a %s press, with focus inside", (pointerType) => {
  withFakeTimers();
  mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  pointerPress(mark(document.body), pointerType);
  expect(dialog()?.contains(document.activeElement)).toBe(true);
  expect(openTooltip()).toBeNull();
});

it("opens on an assistive technology's activation, with focus inside", async () => {
  const container = mountForTooltip(<StatusMark label="Failed">Path not found.</StatusMark>);
  act(() => mark(container).click());
  // react-aria hands focus over once a transition has run for a virtual press.
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 50)); });
  expect(dialog()?.contains(document.activeElement)).toBe(true);
});

it("opens on Enter with focus inside", () => {
  const container = mountForTooltip(<StatusMark label="Failed">Path not found.</StatusMark>);
  act(() => mark(container).focus());
  tap(mark(container), "Enter");
  expect(dialog()?.contains(document.activeElement)).toBe(true);
});

it("gives a content-less mark the label as its one line, naming the popover, and is valid inside a paragraph", () => {
  withFakeTimers();
  const container = mountForTooltip(<p>Before <StatusMark inline label="Failed to read" /></p>);
  const button = mark(container);
  expect(button.closest("p")).not.toBeNull();
  expect(button.tabIndex).toBe(0);
  expect(button.getAttribute("aria-label")).toBe("Failed to read");
  enter(button);
  hoverDelay();
  expect(dialog()?.textContent).toBe("Failed to read");
  const name = dialog()!.getAttribute("aria-labelledby")!;
  expect(document.getElementById(name)?.textContent).toBe("Failed to read");
  expect(openTooltip()).toBeNull();
});

it("leaves focus alone when a hover-opened header mark closes", async () => {
  withFakeTimers();
  const container = mountForTooltip(
    <>
      <div role="row">
        <div data-section-heading tabIndex={-1}>Changes</div>
        <StatusMark label="Failed" inHeader>Path not found.</StatusMark>
      </div>
      <button data-outside>Elsewhere</button>
    </>,
  );
  const outside = container.querySelector<HTMLButtonElement>("[data-outside]")!;
  act(() => outside.focus());
  enter(container.querySelector("[role=row] button")!);
  hoverDelay();
  leave(container.querySelector("[role=row] button")!);
  graceDelay();
  expect(dialog()).toBeNull();
  await act(async () => void vi.advanceTimersByTime(100));
  expect(document.activeElement).toBe(outside);
});

it("is the tooltip wrapper for a note: a hover tooltip, no popover, no button", () => {
  withFakeTimers();
  const container = mountForTooltip(<StatusMark tone="note" label="Large file">Shown without highlighting.</StatusMark>);
  expect(container.querySelector("button")).toBeNull();
  const trigger = container.querySelector('[role="img"]')!;
  expect(trigger.getAttribute("aria-label")).toBe("Large file");
  enter(trigger);
  act(() => void vi.runOnlyPendingTimers());
  expect(openTooltip()?.textContent).toBe("Shown without highlighting.");
  expect(dialog()).toBeNull();
});

it("does not reopen under a pointer still resting on the mark after Escape, until it has left and come back", () => {
  withFakeTimers();
  const { container, elsewhere } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  act(() => void elsewhere.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  expect(dialog()).toBeNull();
  // The pointer has not moved: what the browser does as the popover goes is to report it over the mark again.
  leave(mark(container));
  enter(mark(container));
  hoverDelay();
  expect(dialog()).toBeNull();
  // It leaves, then comes back.
  leave(mark(container));
  act(() => void elsewhere.dispatchEvent(pointer("pointermove")));
  enter(mark(container));
  hoverDelay();
  expect(dialog()).not.toBeNull();
});

it("does not reopen under a resting pointer after the actions close a hover-opened popover either", () => {
  withFakeTimers();
  const container = mountForTooltip(<StatusMark label="Failed" actions={(close) => <button onClick={close}>Try again</button>}>Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  const retry = [...dialog()!.querySelectorAll("button")].find((candidate) => candidate.textContent === "Try again")!;
  pointerPress(retry, "mouse");
  expect(dialog()).toBeNull();
  leave(mark(container));
  enter(mark(container));
  hoverDelay();
  expect(dialog()).toBeNull();
});

it("gives focus back to the mark when a popover turned from hover to press closes, as for a press from scratch", async () => {
  withFakeTimers();
  const { container } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  // A mouse press focuses the mark as it goes down, before the popover turns.
  act(() => mark(container).focus());
  pointerPress(mark(container), "touch");
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await act(async () => void vi.advanceTimersByTime(100));
  expect(dialog()).toBeNull();
  expect(document.activeElement).toBe(mark(container));
});

it("gives focus to the heading's band when a header mark's popover turned from hover to press closes", async () => {
  withFakeTimers();
  const container = mountForTooltip(
    <div role="row">
      <div data-section-heading tabIndex={-1}>Changes</div>
      <StatusMark label="Failed" inHeader>Path not found.</StatusMark>
    </div>,
  );
  const band = container.querySelector<HTMLElement>("[data-section-heading]")!;
  enter(mark(container));
  hoverDelay();
  pointerPress(mark(container), "touch");
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await act(async () => void vi.advanceTimersByTime(100));
  expect(document.activeElement).toBe(band);
});

it("leaves a hover-opened popover's Escape to a terminal or a text field holding text, where focus is", () => {
  withFakeTimers();
  const container = mountForTooltip(
    <>
      <StatusMark label="Failed">Path not found.</StatusMark>
      <div className="xterm"><textarea data-terminal className="xterm-helper-textarea" /></div>
      <input data-field defaultValue="text" />
      <input data-empty />
    </>,
  );
  const escape = (target: Element) => {
    const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    act(() => void target.dispatchEvent(event));
    return event.defaultPrevented;
  };
  for (const [selector, taken] of [[".xterm-helper-textarea", false], ["[data-field]", false], ["[data-empty]", true]] as const) {
    enter(mark(container));
    hoverDelay();
    const target = container.querySelector(selector)!;
    act(() => (target as HTMLElement).focus());
    expect(escape(target)).toBe(taken);
    expect(dialog() === null).toBe(taken);
    // Let the pointer go, so the next round opens afresh.
    leave(mark(container));
    graceDelay();
    act(() => void container.dispatchEvent(pointer("pointermove")));
  }
});

it("leaves Escape alone while an input method is composing", () => {
  withFakeTimers();
  const { container, elsewhere } = mountFocused(<StatusMark label="Failed">Path not found.</StatusMark>);
  enter(mark(container));
  hoverDelay();
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true, isComposing: true });
  act(() => void elsewhere.dispatchEvent(event));
  expect(event.defaultPrevented).toBe(false);
  expect(dialog()).not.toBeNull();
});
