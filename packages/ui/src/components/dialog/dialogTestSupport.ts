// Shared by the tests of dialogs built on react-aria's `useDialog`, which arms a timer on mount that
// moves focus on its own, and by those of the focus a press without a pointer puts off to a frame.
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { act, createElement, useState } from "react";
import { onTestFinished, vi } from "vitest";
import { FAKE_FRAME_MS, flushPromises, framesFaked } from "../../test/clock";

/** Sends the input behind react-aria's `keyboard` or `pointer` modality (a hardware key, a mouse press) and sets the
 * modality, so a focus move nobody asked for, which follows the last input (`ringForAutomaticFocus`), does not depend
 * on what an earlier test left behind. Any other modality is only set. */
function sendInput(modality: ReturnType<typeof getInteractionModality>): void {
  if (modality === "keyboard") document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  if (modality === "pointer") {
    const press = { pointerType: "mouse", width: 1, height: 1, bubbles: true };
    document.dispatchEvent(new PointerEvent("pointerdown", press));
  }
  act(() => Reflect.apply(setInteractionModality, undefined, [modality]));
}

/** Sets react-aria's interaction modality for the test and restores the previous one when it finishes, even the
 * initial null, which react-aria accepts at runtime despite its setter's type. Call it before `fakeTimers()`, so the
 * restore runs after the clock's drain. The keyboard's and the pointer's modality also send the input that makes
 * them (see `sendInput`); that input stays after the test. */
export function withModality(modality: ReturnType<typeof getInteractionModality>): void {
  const previous = getInteractionModality();
  onTestFinished(() => { act(() => Reflect.apply(setInteractionModality, undefined, [previous])); });
  sendInput(modality);
}

/** How long react-aria's `useDialog` waits after a dialog mounts before taking focus to the dialog
 * itself, when focus is then on the dialog or on `<body>` (a workaround for VoiceOver on iOS).
 * Taken from react-aria 3.52, `useDialog`; recheck it on an upgrade. */
const MOUNT_REFOCUS_MS = 500;

/** Rounds `settleFrames` runs by default. */
const SETTLE_ROUNDS = 10;

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
 * assistive technology's, a script's `click()`, whose opening focus waits a frame; `pointer` and `keyboard` also send
 * that input, as `withModality` does). Both are put back when the test
 * finishes. A `fakeTimers()` installed after it replaces the frame stub with the fake clock's 16 ms frames; use
 * `fakeTimersKeepingFrames()` to keep it.
 */
export function framesAsTimers(modality: "virtual" | "pointer" | "keyboard" = "virtual"): void {
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) =>
    Number(setTimeout(() => callback(performance.now()), 0)));
  vi.stubGlobal("cancelAnimationFrame", (handle: number) => clearTimeout(handle));
  onTestFinished(() => { vi.unstubAllGlobals(); });
  sendInput(modality);
  onTestFinished(() => { act(() => setInteractionModality("pointer")); });
}

/**
 * Settles a bounded chain of animation frames, committing React work between rounds. With `framesAsTimers`, each
 * round runs zero-delay work; with Vitest's fake frames it advances one frame, including timers due before it.
 * A fake clock that leaves frames real also requires `framesAsTimers`; zero-delay advances drive its timer stubs.
 * On the real clock this requires `framesAsTimers`: zero-delay turns cannot reliably reach jsdom's native frames.
 * The default rounds outlast the opening focus, containment and deferred refocus chains in a dialog; callers can
 * request more rounds for a longer chain. Timers waiting beyond these rounds are left pending.
 */
export async function settleFrames(rounds = SETTLE_ROUNDS): Promise<void> {
  for (let round = 0; round < rounds; round += 1) {
    if (vi.isFakeTimers()) act(() => void vi.advanceTimersByTime(framesFaked() ? FAKE_FRAME_MS : 0));
    await flushPromises();
  }
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

/**
 * Holds every animation frame asked for from here to the end of the test, for a test that asserts focus stayed put
 * after it left a react-aria `FocusScope` with `contain`: the scope puts focus back one frame after the blur, so an
 * assertion made before that frame passes whether or not the scope would have taken focus back. Call it before the
 * blur; the stubs are removed when the test finishes.
 */
export function framesHeld(): void {
  vi.stubGlobal("requestAnimationFrame", () => 0);
  vi.stubGlobal("cancelAnimationFrame", () => {});
  onTestFinished(() => { vi.unstubAllGlobals(); });
}
