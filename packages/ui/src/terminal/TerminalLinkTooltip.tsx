import React, { useEffect, useRef, useState } from "react";

import { AnchoredTooltip } from "../components/AnchoredTooltip";
import type { HoveredTerminalLink } from "./terminalLinks";

/**
 * Tells the target of the OSC 8 hyperlink the pointer is on, which its visible text may differ
 * from. It holds the hovered link itself, so a hover re-renders this and not the pane: the pane
 * feeds it through `setLink`, which is set here while mounted.
 *
 * The tooltip answers the pointer alone, and the target reaches no assistive technology by another
 * route. That is accepted, as the terminal's output is not exposed to it at all (xterm's rows are
 * `aria-hidden`, and its screen reader mode is off).
 */
export function TerminalLinkTooltip({
  setLink,
}: {
  setLink: React.RefObject<((link: HoveredTerminalLink | undefined) => void) | undefined>;
}): React.ReactElement | null {
  const [link, setHovered] = useState<HoveredTerminalLink>();
  const anchor = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setLink.current = setHovered;
    return () => {
      setLink.current = undefined;
    };
  }, [setLink]);
  if (!link) return null;
  return (
    <>
      {/* The tooltip is placed against this empty box lying over the link. */}
      <div
        ref={anchor}
        aria-hidden="true"
        className="pointer-events-none fixed"
        style={{ left: link.left, top: link.top, width: link.width, height: link.height }}
      />
      {/* Keyed so a move straight from one link to another places the tooltip afresh. A URI reads
          left to right whatever the interface's direction. */}
      <AnchoredTooltip
        key={`${link.uri}\n${link.left},${link.top}`}
        tooltip={<span dir="ltr">{link.uri}</span>}
        anchor={anchor}
        hovered
      />
    </>
  );
}
