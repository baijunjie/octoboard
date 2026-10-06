import { Kbd } from "@heroui/react";
import React, { useEffect, useRef } from "react";

import { MODAL_OPEN } from "../layout/useRegionCycle";

const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

/** ⇧⌘F on macOS, Ctrl+Shift+F elsewhere: a combination the terminal's agents do not use, since
 * on macOS ⌘ never reaches a terminal application, and not one of the system's own. Matched on the
 * physical key so a keyboard layout or a held Shift does not change it. */
function isFocusShortcut(event: KeyboardEvent): boolean {
  if (event.code !== "KeyF" || !event.shiftKey || event.altKey) return false;
  return IS_MAC ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}

/** Toggles focus mode from the keyboard: enters it for the project of the selected session, or
 * leaves it. Listened for on the window's capture phase so the key never reaches the terminal
 * (and through it the agent); ignored while a dialog or menu is open and during an input method
 * composition. */
export function useFocusShortcut(toggle: () => void): void {
  const latest = useRef(toggle);
  latest.current = toggle;
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing || !isFocusShortcut(event)) return;
      event.preventDefault();
      event.stopPropagation();
      if (document.querySelector(MODAL_OPEN)) return;
      latest.current();
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, []);
}

/** The shortcut as HeroUI shows one in a menu item. */
export function FocusShortcutKbd(): React.ReactElement {
  return (
    // HeroUI's key cap is filled with `--default`, the fill a menu item takes while hovered, so in
    // a hovered item it would vanish into it; there it takes the surface colour instead. Hover
    // only: an item keeps focus after the pointer leaves it but loses that fill, and a surface cap
    // would then vanish into the menu.
    <Kbd slot="keyboard" className="ms-auto in-data-[hovered=true]:bg-surface">
      <Kbd.Abbr keyValue="shift" />
      <Kbd.Abbr keyValue={IS_MAC ? "command" : "ctrl"} />
      <Kbd.Content>F</Kbd.Content>
    </Kbd>
  );
}
