import React from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { SessionAgentIcon } from "../components/SessionAgentIcon";
import { StatusIcon } from "../components/StatusIcon";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { useAnnouncement } from "../components/useAnnouncement";
import { useListMotion } from "../components/useListMotion";
import { useT } from "../i18n/react";
import type { Session } from "../protocol";
import { sessionAgentLabel, sessionDisplayTitle, sessionShownStatusKind } from "../sessionLabel";
import { useDaemonStore } from "../store";
import type { ConsequenceLine } from "./BranchCleanupOptions";

/** The sessions a confirmation names, each with its agent's icon and title, plus a status glyph for live
 * sessions; archived sessions are already identified by the wording of what heads the list. */
export function SessionList({ sessions }: { sessions: Session[] }): React.ReactElement {
  const t = useT();
  const sessionsRef = useListMotion<HTMLUListElement>();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  return (
    <ul ref={sessionsRef} className="relative flex min-w-0 flex-col gap-1">
      {sessions.map((session) => (
        <li key={session.id} data-list-motion={session.id} className="flex min-w-0 items-center gap-2">
          {/* The list items are not pressable, so a tap on a mark opens the popover. */}
          {session.status !== "archived" && <StatusIcon status={sessionShownStatusKind(session)} withTooltip touchPopover />}
          <SessionAgentIcon session={session} accounts={accounts} defaultNames={defaultNames} touchPopover />
          {/* The icon is decorative, and nothing else here names the agent. The comma keeps the name apart from the
              title when the row is read as one phrase: a hidden element's trailing space is dropped in rendering. */}
          <span className="sr-only">{`${sessionAgentLabel(t, session, accounts, defaultNames)}, `}</span>
          <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={sessionDisplayTitle(t, session)}>
            {sessionDisplayTitle(t, session)}
          </FadeOverflow>
        </li>
      ))}
    </ul>
  );
}

/** The consequence lines of what is ticked, one paragraph each. Always mounted, so a line added or
 * removed by an option moves in and out. */
export function ConsequenceLines({ lines }: { lines: ConsequenceLine[] }): React.ReactElement {
  const linesRef = useListMotion<HTMLDivElement>();
  return (
    <div ref={linesRef} className="relative flex min-w-0 flex-col gap-1">
      {lines.map((line) => <p key={line.key} data-list-motion={line.key}>{line.text}</p>)}
    </div>
  );
}

/** What a confirmation says it is about to take along or take away, as body text under its options:
 * `title` when there is one, over the list of sessions when there is one to show (`SessionList`),
 * and `lines`: further consequences, such as one for each option the user has ticked.
 * Nothing at all when there is neither a title nor a line. `titleId` names the title for a control it describes, and
 * `announceTitle` speaks it once as the consequences appear, after the move of focus or the state change that brought
 * them (`ErrorAlert`'s `announceDelayMs`). */
export function ConfirmConsequences({ title, titleId, announceTitle = false, sessions, lines = [] }: { title?: string; titleId?: string; announceTitle?: boolean; sessions?: Session[]; lines?: ConsequenceLine[] }): React.ReactElement | null {
  const announcement = useAnnouncement(announceTitle, undefined, announceTitle ? AFTER_FOCUS_MOVE_DELAY_MS : undefined);
  if (!title && lines.length === 0) return null;
  return (
    <div className="flex min-w-0 flex-col gap-2 text-sm">
      {title && <p id={titleId} key={announceTitle ? announcement.key : undefined} role={announceTitle ? announcement.role : undefined}>{title}</p>}
      {sessions && <SessionList sessions={sessions} />}
      <ConsequenceLines lines={lines} />
    </div>
  );
}
