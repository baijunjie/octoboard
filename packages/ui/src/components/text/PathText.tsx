import React from "react";

import { FadeOverflow } from "./FadeOverflow";

/** A directory path on one line, cut from its start when too long so the directory's own name
 * stays visible, with the full path as the tooltip while it is cut. `shown` is the wording when
 * it differs from the path (the path with the home directory as `~`): the tooltip is then the
 * full path even while it fits. `className` supplies the sizing, as for `FadeOverflow`. It is
 * always a `span`. */
export function PathText({
  path,
  shown = path,
  className,
}: {
  path: string;
  shown?: string;
  className?: string;
}): React.ReactElement {
  return (
    <FadeOverflow as="span" className={className} dir="ltr" clip="start" titleWhenClipped={path} title={shown === path ? undefined : path}>
      {shown}
    </FadeOverflow>
  );
}
