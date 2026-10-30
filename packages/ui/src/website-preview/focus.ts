import { isTabbable, TABBABLE } from "../components/tabbable";

// Whether the browser's own Tab order has another stop in this document beyond `from`. Outside an
// open modal everything is `inert`, so the modal's ends are the document's.
function hasTabStopBeyond(doc: Document, from: Element, backwards: boolean): boolean {
  const beyond = backwards ? Node.DOCUMENT_POSITION_PRECEDING : Node.DOCUMENT_POSITION_FOLLOWING;
  return Array.from(doc.querySelectorAll(TABBABLE)).some(
    (element) => element !== from && (from.compareDocumentPosition(element) & beyond) !== 0 && isTabbable(element),
  );
}

/**
 * Keep keyboard focus on the homepage until the visitor moves it into the embedded preview, and let
 * Tab carry it through and out again.
 *
 * A script's `focus()` inside a same-origin frame moves the page's focus into that frame, so the
 * application's own moves (the agent terminal focusing itself as it opens, a scripted step's press)
 * would pull a keyboard visitor from wherever they were on the page. While this document does not
 * hold focus they are dropped; once the visitor has tabbed or clicked in, they apply as usual. A
 * press in the frame gives it focus, as a click in any frame does: the application's controls cancel
 * the browser's own focusing on press so as to keep focus where it is, which would leave the frame
 * without focus and the press's own focus move dropped.
 *
 * Two of the application's Tab rules hold a keyboard visitor inside the frame, so Tab and Shift+Tab,
 * with or without Option (how Safari reaches every control by default), are kept from the
 * application there and move focus as they do anywhere else on the page:
 * - The terminals keep Tab for their program and leave F6 as the way out, which only cycles regions
 *   inside the frame and is unknown to a visitor.
 * - An open modal (the code review scene's diff viewer) wraps Tab around at its ends; there Tab
 *   leaves the frame instead. A menu or a list box the visitor opened keeps Tab as the application
 *   has it, Escape closing it.
 */
export function installPreviewFocusGuard(win: Window & typeof globalThis = window): () => void {
  if (win.parent === win) return () => {};

  const doc = win.document;
  const prototypes = [win.HTMLElement.prototype, win.SVGElement.prototype];
  const originals = prototypes.map((prototype) => prototype.focus);
  prototypes.forEach((prototype, index) => {
    const focus = originals[index]!;
    prototype.focus = function (this: HTMLElement | SVGElement, options?: FocusOptions) {
      if (doc.hasFocus()) focus.call(this, options);
    };
  });

  const onPointerDown = (event: PointerEvent) => {
    if (event.isTrusted && !doc.hasFocus()) win.focus();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const target = event.target;
    if (
      event.key !== "Tab" ||
      event.ctrlKey ||
      event.metaKey ||
      !(target instanceof win.Element)
    )
      return;
    if (
      target.closest(".xterm") ||
      (!target.closest('[role="menu"], [role="listbox"]') &&
        !hasTabStopBeyond(doc, target, event.shiftKey))
    )
      event.stopImmediatePropagation();
  };
  // Registered before the application loads, so they run ahead of every listener of the application's.
  win.addEventListener("pointerdown", onPointerDown, true);
  win.addEventListener("keydown", onKeyDown, true);

  return () => {
    prototypes.forEach((prototype, index) => {
      prototype.focus = originals[index]!;
    });
    win.removeEventListener("pointerdown", onPointerDown, true);
    win.removeEventListener("keydown", onKeyDown, true);
  };
}
