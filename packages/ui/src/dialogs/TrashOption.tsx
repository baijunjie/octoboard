import { Checkbox, Description, Label } from "@heroui/react";
import React from "react";

/** The checkbox, with its note, that a deletion's confirmation offers for also moving something
 * outside Octoboard's own records to the Trash. Unchecked by default, by whoever holds its state. */
export function TrashOption({
  isSelected,
  onChange,
  isReadOnly,
  label,
  hint,
}: {
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  /** Frozen while the request runs. */
  isReadOnly: boolean;
  label: string;
  hint: string;
}): React.ReactElement {
  return (
    <Checkbox variant="secondary" isSelected={isSelected} onChange={onChange} isReadOnly={isReadOnly}>
      <Checkbox.Content className="min-h-6">
        <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
        <Label>{label}</Label>
      </Checkbox.Content>
      <Description>{hint}</Description>
    </Checkbox>
  );
}
