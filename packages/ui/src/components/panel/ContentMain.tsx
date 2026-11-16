import React from "react";

/**
 * The `main` of the content panel, which every page that takes the area beside the sidebar renders (the sessions', the
 * Remote devices'): the window's one main landmark, positioned so that it paints above the sidebar and casts the
 * panel's shadow (`--panel-shadow`) over it while a sidebar is docked (`data-pane-docked` on the window), which is how
 * the sidebar's edge reads as a step in the surface, and flexible in the panel's row. A page adds its own layout and
 * attributes through `className` and the element's props.
 */
export function ContentMain({ className = "", ...props }: React.ComponentProps<"main">): React.ReactElement {
  return <main {...props} className={`relative flex min-w-0 flex-1 pane-docked:shadow-(--panel-shadow) ${className}`} />;
}
