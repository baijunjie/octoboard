import { useEffect, useRef, useState } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { usePlatform } from "../platform/react";
import type { Session, ShellTerminal } from "../protocol";
import { isLive, sessionIsWorking } from "../sessions/sessionStatus";
import { reportDaemonExit, reportExitFailure } from "./exitFailure";

/** How long `doQuit` waits for `begin_quit`, and for `shutdown`'s or `planned_stop`'s `ack`, before giving up on it. The
 * daemon drops the serving future as soon as it starts stopping, so the ack usually never arrives at all — that
 * is the expected case, not a failure, since the whole point of asking is that the process is about
 * to be gone regardless. Without this bound, a quit requested while the control socket is dead (no
 * reconnect left to retry it) would wait on a request that can now only time out on its own much
 * later, or never settle at all, leaving the exit confirmation dialog wedged with no way out. */
const SHUTDOWN_TIMEOUT_MS = 2000;

/** How long a restart waits for the daemon to say which sessions and shell terminals it would interrupt, before it
 * asks as if it could not say. */
const HANDOVER_CHECK_TIMEOUT_MS = 2000;

/** What the daemon says a restart into an update would not carry over: the ids of the running sessions and shell
 * terminals whose process holder is outside the range the update's daemon adopts, whose version is unknown, or that
 * have no holder. */
export interface HandoverCheck {
  /** Whether the daemon judged against a floor and version it could read. When not (the update gave none), the lists
   * hold everything running rather than what would be interrupted, and the answer proves nothing. */
  known: boolean;
  sessions: readonly string[];
  /** The shell terminals the daemon runs, whose busy state the page knows. */
  shellTerminals: readonly string[];
  /** The shell terminals of a holder the daemon has not reached, which it does not run: whether one is busy is not
   * known. */
  unreachedShellTerminals: readonly string[];
}

/** What an open restart confirmation says beyond the sessions it names. */
export interface RestartDetail {
  /** The daemon's answer could not be had or proved nothing, so the confirmation says what may be interrupted rather
   * than what will be. */
  uncertain: boolean;
  /** How many sessions the daemon listed that this window does not hold (so there is no object to name). */
  unnamedSessions: number;
  /** How many shell terminals the daemon said it could not reach, those of a holder it has not linked to: they are not
   * registered with it, so whether one is busy is not known. A listed terminal this window does not hold is not one of
   * these: the daemon runs it, and it counts as busy. */
  unreachedTerminals: number;
}

const NO_RESTART_DETAIL: RestartDetail = { uncertain: false, unnamedSessions: 0, unreachedTerminals: 0 };

/** Settles with `promise`'s outcome, or with `undefined` when it rejects or takes longer than `ms`. */
async function withinTimeout<T>(promise: Promise<T>, ms: number): Promise<T | undefined> {
  return Promise.race([promise.catch(() => undefined), new Promise<undefined>((resolve) => setTimeout(resolve, ms))]);
}

/** What holds a quit up: sessions in the middle of something (at work, or waiting for the user), and shell terminals
 * running something, which the quit would end. An idle or exited shell does not count. */
export interface QuitHolders {
  sessions: boolean;
  terminals: boolean;
}

/** What holds a quit up among `sessions` and `terminals`; a quit asks for confirmation when either does. A
 * session idle but running in the background holds it up too, as the quit would end that
 * work. One at its prompt with nothing left running is in the middle of nothing that is reported, so it does
 * not, though the quit still ends it and leaves it interrupted and resumable like the rest; one whose child
 * sessions are working holds it up only through those sessions. */
export function quitHolders(sessions: readonly Session[], terminals: readonly ShellTerminal[] = []): QuitHolders {
  return {
    sessions: sessions.some((s) => sessionIsWorking(s) || s.status === "waiting_user"),
    terminals: terminals.some((terminal) => terminal.state === "running" && terminal.busy),
  };
}

/** What a restart asks about: the sessions it names, what holds it up among them and the terminals, and what it says
 * beyond them. */
interface RestartConfirmation {
  holders: QuitHolders;
  sessions: Session[];
  detail: RestartDetail;
  keepsRest: boolean;
}

interface UseAppExitOptions {
  /** Current sessions, read fresh each time a quit is requested rather than once at mount — omit
   * entirely where there is no daemon connection to ask about them at all (the startup screens),
   * which skips the confirmation step unconditionally. */
  getSessions?: () => Session[];
  /** Current shell terminals, read fresh the same way; a busy one holds the quit up too. */
  getShellTerminals?: () => ShellTerminal[];
  /** Sends `shutdown` to the daemon before confirming a quit. Omit together with `getSessions`
   * when there is nothing to shut down. */
  requestShutdown?: () => Promise<unknown>;
  /** Sends `planned_stop` to the daemon before confirming a restart: it hands every terminal over to its holder and
   * exits, interrupting nothing. */
  requestPlannedStop?: () => Promise<unknown>;
  /** Asks the daemon what a restart into an update with this holder floor and version (`null` for none known: an
   * answer that is everything running, and not known) would interrupt. Without it, or when it fails or takes too long,
   * a restart asks about everything that may be interrupted. */
  checkHandover?: (holderFloor: string | null, targetVersion: string | null) => Promise<HandoverCheck>;
}

export interface UseAppExitResult {
  /** Whether the confirmation that sessions or shell terminals are still running is open. */
  exitConfirmOpen: boolean;
  /** What the open confirmation is about, as it was when the quit was asked for. */
  exitHolders: QuitHolders;
  /** The sessions an open restart confirmation names as the ones to be interrupted (or that may be, see
   * `exitRestartDetail`); empty for a quit's, which ends every session alike. */
  exitInterrupted: Session[];
  /** Whether an open restart confirmation can say that everything else keeps running: the daemon proved what it
   * interrupts, and every shell terminal it listed as running is busy, so the confirmation counts it, with none
   * unreached. `false` when the daemon could not say, or when a listed terminal at its prompt would end unmentioned. */
  exitKeepsRest: boolean;
  /** What an open restart confirmation says beyond `exitInterrupted`. Nothing of it for a quit's. */
  exitRestartDetail: RestartDetail;
  /** Whether the open confirmation is about a restart rather than a quit. */
  exitRestarts: boolean;
  /** Call when the user cancels the dialog above. */
  closeExitConfirm: () => void;
  /** Call when the user confirms the dialog above. */
  confirmExit: () => Promise<void>;
  /** Asks for a restart rather than a quit (a quit is only ever asked for by the shell's gestures, Cmd+Q, the app
   * menu and the menu bar icon's Quit among them); once confirmed the application relaunches instead of ending, which
   * is how an installed update takes effect. `holderFloor` and `targetVersion` are the update's (see
   * `AvailableUpdate`). The restart hands the sessions over to the new daemon rather than ending them, so it asks, and
   * the user can cancel there, only when the daemon says some session, busy shell terminal or unreachable shell
   * terminal would be interrupted, or cannot say and something running may be (see `checkHandover`). */
  requestRestart: (holderFloor: string | null, targetVersion: string | null) => Promise<void>;
  /** Asks for a quit from a control of the window's own, as the shell's quit gestures do, confirmation included. */
  requestQuit: () => Promise<void>;
}

/**
 * Owns the whole quit sequence: registering this webview as the one handling the exit flow (and re-pinging that same
 * registration on every gesture handled thereafter, which is what keeps the Rust side's force-quit debounce from arming
 * against a webview that is actually still alive), listening for an `exit-requested` event (fired for every way to quit
 * — Cmd+Q, the app menu, the menu bar icon's Quit, the Dock icon's own Quit — that the Rust side cannot itself ask the
 * user about; the window's close button is one too unless the user has chosen to run in the background, in which case
 * it hides the window; on macOS a system logout, restart or shutdown is handled by the shell itself and never asks),
 * asking for confirmation when a session or a busy shell terminal holds the quit up (see `quitHolders`; bringing the
 * window to the front to do so), and the `begin_quit`-`shutdown`-`confirm_quit` sequence that actually ends the process (or,
 * for a restart the UI asks for itself, the `planned_stop`-then-`confirm_quit` one that relaunches it). With
 * `getSessions` and `requestShutdown` omitted it quits straight away, which is what a screen with no daemon behind it
 * needs.
 */
export function useAppExit(options: UseAppExitOptions = {}): UseAppExitResult {
  const { exit, nativeWindow } = usePlatform();
  const [exitConfirmOpen, setExitConfirmOpen] = useState(false);
  const [exitHolders, setExitHolders] = useState<QuitHolders>({ sessions: false, terminals: false });
  const [exitInterrupted, setExitInterrupted] = useState<Session[]>([]);
  const [exitKeepsRest, setExitKeepsRest] = useState(false);
  const [exitRestartDetail, setExitRestartDetail] = useState<RestartDetail>(NO_RESTART_DETAIL);
  const [exitRestarts, setExitRestarts] = useState(false);
  // Read through a ref rather than closed over directly: the listener-registration effect below
  // runs once (mount/unmount only), so `request`/`doQuit` must look up the latest sessions and
  // callbacks at call time instead of capturing whatever they were when the effect first ran.
  const optionsRef = useLatest(options);

  // Set from the moment a quit starts shutting the daemon down, which exits on purpose then.
  const quittingRef = useRef(false);
  // Counts the requests, so one waiting on the daemon's answer gives way to a gesture that arrived meanwhile.
  const requestCount = useRef(0);

  const doQuit = async (restart: boolean): Promise<void> => {
    // Without a shell that owns the process there is no quit, and a daemon this page merely
    // connects to must not be shut down by it.
    if (!exit) return;
    quittingRef.current = true;
    const { requestShutdown, requestPlannedStop } = optionsRef.current;
    const stop = restart ? requestPlannedStop : requestShutdown;
    // A quit leaves its intent with the shell first, so that one cut short is finished at the next start; a restart's
    // planned stop is not a quit and leaves none. Bounded, and never in the way of the quit.
    if (!restart) await withinTimeout(exit.beginQuit(), SHUTDOWN_TIMEOUT_MS);
    if (stop) await withinTimeout(stop(), SHUTDOWN_TIMEOUT_MS);
    try {
      await exit.confirmQuit({ restart });
    } catch (err) {
      // Quitting must not depend on this succeeding, but a quit that silently does nothing is
      // worse than one that says why, so the failure is surfaced rather than swallowed.
      quittingRef.current = false;
      reportExitFailure(restart, (err as Error).message);
    }
  };

  /** What a restart would interrupt. With the daemon's proven answer, every running session it lists, whatever it is
   * doing, and its busy shell terminals (one this window does not hold counts as busy) and those it could not reach;
   * the sessions it lists that this window does not hold are kept as a count. Without one (no answer, or one that
   * proves nothing), every session with a running process and every busy shell terminal, as what may be interrupted,
   * with the terminals it could not reach where an answer said so. */
  const interruptedByRestart = async (holderFloor: string | null, targetVersion: string | null): Promise<RestartConfirmation> => {
    const { getSessions, getShellTerminals, checkHandover } = optionsRef.current;
    const answer = checkHandover && (await withinTimeout(checkHandover(holderFloor, targetVersion), HANDOVER_CHECK_TIMEOUT_MS));
    // Read after the answer, which can take a while.
    const sessions = getSessions?.() ?? [];
    const terminals = getShellTerminals?.() ?? [];
    if (!answer?.known) {
      const mayEnd = sessions.filter((s) => isLive(s.status));
      const holders = { sessions: mayEnd.length > 0, terminals: quitHolders([], terminals).terminals };
      // An answer that proves nothing still says which terminals the daemon could not reach.
      const unreachedTerminals = answer ? answer.unreachedShellTerminals.length : 0;
      return { holders, sessions: mayEnd, detail: { ...NO_RESTART_DETAIL, uncertain: true, unreachedTerminals }, keepsRest: false };
    }
    const listedSessions = sessions.filter((s) => answer.sessions.includes(s.id));
    const listedTerminals = terminals.filter((terminal) => answer.shellTerminals.includes(terminal.id));
    const unnamedSessions = answer.sessions.length - listedSessions.length;
    // A terminal the daemon lists that this window does not hold may be busy, so it counts as busy.
    const unheldTerminals = answer.shellTerminals.length - listedTerminals.length;
    const unreachedTerminals = answer.unreachedShellTerminals.length;
    // A listed terminal at its prompt is ended by the restart too but does not ask on its own, so nothing could be
    // said about the rest.
    const keepsRest = unreachedTerminals === 0 && listedTerminals.every((terminal) => terminal.state !== "running" || terminal.busy);
    const holders = {
      sessions: listedSessions.length + unnamedSessions > 0,
      terminals: unheldTerminals > 0 || quitHolders([], listedTerminals).terminals,
    };
    return { holders, sessions: listedSessions, detail: { uncertain: false, unnamedSessions, unreachedTerminals }, keepsRest };
  };

  const request = async (restart: boolean, holderFloor: string | null = null, targetVersion: string | null = null): Promise<void> => {
    if (!exit) return;
    const requestId = ++requestCount.current;
    // Re-invoking `frontend_exit_heartbeat` here, on every quit gesture this webview actually
    // receives (not only when the user cancels the dialog), is what clears the Rust side's
    // `FORCE_QUIT_WINDOW` debounce: it is itself proof this webview is alive and answering, so
    // the next gesture should wait for it rather than treat this one as a wedge. A wedged webview
    // never reaches this line at all, so the debounce stays armed and the escape hatch in
    // `should_let_quit_through` (`apps/desktop/src-tauri/src/exit.rs`) still fires for it.
    await exit.heartbeat().catch(() => {});
    // A restart always asks the daemon what it would interrupt, whatever a quit would ask about: an incompatible
    // holder ends an idle session as much as a busy one.
    const { holders, sessions, detail, keepsRest } = restart && optionsRef.current.getSessions
      ? await interruptedByRestart(holderFloor, targetVersion)
      : {
        holders: quitHolders(optionsRef.current.getSessions?.() ?? [], optionsRef.current.getShellTerminals?.() ?? []),
        sessions: [],
        detail: NO_RESTART_DETAIL,
        keepsRest: false,
      };
    if (requestId !== requestCount.current) return;
    if (holders.sessions || holders.terminals || detail.unreachedTerminals > 0) {
      setExitHolders(holders);
      setExitInterrupted(sessions);
      setExitKeepsRest(keepsRest);
      setExitRestartDetail(detail);
      // The latest gesture decides what the open confirmation is about: a quit asked for while a restart is
      // being confirmed turns it into a quit.
      setExitRestarts(restart);
      setExitConfirmOpen(true);
      // A quit from outside the window (the Dock icon's own Quit, the menu bar icon's) leaves it
      // wherever it was, hidden in the background or behind another app, where the dialog would go
      // unseen. Best effort: the dialog opens whether or not the window can be raised.
      await nativeWindow?.bringToFront().catch(() => {});
      return;
    }
    await doQuit(restart);
  };

  const requestRestart = (holderFloor: string | null, targetVersion: string | null) => request(true, holderFloor, targetVersion);
  const requestQuit = () => request(false);

  const confirmExit = async (): Promise<void> => {
    await doQuit(exitRestarts);
    setExitConfirmOpen(false);
  };

  // Registration (and the listener-leak protection that goes with it) is the adapter's; this only
  // hands it what to call.
  useEffect(
    () =>
      exit?.register({
        onQuitRequested: () => void request(false),
        onDaemonExited: (detail) => {
          // The daemon going away is the quit's own doing, which needs neither a notice nor the
          // window.
          if (quittingRef.current) return;
          reportDaemonExit(detail);
          // A window hidden in the background would leave the notice unseen; a visible one is left
          // where it is, so a crash does not take the focus from whatever the user is doing.
          void (async () => {
            if (nativeWindow && !(await nativeWindow.isVisible())) await nativeWindow.bringToFront();
          })().catch(() => {});
        },
      }),
    // Registration happens once; `request`/`doQuit` read live state through `optionsRef`.
    [],
  );

  const closeExitConfirm = (): void => {
    // Nothing to tell the Rust side here: `request` above already pinged the debounce-clearing
    // heartbeat on the way into this dialog, and that is what matters for the next gesture — a
    // cancel does not need a signal of its own.
    setExitConfirmOpen(false);
  };

  return {
    exitConfirmOpen,
    exitHolders,
    exitInterrupted,
    exitKeepsRest,
    exitRestartDetail,
    exitRestarts,
    closeExitConfirm,
    confirmExit,
    requestRestart,
    requestQuit,
  };
}
