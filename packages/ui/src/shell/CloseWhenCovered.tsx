import { useEffect } from "react";

import { MODAL_OPEN } from "../components/focus/selectors";
import { nestedPopovers } from "./nestedPopover";

/** Closes the panel it is rendered in when a modal is opened by something other than a press here
 * (Settings from the native menu, or the popover of a status mark elsewhere on the page), which covers the panel
 * and would otherwise leave it up under the modal, taking the modal's Escape. Modals and menus portal in as direct
 * children of the body, so only those are watched. Not covering: a hover-opened panel (`aria-modal="false"`), the
 * panel's own dialog in its press form (`own`), and a status mark's or a Details popover that was opened from
 * inside the panel (`owner` tags them), whose portal would otherwise read as such a modal and close the panel
 * it came from. */
export function CloseWhenCovered({ close, own, owner }: { close: () => void; own: string; owner: string }): null {
  useEffect(() => {
    const ours = nestedPopovers(owner);
    const modalWatch = new MutationObserver(() => {
      const covering = Array.from(document.querySelectorAll(MODAL_OPEN)).some((modal) => !ours.holding(modal) && modal.id !== own);
      if (covering) close();
    });
    modalWatch.observe(document.body, { childList: true });
    return () => modalWatch.disconnect();
  }, []);
  return null;
}
