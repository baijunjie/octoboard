import { act } from "react";
import { onTestFinished, vi } from "vitest";

/** One call of the Web Animations API: the element it plays on and its keyframes. */
export interface PlayedAnimation {
  element: Element;
  frames: Keyframe[];
}

export interface ListMotionStub {
  /** The collection motion's entries, exits and slides since the stub went in or was last cleared. */
  played: () => PlayedAnimation[];
  clear: () => void;
  /** Asks for reduced motion from now on, telling whoever listens for the change. */
  reduce: () => void;
}

/** Gives jsdom the `animate` the shared list motion plays its effects with, recording each call, and a
 * reduced-motion preference that can be turned on mid-test. jsdom lays nothing out, so rows enter and leave
 * but never slide unless the test also gives them positions. Install it before the collection mounts. */
export function stubListMotion({ reduced = false }: { reduced?: boolean } = {}): ListMotionStub {
  let reduce = reduced;
  const preference = new EventTarget();
  Object.defineProperty(preference, "matches", { get: () => reduce });
  const matchMedia = Object.getOwnPropertyDescriptor(globalThis, "matchMedia");
  vi.stubGlobal("matchMedia", (query: string) => (query.includes("prefers-reduced-motion") ? preference : { matches: false, media: query, addEventListener() {}, removeEventListener() {} }));
  const played: PlayedAnimation[] = [];
  // On `HTMLElement`, over any stand-in a test file already gives it (for a popover's exit, say), which comes back after.
  const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "animate");
  HTMLElement.prototype.animate = function (this: HTMLElement, frames: Keyframe[] | PropertyIndexedKeyframes | null) {
    played.push({ element: this, frames: frames as Keyframe[] });
    return { cancel() {}, finished: Promise.resolve(), effect: { getComputedTiming: () => ({ progress: 0 }) }, onfinish: null, oncancel: null } as unknown as Animation;
  };
  onTestFinished(() => {
    if (original) Object.defineProperty(HTMLElement.prototype, "animate", original);
    else delete (HTMLElement.prototype as Partial<HTMLElement>).animate;
    if (matchMedia) Object.defineProperty(globalThis, "matchMedia", matchMedia);
    else delete (globalThis as { matchMedia?: unknown }).matchMedia;
  });
  return {
    // Only the collection's own: other parts (a popover's entry) animate too.
    played: () => played.filter(({ element }) => element.matches("[data-list-motion], [data-flip], [data-list-motion-ghost]")),
    clear: () => { played.length = 0; },
    reduce: () => {
      reduce = true;
      act(() => void preference.dispatchEvent(new Event("change")));
    },
  };
}

/** Whether `frames` are a presence effect's: an entry (fading in) or an exit (fading out). */
export function presence(frames: Keyframe[]): "enter" | "exit" | undefined {
  if (frames[0]?.opacity === undefined) return undefined;
  return frames[0].opacity === 0 ? "enter" : "exit";
}
