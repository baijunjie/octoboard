import React from "react";

import { useOctoboardTheme } from "../preferences/theme";
import { useArrivalFocusProps } from "../primitives";
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
 *
 * The root draws the focus ring while xterm's textarea, the input that holds the terminal's keyboard focus, has it
 * and that focus arrived by keyboard: F6 or Tab from elsewhere, or a handoff the user did not ask for
 * (`useArrivalFocusProps`). Escape and Tab typed into the terminal after a click are the terminal's keys and do not
 * draw it. The ring is on the root rather than on the textarea, which xterm keeps out of sight; it is not drawn while
 * `covered`, as a cover (every terminal cover is opaque) hides the terminal it would frame.
 */
export function TerminalSurface({
  ref,
  containerRef,
  setLinkRef,
  padding,
  inert,
  covered = false,
  mountMarker,
  editMenu,
  className,
  children,
  onFocus,
  onBlur,
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
  /** A cover hides the terminal (every cover is opaque): no focus ring is drawn for it. */
  covered?: boolean;
  /** A `data-*` attribute that marks the mount element, for what looks for a terminal by it. */
  mountMarker?: `data-${string}`;
  editMenu?: React.ReactNode;
  className: string;
  children?: React.ReactNode;
} & Omit<React.HTMLAttributes<HTMLDivElement>, "style">): React.ReactElement {
  const { resolved: colorTheme } = useOctoboardTheme();
  const focusVisible = useArrivalFocusProps();
  const ring = "ui-keyboard-focus:has-[.xterm-helper-textarea:focus]:ring-2 ui-keyboard-focus:has-[.xterm-helper-textarea:focus]:ring-inset ui-keyboard-focus:has-[.xterm-helper-textarea:focus]:ring-focus";
  return (
    <div
      ref={ref}
      className={covered ? className : `${className} ${ring}`}
      style={{ "--terminal-background": XTERM_THEMES[colorTheme].background, backgroundColor: "var(--terminal-background)" } as React.CSSProperties}
      {...rootProps}
      {...focusVisible}
      onFocus={(event) => { focusVisible.onFocus?.(event); onFocus?.(event); }}
      onBlur={(event) => { focusVisible.onBlur?.(event); onBlur?.(event); }}
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
