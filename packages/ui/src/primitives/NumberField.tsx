import { NumberField as HeroNumberField } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";
import type { FieldVariant } from "./Field";

/** HeroUI's number field: an input for a number with a decrement and an increment button either side
 * of it. `onChange` is told of each number the input commits (on blur or Enter, and by the buttons and the arrow keys):
 * one outside `min`..`max` is clamped into it and one between steps is moved to the nearest, so a typed value never
 * reaches `onChange` out of range. The arrow keys step by `step` (1 by default), Page Up and Page Down by ten times that,
 * and Home and End go to the bounds. A button is disabled at its bound. The number is centred between the buttons, with
 * no unit: the library writes it with `Intl.NumberFormat`, which has no unit for every quantity (`pixel` throws), and
 * HeroUI's group has no place for a suffix. `label` is the accessible name, for a field with no visible `Label`. The buttons are no tab stops, the arrow keys
 * doing what they do, and carry a name each (`incrementLabel`, `decrementLabel`) written as a whole phrase, which the
 * library would otherwise build from `label` in English only; they have no tooltip, their glyphs being the
 * convention for a stepper. `inputRef` is the input, for a caller that moves focus to it. */
export function NumberField({ value, onChange, min, max, step, label, incrementLabel, decrementLabel, inputRef, variant, className }: {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  label: string;
  incrementLabel: string;
  decrementLabel: string;
  inputRef?: React.Ref<HTMLInputElement>;
  variant?: FieldVariant;
  className?: string;
}): React.ReactElement {
  return (
    <HeroNumberField
      value={value}
      // An emptied input commits as `NaN`, which is not passed on: the input goes back to showing the value.
      onChange={(next) => !Number.isNaN(next) && onChange(next)}
      minValue={min}
      maxValue={max}
      step={step}
      aria-label={label}
      incrementAriaLabel={incrementLabel}
      decrementAriaLabel={decrementLabel}
      isDisabled={useDisabled()}
      variant={variant}
      className={className}
    >
      <HeroNumberField.Group>
        <HeroNumberField.DecrementButton />
        <HeroNumberField.Input ref={inputRef} className="text-center" />
        <HeroNumberField.IncrementButton />
      </HeroNumberField.Group>
    </HeroNumberField>
  );
}
