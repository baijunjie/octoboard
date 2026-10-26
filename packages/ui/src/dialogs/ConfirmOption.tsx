import { Checkbox, Description, Label } from "@heroui/react";
import React, { useId } from "react";

/** A dialog's option checkbox with its note: what a confirmation offers for widening what it does
 * (moving something outside Octoboard's own records to the Trash, or taking along what git refuses
 * to remove), or a choice that changes what a dialog produces. Its default is set by whoever holds
 * its state. */
export function ConfirmOption({
  isSelected,
  onChange,
  isReadOnly,
  label,
  hint,
  mark,
}: {
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  /** Frozen while the request runs. */
  isReadOnly: boolean;
  label: React.ReactNode;
  hint: React.ReactNode;
  /** A mark beside the checkbox, for a warning about the option itself. It stays outside the checkbox's label, so it is
   * no interactive content inside a label and does not become part of the checkbox's name. */
  mark?: React.ReactNode;
}): React.ReactElement {
  const hintId = useId();
  // The row is there with or without a mark, so a mark appearing does not remount the checkbox under focus. The
  // checkbox is only as wide as its label, so the mark follows the label's text; the note sits under the row and is
  // the checkbox's description. The mark's box is as tall as the label's line, so it lines up on it.
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-start gap-1">
        <Checkbox variant="secondary" className="w-fit min-w-0" isSelected={isSelected} onChange={onChange} isReadOnly={isReadOnly} aria-describedby={hintId}>
          <Checkbox.Content className="min-h-6">
            <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
            <Label>{label}</Label>
          </Checkbox.Content>
        </Checkbox>
        {mark && <span className="flex min-h-6 items-center">{mark}</span>}
      </div>
      <Description id={hintId} className="ps-7">{hint}</Description>
    </div>
  );
}
