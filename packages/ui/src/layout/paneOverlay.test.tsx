// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { ChromeButton } from "../components/ChromeButton";
import { AsidePane } from "./AsidePane";
import { PANE_ID } from "./paneOverlay";
import type { PanePeek } from "./usePaneToggles";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;
let container: HTMLElement | undefined;

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
  // Registered first, so it runs after the unmounts that `mount` and `mountToggle` register.
  onTestFinished(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    root = container = undefined;
  });
  // jsdom loads no stylesheet, so the token `peekGapPx` reads is declared here.
  document.documentElement.style.setProperty("--peek-gap", "8px");
});

/** Stubs the geometry of the rendered card: it will end up spanning x 1152..1432 and y 48..792 in a
 * 1440x800 window, an 8px gap in from the right edge, the 40px top bar and the bottom edge (no
 * connection banner), wherever it is on its way. */
function placePane(): HTMLElement {
  const pane = container!.querySelector<HTMLElement>("[data-pane=aside]")!;
  Object.defineProperties(pane, { offsetLeft: { value: 1152 }, offsetWidth: { value: 280 }, offsetTop: { value: 48 }, offsetHeight: { value: 744 } });
  // The entry is still 6px below its final position; its layout offsets describe the final halo.
  pane.getBoundingClientRect = () => ({ top: 54, bottom: 798, left: 1152, right: 1432 }) as DOMRect;
  return pane;
}

const SLIDE_AWAY_MS = 200;

/** `leave` arms a slide-away that `keep` cancels, as `usePaneToggles` has it, so a test can tell
 * whether the card would have gone (`hide`) rather than which method was called. */
function mount(active = false) {
  let slideAway: ReturnType<typeof setTimeout> | undefined;
  const hide = vi.fn();
  const peek = {
    active,
    reveal: vi.fn(),
    keep: vi.fn(() => clearTimeout(slideAway)),
    leave: vi.fn(() => {
      clearTimeout(slideAway);
      slideAway = setTimeout(() => hide(), SLIDE_AWAY_MS);
    }),
  } satisfies PanePeek;
  const mine = (container = document.body.appendChild(document.createElement("div")));
  const mounted = (root = createRoot(mine));
  onTestFinished(() => {
    act(() => mounted.unmount());
    mine.remove();
  });
  const render = (next: PanePeek) => act(() => root!.render(<AsidePane layout={{ open: false, width: 280, peek: next }} />));
  render(peek);
  return { peek, render, hide };
}

/** Mounts the pane with the real toggle button beside it, as the top bar renders it. */
function mountToggle() {
  const mounted = mount(true);
  const bar = document.body.appendChild(document.createElement("div"));
  const barRoot = createRoot(bar);
  onTestFinished(() => {
    act(() => barRoot.unmount());
    bar.remove();
  });
  act(() =>
    barRoot.render(
      <ChromeButton label="Toggle" onPress={() => {}} controls={PANE_ID.aside}>
        <svg />
      </ChromeButton>,
    ),
  );
  return { ...mounted, toggle: bar.querySelector("button")! };
}

const zone = () => container!.querySelector("[data-peek-zone=aside]")!;
const pointer = (type: string, init: MouseEventInit = {}) => {
  const event = new MouseEvent(type, { bubbles: true, ...init });
  Object.defineProperty(event, "pointerType", { value: "mouse" });
  return event;
};
const enter = () => act(() => void zone().dispatchEvent(pointer("pointerover")));
// From an element, as a browser does: the target is an `Element` the toggle rule has to look at.
const move = (clientX: number, clientY = 400) =>
  act(() => void document.body.dispatchEvent(pointer("pointermove", { clientX, clientY })));
// React raises `onPointerLeave` / `onPointerEnter` from `pointerout` / `pointerover`.
const leavePane = (pane: HTMLElement) => act(() => void pane.dispatchEvent(pointer("pointerout")));
const enterPane = (pane: HTMLElement) => act(() => void pane.dispatchEvent(pointer("pointerover")));
const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

it("floats the pane in only after the pointer has stayed for the dwell", () => {
  const { peek } = mount();
  enter();
  wait(199);
  expect(peek.reveal).not.toHaveBeenCalled();
  wait(1);
  expect(peek.reveal).toHaveBeenCalledTimes(1);
});

it("does not float the pane in when the pointer leaves the strip before the dwell is over", () => {
  const { peek } = mount();
  enter();
  wait(100);
  act(() => void zone().dispatchEvent(pointer("pointerout")));
  wait(300);
  expect(peek.reveal).not.toHaveBeenCalled();
});

it.each<[string, [x: number, y?: number][], number]>([
  ["counts a move outside the pane's final extent as leaving it, once", [[600], [700]], 1],
  ["counts a move above or below the pane as leaving it", [[1200, 20], [1200, 820]], 1],
  ["reports leaving again after a move back inside", [[1200], [700], [1200], [700]], 2],
  ["does not count a move in the gap around the card as leaving", [[1437]], 0],
  ["counts leaving through the gap around the card, never touching the card, as leaving", [[1437], [1436], [1436, 20]], 1],
])("%s", (_name, moves, expectedLeaves) => {
  const { peek } = mount(true);
  placePane();
  for (const [x, y] of moves) move(x, y);
  expect(peek.leave).toHaveBeenCalledTimes(expectedLeaves);
});

it("keeps the card when the pointer goes from it into the gap around it, whose pointerleave armed a slide-away", () => {
  const { hide } = mount(true);
  const pane = placePane();
  leavePane(pane);
  move(1435);
  wait(1000);
  expect(hide).not.toHaveBeenCalled();
});

it("marks the real toggle button with what it controls, as the toggle rule matches it", () => {
  const { toggle } = mountToggle();
  expect(toggle.getAttribute("data-pane-toggle")).toBe(PANE_ID.aside);
});

it("does not count a move on the aside's toggle as leaving, though the top bar is outside the card", () => {
  const { peek, hide, toggle } = mountToggle();
  placePane();
  const icon = toggle.firstElementChild!;
  for (const x of [1200, 1201]) act(() => void icon.dispatchEvent(pointer("pointermove", { clientX: x, clientY: 20 })));
  wait(1000);
  expect(peek.leave).not.toHaveBeenCalled();
  expect(hide).not.toHaveBeenCalled();
  // Off the toggle, in the same bar, the card is left.
  move(1100, 20);
  expect(peek.leave).toHaveBeenCalledTimes(1);
});

it("keeps watching after the pointer went outside and came back onto the card, and keeps it when it then goes into the gap", () => {
  const { hide } = mount(true);
  const pane = placePane();
  move(1200);
  move(700);
  // Back onto the card within the slide-away's delay.
  wait(SLIDE_AWAY_MS / 2);
  enterPane(pane);
  move(1200);
  leavePane(pane);
  move(1435);
  wait(1000);
  expect(hide).not.toHaveBeenCalled();
});

it("dismisses the card once the pointer moves from the gap to outside the card's extent", () => {
  const { hide } = mount(true);
  const pane = placePane();
  leavePane(pane);
  move(1435);
  move(1439, 20);
  wait(SLIDE_AWAY_MS);
  expect(hide).toHaveBeenCalledTimes(1);
});

it("counts the pointer leaving the window, through the gap or not, as leaving the pane", () => {
  const { peek, hide } = mount(true);
  placePane();
  move(1439);
  act(() => void document.documentElement.dispatchEvent(new MouseEvent("pointerleave")));
  expect(peek.leave).toHaveBeenCalledTimes(1);
  wait(SLIDE_AWAY_MS);
  expect(hide).toHaveBeenCalledTimes(1);
});

it("keeps a pane whose fade in left a pointerleave behind, with the pointer still", () => {
  const { hide } = mount(true);
  const pane = placePane();
  // The card's edge clears the pointer as it arrives, near the end of the 240ms fade.
  wait(230);
  leavePane(pane);
  wait(1000);
  expect(hide).not.toHaveBeenCalled();
});

it("stops watching the pointer once the pane is no longer floating", () => {
  const { peek, render } = mount(true);
  placePane();
  render({ ...peek, active: false });
  move(600);
  expect(peek.leave).not.toHaveBeenCalled();
});

it("stops watching the pointer on unmount", () => {
  const { peek } = mount(true);
  placePane();
  act(() => root!.unmount());
  move(600);
  expect(peek.leave).not.toHaveBeenCalled();
});
