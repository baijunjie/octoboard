/** What react-aria's `ariaHideOutside` leaves out of what a modal hides: an element marked a top layer (the overlay of a
 * modal that must stay on top) or a live announcer (a region whose announcements stay heard while a modal is open),
 * with its ancestors. The attributes are react-aria's business, so they are named here only. */
const TOP_LAYER = "data-react-aria-top-layer";

/** Props that mark an element a top layer when `on`, for an element that stays in the page while a modal hides the
 * rest, as a modal's overlay does (`Modal`'s `topLayer`). */
export function topLayerProps(on: boolean | undefined): { "data-react-aria-top-layer"?: "true" } {
  return on ? { [TOP_LAYER]: "true" } : {};
}

/** The nearest element at or around `element` that is marked a top layer. */
export function closestTopLayer(element: Element | null | undefined): Element | null {
  return element?.closest(`[${TOP_LAYER}]`) ?? null;
}

/** Takes the top-layer mark off `element`. */
export function clearTopLayer(element: Element): void {
  element.removeAttribute(TOP_LAYER);
}

/** Props that mark an element a live announcer, which the modal's hiding of the page leaves exposed to assistive
 * technology, with its ancestors, while a modal is open. */
export function liveAnnouncerProps(): { "data-live-announcer": "true" } {
  return { "data-live-announcer": "true" };
}
