import React from "react";

/** Gives a control a native tooltip. HeroUI's buttons are react-aria-components', which drop a
 * `title` prop (`filterDOMProps`), so the tooltip goes on a wrapper instead; `block` makes the
 * wrapper a block box, for a control that fills its row; `contents` gives it no box at all, for one
 * that is positioned out of flow and must not add a flex item (or its gap) to its parent. */
export function TitledControl({
  title,
  block,
  contents,
  children,
}: {
  title: string | undefined;
  block?: boolean;
  contents?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <span title={title} className={block ? "block" : contents ? "contents" : undefined}>
      {children}
    </span>
  );
}
