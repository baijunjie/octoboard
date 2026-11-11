import { Label as HeroLabel, Slider as HeroSlider } from "@heroui/react";
import React from "react";

import { useDisabled } from "./busy";

/** HeroUI's slider: a `label` over its current value and a track with one thumb, moving between `min` and `max` by
 * `step` (1 by default) as `value` is told of by `onChange`. It has no read-only state, so a slider that may not move is
 * `disabled`, and `locked` draws it at full strength rather than dimmed, as a value to read. */
export function Slider({ label, labelDir, value, onChange, min, max, step, describedBy, disabled, locked, className }: {
  label: string;
  /** `auto` for a label that is a page's or a user's text. */
  labelDir?: "auto";
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  describedBy?: string;
  disabled?: boolean;
  locked?: boolean;
  className?: string;
}): React.ReactElement {
  return (
    <HeroSlider
      value={value}
      onChange={(next) => onChange(next as number)}
      minValue={min}
      maxValue={max}
      step={step}
      aria-describedby={describedBy}
      isDisabled={useDisabled(disabled)}
      className={locked ? `${className ?? ""} data-[disabled=true]:opacity-100` : className}
    >
      <HeroLabel dir={labelDir}>{label}</HeroLabel>
      <HeroSlider.Output />
      <HeroSlider.Track>
        {/* The fill is placed with an inline `left: 0`, which in a right-to-left window leaves it at the wrong end of
          the track: the thumb is mirrored, so the fill starts from the right as well. */}
        <HeroSlider.Fill className="rtl:right-0 rtl:left-auto!" />
        <HeroSlider.Thumb />
      </HeroSlider.Track>
    </HeroSlider>
  );
}
