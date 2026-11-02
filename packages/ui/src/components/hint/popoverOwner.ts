import { createContext, useContext } from "react";

/** Names the surface whose content a status mark or a Details button is rendered in, so that the popover it opens,
 * which is portalled out of that surface, can be told from the same kind opened anywhere else (`data-popover-owner`). */
export const PopoverOwnerContext = createContext<string | undefined>(undefined);

/** The selector for the popovers opened from the surface named `owner`. */
export const ownedBy = (owner: string): string => `[data-popover-owner="${owner}"]`;

/** The attributes that tie a popover to the surface it was opened from, for the element that holds it. */
export function usePopoverOwnerProps(): Record<"data-popover-owner", string> | undefined {
  const owner = useContext(PopoverOwnerContext);
  return owner === undefined ? undefined : { "data-popover-owner": owner };
}
