import { focusLanding, focusLandingWithoutText } from "../components/focus/regionFocus";
import { interactionIsTouch } from "../input/touchInput";
import type { PanelKind } from "../layout/panelKind";
import { panelId } from "./stackState";

/** How long a request waits, from the moment it is made, for the panel's card to show up and fill in with a control. */
export const LANDING_MS = 3000;

/** Sends focus into the `kind` panel, where F6 would land in it, once its card is on screen and has a control to take
 * it: a panel the user opened deliberately, docked or floating, whose card may not be rendered yet (the console it
 * belongs to is only now becoming current) or may fill in later. A tap lands where no on-screen keyboard is raised
 * (`interactionIsTouch`). Focus the user moves elsewhere meanwhile stays there; focus that only fell to `<body>`, as
 * it does from the agent terminal the panel has just made inert, was moved by no one. Gives up after `LANDING_MS`
 * whatever is still missing, and returns what ends the wait sooner. A docked card mounted by the same commit is landed
 * in once that commit's effects are over, not from one of them: in development StrictMode's re-run of the card's
 * `HandoffOnUnmount` would hand focus straight back out. */
export function landInPanel(kind: PanelKind): () => void {
  const land = interactionIsTouch() ? focusLandingWithoutText : focusLanding;
  const from = document.activeElement;
  const card = () => document.getElementById(panelId(kind));
  const landed = () => {
    const element = card();
    return element !== null && land(element);
  };
  if (landed()) return () => {};
  const movedByUser = () => {
    const active = document.activeElement;
    return active !== from && active !== document.body && active !== null;
  };
  let watched: Element | null = card();
  const observer = new MutationObserver(() => {
    if (movedByUser() || landed()) return end();
    // The card has appeared: watch it alone, until the deadline, for a control.
    const element = card();
    if (element !== null && element !== watched) {
      watched = element;
      observer.disconnect();
      observer.observe(element, { childList: true, subtree: true });
    }
  });
  const timer = setTimeout(end, LANDING_MS);
  function end() {
    observer.disconnect();
    clearTimeout(timer);
  }
  observer.observe(watched ?? document.body, { childList: true, subtree: true });
  return end;
}
