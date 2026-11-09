import React from "react";

import { useOctoboardTheme } from "../preferences/theme";
import { TerminalLinkTooltip } from "./TerminalLinkTooltip";
import type { HoveredTerminalLink } from "./links/terminalLinks";
import { XTERM_THEMES } from "./xtermThemes";

/**
 * What a terminal view is drawn on, shared by the session pane and a shell terminal's view: a root
 * in the active xterm theme's background, the padding wrapper around the element xterm is mounted
 * in, that element, and the link tooltip. `children` (the cover, the status regions, the notice bar)
 * follow it in the root, and `editMenu` is the terminal's editing menu, which attaches to the
 * mount element.
 *
 * The root's `--terminal-background` is `XTERM_THEMES`' own background for the resolved theme, read
 * here rather than duplicated as a literal, so the padding around the terminal is never a different
 * colour; a plain Tailwind class cannot follow a colour that changes with the theme at runtime. The
 * padding sits on a wrapper, not on the mount element: the fit addon sizes the terminal from that
 * element's computed height and width, padding included, so padding there would give it rows and
 * columns that the overflow then clips. Isolation keeps xterm's transparent link canvas below the
 * overlays so it cannot intercept their clicks.
 */
export function TerminalSurface({
  ref,
  containerRef,
  setLinkRef,
  padding,
  inert,
  mountMarker,
  editMenu,
  className,
  children,
  ...rootProps
}: {
  ref?: React.Ref<HTMLDivElement>;
  /** The element xterm is mounted in. */
  containerRef: React.RefObject<HTMLDivElement | null>;
  setLinkRef: React.RefObject<((link: HoveredTerminalLink | undefined) => void) | undefined>;
  /** The padding classes of the wrapper around the mount element. */
  padding: string;
  /** The terminal is out of reach (no session to show, switching, or its process cannot be reached). */
  inert?: boolean;
  /** A `data-*` attribute that marks the mount element, for what looks for a terminal by it. */
  mountMarker?: `data-${string}`;
  editMenu?: React.ReactNode;
  className: string;
  children?: React.ReactNode;
} & Omit<React.HTMLAttributes<HTMLDivElement>, "style">): React.ReactElement {
  const { resolved: colorTheme } = useOctoboardTheme();
  return (
    <div
      ref={ref}
      className={className}
      style={{ "--terminal-background": XTERM_THEMES[colorTheme].background, backgroundColor: "var(--terminal-background)" } as React.CSSProperties}
      {...rootProps}
    >
      <div inert={inert} className={`isolate min-h-0 flex-1 ${padding}`}>
        {/* xterm lays its cells out left to right whatever the page's direction. */}
        <div dir="ltr" {...(mountMarker && { [mountMarker]: true })} className="h-full overflow-hidden" ref={containerRef} />
      </div>
      {editMenu}
      <TerminalLinkTooltip setLinkRef={setLinkRef} />
      {children}
    </div>
  );
}
