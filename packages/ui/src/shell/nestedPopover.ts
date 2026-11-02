import { DETAILS_POPOVER } from "../components/alerts/DetailsButton";
import { ownedBy } from "../components/hint/popoverOwner";
import { STATUS_MARK_POPOVER } from "../components/status/StatusMark";

/** What a rail entry asks about the status mark and Details popovers opened from inside its panel, as against the
 * same kind opened anywhere else in the document, which are not the panel's to hold open, shelter from an outside
 * press or Escape, or hand focus back from. The panel's content is rendered inside a `PopoverOwnerContext` whose
 * value is `owner`, which tags the popovers it opens. */
export function nestedPopovers(owner: string): {
  /** The popovers up that were opened from inside the panel. */
  opened: () => Element[];
  /** The panel's popover that holds `target`, if any. */
  holding: (target: EventTarget | null) => Element | null;
} {
  const selector = [STATUS_MARK_POPOVER, DETAILS_POPOVER].map((popover) => `${popover}${ownedBy(owner)}`).join(", ");
  return {
    opened: () => Array.from(document.querySelectorAll(selector)),
    holding: (target) => (target instanceof Element ? target.closest(selector) : null),
  };
}
