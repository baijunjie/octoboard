/** The sizes of the delegated glyph's ring and dot, which every drawing of it derives from, in CSS pixels at the
 * glyph's normal 16px slot: the hollow ring's outer diameter and the filled dot in its centre. The ring's line and
 * the slot itself are `WorkingDot`'s. */
export const DELEGATED_RING_PX = 10;
export const DELEGATED_DOT_PX = 4;

/** The slot the ring sits in; its ping grows by this over the ring, so the fading copy ends at the slot's edge as the
 * other dots' copies end at theirs (the stock ping's 2x would carry it past). */
const SLOT_PX = 16;
export const DELEGATED_PING_SCALE = SLOT_PX / DELEGATED_RING_PX;
