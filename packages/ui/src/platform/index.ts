import { browserPlatform } from "./browser";
import { tauriPlatform } from "./tauri";

/**
 * The only route from the UI to anything native. Every capability is optional: a member that is
 * absent means the environment has no such thing, and the feature built on it is absent too — not
 * an error. Code reads a member, checks it is there, and otherwise does nothing.
 *
 * Every direct use of a Tauri API in the UI lives behind this interface, in `./tauri.ts`; nothing
 * else may import `@tauri-apps/*`.
 */
export interface PlatformAdapter {
  readonly kind: "tauri" | "browser";
  /** The application's own lifetime: present only where a shell owns the process and must be told
   * when it may end. */
  readonly exit?: ExitCapability;
  readonly notifications?: NotificationCapability;
  readonly badge?: BadgeCapability;
}

export interface ExitHandlers {
  /** The user asked to quit by any gesture the shell reports: the window's close button, or
   * Cmd+Q, the app menu, the Dock icon's own Quit, a system-initiated logout/restart/shutdown. */
  onQuitRequested: () => void;
  /** The shell's daemon sidecar exited on its own; `detail` says how. */
  onDaemonExited: (detail: string) => void;
}

export interface ExitCapability {
  /**
   * Registers this webview as the one handling the exit flow and subscribes to the shell's quit
   * gestures, returning the function that undoes the subscriptions. The shell only starts asking
   * before letting an exit through once this has run, so every screen that can be on show calls
   * it — including the daemon-failed-to-start one — or a window stuck there would not be
   * quittable.
   */
  register(handlers: ExitHandlers): () => void;
  /** Proof this webview is alive and answering; see `useAppExit` for when it is sent. */
  heartbeat(): Promise<void>;
  /** Ends the process. Only after the daemon has been shut down. */
  confirmQuit(): Promise<void>;
}

export interface NotificationCapability {
  /** Whether notifications may be shown, asking the user if that has not happened yet. */
  ensurePermission(): Promise<boolean>;
  notify(notification: { title: string; body: string }): Promise<void>;
}

export interface BadgeCapability {
  /** Shows `count` on the application's icon; 0 clears it. */
  set(count: number): Promise<void>;
}

/** Picks the implementation once, at startup, from the environment the page is running in. */
export function selectPlatform(): PlatformAdapter {
  return "__TAURI_INTERNALS__" in window ? tauriPlatform() : browserPlatform();
}
