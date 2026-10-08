import React from "react";

import { FadeOverflow } from "./FadeOverflow";

/** A directory path on one line, cut from its start when too long so the directory's own name
 * stays visible, with the full path as the tooltip while it is cut. */
export function PathText({ path, as }: { path: string; as?: "div" | "span" }): React.ReactElement {
  return (
    <FadeOverflow as={as} dir="ltr" clip="start" titleWhenClipped={path}>
      {path}
    </FadeOverflow>
  );
}
