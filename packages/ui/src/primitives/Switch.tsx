import { Switch as HeroSwitch } from "@heroui/react";
import React, { useEffect, useRef } from "react";

import { useDisabled } from "./busy";
import { Tooltip, useOwnTooltipGuard } from "./Tooltip";
import { UNAVAILABLE_FADE, useInputAriaDisabled, useUnavailable } from "./unavailable";

/** HeroUI's switch, without a visible label: `label` names it. It is on while `selected`, told of presses by
 * `onSelectedChange`. `unavailable` freezes it and says it is unavailable (`aria-disabled`) rather than merely fixed,
 * which is all react-aria's read-only state says: given the reason it is also the switch's tooltip and description,
 * given `true` it has none; it keeps focus, is faded and takes no press (see `./unavailable`). `tooltip` is its own
 * tooltip while it is available; the switch carries it itself, so the caller wraps it in no `Tooltip`. The tooltip and
 * the touch popover are placed against the track, which also takes the hover, not against the hidden input that takes
 * focus. `opensFocused` marks it as where its dialog's opening focus goes (`openingFocus`). */
export function Switch({ label, describedBy, selected, onSelectedChange, unavailable, tooltip, tooltipPlacement, opensFocused, size, className }: {
  label: string;
  describedBy?: string;
  selected: boolean;
  onSelectedChange: (selected: boolean) => void;
  /** Why the switch is unavailable, or `true` for no reason; `undefined` or `""` when it is available. */
  unavailable?: React.ReactNode;
  /** The switch's own tooltip while it is available, which the reason replaces while it is unavailable. */
  tooltip?: React.ReactNode;
  tooltipPlacement?: "end" | "bottom";
  opensFocused?: boolean;
  size?: "sm" | "md" | "lg";
  className?: string;
}): React.ReactElement {
  const root = useRef<HTMLDivElement>(null);
  const why = useUnavailable(unavailable);
  const frozen = why.frozen;
  useOwnTooltipGuard("Switch");
  useInputAriaDisabled(root, frozen);
  // react-aria gives the switch's input no `data-opening-focus` as a prop: it is set on it once it is in the page.
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
      aria-describedby={why.id ?? describedBy}
      isDisabled={useDisabled()}
      isReadOnly={frozen}
      isSelected={selected}
      onChange={onSelectedChange}
      className={frozen ? [className, UNAVAILABLE_FADE].filter(Boolean).join(" ") : className}
    >
      <HeroSwitch.Content>
        <Tooltip trigger="box" content={why.reason ?? tooltip} placement={tooltipPlacement} label={label} root={root} touchPopover={why.reason !== undefined}>
          <HeroSwitch.Control>
            <HeroSwitch.Thumb />
          </HeroSwitch.Control>
        </Tooltip>
      </HeroSwitch.Content>
      {why.element}
    </HeroSwitch>
  );
}
