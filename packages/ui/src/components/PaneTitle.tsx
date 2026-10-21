import React from "react";

import { FadeOverflow } from "./FadeOverflow";

/** The title of a pane's header: an optional small muted line over the pane's heading, as the flexing
 * item of a header row. The heading's line is held to the small line's 16px (`text-sm` alone would
 * give it 20px, and the body's 24px if it were left to inherit), so the gap between the two lines is
 * no larger than the header's margin above and below them.
 * Both lines clip at the item's width, and each label gets 1px of room above and below its line
 * (`py-px -my-px`, which adds nothing to the layout): the clip would otherwise cut the ink that
 * reaches past a 16px line, such as descenders, accented capitals and emoji. */
export function PaneTitle({
  heading,
  headingTitle = heading,
  overline,
  className = "min-w-0 flex-1",
}: {
  heading: string;
  /** What the tooltip of a clipped heading shows, when that is more than the heading itself. */
  headingTitle?: React.ReactNode;
  overline?: string;
  /** The item's own sizing; the default lets it flex and shrink to nothing. */
  className?: string;
}): React.ReactElement {
  return (
    <div className={className}>
      {overline !== undefined && (
        <FadeOverflow className="py-px -my-px text-xs text-app-muted" dir="auto" titleWhenClipped={overline}>
          {overline}
        </FadeOverflow>
      )}
      <h2 className="min-w-0 text-sm leading-4 font-semibold">
        <FadeOverflow as="span" className="block min-w-0 py-px -my-px" dir="auto" titleWhenClipped={headingTitle}>
          {heading}
        </FadeOverflow>
      </h2>
    </div>
  );
}
