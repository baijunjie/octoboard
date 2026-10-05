import { useEffect, useRef, useState } from "react";

import { usePlatform } from "../platform/react";
import { isLive, type Session } from "../protocol";

/** How long `doQuit` waits for `shutdown`'s `ack` before giving up on it. The daemon drops the
 * serving future as soon as it starts shutting down, so the ack usually never arrives at all — that
 * is the expected case, not a failure, since the whole point of asking is that the process is about
 * to be gone regardless. Without this bound, a quit requested while the control socket is dead (no
 * reconnect left to retry it) would wait on a request that can now only time out on its own much
 * later, or never settle at all, leaving the exit confirmation dialog wedged with no way out. */
const SHUTDOWN_TIMEOUT_MS = 2000;

interface UseAppExitOptions {
  /** Current sessions, read fresh each time a quit is requested rather than once at mount — omit
   * entirely where there is no daemon connection to ask about them at all (the daemon-failed-to-
   * start screen), which skips the confirmation step unconditionally. */
  getSessions?: () => Session[];
  /** Sends `shutdown` to the daemon before confirming the quit. Omit together with `getSessions`
   * when there is nothing to shut down. */
  requestShutdown?: () => Promise<unknown>;
  /** Reports a message with nowhere inline to show it (no dialog is open on this path). Omit where
   * there is no toast UI to show it in either. */
  toastError?: (message: string) => void;
}

interface UseAppExitResult {
  /** Whether the "sessions are still running" confirmation is open. */
  exitConfirmOpen: boolean;
  /** Call when the user cancels the dialog above. */
  closeExitConfirm: () => void;
  /** Call when the user confirms the dialog above. */
  confirmExit: () => Promise<void>;
  /** The same entry point the window's close button, Cmd+Q and the app menu all funnel through —
   * exposed so a plain "Quit" affordance (the daemon-failed-to-start screen has one) can use it too.
   * Does nothing where `canQuit` is false. */
  requestQuit: () => Promise<void>;
  /** Whether the platform has a quit flow at all. A plain browser has none — closing a tab quits
   * nothing, and shutting the daemon down from a page that merely connects to it would be wrong —
   * so a screen offers a Quit affordance only when this is true. */
  canQuit: boolean;
}

/**
 * Owns the whole quit sequence: registering this webview as the one handling the exit flow (and
 * re-pinging that same registration on every gesture handled thereafter, which is what keeps the
 * Rust side's force-quit debounce from arming against a webview that is actually still alive),
 * listening for the window's close button and for an `exit-requested` event (fired for every other
 * way to quit — Cmd+Q, the app menu, the Dock icon's own Quit, and a system-initiated
 * logout/restart/shutdown — that the Rust side cannot itself ask the user about), asking for
 * confirmation when a session is still live, and the `shutdown`-then-`confirm_quit` sequence that
 * actually ends the process. With `getSessions` and `requestShutdown` omitted it quits straight
 * away, which is what a screen with no daemon behind it needs.
 */
export function useAppExit(options: UseAppExitOptions = {}): UseAppExitResult {
  const { exit } = usePlatform();
  const [exitConfirmOpen, setExitConfirmOpen] = useState(false);
  // Read through a ref rather than closed over directly: the listener-registration effect below
  // runs once (mount/unmount only), so `requestQuit`/`doQuit` must look up the latest sessions and
  // callbacks at call time instead of capturing whatever they were when the effect first ran.
  const optionsRef = useRef(options);
  optionsRef.current = options;

  const doQuit = async (): Promise<void> => {
    // Without a shell that owns the process there is no quit, and a daemon this page merely
    // connects to must not be shut down by it.
    if (!exit) return;
    const requestShutdown = optionsRef.current.requestShutdown;
    if (requestShutdown) {
      await Promise.race([
        requestShutdown().catch(() => {}),
        new Promise<void>((resolve) => setTimeout(resolve, SHUTDOWN_TIMEOUT_MS)),
      ]);
    }
    try {
      await exit.confirmQuit();
    } catch (err) {
      // Quitting must not depend on this succeeding, but a quit that silently does nothing is
      // worse than one that says why, so the failure is surfaced rather than swallowed.
      optionsRef.current.toastError?.(`Quitting failed: ${(err as Error).message}`);
    }
  };

  const requestQuit = async (): Promise<void> => {
    if (!exit) return;
    // Re-invoking `frontend_exit_heartbeat` here, on every quit gesture this webview actually
    // receives (not only when the user cancels the dialog), is what clears the Rust side's
    // `FORCE_QUIT_WINDOW` debounce: it is itself proof this webview is alive and answering, so
    // the next gesture should wait for it rather than treat this one as a wedge. A wedged webview
    // never reaches this line at all, so the debounce stays armed and the escape hatch in
    // `should_let_quit_through` (`apps/desktop/src-tauri/src/exit.rs`) still fires for it.
    await exit.heartbeat().catch(() => {});
    const liveSessions = (optionsRef.current.getSessions?.() ?? []).filter((s) => isLive(s.status));
    if (liveSessions.length > 0) {
      setExitConfirmOpen(true);
      return;
    }
    await doQuit();
  };

  const confirmExit = async (): Promise<void> => {
    await doQuit();
    setExitConfirmOpen(false);
  };

  // Registration (and the listener-leak protection that goes with it) is the adapter's; this only
  // hands it what to call.
  useEffect(
    () =>
      exit?.register({
        onQuitRequested: () => void requestQuit(),
        onDaemonExited: (detail) => {
          optionsRef.current.toastError?.(
            `The daemon process exited unexpectedly (${detail}). Restart Octoboard to continue.`,
          );
        },
      }),
    // Registration happens once; `requestQuit`/`doQuit` read live state through `optionsRef`.
    [],
  );

  const closeExitConfirm = (): void => {
    // Nothing to tell the Rust side here: `requestQuit` above already pinged the debounce-clearing
    // heartbeat on the way into this dialog, and that is what matters for the next gesture — a
    // cancel does not need a signal of its own.
    setExitConfirmOpen(false);
  };

  return {
    exitConfirmOpen,
    closeExitConfirm,
    confirmExit,
    requestQuit,
    canQuit: exit !== undefined,
  };
}
