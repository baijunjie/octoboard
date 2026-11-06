import { Kbd as HeroKbd } from "@heroui/react";
import React from "react";

/** The keys the application has HeroUI's key caps draw as a symbol (`KbdAbbr`): the modifiers and the keys of a shortcut
 * that have one. */
export type KbdKey = "command" | "shift" | "ctrl" | "option" | "win" | "alt" | "enter" | "delete" | "tab" | "up" | "right" | "down" | "left" | "pageup" | "pagedown" | "home" | "end" | "space";

/** HeroUI's key caps: a key combination, its keys given as `KbdAbbr` and `KbdContent`. `slot` places it in an item that
 * has a slot for it (`"keyboard"` in a menu item); `decorative` keeps it from assistive technology, where the item says
 * the key some other way. */
export function Kbd({ decorative, dir, slot, className, children }: {
  decorative?: boolean;
  dir?: "ltr";
  slot?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return <HeroKbd aria-hidden={decorative || undefined} dir={dir} slot={slot} className={className}>{children}</HeroKbd>;
}

/** A key drawn as its symbol, with the key's name for assistive technology. */
export function KbdAbbr({ keyValue }: { keyValue: KbdKey }): React.ReactElement {
  return <HeroKbd.Abbr keyValue={keyValue} />;
}

/** A key drawn as its text. */
export function KbdContent({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroKbd.Content>{children}</HeroKbd.Content>;
}
