import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { type IBuffer, type IBufferRange, Terminal } from "@xterm/xterm";

import type { TermControl, TermEvent } from "../protocol";
import { interactionIsTouch } from "../touchInput";
import { IS_MAC, isSecondaryPress } from "../useWindowShortcut";
import { TerminalLinkColors } from "./linkColors";
import { OSC_52, osc52ClipboardText } from "./osc52";
import { installPathLinks, type PathAnswer, type PathLinkCache, type PathLinkTarget } from "./pathLinks";
import { watchPointerForLinks } from "./pointerLinks";
import { macEditingSequence } from "./terminalKeys";
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
 * still being parsed is lost, and xterm's answers go as input rather than as reply frames (see
 * `sendReply`). Returns whether the mark is watched.
 */
function watchUserInput(term: Terminal, onUserInput: () => void): boolean {
  const core = (
    term as unknown as {
      _core?: { coreService?: { onUserInput?: (listener: () => void) => unknown } };
    }
  )._core;
  if (!core?.coreService?.onUserInput) return false;
  core.coreService.onUserInput(onUserInput);
  return true;
}

/** A focus report (`CSI I`, `CSI O`): xterm does not mark it as the user's, but it is this view
 * gaining or losing focus, not an answer to a query. */
// eslint-disable-next-line no-control-regex -- Focus reports begin with the escape control character.
const FOCUS_REPORT = /^\x1b\[[IO]$/;

/** Whether `data` is a mouse report that reaches `onData`: SGR and SGR-pixels (`CSI < b;x;y M|m`),
 * with the button code `b` captured. xterm's default encoding always goes through `onBinary`, and
 * it has no urxvt encoding. */
// eslint-disable-next-line no-control-regex -- Terminal mouse reports begin with the escape control character.
const MOUSE_REPORT = /^\x1b\[<(\d+);\d+;\d+[Mm]$/;

/** Turns off the mouse tracking (X10 to any-motion, and the SGR encoding) and focus reporting an
 * agent may have switched on, which xterm answers with input of its own. `?1015l` (urxvt) is a
 * no-op in xterm 6.0.0, which does not support that encoding; it is harmless and kept for an
 * xterm that does. */
const RESET_REPORTING_MODES = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1004l\x1b[?1006l\x1b[?1015l";

/** Output made of nothing but private mode sets and resets (`CSI ? Pm h`, `CSI ? Pm l`), with at
 * most charset designations (`ESC ( B`) and shifts (SI, SO) around them: what a program writes to
 * re-assert its modes, as Claude Code does after a pause in its input, on a resize and in other
 * cases. Of the modes, only the alternate screen's change the text on screen, by switching buffers,
 * which `keepSelectionThroughModes` checks for itself. */
// eslint-disable-next-line no-control-regex -- Mode sequences begin with the escape control character.
const MODES_ONLY = /^(?:\x1b\[\?\d+(?:;\d+)*[hl]|\x1b[()*+][0-9A-Za-z]|[\x0e\x0f])+$/;
/** Longer output is never a bare re-assertion of modes; past this it is not even looked at. */
const MODES_ONLY_MAX_BYTES = 256;

/** Whether `data` only sets or resets modes (see `MODES_ONLY`). */
function onlySetsModes(data: Uint8Array): boolean {
  return data.length > 0 && data.length <= MODES_ONLY_MAX_BYTES && MODES_ONLY.test(String.fromCharCode(...data));
}

/** Whether `data` is an SGR report of the pointer moving with no button held: motion (32) with the
 * "no button" code (3), not the wheel (64), whatever modifiers (4, 8, 16) are held. */
function isButtonlessMotion(data: string): boolean {
  const code = MOUSE_REPORT.exec(data)?.[1];
  return code !== undefined && (Number(code) & 0b110_0011) === 0b010_0011;
}

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
  /** Puts on the clipboard the text a program in the terminal asks to copy (OSC 52), as Claude Code
   * does with the text selected in its own fullscreen view; absent where there is no clipboard, where
   * the text is only offered to the editing menu (`programCopiedText`). Never called for output
   * written before the screen's session was attached: a live session's replay, a dormant session's
   * saved output, or what was still queued when the screen was reset. */
  copyToClipboard?: (text: string) => void;
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

/** Whether xterm forces a selection under mouse reporting with Option (`macOptionClickForcesSelection`)
 * rather than Shift: xterm counts only these platforms as macOS, not an iPhone or an iPad in its
 * mobile mode. */
export const OPTION_FORCES_SELECTION =
  typeof navigator !== "undefined" && ["Macintosh", "MacIntel", "MacPPC", "Mac68K"].includes(navigator.platform);

/** The terminal's font: a monospace stack with CJK coverage, since these agents render box-drawing
 * TUIs and sometimes CJK status text, and the default `courier-new` xterm.js falls back to has
 * neither. */
export const TERMINAL_FONT_FAMILY = '"SF Mono", Menlo, Consolas, "Noto Sans Mono CJK SC", "PingFang SC", monospace';

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
  /** Has xterm ask the link providers again for the cell under a resting pointer; set by `mount`. */
  private askLinksUnderPointer?: () => void;
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
   * to output queries are dropped unless it marked the data as the user's (`userInput`), and what
   * the output asks to copy is ignored. */
  private savedWritesInFlight = 0;
  /** A live session's replay (the first frame after an attach) written and not yet parsed. What it
   * asks to copy was asked before the attach, and may be long gone from the user's mind. */
  private replayWritesInFlight = 0;
  /** The text a program last asked to copy (OSC 52), while it still stands for what the user picked
   * in that program: the editing menu's Copy offers it when the terminal itself has no selection,
   * since a drag under mouse reporting selects in the program, not here. Dropped once the user turns
   * back to the program (a key, a paste, a press that cannot open the menu) and with the screen. */
  private programCopy?: string;
  /** Counts what the user does that may clear or replace the terminal's selection on purpose: input
   * xterm marks as theirs (keys, pastes, mouse reports other than the pointer moving with no button
   * held), Control-C, which the controller sends itself, and presses that cannot open the editing
   * menu. A selection is put back after a
   * re-assertion of modes only if this has not moved (see `keepSelectionThroughModes`). Motion does
   * not count: it is held back only while a selection is up, so once a re-assertion has cleared it
   * the pointer's next move would be reported and would cancel putting it back. */
  private userActs = 0;
  /** Removes the listeners `mount` puts on the container. */
  private readonly mountListeners = new AbortController();
  /** Set by xterm's `onUserInput` right before the `onData` it belongs to, and consumed there:
   * whether that data came from the user (a key, a paste, an input method) rather than being
   * xterm's own answer to a query. */
  private userInput = false;
  /** `userInput` is watched, so data it leaves unmarked can be told to be an answer to a query. */
  private readonly marksUserInput: boolean;
  private disposed = false;
  private pendingInput: Uint8Array[] = [];
  /** This connection's replay mark, until the replay it describes has arrived. */
  private replayMark?: TermEvent;
  /** The point in the output the last frame of this connection ended at, counted as the daemon
   * counts it; unknown before the replay mark. */
  private outputEnd?: number;
  /** The output written to xterm and not yet parsed, oldest first, with the connection it came on
   * and the point in the output it ends at: xterm parses in order, so the first is what an answer
   * xterm gives now answers. No point marks output whose queries were answered already. */
  private parsing: { generation: number; offset?: number }[] = [];
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
      fontFamily: TERMINAL_FONT_FAMILY,
      theme: XTERM_THEMES[initialColorTheme],
      // The colours on screen are the agent's own, picked for whichever background its own theme
      // setting assumes, so on the other one some of them all but vanish (a dark theme's pale
      // greys and blues on the light terminal). xterm.js lightens or darkens a foreground under
      // this ratio against its cell's background as far as it can towards it: the WCAG text bar.
      // Faint text is held to half of it, and box-drawing, block-element and most powerline glyphs
      // are left as they are.
      minimumContrastRatio: 4.5,
      altClickMovesCursor: true,
      // With mouse reporting on (Claude Code's fullscreen view), a drag goes to the program and makes
      // no selection of the terminal's own, so there is nothing for its menu to copy; Option-drag
      // selects all the same, as Shift-drag does elsewhere. It takes Option-drag's column selection
      // on macOS with it. The Option-click that would then move the cursor is held to the screens
      // without mouse reporting (see `mount`).
      macOptionClickForcesSelection: true,
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
      installPathLinks(this.term, handlers.pathLinks, () => this.term.hasSelection(), () => {
        this.linkColors.refresh();
        // A path the pointer already rests on becomes a link without the pointer having to move.
        this.askLinksUnderPointer?.();
      });
    this.pathLinks = paths?.cache;
    // The tooltip is placed against the viewport, so a scroll under the pointer leaves it behind.
    // xterm reports every line feed that scrolls the buffer, even while the view stays put under a
    // streaming output, so only a view that has actually moved counts.
    this.term.onScroll(() => {
      if (this.term.buffer.active.viewportY !== this.hoveredViewportY) this.handlers.onLinkHover?.(undefined);
    });
    this.linkColors = new TerminalLinkColors(this.term, paths?.segments);
    this.term.parser.registerOscHandler(OSC_52, (payload) => {
      const text = osc52ClipboardText(payload);
      if (text && this.savedWritesInFlight === 0 && this.replayWritesInFlight === 0) {
        this.programCopy = text;
        this.handlers.copyToClipboard?.(text);
      }
      // Handled even when ignored, which keeps xterm from logging it as an unknown sequence.
      return true;
    });

    const keepModifiersFromArming = keepModifiersFromArmingKeyDownSeen(this.term);

    // `Ctrl+C` is swallowed by WKWebView above xterm.js while every other modifier combination
    // passes through (see "Known pitfalls of the Tauri / Rust approach" in docs/architecture.md) —
    // it is the most-used key in a terminal, so it is intercepted here and its raw byte (ETX, 0x03) is
    // written directly, bypassing xterm's own key-to-data pipeline entirely.
    this.term.attachCustomKeyEventHandler((event) => {
      keepModifiersFromArming(event);
      const data = IS_MAC ? macEditingSequence(event) : undefined;
      if (data !== undefined) {
        event.preventDefault();
        if (event.type === "keydown") this.term.input(data, true);
        return false;
      }
      if (
        event.type === "keydown" &&
        event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === "c"
      ) {
        event.preventDefault();
        this.programCopy = undefined;
        this.userActs++;
        this.sendInput(new Uint8Array([0x03]));
        return false;
      }
      return true;
    });

    this.marksUserInput = watchUserInput(this.term, () => (this.userInput = true));
    this.term.onData((data) => {
      let fromUser = this.userInput;
      this.userInput = false;
      // Every view attached to the terminal answers the same queries, and a program that got each
      // answer twice would read the second as input, so answers go as reply frames, of which the
      // daemon writes one view's (see `apps/daemon/PROTOCOL.md`, "`GET /ws/term/:session` —
      // terminal stream").
      if (!fromUser && this.marksUserInput && !FOCUS_REPORT.test(data)) {
        this.sendReply(data);
        return;
      }
      if (fromUser && !isButtonlessMotion(data)) this.userActs++;
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
      // A mouse report is not the user turning back to the program: the pointer crossing the screen
      // on the way to a right-click must leave what the program copied on offer.
      if (fromUser && !MOUSE_REPORT.test(data)) this.programCopy = undefined;
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
    const { signal } = this.mountListeners;
    container.addEventListener(
      "mousedown",
      (event) => {
        // Under mouse reporting an Option-click forces a selection rather than reaching the program,
        // and xterm would turn a click that selected nothing into arrow keys towards it, which a
        // fullscreen program reads as keys of its own (Claude Code walks its prompt history). Set
        // before xterm's own listener on its element reads it.
        this.term.options.altClickMovesCursor = this.term.modes.mouseTrackingMode === "none";
        // A secondary press opens the editing menu, which is how what the program copied gets used.
        if (!isSecondaryPress(event)) {
          this.programCopy = undefined;
          this.userActs++;
        }
      },
      { capture: true, signal },
    );
    // xterm reports pointer motion to a program that tracks any motion in the SGR encodings (Claude
    // Code's fullscreen view does) as the user's input, and clears its own selection on any user input. A selection forced
    // under mouse reporting (Option-drag on macOS) would then be gone the moment the pointer moved on
    // its way to a right-click, leaving the editing menu nothing to copy. While the terminal holds a
    // selection, motion with no button held is kept from the report, which xterm's element listens
    // for: the program sees no hover until the selection goes, which a click, a key or the wheel still
    // does. Over the screen the motion is held back only once it has passed the screen element, whose
    // own listener finds the link under the pointer, so a link's hover still ends when it is left.
    const screen = this.term.element?.querySelector<HTMLElement>(".xterm-screen");
    const holdBack = (event: MouseEvent) =>
      event.buttons === 0 && this.term.modes.mouseTrackingMode === "any" && this.term.hasSelection();
    container.addEventListener(
      "mousemove",
      (event) => {
        if (!(event.target instanceof Node && screen?.contains(event.target)) && holdBack(event)) event.stopPropagation();
      },
      { capture: true, signal },
    );
    screen?.addEventListener(
      "mousemove",
      (event) => {
        if (holdBack(event)) event.stopPropagation();
      },
      { signal },
    );
    if (screen) this.askLinksUnderPointer = watchPointerForLinks(this.term, screen, signal);
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
    this.mountListeners.abort();
    this.programCopy = undefined;
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

  /** The text the program on screen last asked to copy (OSC 52), while it is still current; see
   * `programCopy`. */
  get programCopiedText(): string | undefined {
    return this.programCopy;
  }

  /** Whether the program on screen has mouse reporting on, which sends a plain drag to it rather than
   * selecting here. */
  get mouseReporting(): boolean {
    return this.term.modes.mouseTrackingMode !== "none";
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
    this.linkColors.reset();
  }

  /**
   * The one state transition. Tears down any existing socket, resets the screen, connects to the
   * new session's terminal stream, and — only when `userInitiated`, and not after a tap —
   * restores keyboard focus to xterm; atomically enough that nothing outside this method ever
   * observes a mix of old and new session/socket/status/focus.
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
    this.replayMark = undefined;
    this.outputEnd = undefined;
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
    let firstFrame = true;

    socket.addEventListener("open", () => {
      if (generation !== this.generation) return;
      everOpened = true;
      if (this.fitted || !this.options.fittedSizesOnly) this.sendResize(this.term.cols, this.term.rows);
      for (const data of this.pendingInput.splice(0)) socket.send(data);
      this.setStatus("open");
      if (userInitiated && !interactionIsTouch()) this.term.focus();
    });
    socket.addEventListener("message", (ev) => {
      if (generation !== this.generation) return;
      if (typeof ev.data === "string") {
        this.readTermEvent(ev.data);
        return;
      }
      if (!(ev.data instanceof ArrayBuffer)) return;
      // The daemon replays the ring buffer as one frame, ahead of any live output, and sends none when
      // the ring is empty, so a first frame of live output is held to the same rule.
      const replay = firstFrame;
      firstFrame = false;
      if (replay) this.replayWritesInFlight++;
      if (this.painted && !replay) {
        this.writeOutput(new Uint8Array(ev.data));
        return;
      }
      const paint = !this.painted;
      // The write callback fires once xterm has parsed the data; the frame after it is when the
      // renderer has drawn it.
      this.writeOutput(new Uint8Array(ev.data), () => {
        if (replay) this.replayWritesInFlight--;
        if (!paint) return;
        requestAnimationFrame(() => {
          if (generation === this.generation) this.setPainted(true);
        });
      });
    });
    socket.addEventListener("close", () => {
      if (generation !== this.generation) return;
      // The daemon closes at once, after at most the saved output, when the session's process is
      // not running — that is a status to display and offer resume for, not a dropped connection
      // (apps/daemon/PROTOCOL.md, "`GET /ws/term/:session` — terminal stream"). It is also what a
      // *reconnect* gets if it loses a race against the session genuinely ending, so callers must
      // not treat this alone as proof the process is gone — only the session's own stored status
      // says that.
      this.setStatus(everOpened ? "closed" : "not_running");
    });

    // Picked by a tap, focus stays where it is: the terminal's hidden text field would raise the
    // on-screen keyboard over the session just picked (`interactionIsTouch`); a tap on the
    // terminal focuses it.
    if (userInitiated && !interactionIsTouch()) {
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

  private readTermEvent(text: string): void {
    let event: TermEvent;
    try {
      event = JSON.parse(text) as TermEvent;
    } catch {
      return;
    }
    if (event.type !== "replay") return;
    if (event.length > 0) this.replayMark = event;
    else this.outputEnd = event.offset;
  }

  /** Writes a frame of this connection's output, noting the point in the output it ends at for the
   * answers xterm gives while parsing it (see `parsing`). The replay is written in two: the output
   * whose queries were answered already, and the rest. */
  private writeOutput(data: Uint8Array, onParsed?: () => void): void {
    const mark = this.replayMark;
    this.replayMark = undefined;
    if (mark && mark.length === data.length) {
      this.outputEnd = mark.offset;
      this.writeParsed(data.subarray(0, mark.answered), undefined);
      this.writeParsed(data.subarray(mark.answered), mark.offset, onParsed);
      return;
    }
    if (this.outputEnd !== undefined) this.outputEnd += data.length;
    this.writeParsed(data, this.outputEnd, onParsed);
  }

  private writeParsed(data: Uint8Array, offset: number | undefined, onParsed?: () => void): void {
    this.parsing.push({ generation: this.generation, offset });
    this.write(data, () => {
      this.parsing.shift();
      onParsed?.();
    });
  }

  /** Writes `data` to xterm in slices of at most `WRITE_SLICE_BYTES`, in order; `onParsed` is the
   * last slice's write callback, so it fires once all of `data` has been parsed. `data` that only
   * sets modes keeps the terminal's selection through it (`keepSelectionThroughModes`): it is looked
   * at whole, so a re-assertion of modes counts only as output of its own, never inside a larger
   * one. */
  private write(data: Uint8Array, onParsed?: () => void): void {
    const keepSelection = onlySetsModes(data) ? this.keepSelectionThroughModes() : undefined;
    let start = 0;
    do {
      const end = start + WRITE_SLICE_BYTES;
      this.term.write(data.subarray(start, end), end >= data.length ? onParsed : undefined);
      start = end;
    } while (start < data.length);
    if (keepSelection) this.term.write("", keepSelection);
  }

  /**
   * Notes the selection as it stands right before the output written next is parsed, and returns
   * the callback to write right after that output, which puts the selection back if parsing it
   * cleared it. For output that only sets modes (`onlySetsModes`): xterm clears its selection
   * whenever a mouse tracking mode is set, even one already on, so a program re-asserting its modes
   * (Claude Code does so whenever input arrives after a few idle seconds, such as the focus report
   * of the window coming back) would take away a selection forced under mouse reporting, which
   * nothing on screen has invalidated.
   *
   * The selection is put back only when nothing else could have meant to clear it: the tracking
   * mode and the active buffer are what they were, and the user has done nothing in between
   * (`userActs`); xterm may parse queued output across tasks, letting a click or a key in. Output
   * that comes with anything else, text included, is never looked at, so a selection is never put
   * back over redrawn text.
   */
  private keepSelectionThroughModes(): () => void {
    let kept:
      | { range: IBufferRange; tracking: Terminal["modes"]["mouseTrackingMode"]; buffer: IBuffer["type"]; userActs: number }
      | undefined;
    this.term.write("", () => {
      const range = this.term.getSelectionPosition();
      kept = range && {
        range,
        tracking: this.term.modes.mouseTrackingMode,
        buffer: this.term.buffer.active.type,
        userActs: this.userActs,
      };
    });
    return () => {
      if (
        !kept ||
        this.disposed ||
        this.term.hasSelection() ||
        this.term.modes.mouseTrackingMode !== kept.tracking ||
        this.term.buffer.active.type !== kept.buffer ||
        this.userActs !== kept.userActs
      ) {
        return;
      }
      // The range's end is exclusive, and a selection that spans rows runs on through the ends of
      // the rows in between, which is what a length counted in whole rows gives back. A selection
      // ending at column 0 of a later row comes back ending at the end of the row before, without
      // the line break the original ended with: `select` takes a length, and xterm maps one that
      // reaches a row's end exactly to that end, never to the start of the next row. A column
      // (block) selection keeps its mode through `select`, so it comes back as the same rectangle,
      // except one whose end corner is at column 0: that end comes back at (cols, the row before),
      // a different rectangle.
      const { start, end } = kept.range;
      this.term.select(start.x, start.y, (end.y - start.y) * this.term.cols + end.x - start.x);
    };
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
    this.programCopy = undefined;
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

  /** Sends xterm's answer to a query in the output, naming the point in the output it answers.
   * Dropped for saved output, for output of an earlier connection and for output whose queries were
   * answered already: their programs are not waiting for it any more and would read it as input. */
  private sendReply(data: string): void {
    const parsed = this.parsing[0];
    if (this.savedWritesInFlight > 0 || parsed?.offset === undefined || parsed.generation !== this.generation) return;
    if (this.socket?.readyState !== WebSocket.OPEN) return;
    this.socket.send(JSON.stringify({ type: "reply", data, offset: parsed.offset } satisfies TermControl));
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
