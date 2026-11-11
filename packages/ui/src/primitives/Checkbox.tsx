import { Checkbox as HeroCheckbox, Label as HeroLabel } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import { Description, type FieldVariant } from "./Field";

/** HeroUI's checkbox, its box and `children` as its label, on while `selected`, told of presses by `onSelectedChange`.
 * `description` is drawn under the label, indented with it, and describes the checkbox ahead of `describedBy`.
 * `contentClassName` is the class of the row holding the box and the label (what a press lands on), `labelClassName`
 * the label's. */
export function Checkbox({ selected, onSelectedChange, readOnly, describedBy, dir, variant, className, contentClassName, labelClassName, description, children }: {
  selected: boolean;
  onSelectedChange: (selected: boolean) => void;
  /** Shows the state and takes no press, yet keeps the box focusable. */
  readOnly?: boolean;
  describedBy?: string;
  /** `auto` for a label that is a page's or a user's text. */
  dir?: "auto";
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
      isReadOnly={readOnly}
      aria-describedby={describedBy}
    >
      <HeroCheckbox.Content className={contentClassName}>
        <HeroCheckbox.Control>
          <HeroCheckbox.Indicator />
        </HeroCheckbox.Control>
        <HeroLabel dir={dir} className={labelClassName}>{children}</HeroLabel>
      </HeroCheckbox.Content>
      {description !== undefined && <Description dir={dir}>{description}</Description>}
    </HeroCheckbox>
  );
}
