import React from "react";

/** The controls at a row's end that show only while the row is hovered, focused, selected
 * (`data-selected`, or `data-current` where a react-aria row marks the one the viewer showed) or has
 * its menu open. Hidden ones take no width, so the name gets the room; they are clipped rather than
 * `display: none`, so they stay in the tab order and a keyboard user reaching one makes it visible.
 * They fade and grow in and out together: the width is a `max-w` that only has to exceed the
 * widest controls (two size-6 buttons and the gap, 50px), as `auto` cannot be transitioned; the
 * tighter the cap, the less of the transition is spent growing past the content. They stay clipped
 * while shown, the width still animating, except under keyboard focus, where a focus ring has to
 * paint outside the box. */
export function RowControls({
  children,
  always,
  className = "",
}: {
  children: React.ReactNode;
  always?: boolean;
  /** Layout the row gives its controls (`ms-auto` to sit at the end of a row with no filler). */
  className?: string;
}): React.ReactElement {
  return (
    <div
      className={`flex shrink-0 items-center gap-0.5 transition-[opacity,max-width] duration-150 motion-reduce:transition-none ${className} ${
        always
          ? ""
          : "max-w-0 overflow-hidden opacity-0 group-hover:max-w-14 group-hover:opacity-100 group-focus-within:max-w-14 group-focus-within:overflow-visible group-focus-within:opacity-100 group-data-selected:max-w-14 group-data-selected:opacity-100 group-data-current:max-w-14 group-data-current:opacity-100 has-[[aria-expanded=true]]:max-w-14 has-[[aria-expanded=true]]:opacity-100"
      }`}
    >
      {children}
    </div>
  );
}
