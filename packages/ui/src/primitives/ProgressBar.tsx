import { ProgressBar as HeroProgressBar } from "@heroui/react";
import React from "react";

/** HeroUI's progress bar: how far a task has come, `value` out of `maxValue` (100 by default), or an `indeterminate`
 * one whose end is not known yet. `valueLabel` is the value as it is read (a count, a percentage). `children` lays out
 * its parts: a `Label` naming it, its `ProgressBarTrack` and, where the value is shown, its `ProgressBarOutput`. */
export function ProgressBar({ value, maxValue, indeterminate, valueLabel, describedBy, className, children }: {
  value?: number;
  maxValue?: number;
  indeterminate?: boolean;
  valueLabel?: string;
  describedBy?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroProgressBar
      value={value}
      maxValue={maxValue}
      isIndeterminate={indeterminate}
      valueLabel={valueLabel}
      aria-describedby={describedBy}
      className={className}
    >
      {children}
    </HeroProgressBar>
  );
}

/** A `ProgressBar`'s track, with the fill that shows its value. */
export function ProgressBarTrack({ className }: { className?: string }): React.ReactElement {
  return (
    <HeroProgressBar.Track className={className}>
      <HeroProgressBar.Fill />
    </HeroProgressBar.Track>
  );
}

/** A `ProgressBar`'s value shown as text, its `valueLabel`. */
export function ProgressBarOutput({ className }: { className?: string }): React.ReactElement {
  return <HeroProgressBar.Output className={className} />;
}
