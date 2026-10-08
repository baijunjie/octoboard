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
const SAMPLE = [
  `${ESC}1;38;5;208m ✻ Welcome to Claude Code${ESC}0m`,
  "",
  `${ESC}2m  cwd: /Users/dev/code/website${ESC}0m`,
  "",
  `${ESC}36m╭──────────────────────────────────────────────────────────────╮${ESC}0m`,
  `${ESC}36m│${ESC}0m ${ESC}1m>${ESC}0m Fix the summary layout on narrow screens                    ${ESC}36m│${ESC}0m`,
  `${ESC}36m╰──────────────────────────────────────────────────────────────╯${ESC}0m`,
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

  constructor(readonly url: string) {
    super();
    const behaviour = url.slice(FIXTURE_ORIGIN.length).split("/")[0];
    // A refusal takes a moment, as a real one does: closing in the same frame as the attempt would
    // batch the "connecting" and the failure into one render and the pane would never see either.
    setTimeout(() => {
      if (this.readyState === 3) return;
      if (behaviour === "refuse") return this.close();
      this.readyState = 1;
      this.dispatchEvent(new Event("open"));
      const bytes = new TextEncoder().encode(SAMPLE);
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

/** Makes `new WebSocket(url)` for a fixture URL a fake one, and leaves every other URL to the real
 * constructor. */
export function installFakeTerminal(): void {
  const RealWebSocket = window.WebSocket;
  window.WebSocket = new Proxy(RealWebSocket, {
    construct(target, args: [string | URL, (string | string[])?]) {
      if (String(args[0]).startsWith(FIXTURE_ORIGIN)) return new FakeTerminalSocket(String(args[0]));
      return Reflect.construct(target, args);
    },
  });
}
