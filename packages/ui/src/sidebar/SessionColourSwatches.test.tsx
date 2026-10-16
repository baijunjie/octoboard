// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { mountForTooltip } from "../components/tooltipTestSupport";
import { CONSOLE_SESSION_COLOURS, type ConsoleSessionColour } from "../protocol";
import { ThemeProvider } from "../theme";
import { SessionColourSwatches } from "./SessionColourSwatches";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(colour: ConsoleSessionColour | null = "teal") {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => void vi.unstubAllGlobals());
  const onChoose = vi.fn();
  const onClose = vi.fn();
  mountForTooltip(
    <ThemeProvider>
      <SessionColourSwatches colour={colour} onChoose={onChoose} onClose={onClose} labelledBy={undefined} autoFocus />
    </ThemeProvider>,
  );
  const options = Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]'));
  return { options, onChoose, onClose };
}

const key = (target: Element, name: string) => act(() => void target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true })));

it("lists the palette, names every swatch from its colour alone and focuses the current one", () => {
  const { options } = mount();
  expect(options).toHaveLength(CONSOLE_SESSION_COLOURS.length);
  // A colour name from react-aria, in the user's language: the catalog has none of its own.
  for (const option of options) expect(option.querySelector('[role="img"]')!.getAttribute("aria-label")).toMatch(/\w/);
  const current = options[CONSOLE_SESSION_COLOURS.indexOf("teal")];
  expect(options.filter((option) => option.getAttribute("aria-selected") === "true")).toEqual([current]);
  expect(document.activeElement).toBe(current);
});

it("applies a swatch at once, whether clicked or chosen with Enter on the focused one", () => {
  const { options, onChoose, onClose } = mount();
  act(() => options[CONSOLE_SESSION_COLOURS.indexOf("red")].click());
  expect(onChoose).toHaveBeenCalledExactlyOnceWith("red");

  act(() => options[CONSOLE_SESSION_COLOURS.indexOf("gray")].focus());
  key(options[CONSOLE_SESSION_COLOURS.indexOf("gray")], "Enter");
  expect(onChoose).toHaveBeenLastCalledWith("gray");
  expect(onChoose).toHaveBeenCalledTimes(2);
  expect(onClose).not.toHaveBeenCalled();
});

it("only moves focus with the arrow keys, closes on the current swatch, and changes nothing", () => {
  const { options, onChoose, onClose } = mount();
  const current = options[CONSOLE_SESSION_COLOURS.indexOf("teal")];
  key(current, "ArrowRight");
  expect(document.activeElement).toBe(options[CONSOLE_SESSION_COLOURS.indexOf("teal") + 1]);
  expect(onChoose).not.toHaveBeenCalled();

  act(() => current.click());
  expect(onClose).toHaveBeenCalledOnce();
  expect(onChoose).not.toHaveBeenCalled();
});

// Under virtual modality react-aria focuses the swatch only after the list box has mounted.
const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 100)));

it("keeps focus on a swatch, not the list box, when opened by a virtual click", async () => {
  setInteractionModality("virtual");
  const { options } = mount();
  await settle();
  expect(document.activeElement).toBe(options[CONSOLE_SESSION_COLOURS.indexOf("teal")]);
});

it("focuses the first swatch when there is no current colour", async () => {
  setInteractionModality("virtual");
  const { options } = mount(null);
  await settle();
  expect(document.activeElement).toBe(options[0]);
});
