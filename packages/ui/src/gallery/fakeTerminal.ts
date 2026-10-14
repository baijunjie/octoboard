/**
 * The terminal sockets of the gallery. A session's terminal URL points at `FIXTURE_ORIGIN`, which
 * `installFakeTerminal` answers in the page itself, so there is no daemon and no failed network
 * request to read off the console: `output` connects and draws a sample (for a dormant session, as
 * the output its last process left), `refuse` closes at once without ever opening, which is a live
 * session whose terminal cannot be reached.
 */

const FIXTURE_ORIGIN = "ws://gallery.invalid/";

export type TerminalBehaviour = "output" | "refuse";

export function terminalFixtureUrl(behaviour: TerminalBehaviour, session: string): string {
  return `${FIXTURE_ORIGIN}${behaviour}/${session}`;
}

const ESC = "\x1b[";

/** The box's inner width in cells. The docked terminal pane keeps 520px, about 53 columns once its
 * padding is taken off (`docs/product/window-layout.md`, "Pane widths at 1148 px and wider"), so
 * the box (this plus its two bars) is 48 and does not wrap there. That makes only the box safe,
 * and only while docked: below the breakpoint the floor is about 40 columns, and the sample's
 * plain lines already run to 53. */
const BOX_INNER = 46;

/** The cells a string takes: a CJK character is two, everything else one. Enough for this sample
 * only: it strips SGR colour codes and no other escape, and counts BMP CJK alone (a wide ideograph
 * from U+20000 up counts as one), with no combining marks or emoji. */
function cells(text: string): number {
  // eslint-disable-next-line no-control-regex -- ANSI SGR sequences begin with the escape control character.
  return [...text.replace(/\x1b\[[0-9;]*m/g, "")].reduce(
    (sum, ch) => sum + (/[\u1100-\u115f\u2e80-\ua4cf\uac00-\ud7a3\uf900-\ufaff\ufe30-\ufe6f\uff00-\uff60\uffe0-\uffe6]/.test(ch) ? 2 : 1),
    0,
  );
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

const SAMPLE = [
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
  `${ESC}2m  A long line that runs past the terminal's width to show how it wraps: ${"lorem ipsum dolor sit amet ".repeat(5)}${ESC}0m`,
  "",
  `${ESC}35m✻ Pondering…${ESC}0m ${ESC}2m(12s · esc to interrupt)${ESC}0m`,
  "",
].join("\r\n");

class FakeTerminalSocket extends EventTarget {
  binaryType: BinaryType = "blob";
  readyState = 0;

  constructor(readonly url: string, output = SAMPLE) {
    super();
    const behaviour = url.slice(FIXTURE_ORIGIN.length).split("/")[0];
    // A refusal takes a moment, as a real one does: closing in the same frame as the attempt would
    // batch the "connecting" and the failure into one render and the pane would never see either.
    setTimeout(() => {
      if (this.readyState === 3) return;
      if (behaviour === "refuse") return this.close();
      this.readyState = 1;
      this.dispatchEvent(new Event("open"));
      const bytes = new TextEncoder().encode(output);
      setTimeout(() => this.dispatchEvent(new MessageEvent("message", { data: bytes.buffer })), 20);
    }, behaviour === "refuse" ? 100 : 0);
  }

  send(): void {}

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
        return new FakeTerminalSocket(url, options.output?.(session));
      }
      if (options.strict) throw new Error("The website preview cannot open network sockets");
      return Reflect.construct(target, args);
    },
  });
}
