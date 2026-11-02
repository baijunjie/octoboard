/** How far the drawing of a lucide glyph whose ink is lopsided by design is moved on its 24-unit grid, so that it
 * reads centred in its slot: the speech bubble's tail and the hand's thumb pull the ink's weight to one side, though the ink's bounding box sits on the slot's centre. Each shift is half of the distance between the
 * ink's weighted centre and the slot's, which leaves the weighted centre and the bounding box's centre each within half a
 * pixel of the slot's centre at every size the glyph is drawn at (the gallery's rest pose, measured). The turning arc of starting
 * and switching account is not moved: its circle's centre stays on the slot's centre, so it turns in place. */
export type OpticalShift = readonly [x: number, y: number];

export const BUBBLE_SHIFT: OpticalShift = [0.3, -0.3];
export const HAND_SHIFT: OpticalShift = [-0.3, 0.2];

/** The `viewBox` of a lucide glyph moved by `shift`, for the glyph's `viewBox` prop: moving the viewport the other way
 * moves the drawing, and leaves the element's own `transform`, which the hand's wave animates, alone. */
export function shiftedViewBox([x, y]: OpticalShift): string {
  return `${-x} ${-y} 24 24`;
}

/** The same shift as a `transform`, for the menu bar's picture, which draws the glyph's paths into its own group. */
export function shiftedTransform([x, y]: OpticalShift): string {
  return `translate(${x} ${y})`;
}
