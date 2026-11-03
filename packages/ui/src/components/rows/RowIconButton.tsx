import type { LucideIcon } from "lucide-react";
import React, { useId } from "react";
import type { PressEvent } from "react-aria-components";

import { Button } from "../controls/Button";
import { ROW_ICON_BUTTON_BASE_CLASS, ROW_ICON_BUTTON_CLASS } from "../controls/RowControls";
import { TitledControl } from "../hint/TitledControl";

/** A small icon-only button inside a row (the "+" that opens a session), with its tooltip. Its click
 * is kept from reaching the row behind it, and a mouse press leaves focus where it was. */
export function RowIconButton({
  icon: Icon,
  label,
  title,
  ariaLabel = label,
  onPress,
  isDisabled,
  unavailable,
  iconClassName,
  ref,
}: {
  icon: LucideIcon;
  label: string;
  /** Rich visible tooltip; `label` remains the plain action name. */
  title?: React.ReactNode;
  /** The accessible name, when it says more than the tooltip's bare verb (whose row it acts on). */
  ariaLabel?: string;
  onPress: (event: PressEvent) => void;
  isDisabled?: boolean;
  /** Why the action cannot be done now, for a button that looks disabled but stays focusable and under
   * the pointer so that the reason is reachable: it is the tooltip and the button's description, while
   * the name stays the action. HeroUI's disabled style turns pointer events off for `aria-disabled`, so
   * they are turned back on, and the hover tint and press shrink are left off. A press still reaches
   * `onPress`, which answers it. */
  unavailable?: string;
  /** Extra classes on the glyph itself, such as a right-to-left mirror. */
  iconClassName?: string;
  /** The element around the button, for focus to be handed to. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const reasonId = useId();
  return (
    <div ref={ref} className="flex" onClick={(e) => e.stopPropagation()}>
      <TitledControl title={unavailable ?? title ?? label}>
        <Button
          isIconOnly
          size="xs"
          variant="ghost"
          aria-label={ariaLabel}
          aria-describedby={unavailable ? reasonId : title && ariaLabel === label ? "" : undefined}
          aria-disabled={unavailable ? true : undefined}
          preventFocusOnPress
          onPress={onPress}
          isDisabled={isDisabled}
          className={unavailable ? `${ROW_ICON_BUTTON_BASE_CLASS} pointer-events-auto data-[pressed=true]:[transform:none] active:[transform:none]` : ROW_ICON_BUTTON_CLASS}
        >
          {unavailable && <span id={reasonId} className="sr-only">{unavailable}</span>}
          <Icon aria-hidden="true" className={iconClassName ? `size-4 ${iconClassName}` : "size-4"} />
        </Button>
      </TitledControl>
    </div>
  );
}
