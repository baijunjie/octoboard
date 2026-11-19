import type React from "react";

/** One row of a field and what stands beside it: another field, an icon button or a text button. The row centres them
 * on one line and spaces them `--field-row-gap` (8px) apart, the gap the app puts between controls in a row, so a
 * button never sits closer to its field than the next field would. The field grows and may shrink (the caller gives it
 * `min-w-0 flex-1`); a button beside it, icon or text, takes `besideField` (`Button`, `ToggleButton`, `IconToggle`),
 * which gives it the field's height at every width and the field's corners: square when icon-only, its own width
 * otherwise. `className` places and sizes the row in its
 * parent. See "Pointer target size" in `docs/conventions/ui-standard.md`. */
export function FieldRow({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  return <div className={`flex min-w-0 items-center gap-(--field-row-gap) ${className ?? ""}`}>{children}</div>;
}
