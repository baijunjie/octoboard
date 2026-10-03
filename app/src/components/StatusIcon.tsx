import React from "react";

import { AGENT_LABEL } from "../agents";
import type { Agent, SessionStatus } from "../protocol";

/**
 * The per-session state icons from the "Session states" table in docs/mvp.md, as plain SVG glyphs
 * rather than emoji (the project keeps user-facing UI free of decorative emoji, the same
 * convention `CLAUDE.md` sets for committed prose).
 *
 * `waiting_user` (the raised hand) is rendered as a plain state marker here, not bubbled up to its
 * project/console nodes and not wired to notifications — that is milestone 02's job.
 * TODO(milestone 02): bubble `waiting_user` up to the project and console nodes and fire a system
 * notification, per the "Waiting for the user (raised hand)" section of docs/mvp.md.
 */
export function StatusIcon({ status }: { status: SessionStatus }): React.ReactElement {
  switch (status) {
    case "working":
      return (
        <svg className="status-icon status-icon-working" viewBox="0 0 16 16" aria-label="working">
          <circle cx="8" cy="8" r="6" fill="none" strokeWidth="2" strokeDasharray="22 10" />
        </svg>
      );
    case "waiting_user":
      return (
        <svg className="status-icon status-icon-waiting-user" viewBox="0 0 16 16" aria-label="waiting for you">
          <path d="M8 2c-.8 0-1.4.6-1.4 1.4v4.2L5.2 6.2c-.5-.5-1.3-.5-1.8 0-.5.5-.5 1.3 0 1.8l3.6 3.6c.4.4 1 .7 1.7.7h2.1c1.4 0 2.6-1.1 2.6-2.6V5.4C13.4 4.6 12.8 4 12 4s-1.4.6-1.4 1.4V3.4C10.6 2.6 10 2 9.2 2s-1.4.6-1.4 1.4" />
        </svg>
      );
    case "idle":
      return (
        <svg className="status-icon status-icon-idle" viewBox="0 0 16 16" aria-label="awaiting instructions">
          <circle cx="8" cy="8" r="3" />
        </svg>
      );
    case "interrupted":
      return (
        <svg className="status-icon status-icon-interrupted" viewBox="0 0 16 16" aria-label="interrupted">
          <rect x="4" y="3" width="3" height="10" />
          <rect x="9" y="3" width="3" height="10" />
        </svg>
      );
    case "archived":
      return (
        <svg className="status-icon status-icon-archived" viewBox="0 0 16 16" aria-label="archived">
          <rect x="2" y="3" width="12" height="3" fill="none" strokeWidth="1.3" />
          <rect x="3" y="6" width="10" height="7" fill="none" strokeWidth="1.3" />
          <line x1="6.5" y1="8.5" x2="9.5" y2="8.5" strokeWidth="1.3" />
        </svg>
      );
  }
}

/** A small text badge identifying a session's agent — no vendor icon assets are available. */
export function AgentBadge({ agent }: { agent: Agent }): React.ReactElement {
  return <span className={`agent-badge agent-badge-${agent}`}>{AGENT_LABEL[agent]}</span>;
}
