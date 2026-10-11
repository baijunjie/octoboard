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
 */
export function FrameSurface({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <Surface variant="tertiary" className="flex min-h-0 flex-1 flex-col rounded-2xl p-1">
      {children}
    </Surface>
  );
}
