import { useEffect, useRef, useState } from "react";

import { isLive, type Session } from "../protocol";

/** Set once we are actually running inside the Tauri shell — lets `npm run dev` in a plain browser
 * skip the window-close/exit-event integration entirely instead of throwing on a missing API. */
const runningInTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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
  closeExitConfirm: () => void;
  /** Call when the user confirms the dialog above. */
  confirmExit: () => Promise<void>;
  /** The same entry point the window's close button, Cmd+Q and the app menu all funnel through —
   * exposed so a plain "Quit" affordance (the daemon-failed-to-start screen has one) can use it too. */
  requestQuit: () => Promise<void>;
}

/**
 * Owns the whole quit sequence: registering this webview as the one handling the exit flow,
 * listening for the window's close button and for an `exit-requested` event (fired for every other
 * way to quit — Cmd+Q, the app menu — that the Rust side cannot itself ask the user about), asking
 * for confirmation when a session is still live, and the `shutdown`-then-`confirm_quit` sequence
 * that actually ends the process. With `getSessions` and `requestShutdown` omitted it quits
 * straight away, which is what a screen with no daemon behind it needs.
 */
export function useAppExit(options: UseAppExitOptions = {}): UseAppExitResult {
  const [exitConfirmOpen, setExitConfirmOpen] = useState(false);
  // Read through a ref rather than closed over directly: the listener-registration effect below
  // runs once (mount/unmount only), so `requestQuit`/`doQuit` must look up the latest sessions and
  // callbacks at call time instead of capturing whatever they were when the effect first ran.
  const optionsRef = useRef(options);
  optionsRef.current = options;

  const doQuit = async (): Promise<void> => {
    const requestShutdown = optionsRef.current.requestShutdown;
    if (requestShutdown) {
      await Promise.race([
        requestShutdown().catch(() => {}),
        new Promise<void>((resolve) => setTimeout(resolve, SHUTDOWN_TIMEOUT_MS)),
      ]);
    }
    if (!runningInTauri) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("confirm_quit");
    } catch (err) {
      // Quitting must not depend on this succeeding, but a quit that silently does nothing is
      // worse than one that says why, so the failure is surfaced rather than swallowed.
      optionsRef.current.toastError?.(`Quitting failed: ${(err as Error).message}`);
    }
  };

  const requestQuit = async (): Promise<void> => {
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

  useEffect(() => {
    if (!runningInTauri) return;
    let unlistenClose: (() => void) | undefined;
    let unlistenExit: (() => void) | undefined;
    let unlistenDaemonExited: (() => void) | undefined;
    (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      // Tells the Rust side a confirmation flow actually exists now, so it starts asking before
      // letting an exit through instead of defaulting to letting every exit straight through —
      // called from every screen that mounts this hook, including the daemon-failed-to-start one,
      // so a window stuck there is always still quittable.
      await invoke("frontend_handles_exit");

      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const { listen } = await import("@tauri-apps/api/event");
      const win = getCurrentWindow();
      unlistenClose = await win.onCloseRequested((event) => {
        event.preventDefault();
        void requestQuit();
      });
      unlistenExit = await listen("exit-requested", () => void requestQuit());
      unlistenDaemonExited = await listen<string>("daemon-exited", (event) => {
        optionsRef.current.toastError?.(
          `The daemon process exited unexpectedly (${event.payload}). Restart Octoboard to continue.`,
        );
      });
    })();
    return () => {
      unlistenClose?.();
      unlistenExit?.();
      unlistenDaemonExited?.();
    };
    // Registration happens once; `requestQuit`/`doQuit` read live state through `optionsRef`.
  }, []);

  return {
    exitConfirmOpen,
    closeExitConfirm: () => setExitConfirmOpen(false),
    confirmExit,
    requestQuit,
  };
}
