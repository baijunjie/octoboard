import React from "react";
import type { PressEvent } from "react-aria";

import { Button } from "./Button";
import { TitledControl } from "./TitledControl";

/** The hover and pressed fills of a control on the window chrome: translucent tints (`style.css`), so
 * they read on the native material as well as on the browser's opaque chrome colour. HeroUI's button
 * takes its fills from these custom properties. */
export const CHROME_BUTTON_FILLS = "[--button-bg-hover:var(--chrome-hover)] [--button-bg-pressed:var(--chrome-active)]";

/** The fills of the current console's tile on the rail, and of a toggle whose panel is open: its
 * resting fill is also what it keeps under the pointer and while pressed, so pointing at it never
 * makes it paler. */
export const CHROME_CURRENT_FILLS =
  "[--button-bg:var(--chrome-current)] [--button-bg-hover:var(--chrome-current)] [--button-bg-pressed:var(--chrome-current)]";

/** A press on any of the window chrome's controls (the top bar's, the rail's) must leave keyboard
 * focus on the terminal, so every one of them is built from this. `TitledControl` gives it the
 * tooltip; a control on the rail opens it toward the content (`tooltipPlacement`). */
export function ChromeButton({
  id,
  label,
  onPress,
  children,
  isDisabled,
  onMouseHoverChange,
  expanded,
  controls,
  popupId,
  regionId,
  paneToggle,
  selected,
  onBlur,
  buttonRef,
  noTooltip,
  tooltipPlacement,
  className = "",
}: {
  id?: string;
  label: string;
  /** Called with the press, so a caller can tell a keyboard activation from a pointer's. */
  onPress: (event: PressEvent) => void;
  children: React.ReactNode;
  isDisabled?: boolean;
  /** For a button that shows or hides a region: whether the region is shown, as `aria-expanded`;
   * `controls` is the region's id, which `data-pane-toggle` carries too: `aria-controls` is for
   * assistive technology and may not survive the component library, so code that must recognise the
   * toggle in the DOM (`usePanePeekPointer`) matches `data-pane-toggle`. */
  expanded?: boolean;
  controls?: string;
  /** For a button that opens a floating panel: the panel's id, as `aria-controls`. Unlike `controls`
   * it does not mark the button as a pane toggle. */
  popupId?: string;
  /** For a button that shows or hides one part of a pane: the part's id, as `aria-controls`, given
   * only while the part is in the document. Unlike `controls` it does not mark a pane toggle. */
  regionId?: string;
  /** For a button that floats a pane in while the pointer is on it: the pane's id (`PANE_ID`), as
   * `data-pane-toggle`, so the pane's pointer watch counts the button as part of it. */
  paneToggle?: string;
  /** The button shows something that is on, such as a panel that is open: it keeps the fills of a
   * pressed button. */
  selected?: boolean;
  onBlur?: () => void;
  buttonRef?: React.Ref<HTMLButtonElement>;
  /** The button has no tooltip, for one that shows what it says some other way (hovering it floats its
   * panel in); `label` is still its accessible name. */
  noTooltip?: boolean;
  /** Called as a mouse pointer enters or leaves the button; touch and pen are ignored. */
  onMouseHoverChange?: (hovered: boolean) => void;
  tooltipPlacement?: "end";
  /** Layout classes for the button, beside its fills. */
  className?: string;
}): React.ReactElement {
  return (
    <TitledControl title={noTooltip ? "" : label} placement={tooltipPlacement}>
      <Button
        id={id}
        ref={buttonRef}
        isIconOnly
        isDisabled={isDisabled}
        size="sm"
        variant="ghost"
        aria-label={label}
        aria-expanded={expanded}
        aria-controls={controls ?? regionId ?? popupId}
        data-pane-toggle={paneToggle ?? controls}
        preventFocusOnPress
        onBlur={onBlur}
        className={`${selected ? CHROME_CURRENT_FILLS : CHROME_BUTTON_FILLS} ${className}`}
        onPress={onPress}
        onHoverStart={(event) => event.pointerType === "mouse" && onMouseHoverChange?.(true)}
        onHoverEnd={(event) => event.pointerType === "mouse" && onMouseHoverChange?.(false)}
      >
        {children}
      </Button>
    </TitledControl>
  );
}
