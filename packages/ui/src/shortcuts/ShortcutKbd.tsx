import { Kbd } from "@heroui/react";
import type { ReactElement } from "react";

import { bindingParts, type Binding } from "./binding";
import type { CommandId } from "./commands";
import { useBinding } from "./store";

/**
 * A key combination as HeroUI draws one. `dir` holds the caps in the order the shortcut is written
 * in: the notation is the same sequence in every language, and a right-to-left interface would
 * otherwise reverse it.
 */
export function BindingKbd({ binding, className, slot }: { binding: Binding; className?: string; slot?: string }): ReactElement {
  return (
    <Kbd dir="ltr" slot={slot} className={className}>
      {bindingParts(binding).map((part) => (part.abbr ? <Kbd.Abbr key={part.abbr} keyValue={part.abbr} /> : <Kbd.Content key={part.text}>{part.text}</Kbd.Content>))}
    </Kbd>
  );
}

/** The command's current binding, for a menu item's end slot or a tooltip. */
export function ShortcutKbd({ command, className = "" }: { command: CommandId; className?: string }): ReactElement {
  // HeroUI's key cap is filled with `--default`, the fill a menu item takes while hovered, so in
  // a hovered item it would vanish into it; there it takes the surface colour instead. Hover
  // only: an item keeps focus after the pointer leaves it but loses that fill, and a surface cap
  // would then vanish into the menu.
  return <BindingKbd binding={useBinding(command)} slot="keyboard" className={`ms-auto shrink-0 in-data-[hovered=true]:bg-surface ${className}`.trim()} />;
}
