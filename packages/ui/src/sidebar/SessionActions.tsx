import { Archive, Trash2 } from "lucide-react";
import React, { useContext, type RefObject } from "react";
import { useShallow } from "zustand/react/shallow";

import { ariaBinding } from "../shortcuts/binding";
import { useBinding } from "../shortcuts/store";
import { FocusModeContext } from "./focusModeContext";
import { selectedSessionFocusTarget } from "./focus";
import { pickableAgents } from "../agents/agents";
import { ActionMenu } from "../components/menus/ActionMenu";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { useHost } from "../platform/react";
import type { Account, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { indexSessions, sessionsOf } from "../sessions/sessionIndex";
import { useAccountSwitchHeld, useDaemon, useDaemonStore } from "../store";
import { type MenuPlacement, offersDelete, sessionMenu } from "./menus";
import { archiveNeedsConfirmation, isUnusedSession } from "../sessions/order";
import { RowIconButton } from "../components/rows/RowIconButton";
import type { SidebarHandlers } from "./types";

/** A session row's or card's controls: its action menu, and before it a Delete or Archive shortcut
 * only while the sessions alone call for no confirmation of that action. An archive through the
 * shortcut can still ask, once the daemon says a branch or worktree bound to the session needs the
 * user's word. When a shortcut disappears while holding keyboard focus, focus moves to the row
 * (`rowRef`). */
export function SessionActions({
  handlers,
  session,
  accounts,
  hasBoundSessions,
  rowRef,
  placement,
  selected = false,
}: {
  handlers: SidebarHandlers;
  session: Session;
  accounts: Account[];
  hasBoundSessions: boolean;
  rowRef: RefObject<HTMLElement | null>;
  /** Where the menu is opened from; `focused` is a console session's own focus mode. */
  placement?: MenuPlacement;
  selected?: boolean;
}): React.ReactElement {
  const t = useT();
  const inFocusMode = useContext(FocusModeContext);
  const host = useHost();
  const focusBinding = useBinding("focusMode");
  const focusTarget = useDaemonStore(useShallow((s) => selected && !inFocusMode ? selectedSessionFocusTarget(session, s.projects, s.sessions) : undefined));
  const { store } = useDaemon();
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  // Only what is bound to the session can ask for more than its status does, and that is all in its
  // console, so a row reads its console's sessions alone, and none when nothing is bound to it.
  const atOnce = useDaemonStore((s) => {
    const index = indexSessions(s.sessions);
    const related = index.boundOwnerIds.has(session.id) ? sessionsOf(index.byConsole, session.console_id) : [];
    return !archiveNeedsConfirmation(related, session);
  });
  const canHandoff = useDaemonStore((s) => pickableAgents(s.agentAvailability, s.settings.added_agents, s.providers).some((agent) => agent !== session.agent));
  // The menu's switch-account holds follow it while open, so it is read here rather than when the menu opens.
  const switching = useAccountSwitchHeld(session);
  const handoff = useFocusHandoff(() => rowRef.current?.focus());
  const title = sessionDisplayTitle(t, session);
  const deletable = isUnusedSession(session, hasBoundSessions);
  const archivable = !offersDelete(session, hasBoundSessions) && atOnce;
  return (
    <>
      {!session.locked && (deletable || archivable) && (
        <div className="contents" {...handoff}>
          <RowIconButton
            icon={deletable ? Trash2 : Archive}
            label={t(deletable ? "sidebar.session.delete" : "sidebar.session.archive")}
            ariaLabel={t(deletable ? "sidebar.session.deleteItem" : "sidebar.session.archiveItem", { title: isolate(title) })}
            onPress={() => deletable
              ? handlers.onOpenDialog({ kind: "delete-session", session })
              : handlers.onArchiveSession(session)}
          />
        </div>
      )}
      <ActionMenu label={t("sidebar.session.actions", { title: isolate(title) })} items={() => sessionMenu(t, handlers, session, accounts, defaultNames, { host, placement, focusTarget, focusShortcut: ariaBinding(focusBinding, host), hasBoundSessions, canHandoff, switching, quota: { quotas: store.getState().accountQuotas, now: Date.now() }, providers: store.getState().providers, sharing: store.getState().accountSharing, disabledDefaults: store.getState().settings.disabled_default_accounts, accountOrder: store.getState().settings.account_order })} contextTargetRef={rowRef} />
    </>
  );
}
