import { Archive, CircleSmall, Hand, LoaderCircle, Pause } from "lucide-react";
import React from "react";

import type { SessionStatus } from "../protocol";
import { STATUS_LABEL } from "../sessionLabel";

const ICON_CLASS = "size-4 shrink-0";

/**
 * The per-session status icons, one per status in the "Session statuses" table of
 * docs/product/sessions.md, as lucide glyphs rather than emoji (the project keeps user-facing UI
 * free of decorative emoji, the same convention `CLAUDE.md` sets for committed prose). Each
 * carries the status as its accessible name, since it is the row's only visible status cue.
 */
export function StatusIcon({ status }: { status: SessionStatus }): React.ReactElement {
  const a11y = { role: "img", "aria-label": STATUS_LABEL[status] } as const;
  switch (status) {
    case "working":
      return <LoaderCircle {...a11y} className={`${ICON_CLASS} animate-spin text-accent`} />;
    case "waiting_user":
      return <Hand {...a11y} className={`${ICON_CLASS} text-warning`} />;
    case "idle":
      return <CircleSmall {...a11y} className={`${ICON_CLASS} fill-success text-success`} />;
    case "interrupted":
      return <Pause {...a11y} className={`${ICON_CLASS} fill-muted text-muted`} />;
    case "archived":
      return <Archive {...a11y} className={`${ICON_CLASS} text-muted`} />;
  }
}

/** The secondary marker a project or console row shows when a session beneath it (not the row's
 * own session) is `waiting_user`, so the raised hand stays visible with the tree collapsed. The
 * row's `aria-label` carries the words, since this glyph is decorative beside it. */
export function BubbledWaitingHand(): React.ReactElement {
  return <Hand aria-hidden="true" className="size-3.5 shrink-0 text-warning" />;
}
