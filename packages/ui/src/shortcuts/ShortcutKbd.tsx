import type { ReactElement } from "react";

import { useT } from "../i18n/react";
import { useHost } from "../platform/react";
import { Kbd, KbdAbbr, KbdContent } from "../primitives";

import { bindingParts, describeBinding, type Binding } from "./binding";
import type { CommandId } from "./commands";
import { useBinding } from "./store";

/**
 * A key combination as HeroUI draws one: macOS symbols on an Apple host, the key names (Ctrl, Shift) elsewhere,
 * where those symbols mean nothing to the user. `dir` holds the caps in the order the shortcut is written
 * in: the notation is the same sequence in every language, and a right-to-left interface would
 * otherwise reverse it. Its accessible name is the combination as `describeBinding` says it, each key named in the UI's
 * language (see "Keyboard" in `docs/ui-standard.md`).
 */
export function BindingKbd({ binding, className, slot }: { binding: Binding; className?: string; slot?: string }): ReactElement {
  const host = useHost();
  const t = useT();
  return (
    <Kbd dir="ltr" spelled={host.modifier !== "meta"} label={describeBinding(binding, t, host)} slot={slot} className={className}>
      {bindingParts(binding, host).map((part) => (part.abbr && !part.display ? <KbdAbbr key={part.abbr} keyValue={part.abbr} /> : <KbdContent key={part.abbr ?? part.text}>{part.display ?? part.text}</KbdContent>))}
    </Kbd>
  );
}

/** The command's current binding, for a menu item's end slot or a tooltip. */
export function ShortcutKbd({ command, className = "" }: { command: CommandId; className?: string }): ReactElement {
  // HeroUI's key cap is filled with `--default`, the fill a menu item takes while hovered, so in
  // a hovered item it would vanish into it; there it takes the surface colour instead. Hover
  // only: an item keeps focus after the pointer leaves it but loses that fill, and a surface cap
  // would then vanish into the menu.
  return <BindingKbd binding={useBinding(command)} slot="keyboard" className={`ms-auto shrink-0 in-ui-hovered:bg-surface ${className}`.trim()} />;
}
