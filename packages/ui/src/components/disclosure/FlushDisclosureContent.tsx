import type React from "react";

import { DisclosureBody, DisclosureContent } from "../../primitives";

/** What a disclosure folds away, set flush with what stands above it. The content clips what overflows it, so focus
 * rings and shadows inside need room within the clip: the content reaches 4px past each side, which is the slack a
 * dialog body has, and the body pads those 4px back in place of HeroUI's 8px, which lands what is inside in line with
 * what is above it. Below, the body keeps 4px for the same rings; above, HeroUI's 8px, the gap to the trigger, which
 * as a margin would stay when the disclosure is folded. `indent` sets it in further at the start, under the label of a
 * `TextDisclosure`'s trigger. `className` goes on the body's inner element. */
export function FlushDisclosureContent({ indent = false, className = "", children }: {
  indent?: boolean;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <DisclosureContent className="-mx-1">
      <DisclosureBody flush indent={indent} className={className}>{children}</DisclosureBody>
    </DisclosureContent>
  );
}
