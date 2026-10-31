/** The base classes of a veil laid over a row to dim it: the row's `::after`, which the caller colours with a
 * `data-…:after:bg-<surface>/55` variant of its own, on its surface's colour. Plain paint that lets every
 * pointer event through; it fades in and out, and holds still under reduced motion. Not `opacity`: under a
 * translucent ancestor WebKit paints descendants that fade or sit on a GPU layer as blank tiles. */
export const INACTIVE_VEIL_CLASS = "relative after:pointer-events-none after:absolute after:inset-0 after:transition-colors after:duration-200 motion-reduce:after:transition-none";
