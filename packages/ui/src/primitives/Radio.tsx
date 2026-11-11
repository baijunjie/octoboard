import { Label as HeroLabel, Radio as HeroRadio, RadioGroup as HeroRadioGroup } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import { Description, type FieldVariant } from "./Field";

/** HeroUI's radio group: its `Radio`s, one of them chosen at a time, keyed by their `value`s, under `label`, which
 * `hideLabel` leaves off the screen while it still names the group. */
export function RadioGroup({ label, labelDir, labelClassName, hideLabel, value, onChange, readOnly, invalid, required, variant, className, children }: {
  label: React.ReactNode;
  /** `auto` for a label that is a page's or a user's text, not the app's. */
  labelDir?: "auto";
  labelClassName?: string;
  hideLabel?: boolean;
  /** `""` for none chosen. */
  value: string;
  onChange: (value: string) => void;
  /** Shows the choice and takes no change, yet keeps the group focusable. */
  readOnly?: boolean;
  /** Marks the group invalid, which shows the `FieldError` among its children. */
  invalid?: boolean;
  /** Says the group needs a choice (`aria-required`). HeroUI draws a required mark after the group's label and its
   * options' labels; the application draws none (`docs/ui-standard.md`, "Confirmed visual choices": report page fields draw no required
   * mark), so the group hides those marks. */
  required?: boolean;
  variant?: FieldVariant;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroRadioGroup variant={variant} value={value} onChange={onChange} isReadOnly={readOnly} isInvalid={invalid} isRequired={required} validationBehavior="aria" isDisabled={useDisabled()} className={[className, required && "[&_.label]:after:content-none"].filter(Boolean).join(" ") || undefined}>
      <HeroLabel dir={labelDir} className={hideLabel ? "sr-only" : labelClassName}>{label}</HeroLabel>
      {children}
    </HeroRadioGroup>
  );
}

/** One choice of a `RadioGroup`, keyed `value`: its circle and `children` as its label, with `description` under it,
 * describing it. */
export function Radio({ value, description, dir, children }: {
  value: string;
  description?: React.ReactNode;
  /** `auto` for a choice that is a page's or a user's text. */
  dir?: "auto";
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroRadio value={value}>
      <HeroRadio.Content>
        <HeroRadio.Control>
          <HeroRadio.Indicator />
        </HeroRadio.Control>
        <HeroLabel dir={dir}>{children}</HeroLabel>
      </HeroRadio.Content>
      {description !== undefined && <Description dir={dir}>{description}</Description>}
    </HeroRadio>
  );
}
