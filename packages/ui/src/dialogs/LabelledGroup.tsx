import { Description, Label } from "@heroui/react";
import React, { useId } from "react";

/** A visible label over a group of controls that read as one field, named for assistive technology
 * as the group, with an optional `description` under them that the group is described by. HeroUI's
 * `Fieldset` is not used: its legend is drawn a size up from `Label` and flush against what follows
 * it, and a native `<fieldset>` lays its legend out by rules of its own rather than as a flex item,
 * so matching the labels of the fields beside it would take an override on the root and on the
 * legend alike — more than the `div`, `Label` and `Description` this assembles. HeroUI dims a
 * field's label through `[data-disabled=true] .label`, which a label outside the controls never
 * sits under, so `isDisabled` marks the group `div` for it. */
export function LabelledGroup({
  label,
  description,
  isDisabled,
  children,
}: {
  label: string;
  description?: string;
  isDisabled?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  const labelId = useId();
  const descriptionId = useId();
  return (
    <div
      role="group"
      aria-labelledby={labelId}
      aria-describedby={description ? descriptionId : undefined}
      data-disabled={isDisabled || undefined}
      className="flex flex-col gap-1.5"
    >
      <Label id={labelId}>{label}</Label>
      {children}
      {description && <Description id={descriptionId}>{description}</Description>}
    </div>
  );
}
