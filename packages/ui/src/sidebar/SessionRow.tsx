import React, { memo, useId, useRef } from "react";

import { SessionAgentIcon } from "../components/icons/SessionAgentIcon";
import { RowControls, RowIconSlot } from "../components/controls/RowControls";
import { RowLabel, TreeRow } from "../components/rows/TreeRow";
import { hasVisibleContext, SessionContextLabel } from "../components/status/SessionMetadata";
import { StatusIcon } from "../components/status/StatusIcon";
import { BindingBadge } from "../components/status/BindingBadge";
import { useT } from "../i18n/react";
import type { Session } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { useHasBoundSessions, useSession, useSessionShownStatus } from "../store";
import { SessionMarkButtons, hasSessionMarks } from "./MarkButtons";
import { SessionActions } from "./SessionActions";
import { useSessionRowModel } from "./sessionRowModel";
import type { SidebarHandlers } from "./types";

/** A session row: a project session's own, or one of the console's console sessions
 * (`ConsoleSessionsSection`), which share the row and its menu — pin, rename, archive, no resume —
 * since both are ordinary sessions once the row stops being a console session's sole, special one.
 * The session a bound one reports to is its `under` when that is the project session listed right
 * above it, otherwise the console session it is bound to, read from the store here, so that a change
 * of another session never renders this row: a console session is drawn as the binding badge in its
 * colour, while a project session — a bound coordinator, or any project session that started this
 * one — is not, since the row is already listed under it; either way the row's own accessible name
 * names it, which is what carries the nesting to assistive technology. Memoised, with `selected` a
 * flag rather than the selected id, so a change of selection renders only the two rows it moves
 * between. */
export const SessionRow = memo(function SessionRow({
  handlers,
  session,
  under,
  selected,
}: {
  handlers: SidebarHandlers;
  session: Session;
  under?: Session;
  selected: boolean;
}): React.ReactElement {
  const metadataId = useId();
  const t = useT();
  const hasBound = useHasBoundSessions(session.id);
  const bound = useSession(session.bound_to);
  const owner = under ?? (bound?.role === "console" ? bound : undefined);
  const rowRef = useRef<HTMLDivElement>(null);
  const badgeOwner = session.role === "console" ? session : owner?.role === "console" ? owner : undefined;
  const status = useSessionShownStatus(session);
  // The model and effort are in the agent mark's tooltip only, so the row's name carries them too.
  const { accounts, defaultNames, launch, name } = useSessionRowModel(session, [owner], status);
  return (
    <TreeRow
      ref={rowRef}
      describedBy={hasVisibleContext(session.context) ? metadataId : undefined}
      ariaLabel={name}
      selected={selected}
      locked={session.locked}
      onActivate={() => handlers.onSelectSession(session)}
    >
      {/* No touch popover on the marks of a pressable row: a tap on one is the row's press. */}
      <StatusIcon status={status} decorative withTooltip touchPopover={false} />
      <SessionAgentIcon session={session} accounts={accounts} defaultNames={defaultNames} launch={launch} touchPopover={false} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <RowLabel title={sessionDisplayTitle(t, session)} className="min-w-0">{sessionDisplayTitle(t, session)}</RowLabel>
        {hasVisibleContext(session.context) && <span id={metadataId} className="shrink-0"><SessionContextLabel context={session.context} touchPopover={false} /></span>}
      </div>
      <RowControls
        fixed={badgeOwner?.colour || hasSessionMarks(session) ? (
          <>
            {/* A console session shows its own colour; a bound session shows its owner's. */}
            {badgeOwner?.colour && <RowIconSlot><BindingBadge owner={badgeOwner} decorative={session.role === "console"} /></RowIconSlot>}
            <SessionMarkButtons handlers={handlers} session={session} returnFocusTo={rowRef} />
          </>
        ) : undefined}
      >
        <SessionActions handlers={handlers} session={session} accounts={accounts} hasBoundSessions={hasBound} rowRef={rowRef} />
      </RowControls>
    </TreeRow>
  );
});
