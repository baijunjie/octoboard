import React from "react";

import { FadeOverflow } from "./FadeOverflow";

/** A directory path on one line, cut from its start when too long so the directory's own name
 * stays visible, with the full path as the tooltip while it is cut. */
export function PathText({
  path,
  as,
  className,
}: {
  path: string;
  as?: "div" | "span";
  className?: string;
}): React.ReactElement {
  return (
    <FadeOverflow as={as} className={className} dir="ltr" clip="start" titleWhenClipped={path}>
      {path}
    </FadeOverflow>
  );
}
