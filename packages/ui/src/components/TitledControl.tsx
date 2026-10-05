import React from "react";

/** Gives a control a native tooltip. HeroUI's buttons are react-aria-components', which drop a
 * `title` prop (`filterDOMProps`), so the tooltip goes on a wrapper instead; `block` makes the
 * wrapper a block box, for a control that fills its row. */
export function TitledControl({
  title,
  block,
  children,
}: {
  title: string | undefined;
  block?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <span title={title} className={block ? "block" : undefined}>
      {children}
    </span>
  );
}
