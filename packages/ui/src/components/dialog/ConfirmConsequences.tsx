import React from "react";

import { FadeOverflow } from "../text/FadeOverflow";
import { SessionAgentIcon } from "../icons/SessionAgentIcon";
import { StatusIcon } from "../status/StatusIcon";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../announce/StatusAnnouncer";
import { useAlertAnnouncement } from "../announce/useAlertAnnouncement";
import { useListMotion } from "../motion/useListMotion";
import { isolate } from "../../i18n/isolate";
import { useT } from "../../i18n/react";
import type { Session } from "../../protocol";
import { sessionAgentLabel, sessionDisplayTitle } from "../../sessions/sessionLabel";
import { useDaemonStore, useProviders, useSessionShownStatus } from "../../store";

/** A line the dialog's consequences add while an option is ticked: `key` names its kind, so a line
 * whose count changes stays the same line. */
export type ConsequenceLine = { key: string; text: React.ReactNode };

function SessionStatusIcon({ session }: { session: Session }): React.ReactElement {
  return <StatusIcon status={useSessionShownStatus(session)} withTooltip touchPopover />;
}

/** The sessions a confirmation names, each with its agent's icon and title, plus a status glyph for live
 * sessions; archived sessions are already identified by the wording of what heads the list. */
export function SessionList({ sessions }: { sessions: Session[] }): React.ReactElement {
  const t = useT();
  const sessionsRef = useListMotion<HTMLUListElement>();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const providers = useProviders();
  return (
    <ul ref={sessionsRef} className="relative flex min-w-0 flex-col gap-1">
      {sessions.map((session) => (
        <li key={session.id} data-list-motion={session.id} className="flex min-w-0 items-center gap-2">
          {/* The list items are not pressable, so a tap on a mark opens the popover. */}
          {session.status !== "archived" && <SessionStatusIcon session={session} />}
          <SessionAgentIcon providers={providers} session={session} accounts={accounts} defaultNames={defaultNames} touchPopover />
          {/* The icon is decorative, and nothing else here names the agent. The comma keeps the name apart from the
              title when the row is read as one phrase: a hidden element's trailing space is dropped in rendering. */}
          <span className="sr-only">{`${isolate(sessionAgentLabel(t, providers, session, accounts, defaultNames))}, `}</span>
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
  const announcement = useAlertAnnouncement(announceTitle, undefined, announceTitle ? AFTER_FOCUS_MOVE_DELAY_MS : undefined);
  if (!title && lines.length === 0) return null;
  return (
    <div className="flex min-w-0 flex-col gap-2 text-sm">
      {title && <p id={titleId} key={announceTitle ? announcement.key : undefined} role={announceTitle ? announcement.role : undefined}>{title}</p>}
      {sessions && <SessionList sessions={sessions} />}
      <ConsequenceLines lines={lines} />
    </div>
  );
}
