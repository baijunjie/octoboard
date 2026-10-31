import React from "react";

import { Hint, type IndicatorHintProps } from "./Hint";

/** `Hint` of kind `indicator` under a shorter name: a tooltip on a non-control mark (the rules are in
 * `hintTooltips.tsx`). */
export function IndicatorTooltip(props: IndicatorHintProps): React.ReactElement {
  return <Hint kind="indicator" {...props} />;
}
