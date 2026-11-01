import React from "react";

import type { Reason } from "../../format/reason";

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
