/** A device whose primary input cannot hover: a phone, or a tablet's touch screen. What depends on
 * the device's pointer rather than on one interaction keys on this query: no pane floats in on a
 * rest of the pointer, and controls otherwise revealed on hover are always shown (in CSS spelled
 * `[@media(hover:none)]:`, as Tailwind needs the literal). */
export const CANNOT_HOVER = "(hover: none)";

/** A field that takes text: focusing it raises a touch screen's keyboard. */
export const TEXT_ENTRY =
  'input:not([type=checkbox], [type=radio], [type=range], [type=button], [type=submit], [type=reset], [type=color], [type=file]), textarea, [contenteditable]:not([contenteditable="false"])';
