import { CircleAlert, Info, TriangleAlert } from "lucide-react";
import React, { useMemo } from "react";

import { StatusMark } from "../components/StatusMark";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useMarkBesideLink } from "./linkMarks";
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
 *
 * A `failed` notice is no sentence in the paragraph: it is the minimal mark of a failure that
 * belongs to one spot, the glyph alone, named by the sentence and opening it in a popover that a
 * press or a resting mouse opens (`docs/ui-accessibility.md`, "A failure that belongs to one row").
 * The sentence is `speak`, or `children` when that is a string; `speak` is announced as for the
 * other tones. The mark is a button, so it is a tab stop in the running text; inside a link it is
 * drawn right after the link rather than in it.
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
  const failure = tone === "failed" ? speak ?? (typeof children === "string" ? children : undefined) : undefined;
  const announcer = speak !== undefined && <StatusAnnouncer text={speak} firstWriteDelayMs={FIRST_ANNOUNCEMENT_DELAY_MS} element="span" />;
  const mark = useMemo(() => (failure === undefined ? undefined : <span className="mx-1"><StatusMark inline label={failure} /></span>), [failure]);
  // Inside a link the mark is drawn beside it, outside the anchor (see `LinkWithMarks`).
  const besideLink = useMarkBesideLink(mark);
  if (mark !== undefined) {
    return (
      <>
        {!besideLink && mark}
        {announcer}
      </>
    );
  }
  const Icon = ICONS[tone];
  return (
    <>
      <span aria-hidden={speak !== undefined || undefined} className="mx-1 inline-flex items-baseline gap-1 align-baseline">
        <Icon aria-hidden="true" className="size-3.5 shrink-0 translate-y-0.5" />
        <span>{children}</span>
      </span>
      {announcer}
    </>
  );
}
