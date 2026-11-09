import type { LucideIcon } from "lucide-react";
import type React from "react";

import type { Reason } from "../../format/reason";

export interface ActionMenuItem {
  /** A single letter that chooses this entry while its menu is open. */
  shortcut?: string;
  /** A window command advertised by this item, which it can take while its root menu has focus. */
  commandShortcut?: { label: string; matches: (event: KeyboardEvent) => boolean };
  label: string;
  /** What the item shows in place of `label` (a path that has to read left to right); `label` stays
   * its text value for typeahead. It takes the pointer as `label` does, so a tooltip or marquee in it
   * needs nothing added (see `labelPointerClass`). */
  content?: React.ReactNode;
  /** A lucide glyph, or any element of about the same size (an agent's mark, a console's avatar). */
  icon: LucideIcon | React.ReactElement;
  /** Added to a lucide glyph's own classes (`rtl:-scale-x-100` for one whose meaning is a direction);
   * an element keeps the classes it was given. */
  iconClassName?: string;
  /** Runs when the item is chosen. */
  onClick: () => void;
  /** Resolves this item when its menu opens; the item stays visible and disabled until it settles.
   * `key` identifies the input, so changing the target drops an earlier answer. */
  load?: { key: string; run: () => Promise<ActionMenuItem> };
  destructive?: boolean;
  closeOnSelect?: boolean;
  disabled?: boolean;
  /** The accessible name when it should say more than `label` (a console and what is going on in it). */
  ariaLabel?: string;
  /** Drawn after the label, at the item's end edge (a status marker). */
  end?: React.ReactNode;
  /** Why the item cannot be chosen, or how to make it available ("Option-drag to select"): it is
   * disabled and the reason is drawn under the label in place of `end`. The accessible name is the
   * label (`ariaLabel` if set) followed by the reason, unless the item has a `description`, which then
   * carries the reason in full and leaves the name the label. */
  reason?: Reason;
  /** A note that assistive technology reads as the item's description and that is not drawn (what
   * remains of an account's quota, which the usage bar in `icon` shows only to the eye; for an item
   * with a `reason`, the reason in full). */
  description?: string;
  /** Makes the item one of a single-selection group (consecutive items that set it): HeroUI marks
   * the chosen one (`true`) with its check and announces it as checked. */
  selected?: boolean;
}

/** An item that opens a nested menu of its own items. */
export interface ActionMenuSubmenu {
  label: string;
  icon: LucideIcon | React.ReactElement;
  /** Added to a lucide glyph's own classes, as an item's `iconClassName`. */
  iconClassName?: string;
  items: ActionMenuEntry[];
  /** Loads fresh items when this submenu opens. */
  loadItems?: () => Promise<ActionMenuEntry[]>;
}

/** What a panel is given to be part of the menu it opens from. */
export interface ActionMenuPanelContext {
  /** Closes the whole menu, the submenu with it. */
  close: () => void;
  /** The id of the item that opened the panel, which names what the panel holds (the panel itself
   * goes unnamed, so the name is not spoken twice). */
  labelledBy: string | undefined;
  /** Whether keyboard focus should move into the panel as it opens: it does for a submenu opened
   * from the keyboard and not for one a hover opened. */
  autoFocus: boolean;
}

/** An item that opens a panel of custom content beside the menu in place of a nested menu (a
 * palette of swatches). The panel is part of the menu: arrow keys leave it towards the menu as they
 * do a nested one, and it closes the whole menu to finish. */
export interface ActionMenuPanel {
  label: string;
  icon: LucideIcon | React.ReactElement;
  /** Added to a lucide glyph's own classes, as an item's `iconClassName`. */
  iconClassName?: string;
  panel: (context: ActionMenuPanelContext) => React.ReactNode;
}

/** A line of muted text at the menu's end that says something about the items above it (a limit the list
 * hit): it carries no glyph, its text lines up with the items' labels (past the check indicator of a menu with
 * choices too), and it cannot be chosen. It stays in
 * the arrow-key walk, so it is reached and read from the keyboard. */
export interface ActionMenuNote {
  /** The note's text. */
  label: string;
  note: true;
}

export type ActionMenuEntry = ActionMenuItem | ActionMenuSubmenu | ActionMenuPanel | ActionMenuNote | "separator";
