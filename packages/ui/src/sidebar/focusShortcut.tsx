import { Kbd } from "@heroui/react";
import React from "react";

import { IS_MAC, isPrimaryModifier, useWindowShortcut } from "../useWindowShortcut";

/** ⇧⌘F on macOS, Ctrl+Shift+F elsewhere: a combination the terminal's agents do not use, since
 * on macOS ⌘ never reaches a terminal application, and not one of the system's own. Matched on the
 * physical key so a keyboard layout or a held Shift does not change it. */
function matchFocusShortcut(event: KeyboardEvent): true | undefined {
  if (event.code !== "KeyF" || !event.shiftKey || event.altKey) return undefined;
  return isPrimaryModifier(event) ? true : undefined;
}

/** Toggles focus mode from the keyboard: enters it for the project of the selected session, or
 * leaves it. */
export function useFocusShortcut(toggle: () => void): void {
  useWindowShortcut(matchFocusShortcut, () => toggle());
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
