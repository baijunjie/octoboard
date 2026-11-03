/** Set when a menu closed but could not hand focus back, because the item it ran opened a dialog
 * that holds it. The dialog takes it when it closes (see `takeMenuFocusToRestore`). */
let menuFocusToRestore: HTMLElement | null = null;

/** The element a menu could not give focus back to while a dialog was open, if any; the caller
 * returns focus to it once the dialog is gone. Each element is handed out once. The dialog's owner
 * may take it first and drop it, when the dialog's outcome sends focus somewhere else. */
export function takeMenuFocusToRestore(): HTMLElement | null {
  const element = menuFocusToRestore;
  menuFocusToRestore = null;
  return element;
}

/** Remembers `element` for `takeMenuFocusToRestore`: the menu could not give focus back to it. */
export function keepMenuFocusToRestore(element: HTMLElement): void {
  menuFocusToRestore = element;
}

/** How many menus are open right now; see `isActionMenuOpen`. */
let openMenus = 0;

/** Counts a menu as open or closed. */
export function countOpenMenu(open: boolean): void {
  openMenus += open ? 1 : -1;
}

/** Whether any action menu's popover is open, which holds keyboard focus like a modal does. */
export function isActionMenuOpen(): boolean {
  return openMenus > 0;
}

/** The element a menu that just closed will give focus back to, until it does; a menu opened by a
 * right-click in the meantime takes it over, so focus is returned once, when that one closes. */
let handedFocus: HTMLElement | null = null;

/** Offers `element` to the menu that opens next, for the closing menu to give focus back to otherwise. */
export function offerHandedFocus(element: HTMLElement): void {
  handedFocus = element;
}

/** Takes what a closing menu offered, whoever it was offered to; it is no longer on offer. */
export function takeHandedFocus(): HTMLElement | null {
  const element = handedFocus;
  handedFocus = null;
  return element;
}

/** Whether `element` is still on offer; if it is, it is taken. */
export function claimHandedFocus(element: HTMLElement): boolean {
  if (handedFocus !== element) return false;
  handedFocus = null;
  return true;
}
