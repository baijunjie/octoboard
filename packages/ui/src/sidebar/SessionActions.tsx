import { Archive } from "lucide-react";
import React, { type RefObject } from "react";

import { ActionMenu } from "../components/ActionMenu";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Account, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { offersDelete, sessionMenu } from "./menus";
import { archiveNeedsConfirmation } from "./order";
import { RowIconButton } from "./rows";
import type { SidebarHandlers } from "./types";

/** A session row's or card's own controls: its action menu, and before it an Archive shortcut shown
 * only while archiving would go through at once — a session at its prompt or with no process and
 * nothing unarchived bound under it — and the menu offers Archive rather than Delete. A session the
 * app is resuming still asks first when it is pressed. The shortcut disappears while it holds keyboard
 * focus when the session starts working or gets a session bound to it, and focus then moves to the row
 * (`rowRef`); when pressing it archives the session, the row itself leaves. */
export function SessionActions({
  handlers,
  session,
  accounts,
  hasBoundSessions,
  rowRef,
}: {
  handlers: SidebarHandlers;
  session: Session;
  accounts: Account[];
  hasBoundSessions: boolean;
  rowRef: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  const atOnce = useDaemonStore((s) => !archiveNeedsConfirmation(s.sessions.values(), session));
  const handoff = useFocusHandoff(() => rowRef.current?.focus());
  const title = sessionDisplayTitle(t, session);
  return (
    <>
      {atOnce && !offersDelete(session, hasBoundSessions) && (
        <div className="contents" {...handoff}>
          <RowIconButton
            icon={Archive}
            label={t("sidebar.session.archive")}
            ariaLabel={t("sidebar.session.archiveItem", { title: isolate(title) })}
            onPress={() => handlers.onArchiveSession(session)}
          />
        </div>
      )}
      <ActionMenu label={t("sidebar.session.actions", { title: isolate(title) })} items={sessionMenu(t, handlers, session, accounts, { hasBoundSessions })} contextTargetRef={rowRef} />
    </>
  );
}
