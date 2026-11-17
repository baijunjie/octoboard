/** The file viewer's image zoom, as numbers: a scale is CSS pixels per image pixel, so 1 is the image's actual size. */

/** How far an image can be zoomed in: far enough to tell single pixels of a screenshot apart. */
export const MAX_SCALE = 32;

/** The longest side, in CSS pixels, an image is drawn at: zooming a large image further would only make an element
 * browsers draw slowly or not at all, for detail already visible at that size. */
const MAX_DRAWN_SIDE = 65536;

/** The levels Zoom in and Zoom out step through, the familiar stops of a system image viewer. The fitted scale is a
 * stop too, wherever it falls between them (`stepScale`). */
const LEVELS = [0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8, 12, 16, 24, 32];

/** The least factor one step of Zoom in or Zoom out changes the scale by, short of the bounds. */
const MIN_STEP = 1.1;

/** How the image is drawn: fitted to the view, following the view's size as it changes, or at a scale the user chose. */
export type ImageZoom = { mode: "fit" } | { mode: "scale"; scale: number };

export const FIT: ImageZoom = { mode: "fit" };

export interface Size {
  width: number;
  height: number;
}

/** The scale that fits the whole image in `box`, never above actual size: an image smaller than the view opens at its
 * own size. An empty box (not laid out yet) counts as fitting. */
export function fitScale(natural: Size, box: Size): number {
  if (natural.width <= 0 || natural.height <= 0 || box.width <= 0 || box.height <= 0) return 1;
  return Math.min(1, box.width / natural.width, box.height / natural.height);
}

/** The bounds of zooming one image in one view: the fitted scale, which is also the smallest, and the largest. */
export interface ZoomBounds {
  fit: number;
  max: number;
}

/** The bounds for an image of `natural` size in `box`: never past `MAX_SCALE`, nor past `MAX_DRAWN_SIDE`, but always
 * at least actual size. */
export function zoomBounds(natural: Size, box: Size): ZoomBounds {
  const fit = fitScale(natural, box);
  const side = Math.max(natural.width, natural.height, 1);
  return { fit, max: Math.max(1, fit, Math.min(MAX_SCALE, MAX_DRAWN_SIDE / side)) };
}

/** The scale `zoom` draws at, given the fitted one. */
export function scaleOf(zoom: ImageZoom, fit: number): number {
  return zoom.mode === "fit" ? fit : zoom.scale;
}

/** `scale` held within `bounds`, the smallest being the fitted scale: zooming out further only leaves the image
 * smaller in an emptier view. Landing on the fitted scale (within rounding) is fitting, so the image keeps fitting as
 * the view is resized. */
export function zoomTo(scale: number, { fit, max }: ZoomBounds): ImageZoom {
  const held = Math.min(max, Math.max(fit, scale));
  return Math.abs(held - fit) < 1e-6 * fit ? FIT : { mode: "scale", scale: held };
}

/** One step of Zoom in or Zoom out from `scale`: the next level of `LEVELS`, or the fitted scale, in that direction. */
export function stepScale(scale: number, direction: 1 | -1, bounds: ZoomBounds): ImageZoom {
  const stops = [...LEVELS, bounds.fit, bounds.max].sort((a, b) => a - b);
  // A stop less than `MIN_STEP` away is passed over, so a press always visibly zooms: a fitted 31% steps to 50%, not
  // to the 33% beside it, and rounding never makes a step land where it started.
  const next = direction > 0 ? stops.find((stop) => stop >= scale * MIN_STEP) : [...stops].reverse().find((stop) => stop <= scale / MIN_STEP);
  return zoomTo(next ?? (direction > 0 ? bounds.max : bounds.fit), bounds);
}

/** Whether Zoom in or Zoom out has anywhere to go from `scale`. */
export function canStep(scale: number, direction: 1 | -1, { fit, max }: ZoomBounds): boolean {
  return direction > 0 ? scale < max / 1.001 : scale > fit * 1.001;
}

/** How much one wheel event zooms by. A mouse wheel's notch arrives as one large delta and a trackpad pinch as a run
 * of small ones; each event's delta is capped so a notch is one moderate step, while a pinch follows the fingers. */
export function wheelFactor(event: Pick<WheelEvent, "deltaY" | "deltaMode">): number {
  const pixels = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 400 : 1);
  const capped = Math.max(-20, Math.min(20, pixels));
  return Math.exp(-capped * 0.01);
}

/** Where the scroller must scroll so the image point that was under `anchor` (a point in the scroller's client area)
 * stays under it once the image is drawn at `next` instead of `previous`. `imageOffset` is the image's offset in the
 * scroller's content, before and after: the image is centred while it is smaller than the view. */
export function anchoredScroll({
  scroll,
  anchor,
  previous,
  next,
  imageOffsetBefore,
  imageOffsetAfter,
}: {
  scroll: { left: number; top: number };
  anchor: { x: number; y: number };
  previous: number;
  next: number;
  imageOffsetBefore: { left: number; top: number };
  imageOffsetAfter: { left: number; top: number };
}): { left: number; top: number } {
  const imageX = (scroll.left + anchor.x - imageOffsetBefore.left) / previous;
  const imageY = (scroll.top + anchor.y - imageOffsetBefore.top) / previous;
  return { left: imageX * next + imageOffsetAfter.left - anchor.x, top: imageY * next + imageOffsetAfter.top - anchor.y };
}

/** The zoom command a key press is, held with the platform's shortcut modifier: in (⌘= or ⌘+), out (⌘-), or back to
 * fitted (⌘0). The keys are matched by the character they type as well as by their position, since `+` and `-` stand
 * elsewhere on many layouts; Shift is let through, as `+` needs it on a US keyboard. */
export function zoomKey(event: Pick<KeyboardEvent, "key" | "code" | "altKey">): "in" | "out" | "fit" | undefined {
  if (event.altKey) return undefined;
  if (event.key === "+" || event.key === "=" || event.code === "NumpadAdd") return "in";
  if (event.key === "-" || event.key === "_" || event.code === "NumpadSubtract") return "out";
  if (event.key === "0" || event.code === "Digit0" || event.code === "Numpad0") return "fit";
  if (event.code === "Equal") return "in";
  if (event.code === "Minus") return "out";
  return undefined;
}

/** A scale as the percentage the zoom bar shows and the announcement speaks, in `language`'s format. */
export function formatPercent(language: string, scale: number): string {
  return new Intl.NumberFormat(language, { style: "percent", maximumFractionDigits: 0 }).format(scale);
}
