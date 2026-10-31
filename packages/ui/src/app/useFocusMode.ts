import { flushSync } from "react-dom";
import { useShallow } from "zustand/react/shallow";

import { useStableHandler } from "../components/hooks/useLatest";
import type { Project, Session } from "../protocol";
import { switchStrip } from "../sessions/order";
import { indexSessions, NO_SESSIONS, sessionsOf } from "../sessions/sessionIndex";
import { KEY_SWITCH_SELECTION, shortcutOutcome } from "../sidebar/focus";
import { useFocusShortcut } from "../sidebar/focusShortcut";
import { useSwitchShortcut } from "../sidebar/switchShortcut";
import type { FocusTarget } from "../sidebar/types";
import { useDaemonStore } from "../store";

/**
 * The sidebar's focus mode from the keyboard: ⇧⌘F enters the selected session's context or leaves focus mode, and
 * ⌃Tab moves along a console session's switch strip. `moveConsoleSession` is the move for a key press that reaches
 * the window another way, and `stableFocus` sets the focus mode for the sidebar's rows.
 */
export function useFocusMode({
  focus,
  setFocus,
  selectConsole,
  currentConsoleId,
  selectedSession,
  projects,
  sidebarShown,
  selectSession,
  focusTerminal,
}: {
  /** The sidebar's focus mode. */
  focus: FocusTarget | undefined;
  setFocus: (target: FocusTarget | undefined) => void;
  selectConsole: (consoleId: string) => void;
  currentConsoleId: string | undefined;
  selectedSession: Session | undefined;
  projects: Map<string, Project>;
  sidebarShown: boolean;
  selectSession: (session: Session, options?: { enterFocus?: boolean; resume?: boolean }) => void;
  focusTerminal: () => void;
}) {
  // ⇧⌘F: into the focus mode of the selected session's context, or back out of focus mode.
  useFocusShortcut(() => {
    const outcome = shortcutOutcome(focus, selectedSession, projects);
    if (!outcome) return;
    if (selectedSession && outcome.focus) selectConsole(selectedSession.console_id);
    setFocus(outcome.focus);
  });

  // The console sessions a console session's focus mode offers to switch between (its switch strip),
  // for ⌃Tab.
  // Only while a console session is in focus, where the strip is on screen and the shortcut exists.
  const stripConsoleId = focus && "consoleSession" in focus ? currentConsoleId : undefined;
  const switchableConsoleSessions = useDaemonStore(
    useShallow((s) =>
      stripConsoleId !== undefined
        ? switchStrip(sessionsOf(indexSessions(s.sessions).byConsole, stripConsoleId), stripConsoleId).map((e) => e.consoleSession)
        : NO_SESSIONS,
    ),
  );
  // ⌃Tab only shows the console session, without resuming an interrupted one (`KEY_SWITCH_SELECTION`),
  // and hands keyboard focus to its terminal, as selecting a session does. The switch is applied in
  // one render first, so the terminal is already showing the new session when it takes focus: taken
  // any earlier, the focus-in report xterm sends would reach the session being left.
  const switchConsoleSessionByKey = (session: Session) => {
    flushSync(() => selectSession(session, KEY_SWITCH_SELECTION));
    focusTerminal();
  };
  const moveConsoleSession = useSwitchShortcut({
    focus,
    strip: switchableConsoleSessions,
    shown: sidebarShown,
    onSwitch: switchConsoleSessionByKey,
  });
  const stableFocus = useStableHandler(setFocus);
  return { moveConsoleSession, stableFocus };
}
