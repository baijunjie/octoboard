import { useLayoutEffect, useState, type RefObject } from "react";

/** The overlays of the mounted dialogs that were marked top layers as they mounted (`topLayerAtMount`), each with the
 * order in which its dialog was first rendered. */
const markedAtMount = new Map<Element, number>();
let renderedMarked = 0;

/**
 * Whether the overlay of the dialog `inside` is in should be marked a react-aria top layer: from its mount, when
 * `enabled`, to the next render, which comes in the same task, before any frame. Each modal hides the rest of the page
 * with `ariaHideOutside` from a passive effect, and two modals mounting in one commit would each hide the other, for
 * good, as no hider ever shows what another one hid; a hider leaves out what is marked when it runs, and keeps leaving it
 * out. React runs every passive effect of a commit, those `StrictMode` runs twice included, before it renders again, so
 * the marking lasts through the hiders of the mount commit and is gone for any modal that mounts later: that one hides
 * this dialog as usual and goes on top of it, so nothing in the covered dialog takes focus or presses meanwhile, and
 * what closes as its owner is hidden (a tooltip) closes. Of the dialogs marked in one commit only the one rendered last
 * keeps its marking, the one on top by its place in `<body>`, so that its own hider hides the others, as the hider of
 * the modal on top always does. The order is taken from the first render rather than from the layout effect, which
 * `StrictMode` runs a second time after those of the dialogs rendered after it.
 */
export function useTopLayerAtMount(enabled: boolean, inside: RefObject<Element | null>): boolean {
  const [order] = useState(() => (renderedMarked += 1));
  const [mountCommitDone, setMountCommitDone] = useState(false);
  useLayoutEffect(() => {
    if (!enabled) return;
    const overlay = inside.current?.closest("[data-react-aria-top-layer]");
    if (!overlay) return;
    for (const [other, otherOrder] of markedAtMount) {
      if (otherOrder < order) other.removeAttribute("data-react-aria-top-layer");
    }
    markedAtMount.set(overlay, order);
    setMountCommitDone(true);
    return () => { markedAtMount.delete(overlay); };
  }, []);
  return enabled && !mountCommitDone;
}
