import type React from "react";

/** One row of a field and what stands beside it: another field, an icon button or a text button. The row centres them
 * on one line and spaces them `--field-row-gap` (8px) apart, the gap the app puts between controls in a row, so a
 * button never sits closer to its field than the next field would. The field grows and may shrink (the caller gives it
 * `min-w-0 flex-1`); a button beside it, icon or text, takes `besideField` (`Button`, `ToggleButton`, `IconToggle`),
 * which gives it the field's height at every width and the field's corners: square when icon-only, its own width
 * otherwise. `className` places and sizes the row in its parent.
 *
 * `labelled` is for a select whose label stands beside it (`OptionSelect`'s `besideLabel`), which draws its label and
 * trigger as two cells of its parent's grid: the row becomes that grid, the label's column sized to the label and the
 * trigger's taking the rest, and what follows the select takes a column of its own, sized to it, at the same gap. See
 * "Pointer target size" in `docs/conventions/ui-accessibility.md`. */
export function FieldRow({ labelled = false, className, children }: { labelled?: boolean; className?: string; children: React.ReactNode }): React.ReactElement {
  const layout = labelled ? "grid grid-cols-[auto_minmax(0,1fr)] grid-flow-col" : "flex";
  return <div className={`${layout} min-w-0 items-center gap-(--field-row-gap) ${className ?? ""}`}>{children}</div>;
}
