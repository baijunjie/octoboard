import React from "react";

/** A groove between two rail items, an item of the rail's stack like any other: the line is as wide as the items, and the
 * rail's gap above and below it is all the room it has. `drawn` false keeps the room and leaves the line out. */
export function RailSeparator({ drawn = true, ...rest }: { drawn?: boolean } & React.HTMLAttributes<HTMLDivElement>): React.ReactElement {
  return <div aria-hidden="true" {...rest} className={`groove-t w-8 shrink-0 ${drawn ? "" : "invisible"}`} />;
}

/** The free space of the rail's stack, between the consoles and the bottom group: it takes what is left and pushes the
 * group down. Its own `-mb-2` takes back the gap that follows it, so that with no space left the items either side of it
 * are one gap apart, as any two neighbours are, and not two. */
export const RailSpacer = React.forwardRef<HTMLDivElement, React.HTMLAttributes<HTMLDivElement>>(function RailSpacer(props, ref) {
  return <div ref={ref} aria-hidden="true" {...props} className="-mb-2 min-h-0 flex-1" />;
});
