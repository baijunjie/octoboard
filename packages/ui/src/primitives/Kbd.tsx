import { Kbd as HeroKbd } from "@heroui/react";
import React from "react";

/** The keys the application has HeroUI's key caps draw as a symbol (`KbdAbbr`): the modifiers and the keys of a shortcut
 * that have one. */
export type KbdKey = "command" | "shift" | "ctrl" | "option" | "win" | "alt" | "enter" | "delete" | "tab" | "up" | "right" | "down" | "left" | "pageup" | "pagedown" | "home" | "end" | "space";

/** HeroUI's key caps: a key combination, its keys given as `KbdAbbr` and `KbdContent`. `slot` places it in an item that
 * has a slot for it (`"keyboard"` in a menu item); `decorative` keeps it from assistive technology, where the item says
 * the key some other way; `label` names the whole combination for it instead, as one image, in place of what each cap
 * would say. `spelled` is for keys written as their names (Ctrl, Shift) rather than glyphs: names need a word's gap
 * between them, which stays on the side the caps are written in under a right-to-left page, where HeroUI's own reverses
 * it even for caps held left to right (`data-kbd-spelled`, for a container to make room for them). */
export function Kbd({ decorative, label, dir, spelled = false, slot, className = "", children }: {
  decorative?: boolean;
  label?: string;
  dir?: "ltr";
  spelled?: boolean;
  slot?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroKbd
      aria-hidden={decorative || undefined}
      role={label === undefined ? undefined : "img"}
      aria-label={label}
      dir={dir}
      slot={slot}
      data-kbd-spelled={spelled ? "" : undefined}
      className={`${spelled ? "space-x-1" : ""} ${className}`.trim() || undefined}
    >
      {children}
    </HeroKbd>
  );
}

/** A key drawn as its symbol, with the key's name for assistive technology. */
export function KbdAbbr({ keyValue }: { keyValue: KbdKey }): React.ReactElement {
  return <HeroKbd.Abbr keyValue={keyValue} />;
}

/** A key drawn as its text. */
export function KbdContent({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroKbd.Content>{children}</HeroKbd.Content>;
}
