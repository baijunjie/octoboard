import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { type IBufferRange, Terminal } from "@xterm/xterm";

import { TerminalLinkColors } from "./linkColors";
import { installPathLinks, type PathAnswer, type PathLinkCache, type PathLinkTarget } from "./pathLinks";
import {
  brokenUrlLinkProvider,
  type HoveredTerminalLink,
  linkBox,
  TERMINAL_URL_REGEX,
  terminalLinkHandler,
} from "./terminalLinks";
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
 * an Alt+click that moves the cursor; the default encoding comes through `onBinary`
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

/** Turns off the mouse tracking (X10 to any-motion, and the SGR encoding) and focus reporting an
 * agent may have switched on, which xterm answers with input of its own. `?1015l` (urxvt) is a
 * no-op in xterm 6.0.0, which does not support that encoding; it is harmless and kept for an
 * xterm that does. */
const RESET_REPORTING_MODES = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1004l\x1b[?1006l\x1b[?1015l";

/** The most a single `write` hands xterm. xterm parses one write as one task, and a session's whole
 * replay arrives as one frame, which would hold the main thread for tens of milliseconds; between
 * writes it yields, and its decoders keep their state across them, so a character or an escape
 * sequence cut by a slice is still read whole. */
const WRITE_SLICE_BYTES = 64 * 1024;

/** How long the terminal's size must hold still before `syncSize` reports it to the daemon. */
const RESIZE_SETTLE_MS = 120;

/** What `exposeReadOnlyView` hangs on the terminal's element. */
export interface TerminalTextView {
  cols: () => number;
  rows: () => number;
  /** The text of viewport row `y`, one character per column. */
  row: (y: number) => string;
}
export type TerminalElement = HTMLElement & { octoboardTerminalText?: TerminalTextView };

export type TermStatus = "connecting" | "open" | "closed" | "not_running";

export interface TerminalControllerOptions {
  /** Draws on the GPU (`useWebglRenderer`), or keeps xterm's DOM renderer. On by default, meant for the one agent
   * terminal: any other xterm on the page passes `false`. WebKit caps a page's active WebGL contexts, counts a
   * released one until garbage collection (disposing the addon or `loseContext()` does not free the slot), and over
   * the cap takes the context of the terminal that drew least recently, which can be the agent terminal's. */
  gpu?: boolean;
  /** Tells the daemon only a size the terminal was fitted to its container at, never xterm's
   * default 80 by 24: for a terminal mounted out of sight (a hidden tab), which cannot be fitted
   * until it is shown and would otherwise send its process a size it then takes back. */
  fittedSizesOnly?: boolean;
}

export interface TerminalControllerHandlers {
  onStatusChange: (status: TermStatus) => void;
  /** Whether the screen shows anything of the current session yet: false from a reset until the
   * first output after it has been written and drawn. Saved output (see `detach`) does not count:
   * it is not what a session being started is waiting for. */
  onPaintedChange?: (painted: boolean) => void;
  /** Opens a web address the user clicked in the output, in their own browser; absent
   * where the platform has no way to, which leaves the links inert. */
  openLink?: (url: string) => Promise<void>;
  /** The OSC 8 hyperlink the pointer is on, whose visible text may differ from its target, or
   * `undefined` once it is no longer on one. Plain-text addresses are not reported: their text is
   * their target. */
  onLinkHover?: (link: HoveredTerminalLink | undefined) => void;
  /** What the daemon makes of the file paths in the output, for the session on screen: absent where
   * there is no daemon to ask, which leaves paths as plain text. */
  pathLinks?: {
    /** What each of `paths` names, in order. */
    probe: (paths: string[]) => Promise<PathAnswer[]>;
    /** Opens a path that was found: in the file viewer when it has a view (a folder's is its README
     * or index file), otherwise a file with its default application and a folder in the file
     * manager. `onHost` asks for a folder to go to the file manager even when it has a view. */
    open: (target: PathLinkTarget, onHost: boolean) => Promise<void>;
  };
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
 * the job of `TerminalPane` and `ShellTerminalView` (they have the record with the status this
 * needs), not this one's.
 */
export class TerminalController {
  readonly term: Terminal;
  private readonly fitAddon: FitAddon;
  private handlers: TerminalControllerHandlers;

  private container?: HTMLElement;
  /** The buffer line on the first row when the hovered OSC 8 link was reported. */
  private hoveredViewportY = 0;
  private readonly linkColors: TerminalLinkColors;
  private readonly pathLinks?: PathLinkCache;
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
  /** The pending trailing send of `syncSize`, and the size the daemon was last told. */
  private resizeTimer?: ReturnType<typeof setTimeout>;
  private sentSize?: { cols: number; rows: number };

  private readonly options: Required<TerminalControllerOptions>;
  /** Whether the terminal has been fitted to its container at least once. */
  private fitted = false;

  constructor(handlers: TerminalControllerHandlers, initialColorTheme: "light" | "dark", { gpu = true, fittedSizesOnly = false }: TerminalControllerOptions = {}) {
    this.handlers = handlers;
    this.options = { gpu, fittedSizesOnly };
    const onLinkClick = terminalLinkHandler(handlers.openLink, () => this.term.hasSelection());
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
      altClickMovesCursor: true,
      // Decorations, which colour the links, are an experimental API xterm gates behind this.
      allowProposedApi: true,
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
    // Ahead of the addon: where both find a link, the one registered first is used.
    this.term.registerLinkProvider(brokenUrlLinkProvider(this.term, onLinkClick));
    this.term.loadAddon(new WebLinksAddon(onLinkClick, { urlRegex: TERMINAL_URL_REGEX }));
    const paths =
      handlers.pathLinks &&
      installPathLinks(this.term, handlers.pathLinks, () => this.term.hasSelection(), () => this.linkColors.refresh());
    this.pathLinks = paths?.cache;
    // The tooltip is placed against the viewport, so a scroll under the pointer leaves it behind.
    // xterm reports every line feed that scrolls the buffer, even while the view stays put under a
    // streaming output, so only a view that has actually moved counts.
    this.term.onScroll(() => {
      if (this.term.buffer.active.viewportY !== this.hoveredViewportY) this.handlers.onLinkHover?.(undefined);
    });
    this.linkColors = new TerminalLinkColors(this.term, initialColorTheme, paths?.segments);

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

    watchUserInput(this.term, () => (this.userInput = true));
    this.term.onData((data) => {
      let fromUser = this.userInput;
      this.userInput = false;
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
    this.exposeReadOnlyView();
    // Observed: fitting or clearing the terminal in the same task as `open()` throws from inside
    // xterm's own scheduled work, where no caller's `try` can catch it — and an uncaught throw
    // there takes the whole window down with it. Deferring by a frame avoids it. This is anchored
    // to that observation rather than to any internal state of the library, which is not ours to
    // assume.
    this.openFrame = requestAnimationFrame(() => {
      this.openFrame = undefined;
      this.touchable = true;
      if (this.options.gpu) this.useWebglRenderer();
      this.fit();
    });
  }

  /**
   * Hangs a read-only view of the screen's text on the terminal's element, for the gallery's pointer
   * steps and the wkbench harness: the WebGL renderer leaves `.xterm-rows` empty, so the DOM no
   * longer says what the terminal shows. It reads the buffer and changes nothing.
   */
  private exposeReadOnlyView(): void {
    const { term } = this;
    const view: TerminalTextView = {
      cols: () => term.cols,
      rows: () => term.rows,
      // A wide character's second cell and an empty cell both read as a space, so a string index
      // is a column (for text of single UTF-16 units).
      row: (y) => {
        const line = term.buffer.active.getLine(term.buffer.active.viewportY + y);
        if (!line) return "";
        let text = "";
        for (let x = 0; x < term.cols; x++) {
          const cell = line.getCell(x);
          text += cell?.getChars() || " ";
        }
        return text;
      },
    };
    if (term.element) (term.element as TerminalElement).octoboardTerminalText = view;
  }

  /**
   * Draws the terminal on the GPU instead of with DOM elements: a streaming agent repaints
   * constantly, and the DOM renderer's per-row elements are the costly way to do that. Loaded after
   * `open()`, as the addon needs the terminal's element, and given up on without a trace where the
   * webview has no WebGL 2 or loses the context later: xterm then goes on with its DOM renderer,
   * which costs more per repaint for a streaming agent and may draw some details, such as
   * glyph edges, a little differently from the GPU renderer.
   */
  private useWebglRenderer(): void {
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => webgl.dispose());
      this.term.loadAddon(webgl);
    } catch {
      // Stays on the DOM renderer.
    }
  }

  dispose(): void {
    // The scheduled frame would otherwise fit a disposed terminal, and a throw inside it is
    // exactly the uncatchable kind this class defers around in the first place.
    if (this.openFrame !== undefined) cancelAnimationFrame(this.openFrame);
    clearTimeout(this.resizeTimer);
    this.linkColors.dispose();
    this.pathLinks?.clear();
    this.disposed = true;
    this.generation++;
    this.socket?.close();
    this.socket = undefined;
    this.savedLoad?.close();
    this.savedLoad = undefined;
    this.term.dispose();
  }

  /** Text selected in the terminal buffer, independent of the document's selection. */
  get selectedText(): string {
    return this.term.getSelection();
  }

  selectAll(): void {
    if (this.touchable && !this.disposed) this.term.selectAll();
  }

  /** The session that can accept input now: the one the socket is connected or connecting to. */
  get inputSessionId(): string | undefined {
    if (this.disposed || !this.touchable) return undefined;
    if (this.socket?.readyState === WebSocket.OPEN || this.socket?.readyState === WebSocket.CONNECTING) return this.sessionId;
    return undefined;
  }

  /** Checks the destination again after a clipboard read and lets xterm apply bracketed paste. */
  paste(text: string, sessionId: string): void {
    if (this.inputSessionId !== sessionId || !text) return;
    this.term.paste(text);
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
    this.linkColors.setTheme(mode);
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
    // Anything held belonged to the connection before.
    this.pendingInput = [];
    // Reset unconditionally rather than appending the replay to what is already on screen: the
    // ring buffer always replays its full retained window from the start, not just what changed
    // since the drop, so appending would duplicate whatever overlaps. Losing scrollback older than
    // the ring buffer's retention window on every reconnect is the accepted cost of not risking
    // corrupted-looking duplicated output.
    this.resetScreen();
    this.screenSessionId = sessionId;
    this.setStatus("connecting");

    const socket = new WebSocket(wsUrl);
    socket.binaryType = "arraybuffer";
    this.socket = socket;

    let everOpened = false;

    socket.addEventListener("open", () => {
      if (generation !== this.generation) return;
      everOpened = true;
      if (this.fitted || !this.options.fittedSizesOnly) this.sendResize(this.term.cols, this.term.rows);
      for (const data of this.pendingInput.splice(0)) socket.send(data);
      this.setStatus("open");
      if (userInitiated) this.term.focus();
    });
    socket.addEventListener("message", (ev) => {
      if (generation !== this.generation) return;
      if (!(ev.data instanceof ArrayBuffer)) return;
      if (this.painted) {
        this.write(new Uint8Array(ev.data));
        return;
      }
      // The write callback fires once xterm has parsed the data; the frame after it is when the
      // renderer has drawn it.
      this.write(new Uint8Array(ev.data), () =>
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
   * the agent's shutdown cleanup. Read-only: nothing is sent on the saved-output socket, and
   * nothing typed goes anywhere. A later detach for the same dormant session keeps that
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
      this.write(new Uint8Array(ev.data));
      // The process that left these modes on is gone, so nothing is listening for what xterm
      // would report under them.
      this.term.write(RESET_REPORTING_MODES, () => this.savedWritesInFlight--);
    });
  }

  /** Writes `data` to xterm in slices of at most `WRITE_SLICE_BYTES`, in order; `onParsed` is the last
   * slice's write callback, so it fires once all of `data` has been parsed. */
  private write(data: Uint8Array, onParsed?: () => void): void {
    let start = 0;
    do {
      const end = start + WRITE_SLICE_BYTES;
      this.term.write(data.subarray(start, end), end >= data.length ? onParsed : undefined);
      start = end;
    } while (start < data.length);
  }

  /** Tells the daemon the terminal's current size once it has settled for `RESIZE_SETTLE_MS`, and
   * only if it differs from what the daemon already has. Each size sent is a SIGWINCH to the agent,
   * so a continuous container resize (a sidebar drag, a window drag) must not forward every
   * intermediate size; `fit()` still runs live, so the screen itself keeps up. */
  syncSize(): void {
    clearTimeout(this.resizeTimer);
    this.resizeTimer = setTimeout(() => {
      if (!this.fitted && this.options.fittedSizesOnly) return;
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
    // What the daemon said of the paths was for the session shown until now.
    this.pathLinks?.clear();
    this.savedLoad?.close();
    this.savedLoad = undefined;
    this.setPainted(false);
    if (!this.touchable) return;
    // Queued live output may contain a query. Its answer must not reach the next session's
    // connection while this reset waits for that output to finish parsing.
    this.savedWritesInFlight++;
    this.term.write("", () => {
      this.savedWritesInFlight--;
      if (this.disposed) return;
      this.term.reset();
      this.linkColors.reset();
      this.pathLinks?.clear();
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
    this.fitted = true;
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

  private sendInput(data: Uint8Array, fromUser = true): void {
    if (this.savedWritesInFlight > 0 && !fromUser) return;
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
