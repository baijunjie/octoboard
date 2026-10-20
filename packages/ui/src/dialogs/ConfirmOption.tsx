import { Checkbox, Description, Label } from "@heroui/react";
import React from "react";

/** The checkbox, with its note, that a confirmation offers for widening what it does: moving
 * something outside Octoboard's own records to the Trash, or taking along what git refuses to
 * remove. Unchecked by default, by whoever holds its state. */
export function ConfirmOption({
  isSelected,
  onChange,
  isReadOnly,
  label,
  hint,
  describedBy,
}: {
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  /** Frozen while the request runs. */
  isReadOnly: boolean;
  label: React.ReactNode;
  hint: React.ReactNode;
  /** More that describes the option beside its note: the id of a line said elsewhere about it. */
  describedBy?: string;
}): React.ReactElement {
  return (
    <Checkbox variant="secondary" isSelected={isSelected} onChange={onChange} isReadOnly={isReadOnly} aria-describedby={describedBy}>
      <Checkbox.Content className="min-h-6">
        <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
        <Label>{label}</Label>
      </Checkbox.Content>
      <Description>{hint}</Description>
    </Checkbox>
  );
}
