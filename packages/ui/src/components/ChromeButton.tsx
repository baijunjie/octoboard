import { Button } from "@heroui/react";
import React from "react";
import type { PressEvent } from "react-aria";

import { TitledControl } from "./TitledControl";

/** The hover and pressed fills of a control on the window chrome: translucent tints (`style.css`), so
 * they read on the native material as well as on the browser's opaque chrome colour. HeroUI's button
 * takes its fills from these custom properties. */
export const CHROME_BUTTON_FILLS = "[--button-bg-hover:var(--chrome-hover)] [--button-bg-pressed:var(--chrome-active)]";

/** The fills of the current console's tile on the rail: its resting fill is also what it keeps under
 * the pointer and while pressed, so pointing at the current console never makes it paler. */
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
  onBlur,
  buttonRef,
  tooltipDisabled,
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
  onBlur?: () => void;
  buttonRef?: React.Ref<HTMLButtonElement>;
  /** Holds the tooltip back while the button shows what it says some other way (its panel is open). */
  tooltipDisabled?: boolean;
  /** Called as a mouse pointer enters or leaves the button; touch and pen are ignored. */
  onMouseHoverChange?: (hovered: boolean) => void;
  tooltipPlacement?: "end";
  /** Layout classes for the button, beside its fills. */
  className?: string;
}): React.ReactElement {
  return (
    <TitledControl title={label} placement={tooltipPlacement} isDisabled={tooltipDisabled}>
      <Button
        id={id}
        ref={buttonRef}
        isIconOnly
        isDisabled={isDisabled}
        size="sm"
        variant="ghost"
        aria-label={label}
        aria-expanded={expanded}
        aria-controls={controls ?? popupId}
        data-pane-toggle={controls}
        preventFocusOnPress
        onBlur={onBlur}
        className={`${CHROME_BUTTON_FILLS} ${className}`}
        onPress={onPress}
        onHoverStart={(event) => event.pointerType === "mouse" && onMouseHoverChange?.(true)}
        onHoverEnd={(event) => event.pointerType === "mouse" && onMouseHoverChange?.(false)}
      >
        {children}
      </Button>
    </TitledControl>
  );
}
