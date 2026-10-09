import { Button } from "@heroui/react";
import React from "react";

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
  label,
  onPress,
  children,
  isDisabled,
  onMouseHoverChange,
  expanded,
  controls,
  tooltipPlacement,
  className = "",
}: {
  label: string;
  onPress: () => void;
  children: React.ReactNode;
  isDisabled?: boolean;
  /** For a button that shows or hides a region: whether the region is shown, as `aria-expanded`;
   * `controls` is the region's id. */
  expanded?: boolean;
  controls?: string;
  /** Called as a mouse pointer enters or leaves the button; touch and pen are ignored. */
  onMouseHoverChange?: (hovered: boolean) => void;
  tooltipPlacement?: "end";
  /** Layout classes for the button, beside its fills. */
  className?: string;
}): React.ReactElement {
  return (
    <TitledControl title={label} placement={tooltipPlacement}>
      <Button
        isIconOnly
        isDisabled={isDisabled}
        size="sm"
        variant="ghost"
        aria-label={label}
        aria-expanded={expanded}
        aria-controls={controls}
        preventFocusOnPress
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
