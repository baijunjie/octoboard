import { Checkbox as HeroCheckbox, Label as HeroLabel } from "@heroui/react";
import React, { useRef } from "react";

import { useDisabled } from "./busy";
import { Description, type FieldVariant } from "./Field";
import { Tooltip, useOwnTooltipGuard } from "./Tooltip";
import { UNAVAILABLE_FADE, useInputAriaDisabled, useUnavailable } from "./unavailable";

/** HeroUI's checkbox, its box and `children` as its label, on while `selected`, told of presses by `onSelectedChange`.
 * `unavailable` freezes it and says it is unavailable (`aria-disabled`) rather than merely fixed: given the reason it
 * is also the box's tooltip and the checkbox's description, given `true` it has none; it keeps focus, its box is faded
 * (not the label) and it takes no press (see `./unavailable`). `tooltip` is its own tooltip while it is available; the
 * checkbox carries it itself, so the caller wraps it in no `Tooltip`. The tooltip and the touch popover belong to the
 * box, which takes the hover, not to the label text and not to the hidden input that takes focus. `description` is drawn under the label, indented with
 * it, and describes the checkbox ahead of `describedBy`. `contentClassName` is the class of the row holding the box
 * and the label (what a press lands on), `labelClassName` the label's. */
export function Checkbox({ selected, onSelectedChange, readOnly, unavailable, tooltip, tooltipPlacement, describedBy, dir, variant, className, contentClassName, labelClassName, description, children }: {
  selected: boolean;
  onSelectedChange: (selected: boolean) => void;
  /** Shows the state and takes no press, yet keeps the box focusable. */
  readOnly?: boolean;
  /** Why the checkbox is unavailable, or `true` for no reason; `undefined` or `""` when it is available. */
  unavailable?: React.ReactNode;
  /** The checkbox's own tooltip while it is available, which the reason replaces while it is unavailable. */
  tooltip?: React.ReactNode;
  tooltipPlacement?: "end" | "bottom";
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
  const root = useRef<HTMLDivElement>(null);
  const why = useUnavailable(unavailable);
  const frozen = why.frozen;
  useOwnTooltipGuard("Checkbox");
  useInputAriaDisabled(root, frozen);
  return (
    <HeroCheckbox
      ref={root}
      variant={variant}
      className={className}
      isSelected={selected}
      onChange={onSelectedChange}
      isDisabled={useDisabled()}
      isReadOnly={readOnly || frozen}
      aria-describedby={why.id ?? describedBy}
    >
      <HeroCheckbox.Content className={contentClassName}>
        <Tooltip trigger="box" content={why.reason ?? tooltip} placement={tooltipPlacement} label={typeof children === "string" ? children : undefined} root={root} touchPopover={why.reason !== undefined}>
          <HeroCheckbox.Control className={frozen ? UNAVAILABLE_FADE : undefined}>
            <HeroCheckbox.Indicator />
          </HeroCheckbox.Control>
        </Tooltip>
        <HeroLabel dir={dir} className={labelClassName}>{children}</HeroLabel>
      </HeroCheckbox.Content>
      {description !== undefined && <Description dir={dir}>{description}</Description>}
      {why.element}
    </HeroCheckbox>
  );
}
