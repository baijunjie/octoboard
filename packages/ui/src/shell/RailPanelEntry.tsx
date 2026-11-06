import React from "react";

import { Hint } from "../components/hint/Hint";
import { PopoverOwnerContext } from "../components/hint/popoverOwner";
import { ChromeButton } from "./ChromeButton";
import { CloseWhenCovered } from "./CloseWhenCovered";
import { nestedPopovers } from "./nestedPopover";
import { mergeRefs } from "../primitives";

/** What the panel's content is given while the popover is up. */
interface RailPanelState {
  close: () => void;
  /** How the popover was opened: a press, or a mouse resting on the button. */
  form: string;
  /** The popover's element id. */
  popupId: string;
}

/** A rail button and the floating panel it anchors: a `Hint` of kind `popover` whose content is the entry's own
 * (`children`), rendered inside a `PopoverOwnerContext` valued `owner` so the status mark and Details popovers opened
 * from it count as part of the panel. `triggerLabel` names the button, `label` the popover. The button draws `glyph`
 * and takes its ref in `triggerRef`; `onHoverChange` hears the mouse come to and leave it, with whether the popover is
 * open. `yieldsEscape` adds to what the panel leaves Escape to: its own popovers already do. */
export function RailPanelEntry({ owner, label, triggerId, triggerLabel, triggerRef, triggerClassName, glyph, badge, onHoverChange, onOpenChange, yieldsEscape, children }: {
  owner: string;
  label: string;
  triggerId?: string;
  triggerLabel: string;
  triggerRef: React.Ref<HTMLButtonElement>;
  triggerClassName?: string;
  glyph: React.ReactNode;
  badge?: React.ReactNode;
  onHoverChange?: (hovered: boolean, expanded: boolean) => void;
  onOpenChange: (open: boolean) => void;
  yieldsEscape?: (target: Element | null) => boolean;
  children: (panel: RailPanelState) => React.ReactNode;
}): React.ReactElement {
  const nested = nestedPopovers(owner);
  return (
    <Hint
      kind="popover"
      label={label}
      placement="end bottom"
      offset={8}
      // A panel, not a hint: the surface, shadow and corners are the tooltip's, but it pads its own content edge to
      // edge and keeps the text size it had.
      contentClassName="w-96 max-w-[calc(100vw-4rem)] p-0 text-sm"
      // A popover opened from inside the panel is portalled out of it but belongs to it: a press in it is not an
      // outside press, and Escape in it closes it first.
      isInside={(target) => nested.holding(target) !== null}
      yieldsEscape={(event) => nested.holding(event.target) !== null || (yieldsEscape?.(event.target instanceof Element ? event.target : null) ?? false)}
      // Opening a modal one of those makes the panel inert, which the browser answers with a pointer leave on the panel
      // though the pointer did not move. That must not close the panel and take the popover with it.
      holdsOpen={() => nested.opened().length > 0}
      onOpenChange={onOpenChange}
      trigger={({ ref, focusOnPress: _focusOnPress, onMouseHoverChange, ...wiring }) => (
        <ChromeButton
          badge={badge}
          id={triggerId}
          label={triggerLabel}
          className={triggerClassName}
          buttonRef={mergeRefs(ref, triggerRef)}
          noTooltip
          {...wiring}
          onMouseHoverChange={(hovered) => {
            onHoverChange?.(hovered, wiring.expanded);
            onMouseHoverChange(hovered);
          }}
        >
          {glyph}
        </ChromeButton>
      )}
    >
      {({ close, form, popupId }) => (
        <PopoverOwnerContext.Provider value={owner}>
          <CloseWhenCovered close={close} own={popupId} owner={owner} />
          {children({ close, form, popupId })}
        </PopoverOwnerContext.Provider>
      )}
    </Hint>
  );
}
