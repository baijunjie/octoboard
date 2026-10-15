import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { type IBufferRange, Terminal } from "@xterm/xterm";

import { type HoveredTerminalLink, linkBox, terminalLinkHandler } from "./terminalLinks";
import { XTERM_THEMES } from "./xtermThemes";

/** Shift, Ctrl, Alt: the keys xterm itself treats as modifier-only. */
const MODIFIER_KEY_CODES = new Set([16, 17, 18]);

/**
 * Returns a key-event observer that keeps a bare modifier keydown from arming xterm's private
 * `_keyDownSeen` flag. xterm sets that flag on every keydown, `Shift` included, and drops an input
 * method's direct `insertText` commit while it is set. With a CJK input method, WebKit delivers the
 * commit for a shifted full-width mark (`？` is Shift+/) *before* the mark's own keydown, so the
 * `Shift` keydown has already armed the flag and the first press is swallowed.
 *
 * xterm writes the flag before it calls the custom key handler, and clears it before it calls it on
 * keyup, so the observer can tell what the flag held before a modifier keydown and put that back.
 * It is restored rather than cleared: while another key is still down, that key's commit may still
 * be pending through xterm's own deferred path, and letting the `input` event through as well
 * would send it twice.
 *
 * It must run from the terminal's one custom key handler, since xterm accepts only one, and first
 * in it, ahead of any early return, so that it sees every keydown and keyup. It reaches into a
 * private field, so recheck it on every `@xterm/xterm` upgrade.
 *
 * TODO: remove once `@xterm/xterm` stops arming `_keyDownSeen` on a modifier-only keydown.
 */
function keepModifiersFromArmingKeyDownSeen(term: Terminal): (event: KeyboardEvent) => void {
  const core = (term as unknown as { _core?: { _keyDownSeen?: boolean } })._core;
  // True after a non-modifier keydown and until the next keyup: xterm's flag, minus the modifiers.
  let armed = false;
  return (event) => {
    if (event.type === "keyup") {
      armed = false;
    } else if (event.type === "keydown") {
      if (!MODIFIER_KEY_CODES.has(event.keyCode)) armed = true;
      else if (core && "_keyDownSeen" in core) core._keyDownSeen = armed;
    }
  };
}

/**
 * Calls `onUserInput` right before each `onData` that xterm marks as the user's: a key, a paste, an
 * input method's text, a wheel turned into arrow keys. xterm's own answers to queries in the output
 * (device attributes, cursor position, colours, focus and window reports) are not marked. Mouse
 * reports are marked too when they come through `onData` (the SGR and SGR-pixels encodings), as is
 * an Alt+click that moves the cursor (the controller keeps that off outside `attach`..`detach`,
 * so a click never reopens a dormant session); the default encoding comes through `onBinary`
 * unmarked. The controller itself tells the mouse reports apart (see `MOUSE_REPORT`, `onBinary`),
 * since they are not typing. The public API does not expose the mark, so this reaches
 * `coreService.onUserInput` through the private `_core`; recheck it on every `@xterm/xterm`
 * upgrade. Without it nothing is ever marked, so only the user's input typed while saved output is
 * still being parsed is lost.
 */
function watchUserInput(term: Terminal, onUserInput: () => void): void {
  const core = (
    term as unknown as {
      _core?: { coreService?: { onUserInput?: (listener: () => void) => unknown } };
    }
  )._core;
  core?.coreService?.onUserInput?.(onUserInput);
}

/** Whether `data` is a mouse report that reaches `onData`: SGR and SGR-pixels (`CSI < b;x;y M|m`).
 * xterm's default encoding always goes through `onBinary`, and it has no urxvt encoding. */
// eslint-disable-next-line no-control-regex -- Terminal mouse reports begin with the escape control character.
const MOUSE_REPORT = /^\x1b\[<\d+;\d+;\d+[Mm]$/;

/** A focus report (`CSI I` / `CSI O`), which xterm sends unmarked whenever the terminal gains or
 * loses focus while focus reporting (`?1004h`) is on. */
// eslint-disable-next-line no-control-regex -- Terminal focus reports begin with the escape control character.
const FOCUS_REPORT = /^\x1b\[[IO]$/;

/** Turns off the mouse tracking (X10 to any-motion, and the SGR encoding) and focus reporting an
 * agent may have switched on, which xterm answers with input of its own. `?1015l` (urxvt) is a
 * no-op in xterm 6.0.0, which does not support that encoding; it is harmless and kept for an
 * xterm that does. */
const RESET_REPORTING_MODES = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1004l\x1b[?1006l\x1b[?1015l";

/** How long the terminal's size must hold still before `syncSize` reports it to the daemon. */
const RESIZE_SETTLE_MS = 120;

export type TermStatus = "connecting" | "open" | "closed" | "not_running";

export interface TerminalControllerHandlers {
  onStatusChange: (status: TermStatus) => void;
  /** Whether the screen shows anything of the current session yet: false from a reset until the
   * first output after it has been written and drawn. Saved output (see `detach`) does not count:
   * it is not what a session being started is waiting for. */
  onPaintedChange?: (painted: boolean) => void;
  /** Input typed while armed for a session with no process (see `armWake`), at most one call in
   * flight per session; resolves to whether the session was started. When it was not, the held
   * input is dropped and the next input asks again. */
  onWake?: (sessionId: string) => Promise<boolean>;
  /** Opens a web address the user Cmd/Ctrl+clicked in the output, in their own browser; absent
   * where the platform has no way to, which leaves the links inert. */
  openLink?: (url: string) => Promise<void>;
  /** The OSC 8 hyperlink the pointer is on, whose visible text may differ from its target, or
   * `undefined` once it is no longer on one. Plain-text addresses are not reported: their text is
   * their target. */
  onLinkHover?: (link: HoveredTerminalLink | undefined) => void;
}

/**
 * Owns the terminal pane end to end: the one `xterm.js` instance, the active session's socket,
 * the connection status, and keyboard focus — all four change together inside `attach()`, which
 * is the only place any of them is written. Splitting them across independent variables was tried
 * and produced two concrete defects: clicking anything in the UI moved DOM focus off xterm's hidden
 * textarea so keystrokes stopped reaching the agent while output kept flowing, and a superseded
 * socket's `close` event — arriving asynchronously after a session switch — stamped "closed" over
 * the new connection's "open". Routing every state change
 * through one method with a `generation` guard (below) closes off both: nothing reads or writes
 * `status`/`socket`/`sessionId` outside `attach()` and the listeners it installs, and a stale
 * socket's events are discarded by comparing against the generation they were opened for.
 *
 * This class only knows about one session's socket; it has no notion of whether that session's
 * process is still meant to be running. Automatic reconnection after a dropped socket is therefore
 * `TerminalPane`'s job (it has the `Session` record with the status this needs), not this one's.
 */
export class TerminalController {
  readonly term: Terminal;
  private readonly fitAddon: FitAddon;
  private handlers: TerminalControllerHandlers;

  private container?: HTMLElement;
  /** The buffer line on the first row when the hovered OSC 8 link was reported. */
  private hoveredViewportY = 0;
  /** False until a frame has passed since `open()`; see `mount`. */
  private touchable = false;
  private openFrame?: number;
  private socket?: WebSocket;
  private sessionId?: string;
  /** The session whose output is currently on screen. It outlives `sessionId`, which a `detach()`
   * clears: that is what lets a detach tell its own session's last output from another session's
   * (see `detach`). */
  private screenSessionId?: string;
  private status: TermStatus = "closed";
  private painted = false;
  /** Bumped on every `attach()`; a socket's event handlers no-op once their generation is stale. */
  private generation = 0;
  /** The saved output being shown for the session on screen, which has no process (see `detach`).
   * Replaced or cleared with the screen, and compared by identity, so a load that arrives for a
   * screen that has since been reset is dropped. Retained after close to mark this screen as a
   * saved snapshot, so repeated dormant renders do not replace it with another load. */
  private savedLoad?: WebSocket;
  /** Saved output or a reset barrier written and not yet parsed. While any is, xterm's answers
   * to output queries are dropped unless it marked the data as the user's (`userInput`). */
  private savedWritesInFlight = 0;
  /** Set by xterm's `onUserInput` right before the `onData` it belongs to, and consumed there:
   * whether that data came from the user (a key, a paste, an input method) rather than being
   * xterm's own answer to a query. */
  private userInput = false;
  private disposed = false;
  private pendingInput: Uint8Array[] = [];
  /** The session whose process typing should start (an archived one on screen), if any. */
  private wakeFor: string | undefined;
  /** The session an `onWake` is in flight for, so input typed meanwhile does not ask again. */
  private waking: string | undefined;
  /** The pending trailing send of `syncSize`, and the size the daemon was last told. */
  private resizeTimer?: ReturnType<typeof setTimeout>;
  private sentSize?: { cols: number; rows: number };

  constructor(handlers: TerminalControllerHandlers, initialColorTheme: "light" | "dark") {
    this.handlers = handlers;
    const onLinkClick = terminalLinkHandler(handlers.openLink);
    this.term = new Terminal({
      scrollback: 10000,
      // A monospace stack with CJK coverage: these agents render box-drawing TUIs and sometimes
      // CJK status text, and the default `courier-new` xterm.js falls back to has neither.
      fontFamily: '"SF Mono", Menlo, Consolas, "Noto Sans Mono CJK SC", "PingFang SC", monospace',
      theme: XTERM_THEMES[initialColorTheme],
      // The colours on screen are the agent's own, picked for whichever background its own theme
      // setting assumes, so on the other one some of them all but vanish (a dark theme's pale
      // greys and blues on the light terminal). xterm.js lightens or darkens a foreground under
      // this ratio against its cell's background as far as it can towards it: the WCAG text bar.
      // Faint text is held to half of it, and box-drawing, block-element and most powerline glyphs
      // are left as they are.
      minimumContrastRatio: 4.5,
      // An Option+click sends cursor-move arrow keys marked as the user's input, which would
      // reopen a dormant session; it is only on from `attach` until `detach`.
      altClickMovesCursor: false,
      // Leaving `allowNonHttpProtocols` unset is what keeps an OSC 8 link to any other scheme
      // (`file:`, a custom app scheme) from being offered at all. A handler of our own also replaces
      // xterm's default, which asks with `confirm()` before opening; the target tooltip stands in.
      linkHandler: {
        activate: onLinkClick,
        hover: (_event, uri, range) => this.hoverLink(uri, range),
        leave: () => this.handlers.onLinkHover?.(undefined),
      },
    });
    this.fitAddon = new FitAddon();
    this.term.loadAddon(this.fitAddon);
    this.term.loadAddon(new WebLinksAddon(onLinkClick));
    // The tooltip is placed against the viewport, so a scroll under the pointer leaves it behind.
    // xterm reports every line feed that scrolls the buffer, even while the view stays put under a
    // streaming output, so only a view that has actually moved counts.
    this.term.onScroll(() => {
      if (this.term.buffer.active.viewportY !== this.hoveredViewportY) this.handlers.onLinkHover?.(undefined);
    });

    const keepModifiersFromArming = keepModifiersFromArmingKeyDownSeen(this.term);

    // `Ctrl+C` is swallowed by WKWebView above xterm.js while every other modifier combination
    // passes through (see "Known pitfalls of the Tauri / Rust approach" in docs/architecture.md) —
    // it is the most-used key in a terminal, so it is intercepted here and its raw byte (ETX, 0x03) is
    // written directly, bypassing xterm's own key-to-data pipeline entirely.
    this.term.attachCustomKeyEventHandler((event) => {
      keepModifiersFromArming(event);
      if (
        event.type === "keydown" &&
        event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === "c"
      ) {
        event.preventDefault();
        this.sendInput(new Uint8Array([0x03]));
        return false;
      }
      return true;
    });

    // Saved output may be left in the alternate screen, where xterm turns a wheel turn into arrow
    // keys: that would be input, and would wake an archived session. In the normal buffer the wheel
    // scrolls, which is wanted.
    this.term.attachCustomWheelEventHandler(
      () => this.socket !== undefined || this.term.buffer.active.type !== "alternate",
    );

    watchUserInput(this.term, () => (this.userInput = true));
    this.term.onData((data) => {
      let fromUser = this.userInput;
      this.userInput = false;
      // With no socket attached, a focus report is never the user typing, and must not wake a
      // dormant session: focus returning to the terminal (a dialog closing, say) before the mode
      // reset (`RESET_REPORTING_MODES`) is parsed would otherwise reopen it.
      if (!this.socket && FOCUS_REPORT.test(data)) return;
      // Saved output may have turned mouse tracking on, with the reset (`RESET_REPORTING_MODES`)
      // still to be parsed: a click then would be reported, and a report is not the user typing.
      if (
        fromUser &&
        this.savedWritesInFlight > 0 &&
        this.term.modes.mouseTrackingMode !== "none" &&
        MOUSE_REPORT.test(data)
      ) {
        fromUser = false;
      }
      this.sendInput(new TextEncoder().encode(data), fromUser);
    });
    // Mouse reports in the default encoding are not UTF-8: a coordinate is `32 + n`, so a byte can
    // exceed 127, and xterm always sends them through `onBinary` rather than `onData`, as a JS
    // string of raw code units (one per byte), hence the mask back to a byte rather than a UTF-8
    // encode (see "Known pitfalls of the Tauri / Rust approach" in docs/architecture.md). xterm
    // does not mark them as the user's, and they are not typing. While saved output is parsed they
    // are dropped along with xterm's own replies (see `sendInput`); after that the reset reporting
    // modes (`RESET_REPORTING_MODES`) stop xterm from producing them.
    this.term.onBinary((data) =>
      this.sendInput(Uint8Array.from(data, (c) => c.charCodeAt(0) & 0xff), false),
    );
  }

  /** Reports the OSC 8 link the pointer has come onto, placed from the screen's cell grid. */
  private hoverLink(uri: string, range: IBufferRange): void {
    const screen = this.term.element?.querySelector(".xterm-screen")?.getBoundingClientRect();
    if (!screen) return;
    const { cols, rows } = this.term;
    const viewportY = this.term.buffer.active.viewportY;
    this.hoveredViewportY = viewportY;
    this.handlers.onLinkHover?.(linkBox(uri, range, { cols, rows, viewportY, screen }));
  }

  mount(container: HTMLElement): void {
    this.container = container;
    this.term.open(container);
    // Observed: fitting or clearing the terminal in the same task as `open()` throws from inside
    // xterm's own scheduled work, where no caller's `try` can catch it — and an uncaught throw
    // there takes the whole window down with it. Deferring by a frame avoids it. This is anchored
    // to that observation rather than to any internal state of the library, which is not ours to
    // assume.
    this.openFrame = requestAnimationFrame(() => {
      this.openFrame = undefined;
      this.touchable = true;
      this.fit();
    });
  }

  dispose(): void {
    // The scheduled frame would otherwise fit a disposed terminal, and a throw inside it is
    // exactly the uncatchable kind this class defers around in the first place.
    if (this.openFrame !== undefined) cancelAnimationFrame(this.openFrame);
    clearTimeout(this.resizeTimer);
    this.disposed = true;
    this.generation++;
    this.socket?.close();
    this.socket = undefined;
    this.savedLoad?.close();
    this.savedLoad = undefined;
    this.term.dispose();
  }

  /**
   * Names xterm's input textarea for assistive technology. xterm reads its name from a
   * process-wide English string once, in `open()`, so it can neither follow a language change nor
   * be set per instance; this overwrites the attribute, so call it after `mount()` and whenever the
   * label changes.
   */
  setInputLabel(label: string): void {
    this.term.textarea?.setAttribute("aria-label", label);
  }

  get currentStatus(): TermStatus {
    return this.status;
  }

  get currentSessionId(): string | undefined {
    return this.sessionId;
  }

  /**
   * Switches the running terminal's colours to the given mode's palette, without recreating the
   * `xterm.js` instance — xterm applies a theme change to the live screen immediately, so this is
   * the one safe way to follow the app's light/dark switch (the instance itself is mount-only, per
   * `TerminalPane`'s effect).
   */
  setColorTheme(mode: "light" | "dark"): void {
    this.term.options.theme = XTERM_THEMES[mode];
  }

  /**
   * The one state transition. Tears down any existing socket, resets the screen, connects to the
   * new session's terminal stream, and — only when `userInitiated` — restores keyboard focus to
   * xterm; atomically enough that nothing outside this method ever observes a mix of old and new
   * session/socket/status/focus.
   *
   * `userInitiated` separates a selection or a manual Reconnect click (focus belongs on the
   * terminal) from a backoff timer healing a dropped connection in the background (focus must stay
   * wherever it already was — a user mid-keystroke in an open dialog must not be yanked out of it
   * by a reconnect they did not ask for).
   *
   * Call this only for a session that is actually running (`working` / `waiting_user` / `idle`);
   * an interrupted or archived session must be resumed first (`resume_session`), per
   * `apps/daemon/PROTOCOL.md` — attaching to a session whose process is not running gets only the
   * output its last process left, if any, and then the close. That close is still handled below
   * (as `not_running`) as a safety net for the race where a session stops between the click and
   * the socket connecting, not as the normal path for opening a dormant session.
   */
  attach(sessionId: string, wsUrl: string, userInitiated: boolean): void {
    const generation = ++this.generation;
    this.socket?.close();
    this.socket = undefined;
    this.sessionId = sessionId;
    // Input typed to wake this very session carries over into its connection; anything else held
    // belonged to another session.
    if (this.wakeFor !== sessionId) this.pendingInput = [];
    this.wakeFor = undefined;
    // Reset unconditionally rather than appending the replay to what is already on screen: the
    // ring buffer always replays its full retained window from the start, not just what changed
    // since the drop, so appending would duplicate whatever overlaps. Losing scrollback older than
    // the ring buffer's retention window on every reconnect is the accepted cost of not risking
    // corrupted-looking duplicated output.
    this.resetScreen();
    this.screenSessionId = sessionId;
    this.setStatus("connecting");
    this.term.options.altClickMovesCursor = true;

    const socket = new WebSocket(wsUrl);
    socket.binaryType = "arraybuffer";
    this.socket = socket;

    let everOpened = false;

    socket.addEventListener("open", () => {
      if (generation !== this.generation) return;
      everOpened = true;
      this.sendResize(this.term.cols, this.term.rows);
      for (const data of this.pendingInput.splice(0)) socket.send(data);
      this.setStatus("open");
      if (userInitiated) this.term.focus();
    });
    socket.addEventListener("message", (ev) => {
      if (generation !== this.generation) return;
      if (!(ev.data instanceof ArrayBuffer)) return;
      if (this.painted) {
        this.term.write(new Uint8Array(ev.data));
        return;
      }
      // The write callback fires once xterm has parsed the data; the frame after it is when the
      // renderer has drawn it.
      this.term.write(new Uint8Array(ev.data), () =>
        requestAnimationFrame(() => {
          if (generation === this.generation) this.setPainted(true);
        }),
      );
    });
    socket.addEventListener("close", () => {
      if (generation !== this.generation) return;
      // The daemon closes at once, after at most the saved output, when the session's process is
      // not running — that is a status to display and offer resume for, not a dropped connection
      // (apps/daemon/PROTOCOL.md, "GET /ws/term/:session"). It is also what a *reconnect*
      // gets if it loses a race against the session genuinely ending, so callers must not treat
      // this alone as proof the process is gone — only the session's own stored status says that.
      this.setStatus(everOpened ? "closed" : "not_running");
    });

    if (userInitiated) {
      // Focus follows the session switch even while still connecting, so keystrokes typed during
      // the brief "connecting" window are queued (see sendInput below) instead of landing nowhere.
      this.term.focus();
    }
  }

  /** Puts keyboard focus on the terminal, as when a session is selected. Skipped while the terminal
   * is untouchable (see `mount`). */
  focus(): void {
    if (this.touchable) this.term.focus();
  }

  /** Detaches without connecting a new session — used when nothing is selected, or the selected
   * session is dormant (interrupted/archived) and waiting to be resumed.
   *
   * With `savedOutputUrl` (the session's terminal socket), replaces the live screen with the
   * daemon's saved snapshot, even for the same session: a live socket may already have delivered
   * the agent's shutdown cleanup. Read-only: nothing is sent on the saved-output socket, and a
   * keystroke still goes to `armWake`. A later detach for the same dormant session keeps that
   * snapshot, including while its load is pending; `attach()` replaces it on resume.
   *
   * `keepScreenFor` names the session the pane is detaching to. Without a saved-output URL, its
   * existing screen is kept only if it belongs to that session. Pass nothing when the pane loses
   * its session altogether, so output never remains under another session's header or no header.
   */
  detach({
    keepScreenFor,
    savedOutputUrl,
  }: {
    keepScreenFor?: string;
    savedOutputUrl?: string;
  }): void {
    this.generation++;
    this.socket?.close();
    this.socket = undefined;
    this.sessionId = undefined;
    this.term.options.altClickMovesCursor = false;
    if (
      keepScreenFor === undefined ||
      keepScreenFor !== this.screenSessionId ||
      (savedOutputUrl !== undefined && this.savedLoad === undefined)
    ) {
      this.resetScreen();
      if (keepScreenFor !== undefined && savedOutputUrl !== undefined) {
        this.loadSavedOutput(keepScreenFor, savedOutputUrl);
      }
    }
    this.setStatus("closed");
  }

  private loadSavedOutput(sessionId: string, url: string): void {
    const socket = new WebSocket(url);
    socket.binaryType = "arraybuffer";
    this.savedLoad = socket;
    this.screenSessionId = sessionId;
    socket.addEventListener("message", (ev) => {
      if (this.savedLoad !== socket || !(ev.data instanceof ArrayBuffer)) return;
      this.savedWritesInFlight++;
      this.term.write(new Uint8Array(ev.data));
      // The process that left these modes on is gone, so anything xterm would report under them
      // could only wake the session by accident (see `armWake`).
      this.term.write(RESET_REPORTING_MODES, () => this.savedWritesInFlight--);
    });
  }

  /** Tells the daemon the terminal's current size once it has settled for `RESIZE_SETTLE_MS`, and
   * only if it differs from what the daemon already has. Each size sent is a SIGWINCH to the agent,
   * so a continuous container resize (a sidebar drag, a window drag) must not forward every
   * intermediate size; `fit()` still runs live, so the screen itself keeps up. */
  syncSize(): void {
    clearTimeout(this.resizeTimer);
    this.resizeTimer = setTimeout(() => {
      const { cols, rows } = this.term;
      if (this.sentSize?.cols === cols && this.sentSize.rows === rows) return;
      this.sendResize(cols, rows);
    }, RESIZE_SETTLE_MS);
  }

  /** Skipped while the terminal is untouchable (see `mount`); there is nothing on screen to clear
   * that early anyway.
   *
   * The reset is queued behind whatever has been written and not parsed yet, rather than done on
   * the spot: xterm parses a write in a later task, and `reset()` does not drop what is still
   * queued, so a large replay written just before a session switch would otherwise be drawn on the
   * next session's screen. */
  private resetScreen(): void {
    this.screenSessionId = undefined;
    this.savedLoad?.close();
    this.savedLoad = undefined;
    this.setPainted(false);
    if (!this.touchable) return;
    // Queued live output may contain a query. Its answer must not wake the dormant session
    // while this reset waits for that output to finish parsing.
    this.savedWritesInFlight++;
    this.term.write("", () => {
      this.savedWritesInFlight--;
      if (this.disposed) return;
      this.term.reset();
      this.handlers.onLinkHover?.(undefined);
    });
  }

  /** Skipped while the terminal is untouchable (see `mount`) and while the pane has no size: a
   * `ResizeObserver` reports the container's first, zero-sized layout pass, and the next
   * observation fits for real. */
  fit(): void {
    if (
      !this.touchable ||
      !this.container ||
      this.container.offsetWidth === 0 ||
      this.container.offsetHeight === 0
    ) {
      return;
    }
    this.fitAddon.fit();
  }

  private setPainted(painted: boolean): void {
    if (this.painted === painted) return;
    this.painted = painted;
    this.handlers.onPaintedChange?.(painted);
  }

  private setStatus(status: TermStatus): void {
    this.status = status;
    this.handlers.onStatusChange(status);
  }

  /** Makes the next input start `sessionId`'s process: it is held (`pendingInput`) and handed to
   * the session once `attach` connects to it, and `onWake` is told whenever none is already in
   * flight for it. A wake reported as started can still leave the session dormant: an "already
   * running" refusal counts as started, and the daemon gives one to a wake that races the archive
   * of a process still exiting; the next input then asks again rather than piling up behind it.
   * Re-arming the same session keeps what is already held.
   *
   * The screen is kept, along with the modes the dead process left on it (mouse tracking, focus
   * reports), under which xterm would answer a click or a focus change with a report that is not
   * the user typing; so the modes are reset locally, without clearing the screen. */
  armWake(sessionId: string): void {
    if (this.wakeFor === sessionId) return;
    this.wakeFor = sessionId;
    this.pendingInput = [];
    if (this.touchable) this.term.write(RESET_REPORTING_MODES);
  }

  disarmWake(): void {
    this.wakeFor = undefined;
  }

  private sendInput(data: Uint8Array, fromUser = true): void {
    if (this.savedWritesInFlight > 0 && !fromUser) return;
    if (!this.socket && this.wakeFor !== undefined) {
      this.pendingInput.push(data);
      const sessionId = this.wakeFor;
      if (this.handlers.onWake && this.waking !== sessionId) {
        this.waking = sessionId;
        void this.handlers.onWake(sessionId).then((started) => {
          if (this.waking === sessionId) this.waking = undefined;
          // Still waiting on this very session: forget what was typed, so the next input tries again
          // instead of replaying stale keystrokes into whenever the session does start.
          if (!started && this.wakeFor === sessionId && !this.socket) this.pendingInput = [];
        });
      }
      return;
    }
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(data);
    } else if (this.socket?.readyState === WebSocket.CONNECTING) {
      // `WebSocket.send()` throws before OPEN; buffer and flush there instead of dropping the
      // keystroke. CLOSING/CLOSED intentionally do not buffer, or input typed after the pane goes
      // idle would all replay into whatever session connects next.
      this.pendingInput.push(data);
    }
  }

  private sendResize(cols: number, rows: number): void {
    // Defensive: `fit` yields a positive size for a laid-out container, and a zero one must never
    // be handed to the agent.
    if (cols <= 0 || rows <= 0) return;
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.sentSize = { cols, rows };
      this.socket.send(JSON.stringify({ type: "resize", cols, rows }));
    }
  }
}
