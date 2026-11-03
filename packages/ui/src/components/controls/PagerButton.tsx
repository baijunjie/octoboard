import { ChevronLeft, ChevronRight } from "lucide-react";
import React from "react";

import { TitledControl } from "../hint/TitledControl";
import { Button } from "./Button";
import { ROW_ICON_BUTTON_CLASS } from "./RowControls";

/** A previous / next icon button, the pair of a pager. The chevron mirrors with the interface
 * direction, since "previous" points to the start edge in either. A press leaves focus where it was,
 * since the page changes under the pager; it still takes focus from the keyboard. */
export function PagerButton({
  ref,
  label,
  direction,
  isDisabled,
  onPress,
  onFocus,
  onBlur,
}: {
  ref?: React.Ref<HTMLButtonElement>;
  label: string;
  direction: "previous" | "next";
  isDisabled: boolean;
  onPress: () => void;
  onFocus?: () => void;
  onBlur?: (event: React.FocusEvent) => void;
}): React.ReactElement {
  const Icon = direction === "previous" ? ChevronLeft : ChevronRight;
  return (
    <TitledControl title={label}>
      <Button
        ref={ref}
        isIconOnly
        size="xs"
        variant="ghost"
        className={ROW_ICON_BUTTON_CLASS}
        aria-label={label}
        preventFocusOnPress
        isDisabled={isDisabled}
        onPress={onPress}
        onFocus={onFocus}
        onBlur={onBlur}
      >
        <Icon aria-hidden="true" className="size-4 rtl:-scale-x-100" />
      </Button>
    </TitledControl>
  );
}
