import React from "react";

/** Why something cannot be chosen: the state ("Used up") and, where there is one, the detail that goes with it
 * ("resets in 2h"). */
export interface Reason {
  state: string;
  detail?: string;
}

/** The reason as one phrase, for a name or description read aloud: "Used up · resets in 2h". */
export function reasonText({ state, detail }: Reason): string {
  return detail ? `${state} · ${detail}` : state;
}

/** The short reason beside a disabled option or menu item, set on two lines where it has a detail, the state
 * first and the detail under it. Neither line wraps, so a phrase is never broken; the menu or popover around it
 * widens to fit, up to its cap, instead. A reason without a detail is a single line. */
export function ReasonText({ reason }: { reason: Reason }): React.ReactElement {
  if (!reason.detail) return <>{reason.state}</>;
  return (
    <>
      <span className="block whitespace-nowrap">{reason.state}</span>
      <span className="block whitespace-nowrap">{reason.detail}</span>
    </>
  );
}
