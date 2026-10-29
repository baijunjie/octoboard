import React, { useEffect, useRef, useState } from "react";

import { AnchoredTooltip } from "../components/AnchoredTooltip";
import { ValueTooltipText } from "../components/ValueTooltipText";
import { TERMINAL_FONT_FAMILY } from "./TerminalController";
import type { HoveredTerminalLink } from "./terminalLinks";

/**
 * Tells the target of the OSC 8 hyperlink the pointer is on, which its visible text may differ
 * from. It holds the hovered link itself, so a hover re-renders this and not the pane: the pane
 * feeds it through `setLinkRef`, which is set here while mounted.
 *
 * The tooltip answers the pointer alone, and the target reaches no assistive technology by another
 * route. That is accepted, as the terminal's output is not exposed to it at all (xterm's rows are
 * `aria-hidden`, and its screen reader mode is off).
 */
export function TerminalLinkTooltip({
  setLinkRef,
}: {
  setLinkRef: React.RefObject<((link: HoveredTerminalLink | undefined) => void) | undefined>;
}): React.ReactElement | null {
  const [link, setHovered] = useState<HoveredTerminalLink>();
  const anchor = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setLinkRef.current = setHovered;
    return () => {
      setLinkRef.current = undefined;
    };
  }, [setLinkRef]);
  if (!link) return null;
  return (
    <>
      {/* The tooltip is placed against this empty box lying over the link, which stands for the
          link's text and so carries the terminal's font, for the URI to take from it. */}
      <div
        ref={anchor}
        aria-hidden="true"
        className="pointer-events-none fixed"
        style={{ left: link.left, top: link.top, width: link.width, height: link.height, fontFamily: TERMINAL_FONT_FAMILY }}
      />
      {/* Keyed so a move straight from one link to another places the tooltip afresh. */}
      <AnchoredTooltip
        key={`${link.uri}\n${link.left},${link.top}`}
        tooltip={<ValueTooltipText label={anchor} dir="ltr">{link.uri}</ValueTooltipText>}
        anchor={anchor}
        hovered
      />
    </>
  );
}
