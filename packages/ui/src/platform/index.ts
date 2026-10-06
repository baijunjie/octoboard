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
  /** The native window itself — its appearance (the controls and any native chrome), which CSS
   * cannot reach, and its own visibility; present only where a shell owns a native window — a
   * browser tab has neither an appearance to match nor a window to reveal. */
  readonly nativeWindow?: NativeWindowCapability;
  /** Present only where the window draws no titlebar of its own, so the UI's top bar is the
   * window's titlebar: it then carries the drag region and keeps `leadingInset` clear for the
   * window controls the shell floats over it. */
  readonly windowChrome?: WindowChromeCapability;
  /** The shell's native menu bar, for the items that ask the UI to do something. */
  readonly appMenu?: AppMenuCapability;
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

export type NotificationPermissionStatus = "granted" | "denied" | "undecided";

export interface NotificationCapability {
  /** Whether notifications may be shown. Where `permissionPrompt` is present this only reads the
   * answer and never asks; otherwise it asks the user if that has not happened yet. */
  ensurePermission(): Promise<boolean>;
  /** Present only where asking for permission needs a user gesture, as in a browser, which ignores
   * or denies a request made from anywhere else. The UI offers a control that calls `request`. */
  readonly permissionPrompt?: {
    status(): NotificationPermissionStatus;
    request(): Promise<void>;
  };
  notify(notification: { title: string; body: string }): Promise<void>;
}

export interface BadgeCapability {
  /** Shows `count` on the application's icon; 0 clears it. */
  set(count: number): Promise<void>;
}

export interface NativeWindowCapability {
  /** Pins the window's appearance to `theme`; `undefined` hands control back to the OS, so the
   * window keeps following a live system appearance change instead of being pinned to whatever
   * it happened to resolve to at the moment of the call. */
  setTheme(theme: "light" | "dark" | undefined): Promise<void>;
  /** Shows a window the shell created hidden (`open_main_window` in
   * `apps/desktop/src-tauri/src/lib.rs`), once this process has pushed the native theme and
   * painted the page — showing it any earlier is exactly the white/dark flash of the OS's own
   * appearance that creating it hidden exists to avoid. Idempotent: showing an already-visible
   * window, or the shell's own timeout-driven fallback racing this call, is a no-op either way. */
  reveal(): Promise<void>;
}

export interface WindowChromeCapability {
  /** The width, in CSS pixels, at the top bar's leading edge that the window controls cover
   * right now: none while they are hidden, as in fullscreen. */
  leadingInset(): number;
  /** Subscribes to changes of `leadingInset`, returning the function that undoes it. */
  subscribe(callback: () => void): () => void;
}

export interface AppMenuCapability {
  /** Subscribes to the menu's Settings item (Cmd+,), returning the function that undoes it. */
  onSettingsRequested(handler: () => void): () => void;
}

/** Picks the implementation once, at startup, from the environment the page is running in. */
export function selectPlatform(): PlatformAdapter {
  return "__TAURI_INTERNALS__" in window ? tauriPlatform() : browserPlatform();
}
