import { Label as HeroLabel, Radio as HeroRadio, RadioGroup as HeroRadioGroup } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import { Description, type FieldVariant } from "./Field";

/** HeroUI's radio group: its `Radio`s, one of them chosen at a time, keyed by their `value`s, under `label`, which
 * `hideLabel` leaves off the screen while it still names the group. */
export function RadioGroup({ label, hideLabel, value, onChange, variant, className, children }: {
  label: React.ReactNode;
  hideLabel?: boolean;
  value: string;
  onChange: (value: string) => void;
  variant?: FieldVariant;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroRadioGroup variant={variant} value={value} onChange={onChange} isDisabled={useDisabled()} className={className}>
      <HeroLabel className={hideLabel ? "sr-only" : undefined}>{label}</HeroLabel>
      {children}
    </HeroRadioGroup>
  );
}

/** One choice of a `RadioGroup`, keyed `value`: its circle and `children` as its label, with `description` under it,
 * describing it. */
export function Radio({ value, description, children }: {
  value: string;
  description?: React.ReactNode;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroRadio value={value}>
      <HeroRadio.Content>
        <HeroRadio.Control>
          <HeroRadio.Indicator />
        </HeroRadio.Control>
        <HeroLabel>{children}</HeroLabel>
      </HeroRadio.Content>
      {description !== undefined && <Description>{description}</Description>}
    </HeroRadio>
  );
}
