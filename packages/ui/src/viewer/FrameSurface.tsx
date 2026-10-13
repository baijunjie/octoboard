import { Surface } from "@heroui/react";
import React from "react";

/**
 * The HeroUI surface every scrolling frame of the viewer sits in: plain text, highlighted code, a
 * diff, a rendered document and an image's checkerboard. The frames are filled with the code theme's
 * own background, which is white in the light appearance, and so is the dialog; the tertiary
 * surface around them is what bounds them from the dialog there (a band 1.20:1 off the white
 * dialog). In dark it adds to the line the darker code background draws: the band is 1.18:1 off the
 * dialog and the code background 1.31:1 off the band, where it was 1.10:1 off the dialog alone. A
 * surface, not a border, because it is HeroUI's own fill and moves with the theme. The padding is
 * the width of that band.
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
