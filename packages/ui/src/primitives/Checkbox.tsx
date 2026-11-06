import { Checkbox as HeroCheckbox, Label as HeroLabel } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import { Description, type FieldVariant } from "./Field";

/** HeroUI's checkbox, its box and `children` as its label, on while `selected`, told of presses by `onSelectedChange`.
 * `description` is drawn under the label, indented with it, and describes the checkbox ahead of `describedBy`.
 * `contentClassName` is the class of the row holding the box and the label (what a press lands on), `labelClassName`
 * the label's. */
export function Checkbox({ selected, onSelectedChange, describedBy, variant, className, contentClassName, labelClassName, description, children }: {
  selected: boolean;
  onSelectedChange: (selected: boolean) => void;
  describedBy?: string;
  variant?: FieldVariant;
  className?: string;
  contentClassName?: string;
  labelClassName?: string;
  description?: React.ReactNode;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroCheckbox
      variant={variant}
      className={className}
      isSelected={selected}
      onChange={onSelectedChange}
      isDisabled={useDisabled()}
      aria-describedby={describedBy}
    >
      <HeroCheckbox.Content className={contentClassName}>
        <HeroCheckbox.Control>
          <HeroCheckbox.Indicator />
        </HeroCheckbox.Control>
        <HeroLabel className={labelClassName}>{children}</HeroLabel>
      </HeroCheckbox.Content>
      {description !== undefined && <Description>{description}</Description>}
    </HeroCheckbox>
  );
}
