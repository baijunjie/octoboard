import { flushSync } from "react-dom";
import { useShallow } from "zustand/react/shallow";

import { useStableHandler } from "../components/hooks/useLatest";
import type { Session } from "../protocol";
import { switchStrip } from "../sessions/order";
import { indexSessions, NO_SESSIONS, sessionsOf } from "../sessions/sessionIndex";
import { KEY_SWITCH_SELECTION, selectedSessionFocusTarget } from "../sidebar/focus";
import { useFocusShortcut } from "../sidebar/focusShortcut";
import { useSwitchShortcut } from "../sidebar/switchShortcut";
import type { FocusTarget } from "../sidebar/types";
import { useDaemonStore } from "../store";

/**
 * The sidebar's focus mode from the keyboard: one key enters the selected session's context, another leaves focus
 * mode (`enterFocusMode` and `exitFocusMode`), and ⌃Tab moves along a console session's switch strip; none of them
 * while a page shows in place of the sessions (`suspended`). `stableFocus` sets the focus mode for the sidebar's rows.
 */
export function useFocusMode({
  focus,
  setFocus,
  selectConsole,
  currentConsoleId,
  selectedSession,
  sidebarShown,
  selectSession,
  focusTerminal,
  suspended = false,
}: {
  /** The sidebar's focus mode. */
  focus: FocusTarget | undefined;
  setFocus: (target: FocusTarget | undefined) => void;
  selectConsole: (consoleId: string) => void;
  currentConsoleId: string | undefined;
  selectedSession: Session | undefined;
  sidebarShown: boolean;
  selectSession: (session: Session, options?: { enterFocus?: boolean; resume?: boolean }) => void;
  focusTerminal: () => void;
  /** A page (Remote devices, Scheduled tasks) shows in place of the sessions, so there is no session context to enter focus mode on or switch within. */
  suspended?: boolean;
}) {
  const selectedTarget = useDaemonStore(useShallow((s) => selectedSessionFocusTarget(selectedSession, s.projects, s.sessions)));
  // Into the focus mode of the selected session's context, or back out of focus mode; a press that would change
  // nothing (focus mode already in or out) runs neither (`useFocusShortcut`).
  const canEnter = !suspended && !focus && !!selectedTarget;
  const canExit = !suspended && focus !== undefined;
  useFocusShortcut({
    canEnter,
    canExit,
    enter: () => {
      if (!selectedTarget) return;
      if (selectedSession) selectConsole(selectedSession.console_id);
      setFocus(selectedTarget);
    },
    exit: () => setFocus(undefined),
  });

  // The console sessions a console session's focus mode offers to switch between (its switch strip),
  // for ⌃Tab.
  // Only while a console session is in focus, where the strip is on screen and the shortcut exists.
  const stripConsoleId = focus && "consoleSession" in focus ? currentConsoleId : undefined;
  const switchableConsoleSessions = useDaemonStore(
    useShallow((s) => {
      if (stripConsoleId === undefined) return NO_SESSIONS;
      const index = indexSessions(s.sessions);
      return switchStrip(sessionsOf(index.byConsole, stripConsoleId), stripConsoleId, index.delegatingIds, s.queuedAccountSwitches).map((e) => e.consoleSession);
    }),
  );
  // ⌃Tab only shows the console session, without resuming an interrupted one (`KEY_SWITCH_SELECTION`),
  // and hands keyboard focus to its terminal, as selecting a session does. The switch is applied in
  // one render first, so the terminal is already showing the new session when it takes focus: taken
  // any earlier, the focus-in report xterm sends would reach the session being left.
  const switchConsoleSessionByKey = (session: Session) => {
    flushSync(() => selectSession(session, KEY_SWITCH_SELECTION));
    focusTerminal();
  };
  useSwitchShortcut({
    focus,
    strip: switchableConsoleSessions,
    shown: sidebarShown && !suspended,
    onSwitch: switchConsoleSessionByKey,
  });
  const stableFocus = useStableHandler(setFocus);
  return { stableFocus };
}
