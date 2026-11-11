import { TEXT_ENTRY } from "../../input/touchInput";
import { FOCUSABLE } from "./selectors";
import { partSelector } from "../../primitives";

/** A window region, named by its `data-region`. */
export type Region = string;

/** Focuses where F6 lands in `root`, a region's element: the control it marks `data-region-landing`,
 * else its first control a keyboard can reach; says whether focus is there now. */
export function focusLanding(root: Element): boolean {
  const target = root.querySelector<HTMLElement>("[data-region-landing]") ?? root.querySelector<HTMLElement>(FOCUSABLE);
  target?.focus();
  return target != null && target.ownerDocument.activeElement === target;
}

/** Where a tap puts focus in `root`: where `focusLanding` would unless that is a field that takes text
 * (the Terminal panel's shell, say), and then `root`'s first control that is not; says whether focus
 * is there now. A tap must not focus such a field, which would raise the on-screen keyboard
 * (`interactionIsTouch`). */
export function focusLandingWithoutText(root: Element): boolean {
  const landing = root.querySelector<HTMLElement>("[data-region-landing]");
  const target = landing && !landing.matches(TEXT_ENTRY)
    ? landing
    : Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).find((control) => !control.matches(TEXT_ENTRY));
  target?.focus();
  return target != null && target.ownerDocument.activeElement === target;
}

/** Whether `element` can take focus now: in the page, not disabled or hidden, and not behind a modal (`inert`, or
 * `aria-hidden` where the browser has no `inert`, as in jsdom). */
export function canTakeFocus(element: Element | null | undefined): element is HTMLElement {
  return element instanceof HTMLElement && element.isConnected && !element.matches(":disabled") && !element.closest('[inert], [aria-hidden="true"], [hidden]');
}

/** Puts focus on the first control a keyboard can reach in the region marked `data-region`; a
 * disabled control cannot hold focus and is skipped. */
export function focusFirstIn(region: Region): void {
  document.querySelector<HTMLElement>(`[data-region=${region}] :is(${FOCUSABLE})`)?.focus();
}

/** Puts focus on the top bar's first control: where focus goes when there is no better place to
 * hand it. Its Back button is often disabled, in which case the next control takes it. */
export function focusTopBar(): void {
  focusFirstIn("topbar");
}

/** A toast that is on screen, not one fading out. */
const LIVE_TOAST = partSelector.liveToast;

export function regionElements(region: Region): Element[] {
  return Array.from(document.querySelectorAll(`[data-region=${region}]`));
}

/** Focuses where `region` takes focus and says whether it did: a control the region marks with
 * `data-region-landing` (a panel that failed to load, whose Try again follows its header), else the
 * sidebar's selected row or the rail's current console, else the first control, as the other regions
 * that are not the terminal do. The toast stack lists its newest toast first, so its first control
 * is that toast. */
export function focusRegion(region: Region, focusTerminal: () => void): boolean {
  if (region === "terminal") {
    focusTerminal();
    return regionElements(region).some((el) => el.contains(document.activeElement));
  }
  const root = regionElements(region)[0];
  if (!root) return false;
  const target =
    region === "toast"
      ? root.querySelector<HTMLElement>(LIVE_TOAST)
      : root.querySelector<HTMLElement>("[data-region-landing]") ||
        ((region === "sidebar" || region === "rail") && root.querySelector<HTMLElement>('[aria-current="true"]')) ||
        root.querySelector<HTMLElement>(FOCUSABLE);
  if (!target) return false;
  target.focus();
  return target.ownerDocument.activeElement === target;
}

/** Whether focus has fallen to the page, as it does when the element holding it goes. */
export function focusFellToPage(): boolean {
  const active = document.activeElement;
  return !active || active === document.body;
}
