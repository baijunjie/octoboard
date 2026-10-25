// Shared by the tests of dialogs built on react-aria's `useDialog`, which arms a timer on mount that
// moves focus on its own.
import { act } from "react";
import { onTestFinished } from "vitest";

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
