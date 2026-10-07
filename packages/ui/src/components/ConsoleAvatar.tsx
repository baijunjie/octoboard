import { LayoutDashboard } from "lucide-react";
import React from "react";

import type { Console } from "../protocol";

/** A console's avatar: its custom image when it has one, otherwise the default glyph on the accent
 * colour. Round either way. Always decorative: the console's name is written out beside it. */
export function ConsoleAvatar({
  icon,
  className = "size-6",
}: {
  icon: Console["icon"];
  /** Sets the size, and nothing else. */
  className?: string;
}): React.ReactElement {
  if (icon) {
    return <img src={icon} alt="" className={`shrink-0 rounded-full object-cover ${className}`} />;
  }
  return (
    <span className={`flex shrink-0 items-center justify-center rounded-full bg-accent text-accent-foreground ${className}`}>
      <LayoutDashboard aria-hidden="true" className="size-[58%]" />
    </span>
  );
}
