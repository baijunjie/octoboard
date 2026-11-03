import type { LucideIcon } from "lucide-react";
import React, { type RefObject, useRef } from "react";

import { Button } from "../controls/Button";
import { handFocusOff } from "../focus/handFocusOff";
import { TitledControl } from "../hint/TitledControl";

/** The mark of a pinned or locked row, which is also the way to take it off: its glyph becomes
 * `hoverIcon` while the button is hovered or holds keyboard focus. Placed like `RowIconButton`, so
 * pressing it neither selects the row behind it nor moves focus. Removing the mark takes the button
 * away, and keyboard focus with it, so `returnFocusTo` is the element that takes the focus over
 * first when the button holds it. */
export function RowMarkButton({
  icon: Icon,
  hoverIcon: HoverIcon,
  tooltip,
  label,
  tone,
  onPress,
  returnFocusTo,
}: {
  icon: LucideIcon;
  hoverIcon: LucideIcon;
  tooltip: React.ReactNode;
  label: string;
  tone: string;
  onPress: () => void;
  returnFocusTo: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const holder = useRef<HTMLDivElement>(null);
  return (
    <div ref={holder} className="flex shrink-0" onClick={(e) => e.stopPropagation()}>
      <TitledControl title={tooltip}>
        <Button
          isIconOnly
          size="xs"
          variant="ghost"
          aria-label={label}
          preventFocusOnPress
          onPress={() => {
            handFocusOff(holder.current, returnFocusTo.current, true);
            onPress();
          }}
          className={`group/mark shrink-0 rounded-md hover:text-foreground ${tone}`}
        >
          <Icon aria-hidden="true" className="size-4 group-hover/mark:hidden group-data-[focus-visible=true]/mark:hidden" />
          <HoverIcon aria-hidden="true" className="hidden size-4 group-hover/mark:block group-data-[focus-visible=true]/mark:block" />
        </Button>
      </TitledControl>
    </div>
  );
}
