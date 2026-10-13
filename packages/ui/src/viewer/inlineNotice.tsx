import { CircleAlert, Info, TriangleAlert } from "lucide-react";
import React from "react";

import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { FIRST_ANNOUNCEMENT_DELAY_MS, type NoticeTone } from "./viewerStates";

const ICONS = { explained: Info, warned: TriangleAlert, failed: CircleAlert } as const;

/**
 * A notice that sits inside a paragraph, where the viewer's `Notice` cannot: that one is a `div`,
 * and a `div` inside a `p` is not valid. The sentence keeps the surrounding colour, which already
 * meets the document's contrast; the glyph tells the tones apart, so colour is not what does.
 *
 * `speak` is announced once, after the same delay as the viewer's other notices, and the visible
 * sentence is then hidden from assistive technology so it is not read twice. A notice that is
 * present from the first paint passes no `speak`. The announcement is a `span`: this notice
 * already sits where a `div` cannot.
 */
export function InlineNotice({
  tone = "explained",
  speak,
  children = speak,
}: {
  tone?: NoticeTone;
  speak?: string;
  children?: React.ReactNode;
}): React.ReactElement {
  const Icon = ICONS[tone];
  const hidden = speak !== undefined;
  return (
    <>
      <span aria-hidden={hidden || undefined} className="mx-1 inline-flex items-baseline gap-1 align-baseline">
        <Icon aria-hidden="true" className="size-3.5 shrink-0 translate-y-0.5" />
        <span>{children}</span>
      </span>
      {speak !== undefined && (
        <StatusAnnouncer text={speak} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} element="span" />
      )}
    </>
  );
}
