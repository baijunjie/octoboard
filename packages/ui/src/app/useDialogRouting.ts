import { useEffect, useRef } from "react";

import { noAgentState, pickableAgents } from "../agents/agents";
import { FOCUSABLE } from "../components/focus/selectors";
import { useStableHandler } from "../components/hooks/useLatest";
import type { DialogRequest } from "../dialogs/dialogRequest";
import type { Console } from "../protocol";
import { useDaemon } from "../store";

/** What the open drawer is made of: the sidebar and, below the `phone` breakpoint, the rail inside its card. */
export const DRAWER_CONTROLS = "[data-pane=sidebar], [data-region=rail]";

/**
 * Opening and closing the dialogs the window asks for (`setDialogRequest`): a dialog that has nothing to offer is not
 * opened, one opened from the open drawer hands focus back to what opened it there (`focusDrawerTrigger`, which
 * Settings uses too), and a console created from the new-console dialog becomes current through `switchConsole`.
 */
export function useDialogRouting({
  consoles,
  consoleList,
  sidebarOpen,
  setDialogRequest,
  switchConsole,
}: {
  consoles: ReadonlyMap<string, Console>;
  consoleList: Console[];
  sidebarOpen: boolean;
  setDialogRequest: (dialog: DialogRequest | undefined) => void;
  switchConsole: (consoleId: string) => void;
}) {
  const { store } = useDaemon();
  // `create_console` is answered with a bare ack, so the console this window just created is
  // recognised as the one that was not there when its dialog opened, and the sidebar switches to it.
  const consolesBeforeNew = useRef<Set<string> | undefined>(undefined);
  // The wait after the dialog has closed before the marker above is dropped, if no console has appeared by then.
  const forgetTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(forgetTimer.current), []);
  // The new console's broadcast can trail the ack that closed the dialog by a moment.
  const forgetSoon = () => {
    clearTimeout(forgetTimer.current);
    if (consolesBeforeNew.current) forgetTimer.current = setTimeout(() => (consolesBeforeNew.current = undefined), 2000);
  };
  // The control last pressed in the sidebar's open drawer, or in a menu it opened. A tap does not
  // move focus to what it presses, so a dialog it opens would otherwise give focus back, as it
  // closes, to whatever held it before: the terminal behind the drawer.
  const drawerPress = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!sidebarOpen) return;
    // Taken as the press ends, before the press's own handler runs: a pointer that goes down to
    // scroll the drawer ends in a cancel, and records nothing.
    const onPointerUp = (event: PointerEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest("[role=menu], [role=listbox]")) return;
      drawerPress.current = target?.closest(DRAWER_CONTROLS) ? target.closest<HTMLElement>(FOCUSABLE) : null;
    };
    document.addEventListener("pointerup", onPointerUp, true);
    return () => {
      document.removeEventListener("pointerup", onPointerUp, true);
      drawerPress.current = null;
    };
  }, [sidebarOpen]);

  /** Opened from the open drawer, a dialog hands focus back to what opened it there as it closes:
   * focused before the dialog mounts, that is what the dialog restores focus to. Whether it did. */
  const focusDrawerTrigger = (): boolean => {
    const trigger = drawerPress.current;
    drawerPress.current = null;
    if (!(sidebarOpen && trigger?.isConnected) || document.activeElement?.closest(DRAWER_CONTROLS)) return false;
    trigger.focus({ preventScroll: true });
    return true;
  };

  const openDialog = (dialog: DialogRequest) => {
    // Nothing to offer without an agent that is added and installed; the controls are disabled already.
    const { agentAvailability, settings } = store.getState();
    if ((dialog.kind === "new-session" || dialog.kind === "new-console-session") && noAgentState(agentAvailability, settings.added_agents)) return;
    if (dialog.kind === "hand-off" && !pickableAgents(agentAvailability, settings.added_agents).some((agent) => agent !== dialog.session.agent)) return;
    clearTimeout(forgetTimer.current);
    if (dialog.kind === "new-console") consolesBeforeNew.current = new Set(consoles.keys());
    else forgetSoon();
    focusDrawerTrigger();
    setDialogRequest(dialog);
  };
  useEffect(() => {
    const before = consolesBeforeNew.current;
    if (!before) return;
    const created = consoleList.find((c) => !before.has(c.id));
    if (created) {
      consolesBeforeNew.current = undefined;
      switchConsole(created.id);
    }
  }, [consoleList]);
  const closeDialog = () => {
    setDialogRequest(undefined);
    forgetSoon();
  };

  const openConsoleSession = (console_: Console) => openDialog({ kind: "new-console-session", console: console_ });
  const stableOpenConsoleSession = useStableHandler(openConsoleSession);
  const stableOpenDialog = useStableHandler(openDialog);
  return { openDialog, closeDialog, openConsoleSession, focusDrawerTrigger, stableOpenDialog, stableOpenConsoleSession };
}
