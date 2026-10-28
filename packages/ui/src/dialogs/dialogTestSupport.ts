// Shared by the tests of dialogs built on react-aria's `useDialog`, which arms a timer on mount that
// moves focus on its own, and by those of the focus a press without a pointer puts off to a frame.
import { setInteractionModality } from "react-aria";
import { act, createElement, useState } from "react";
import { onTestFinished, vi } from "vitest";

/** How long react-aria's `useDialog` waits after a dialog mounts before taking focus to the dialog
 * itself, when focus is then on the dialog or on `<body>` (a workaround for VoiceOver on iOS).
 * Taken from react-aria 3.52, `useDialog`; recheck it on an upgrade. */
const MOUNT_REFOCUS_MS = 500;

/**
 * Call right after a dialog mounts. Returns what a test awaits before a step whose focus it asserts,
 * so that the step cannot straddle `useDialog`'s mount timer.
 *
 * That timer moves focus on its own: focus passing through `<body>`, as it does from a closing
 * select's list on its way back to the trigger, is taken to the dialog instead, and a focused dialog
 * is blurred and, under the virtual clicks these tests make, focused again only a frame later.
 * Whether a step meets either depends on how fast the test ran up to it, so it fails only under
 * load. The timer armed here has the same delay and is armed later, so it fires after that one; the
 * frame after it is when the deferred refocus has run.
 */
export function trackMountRefocus(): () => Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const fired = new Promise<void>((resolve) => {
    timer = setTimeout(resolve, MOUNT_REFOCUS_MS);
  });
  onTestFinished(() => clearTimeout(timer));
  return () =>
    act(async () => {
      await fired;
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    });
}

/** The accessible name of a field named by `aria-labelledby`, read well enough for a label made of
 * text and `aria-hidden` parts: the text of each element it points to, leaving out what is
 * `aria-hidden`. Testing Library is not a dependency of this package. */
export function accessibleName(field: Element): string {
  const text = (node: Node): string => {
    if (node instanceof Element && node.getAttribute("aria-hidden") === "true") return "";
    if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? "";
    return Array.from(node.childNodes).map(text).join("");
  };
  const ids = field.getAttribute("aria-labelledby")?.split(/\s+/) ?? [];
  const named = ids.map((id) => (id === field.id ? field.getAttribute("aria-label") ?? "" : text(document.getElementById(id)!)));
  return named.join(" ").trim();
}

/**
 * Has each animation frame come as a 0 ms timer, so it runs ahead of any timer set after it was asked for, as a browser
 * may render one before the next timer task, and sets how react-aria reads the press that opens a dialog (`virtual`: an
 * assistive technology's, a script's `click()`, whose opening focus waits a frame). Both are put back when the test
 * finishes.
 */
export function framesAsTimers(modality: "virtual" | "pointer" | "keyboard" = "virtual"): void {
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => Number(setTimeout(() => callback(performance.now()), 0)));
  vi.stubGlobal("cancelAnimationFrame", (handle: number) => clearTimeout(handle));
  onTestFinished(() => { vi.unstubAllGlobals(); });
  act(() => setInteractionModality(modality));
  onTestFinished(() => { act(() => setInteractionModality("pointer")); });
}

/** Runs every frame and timer pending, and those they set in turn. Timers of one delay run in the order they were set,
 * so each round runs every frame asked for before it; ten rounds outlast the longest chain of frames that moves focus
 * in a dialog (its opening focus, its containment taking focus back on a blur a frame later, and that focus put off a
 * frame more). */
export async function settleFrames(): Promise<void> {
  for (let round = 0; round < 10; round += 1) await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
}

/** `dialog` as an element to mount, and `open`, which opens it. With `deferred` the dialog is not shown until `open` is
 * called, for a test to call it in one update with raising an alert or a question, the way a press that fails at once
 * does. */
export function openable(dialog: React.ReactElement, deferred = false): { element: React.ReactElement; open: () => void } {
  let open = (): void => {};
  function Opener() {
    const [shown, setShown] = useState(!deferred);
    open = () => setShown(true);
    return shown ? dialog : null;
  }
  return { element: createElement(Opener), open: () => open() };
}
