import { Switch as HeroSwitch } from "@heroui/react";
import React, { useEffect, useRef } from "react";

import { useDisabled } from "./busy";

/** HeroUI's switch, without a visible label: `label` names it. It is on while `selected`, told of presses by
 * `onSelectedChange`. `unavailable` freezes it and says it is unavailable (`aria-disabled`) rather than merely fixed,
 * which is all react-aria's read-only state says. `opensFocused` marks it as where its dialog's opening focus goes
 * (`openingFocus`). */
export function Switch({ label, describedBy, selected, onSelectedChange, unavailable, opensFocused, size, className }: {
  label: string;
  /** `""` keeps a tooltip that repeats the name from describing it. */
  describedBy?: string;
  selected: boolean;
  onSelectedChange: (selected: boolean) => void;
  unavailable?: boolean;
  opensFocused?: boolean;
  size?: "sm" | "md" | "lg";
  className?: string;
}): React.ReactElement {
  const root = useRef<HTMLDivElement>(null);
  // react-aria gives the switch's input neither of these as a prop: they are set on it once it is in the page.
  useEffect(() => {
    const input = root.current?.querySelector("input");
    if (unavailable) input?.setAttribute("aria-disabled", "true");
    else input?.removeAttribute("aria-disabled");
  }, [unavailable]);
  useEffect(() => {
    const input = root.current?.querySelector("input");
    if (opensFocused) input?.setAttribute("data-opening-focus", "");
    else input?.removeAttribute("data-opening-focus");
  }, [opensFocused]);
  return (
    <HeroSwitch
      ref={root}
      size={size}
      aria-label={label}
      aria-describedby={describedBy}
      isDisabled={useDisabled()}
      isReadOnly={unavailable}
      isSelected={selected}
      onChange={onSelectedChange}
      className={className}
    >
      <HeroSwitch.Content>
        <HeroSwitch.Control>
          <HeroSwitch.Thumb />
        </HeroSwitch.Control>
      </HeroSwitch.Content>
    </HeroSwitch>
  );
}
