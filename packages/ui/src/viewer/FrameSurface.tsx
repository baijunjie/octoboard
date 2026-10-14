import { Surface } from "@heroui/react";
import React from "react";

/**
 * The HeroUI tertiary surface bounds each reading frame from the dialog. Its narrow band also
 * encloses the image checkerboard without changing the image's colours.
 *
 * It also draws the ring of the focused frame it holds, from the state the frame passes as
 * `focused`: the frame's own fade is a mask that clips by its border box, so a ring on the frame
 * itself, which sits outside that box, is cut off. The ring is the project's usual one (2px
 * `--focus`) and is painted outside this band rather than inside it, so the room it needs is the gap
 * the dialog's body leaves around the band, not the band's own padding.
 */
export function FrameSurface({ children, focused }: { children: React.ReactNode; focused?: boolean }): React.ReactElement {
  return (
    <Surface variant="tertiary" className={`flex min-h-0 flex-1 flex-col rounded-2xl p-1 ${focused ? "ring-2 ring-focus" : ""}`}>
      {children}
    </Surface>
  );
}
