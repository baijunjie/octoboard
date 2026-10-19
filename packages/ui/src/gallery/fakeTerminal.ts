/**
 * The terminal sockets of the gallery. A session's terminal URL points at `FIXTURE_ORIGIN`, which
 * `installFakeTerminal` answers in the page itself, so there is no daemon and no failed network
 * request to read off the console: `output` connects and draws a sample (for a dormant session, as
 * the output its last process left), `fullscreen` draws in the alternate screen, `paths` draws
 * output that names files and folders, and `refuse` closes at once without ever opening, which is a live session whose terminal cannot be reached.
 */

const FIXTURE_ORIGIN = "ws://gallery.invalid/";

export type TerminalBehaviour = "output" | "fullscreen" | "paths" | "refuse";

export function terminalFixtureUrl(behaviour: TerminalBehaviour, session: string): string {
  return `${FIXTURE_ORIGIN}${behaviour}/${session}`;
}

const ESC = "\x1b[";
/** Opens (with a URI after it) or closes an OSC 8 hyperlink; each ends with the string terminator `ESC \`. */
const OSC8 = "\x1b]8;;";

/** The box's inner width in cells. The docked terminal pane keeps 520px, about 53 columns once its
 * padding is taken off (`docs/product/window-layout.md`, "Pane widths at 1168 px and wider"), so
 * the box (this plus its two bars) is 48 and does not wrap there. That makes only the box safe,
 * and only while docked: below the breakpoint the floor is about 40 columns, and the sample's
 * plain lines already run to 53. */
const BOX_INNER = 46;

/** Whether a character takes two cells (the same BMP CJK ranges `cells` counts). */
export function isWide(ch: string): boolean {
  return /[\u1100-\u115f\u2e80-\ua4cf\uac00-\ud7a3\uf900-\ufaff\ufe30-\ufe6f\uff00-\uff60\uffe0-\uffe6]/.test(ch);
}

/** The cells a string takes: a CJK character is two, everything else one. Enough for this sample
 * only: it strips SGR colour codes and no other escape, and counts BMP CJK alone (a wide ideograph
 * from U+20000 up counts as one), with no combining marks or emoji. */
function cells(text: string): number {
  // eslint-disable-next-line no-control-regex -- ANSI SGR sequences begin with the escape control character.
  return [...text.replace(/\x1b\[[0-9;]*m/g, "")].reduce((sum, ch) => sum + (isWide(ch) ? 2 : 1), 0);
}

/** A box around `lines`, each padded to `BOX_INNER` by what it measures, so a line cannot drift off
 * the right bar when its text changes. */
function box(lines: string[]): string[] {
  const bar = `${ESC}36m│${ESC}0m`;
  return [
    `${ESC}36m╭${"─".repeat(BOX_INNER)}╮${ESC}0m`,
    ...lines.map((line) => `${bar}${line}${" ".repeat(BOX_INNER - cells(line))}${bar}`),
    `${ESC}36m╰${"─".repeat(BOX_INNER)}╯${ESC}0m`,
  ];
}

/** The columns assumed until the terminal reports its own, as a terminal of the classic width. */
const DEFAULT_COLS = 80;

/** A program-broken address as Claude Code writes it: its first part fills the row to the very last
 * column (`cols`), a hard line break follows, and each next row continues after a two-cell hanging
 * indent. Only the program's own layout can do that, so the sample is laid out against the width
 * the terminal reported. A filler query parameter moves the break so that `lang=zh-Hans` always
 * lands whole on the last row, where a scenario can point at it. */
function brokenUrlLines(cols: number): string[] {
  const prefix = "  - 展开状态： ";
  const head = "http://localhost:5175/gallery-frame.html?scenario=terminal-broken-url-hover&trace=";
  const tail = "&lang=zh-Hans";
  const indent = "  ";
  const first = Math.max(cols - cells(prefix), 1);
  const rowWidth = Math.max(cols - indent.length, tail.length);
  let total = first + tail.length;
  while (total < head.length + tail.length + 8) total += rowWidth;
  const filler = "0123456789abcdef".repeat(Math.ceil(total / 16)).slice(0, total - head.length - tail.length);
  const url = head + filler + tail;
  const lines = [`${ESC}2m${prefix}${ESC}0m${url.slice(0, first)}`];
  for (let at = first; at < url.length; at += rowWidth) lines.push(indent + url.slice(at, at + rowWidth));
  return lines;
}

const sample = (cols: number) => [
  `${ESC}1;38;5;208m ✻ Welcome to Claude Code${ESC}0m`,
  "",
  `${ESC}2m  cwd: /Users/dev/code/website${ESC}0m`,
  "",
  // A follow-up instruction beside the task in the prompt box.
  ...box([
    ` ${ESC}1m>${ESC}0m Fix the summary layout on narrow screens`,
    "   Confirm the layout on narrow screens again",
  ]),
  "",
  `${ESC}32m●${ESC}0m I will start by reading the summary styles.`,
  // An OSC 8 hyperlink whose visible text is not its target, which the terminal's link tooltip tells.
  `  ${ESC}2mSee the${ESC}0m ${OSC8}https://example.com/docs/summary-layout?section=breakpoints\x1b\\layout notes${OSC8}\x1b\\ ${ESC}2mfor the breakpoints.${ESC}0m`,
  "",
  `${ESC}34m● Read${ESC}0m(src/summary/Summary.css)`,
  `  ${ESC}2m⎿  Read 142 lines${ESC}0m`,
  "",
  `${ESC}33m● Update${ESC}0m(src/summary/Summary.css)`,
  `  ${ESC}2m⎿  Updated${ESC}0m with ${ESC}32m3 additions${ESC}0m and ${ESC}31m1 removal${ESC}0m`,
  `     ${ESC}31m- .summary { width: 480px; }${ESC}0m`,
  `     ${ESC}32m+ .summary { width: 100%; max-width: 480px; }${ESC}0m`,
  "",
  `${ESC}32m●${ESC}0m The summary no longer overflows on a narrow screen.`,
  `${ESC}2m  A long line that runs past the terminal's width to show how it wraps: ${"sample output wraps here. ".repeat(5)}${ESC}0m`,
  "",
  // An address the program itself breaks onto the next row, which the terminal joins into one link.
  ...brokenUrlLines(cols),
  "",
  `${ESC}35m✻ Pondering…${ESC}0m ${ESC}2m(12s · esc to interrupt)${ESC}0m`,
  "",
].join("\r\n");

/** Output that names files: each kind of path a terminal turns into a link once the daemon finds it
 * (absolute, relative with a line and column, a document with a line, one in a side worktree, a
 * program the system could launch, in a temp folder, inside parentheses, after an `@` mention), the
 * same shapes the daemon does not find (a path that does not exist, one that more than one of the
 * project's folders holds, and a route whose folder name is in brackets, which is not looked up),
 * a sentence's full stop right after a path, and an address beside them for the link colour to be
 * compared with. The scenario using it says which of these exist. Each line fits the docked pane's
 * 53 columns. */
const pathsSample = () => [
  `${ESC}1;38;5;208m ✻ Welcome to Claude Code${ESC}0m`,
  "",
  `${ESC}1m● Read${ESC}0m(src/summary/Summary.css)`,
  `  ${ESC}2m⎿  Read 142 lines${ESC}0m`,
  "",
  `${ESC}32m●${ESC}0m The layout lives in`,
  "  /Users/dev/code/website/src/summary/Summary.css",
  "  and the type error is at src/summary/Summary.tsx:42:7.",
  "",
  `${ESC}32m●${ESC}0m The icon sheet is written to`,
  "  /private/tmp/octoboard/scratchpad/icon/sheet3.png,",
  "  and the next step goes into src/summary/Draft.ts.",
  "",
  `${ESC}32m●${ESC}0m The notes are in docs/summary-notes.md:13,`,
  "  and the side worktree holds",
  "  /Users/dev/code/search-api-experiments/src/ui.ts:7",
  "",
  `${ESC}32m●${ESC}0m The helper is built at bin/helper.`,
  "",
  `${ESC}32m●${ESC}0m Context was added from @src/summary/Summary.tsx;`,
  "  the route app/[id]/page.tsx and src/shared/index.ts",
  "  are left alone.",
  "",
  `${ESC}32m●${ESC}0m Preview it at`,
  "  http://localhost:5176/gallery.html",
  "",
].join("\r\n");

/** A settled full-screen response, rather than the scrolling output used by the other fixtures. */
const fullscreenSample = () => `${ESC}?1049h${ESC}H${ESC}2J` + [
  `${ESC}1;38;5;208m ✻ Claude Code${ESC}0m`,
  "",
  "  The preview is ready. Open either link below.",
  "",
  `${ESC}2m  Preview: http://localhost:5176/gallery.html${ESC}0m`,
  `${ESC}2m  Notes: ${OSC8}https://example.com/docs/summary-layout?section=breakpoints\x1b\\layout notes${OSC8}\x1b\\ for the breakpoints.${ESC}0m`,
  "",
  "  Both links remain visible without hovering.",
  `${ESC}2m  The surrounding text keeps the agent's muted style.${ESC}0m`,
  "",
  `${ESC}2m  ────────────────────────────────────────────${ESC}0m`,
  "  > ",
].join("\r\n");

class FakeTerminalSocket extends EventTarget {
  binaryType: BinaryType = "blob";
  readyState = 0;

  /** The width the terminal last reported in a resize message. */
  private cols = DEFAULT_COLS;
  /** The width the output on screen was laid out for, once it has been written. */
  private drawnCols?: number;

  /** Whether the output depends on the width, which only the scrolling sample does. Other text is
   * written once and left to the terminal's own resize behavior. */
  private readonly sized: boolean;
  private readonly output: (cols: number) => string;

  constructor(readonly url: string, output?: () => string) {
    super();
    const behaviour = url.slice(FIXTURE_ORIGIN.length).split("/")[0];
    this.sized = output === undefined && behaviour !== "fullscreen" && behaviour !== "paths";
    this.output = output ?? (behaviour === "fullscreen" ? fullscreenSample : behaviour === "paths" ? pathsSample : sample);
    // A refusal takes a moment, as a real one does: closing in the same frame as the attempt would
    // batch the "connecting" and the failure into one render and the pane would never see either.
    setTimeout(() => {
      if (this.readyState === 3) return;
      if (behaviour === "refuse") return this.close();
      this.readyState = 1;
      this.dispatchEvent(new Event("open"));
      // The terminal reports its size as it opens, before this timer fires; a later resize
      // (the terminal settles on its real size a moment after opening) redraws in `send`.
      setTimeout(() => this.draw(""), 20);
    }, behaviour === "refuse" ? 100 : 0);
  }

  /** Writes the output laid out for the current width, after `clear`. */
  private draw(clear: string): void {
    this.drawnCols = this.cols;
    const bytes = new TextEncoder().encode(clear + this.output(this.cols));
    this.dispatchEvent(new MessageEvent("message", { data: bytes.buffer }));
  }

  send(data: unknown): void {
    if (typeof data !== "string") return;
    let message: unknown;
    try {
      message = JSON.parse(data);
    } catch {
      return; // Keyboard input, not a resize message.
    }
    if (typeof message !== "object" || !message || !("cols" in message) || typeof message.cols !== "number") return;
    this.cols = message.cols;
    // A program redraws on SIGWINCH, which is what keeps its own line breaks right at the new
    // width: home, clear the screen and the scrollback, and write it again.
    if (this.sized && this.drawnCols !== undefined && this.drawnCols !== this.cols && this.readyState === 1) this.draw(`${ESC}H${ESC}2J${ESC}3J`);
  }

  close(): void {
    if (this.readyState === 3) return;
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }
}

export interface FakeTerminalOptions {
  /** Alternate sample text, keyed by session, for consumers such as the public website preview. */
  output?: (session: string) => string;
  /** Refuse all other sockets rather than falling back to a real network connection. */
  strict?: boolean;
}

/** Makes fixture sockets local. The gallery keeps real sockets for Vite; a static preview can
 * explicitly refuse every non-fixture URL. */
export function installFakeTerminal(options: FakeTerminalOptions = {}): void {
  const RealWebSocket = window.WebSocket;
  window.WebSocket = new Proxy(RealWebSocket, {
    construct(target, args: [string | URL, (string | string[])?]) {
      const url = String(args[0]);
      if (url.startsWith(FIXTURE_ORIGIN)) {
        const session = url.slice(FIXTURE_ORIGIN.length).split("/").slice(1).join("/");
        const { output } = options;
        return new FakeTerminalSocket(url, output && (() => output(session)));
      }
      if (options.strict) throw new Error("The website preview cannot open network sockets");
      return Reflect.construct(target, args);
    },
  });
}
