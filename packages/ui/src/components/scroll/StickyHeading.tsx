import React from "react";

import { RowIconGroup } from "../controls/RowControls";
import { useStickyHeadingFocus } from "./stickyHeadingFocus";

/** How a sticky heading looks, by where it sits:
 * - `list`: a label over a group of sidebar rows, muted and small, with `action` and `inline` beside it.
 * - `group`: the title of a group of rows in a Settings section, with a divider under it.
 * - `category`: the title of a side pane's category, with a `note` under it, bleeding to the pane's edges. */
export type StickyHeadingVariant = StickyHeadingProps["variant"];

type StickyHeadingProps = { id?: string; children: React.ReactNode } & (
  | { variant: "list"; action?: React.ReactNode; inline?: React.ReactNode }
  | { variant: "group" }
  | { variant: "category"; note: React.ReactNode }
);

/** The fixed height of a heading of each variant, which the scroller pinning it registers as its
 * `stickyHeadingHeight` (see `FadedScroller`). A `category` heading wraps its note, so its owner
 * measures it instead. */
export const STICKY_HEADING_HEIGHT = { list: 38, group: 40 } as const satisfies Partial<Record<StickyHeadingVariant, number>>;

/** The pinned element's own look. Each pins flush with its scroller's edge: the Settings section's
 * scroller has vertical padding (`--section-padding`), which a `group` heading takes negated as its `top`. */
const PINNED: Record<StickyHeadingVariant, string> = {
  list: "top-0 flow-root bg-panel-sidebar",
  group: "top-[calc(-1*var(--section-padding))] mt-4 flex h-[40px] items-center groove-b bg-overlay text-base font-semibold",
  category: "top-0 -mx-(--pane-inset) flex shrink-0 flex-col gap-1 bg-panel-sidebar px-(--pane-inset) py-3",
};

/**
 * A section heading that pins to the top of its scroller while the section's content scrolls under it,
 * and is pushed out of the way by the next section's, since it stays inside its own section. It is an
 * `h3`, under the view's `h2` title; `id` names it for a `<section aria-labelledby>` around it. The
 * pinned element is opaque and marked `data-sticky-heading`, which `FadedScroller` places its top fade
 * by, and keeps focus moves among its controls (`action`, `inline`) from scrolling the list
 * (`useStickyHeadingFocus`).
 *
 * `action` sits at the row's end, and `inline` (a filter in force) between it and the label, taking
 * the room the two leave and sizing and clipping itself within it; several controls in `action` are
 * spaced by the heading, so a caller hands them over as a fragment. `note` is a line under the title.
 */
export function StickyHeading(props: StickyHeadingProps): React.ReactElement {
  const { variant, id, children } = props;
  // A `category` heading has no fixed height (its owner measures it); it holds no control, so the depth its
  // controls' scroll area would move by does not matter.
  const { className: pinnedFocusClass, ...pinnedFocus } = useStickyHeadingFocus(variant === "category" ? 0 : STICKY_HEADING_HEIGHT[variant]);
  const action = props.variant === "list" ? props.action : undefined;
  const inline = props.variant === "list" ? props.inline : undefined;
  const note = props.variant === "category" ? props.note : undefined;
  return (
    <div {...pinnedFocus} className={`sticky z-3 ${PINNED[variant]} ${pinnedFocusClass}`}>
      {variant === "list" ? (
        <div className="mt-2 mb-0.5 px-(--row-content-inset)">
          <div className="flex items-center gap-2">
            <h3 id={id} className={`flex h-7 items-center text-xs font-medium text-app-muted ${inline ? "shrink-0" : "min-w-0 flex-1"}`}>{children}</h3>
            {inline && <div className="flex h-7 min-w-0 flex-1 items-center">{inline}</div>}
            {action && <RowIconGroup className="h-7">{action}</RowIconGroup>}
          </div>
        </div>
      ) : (
        <h3 id={id} className={variant === "category" ? "text-sm font-semibold" : undefined}>{children}</h3>
      )}
      {note && <p className="text-xs text-app-muted">{note}</p>}
    </div>
  );
}
