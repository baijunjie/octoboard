import React from "react";

import { AGENT_LABEL } from "../agents";
import { STATUS_LABEL } from "../sessionLabel";
import type { Agent, SessionStatus } from "../protocol";

/** The raised-hand glyph path, shared by the per-session status icon below and `BubbledWaitingHand`
 * — the marker a project or console row shows when a session beneath it is waiting, per the
 * "The raised hand" section of docs/product/sessions.md. */
const RAISED_HAND_PATH =
  "M8 2c-.8 0-1.4.6-1.4 1.4v4.2L5.2 6.2c-.5-.5-1.3-.5-1.8 0-.5.5-.5 1.3 0 1.8l3.6 3.6c.4.4 1 .7 1.7.7h2.1c1.4 0 2.6-1.1 2.6-2.6V5.4C13.4 4.6 12.8 4 12 4s-1.4.6-1.4 1.4V3.4C10.6 2.6 10 2 9.2 2s-1.4.6-1.4 1.4";

/**
 * The per-session status icons, one per status in the "Session statuses" table of
 * docs/product/sessions.md, as plain SVG glyphs rather than emoji (the project keeps user-facing UI
 * free of decorative emoji, the same convention `CLAUDE.md` sets for committed prose).
 */
export function StatusIcon({ status }: { status: SessionStatus }): React.ReactElement {
  switch (status) {
    case "working":
      return (
        <svg className="status-icon status-icon-working" viewBox="0 0 16 16" aria-label={STATUS_LABEL[status]}>
          <circle cx="8" cy="8" r="6" fill="none" strokeWidth="2" strokeDasharray="22 10" />
        </svg>
      );
    case "waiting_user":
      return (
        <svg className="status-icon status-icon-waiting-user" viewBox="0 0 16 16" aria-label={STATUS_LABEL[status]}>
          <path d={RAISED_HAND_PATH} />
        </svg>
      );
    case "idle":
      return (
        <svg className="status-icon status-icon-idle" viewBox="0 0 16 16" aria-label={STATUS_LABEL[status]}>
          <circle cx="8" cy="8" r="3" />
        </svg>
      );
    case "interrupted":
      return (
        <svg className="status-icon status-icon-interrupted" viewBox="0 0 16 16" aria-label={STATUS_LABEL[status]}>
          <rect x="4" y="3" width="3" height="10" />
          <rect x="9" y="3" width="3" height="10" />
        </svg>
      );
    case "archived":
      return (
        <svg className="status-icon status-icon-archived" viewBox="0 0 16 16" aria-label={STATUS_LABEL[status]}>
          <rect x="2" y="3" width="12" height="3" fill="none" strokeWidth="1.3" />
          <rect x="3" y="6" width="10" height="7" fill="none" strokeWidth="1.3" />
          <line x1="6.5" y1="8.5" x2="9.5" y2="8.5" strokeWidth="1.3" />
        </svg>
      );
  }
}

/** The secondary marker a project or console row shows when a session beneath it (not the row's
 * own session) is `waiting_user`, so the raised hand stays visible with the tree collapsed. Kept
 * visually distinct from `StatusIcon`'s own waiting-user glyph — smaller, its own class — so it
 * reads as "something below needs you" rather than the row's own status; the row's `aria-label`
 * carries the words, since this glyph is decorative beside it. */
export function BubbledWaitingHand(): React.ReactElement {
  return (
    <svg className="status-icon-bubbled-hand" viewBox="0 0 16 16" aria-hidden="true">
      <path d={RAISED_HAND_PATH} />
    </svg>
  );
}

/** A small text badge identifying a session's agent — no vendor icon assets are available. */
export function AgentBadge({ agent }: { agent: Agent }): React.ReactElement {
  return <span className={`agent-badge agent-badge-${agent}`}>{AGENT_LABEL[agent]}</span>;
}
