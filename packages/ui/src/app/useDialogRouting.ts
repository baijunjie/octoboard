import { useEffect, useRef, useSyncExternalStore } from "react";

import { noAgentState, pickableAgents } from "../agents/agents";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { FOCUSABLE } from "../components/focus/selectors";
import { useStableHandler } from "../components/hooks/useLatest";
import type { DialogRequest } from "../dialogs/dialogRequest";
import type { Console } from "../protocol";
import { useDaemon } from "../store";

/** How long after the new-console dialog closes the window keeps looking for the console it created. */
export const CREATED_CONSOLE_WAIT_MS = 2000;

/** What the open drawer is made of: the sidebar and, at tablet width and below, the rail inside its card. */
export const DRAWER_CONTROLS = "[data-pane=sidebar], [data-region=rail]";

/**
 * Opening and closing the dialogs the window asks for (`setDialogRequest`): a dialog that has nothing to offer is not
 * opened, one opened from the open drawer hands focus back to what opened it there (`focusDrawerTrigger`, which
 * Settings uses too), and a console created from the new-console dialog becomes current through `switchConsole`.
 * `openDialogWhenFree` is for a dialog that arrives on its own, after a reply, and so may arrive while the user is in
 * another modal: it waits for the open dialog and `windowModalOpen` (Settings, a question, the exit or launch
 * confirmation) and any error alert to be gone rather than replace or cover them.
 */
export function useDialogRouting({
  dialogRequest,
  windowModalOpen,
  consoles,
  consoleList,
  sidebarOpen,
  setDialogRequest,
  switchConsole,
}: {
  /** The dialog the window shows now (the `setDialogRequest` state). */
  dialogRequest: DialogRequest | undefined;
  /** Whether a modal of the window's own is open besides the dialogs of `dialogRequest`. */
  windowModalOpen: boolean;
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
    if (consolesBeforeNew.current) forgetTimer.current = setTimeout(() => (consolesBeforeNew.current = undefined), CREATED_CONSOLE_WAIT_MS);
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

  // Opening replaces the dialog on screen, which is right for one the user asked for with a press but would take away
  // the question the user is answering from one that comes on its own. The dialogs of the second kind wait in `held`,
  // in the order they came, until no modal is open, and the first is then opened by an effect of a render that has
  // shown no dialog, so that a dialog of the same kind is mounted afresh rather than reused with the state of the one
  // before. `open` is the dialog opened and not yet closed, kept here because the state lags behind a call made in the
  // same event as another.
  const open = useRef<DialogRequest | undefined>(undefined);
  const held = useRef<DialogRequest[]>([]);
  // The queue itself is subscribed to, and its length read live wherever it decides: a boolean selector would not render
  // again for an alert reported and taken back between two renders, leaving a confirmation held that nothing releases, and
  // a late release effect would open one as an alert is reported.
  const alerts = useSyncExternalStore(errorAlerts.subscribe, errorAlerts.getSnapshot);
  const alertOpen = () => errorAlerts.getSnapshot().length > 0;

  const openDialog = (dialog: DialogRequest) => {
    // Nothing to offer without an agent that is added and installed; the controls are disabled already.
    const { agentAvailability, settings, providers } = store.getState();
    if ((dialog.kind === "new-session" || dialog.kind === "new-console-session") && noAgentState(agentAvailability, settings.added_agents, providers)) return;
    if (dialog.kind === "handoff" && !pickableAgents(agentAvailability, settings.added_agents, providers).some((agent) => agent !== dialog.session.agent)) return;
    clearTimeout(forgetTimer.current);
    if (dialog.kind === "new-console") consolesBeforeNew.current = new Set(consoles.keys());
    else forgetSoon();
    focusDrawerTrigger();
    open.current = dialog;
    setDialogRequest(dialog);
  };
  // Asking again about the subject of a dialog that is open or held changes nothing the person has not been asked: the
  // open one stands, and the held one is replaced by the newer, whose figures are the current ones.
  const sameSubject = (a: DialogRequest, b: DialogRequest) =>
    a.kind === "pull-merge-project-branch" && b.kind === a.kind && a.project.id === b.project.id;
  const openDialogWhenFree = (dialog: DialogRequest) => {
    if (open.current && sameSubject(open.current, dialog)) return;
    const waiting = held.current.findIndex((other) => sameSubject(other, dialog));
    if (waiting >= 0) held.current[waiting] = dialog;
    else if (open.current || windowModalOpen || alertOpen() || held.current.length > 0) held.current.push(dialog);
    else openDialog(dialog);
  };
  // A confirmation about a project that was removed while it waited has nothing left to ask.
  const subjectGone = (dialog: DialogRequest) => dialog.kind === "pull-merge-project-branch" && !store.getState().projects.has(dialog.project.id);
  useEffect(() => {
    // `open` is cleared by `closeDialog` alone: this effect may run late, after a dialog opened in the gap that a close
    // left, and must not take that one for closed.
    if (dialogRequest !== undefined || open.current || windowModalOpen || alertOpen()) return;
    let next = held.current.shift();
    while (next && subjectGone(next)) next = held.current.shift();
    if (next) openDialog(next);
  }, [dialogRequest, windowModalOpen, alerts]);
  useEffect(() => {
    const before = consolesBeforeNew.current;
    if (!before) return;
    const created = consoleList.find((c) => !before.has(c.id));
    if (created) {
      consolesBeforeNew.current = undefined;
      switchConsole(created.id);
    }
  }, [consoleList]);
  // `dialog` is the one asking to close: a dialog that closed itself and then settles an action it had under way must not
  // close whatever is open by then.
  const closeDialog = (dialog?: DialogRequest) => {
    if (dialog && open.current !== dialog) return;
    open.current = undefined;
    setDialogRequest(undefined);
    forgetSoon();
  };

  const openConsoleSession = (console_: Console) => openDialog({ kind: "new-console-session", console: console_ });
  const stableOpenConsoleSession = useStableHandler(openConsoleSession);
  const stableOpenDialog = useStableHandler(openDialog);
  const stableOpenDialogWhenFree = useStableHandler(openDialogWhenFree);
  return { openDialog, closeDialog, openConsoleSession, focusDrawerTrigger, stableOpenDialog, stableOpenDialogWhenFree, stableOpenConsoleSession };
}
