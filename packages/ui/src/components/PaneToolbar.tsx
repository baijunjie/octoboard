import type React from "react";

/** The strip of a right-pane panel that holds a field or controls under its header, over the content: the
 * file-name filter of the Files and Changes panels, the session filter of the Processes panel. One padding, one
 * gap and one divider, so a field is the same distance from the header and the content in every panel. */
export function PaneToolbar({ className, ...rest }: React.ComponentProps<"div">): React.ReactElement {
  return <div className={`flex shrink-0 items-center gap-2 groove-b px-(--pane-inset) py-2${className ? ` ${className}` : ""}`} {...rest} />;
}
