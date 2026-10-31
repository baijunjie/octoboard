import React from "react";

import { Hint, type AnchoredHintProps } from "./Hint";

/** `Hint` of kind `anchored` under a shorter name: a tooltip on something that cannot be a trigger, whose
 * hover the caller reports (the rules are in `hintTooltips.tsx`). */
export function AnchoredTooltip(props: AnchoredHintProps): React.ReactElement | null {
  return <Hint kind="anchored" {...props} />;
}
