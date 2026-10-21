// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { brokenUrlsAt, linkSegmentsIn, type TerminalRow } from "../terminal/terminalLinks";
import { installFakeTerminal, isWide, terminalFixtureUrl } from "./fakeTerminal";

const original = window.WebSocket;
afterEach(() => { window.WebSocket = original; vi.useRealTimers(); });

it("renders supplied session output without creating a real socket", async () => {
  vi.useFakeTimers();
  const real = vi.fn();
  window.WebSocket = real as unknown as typeof WebSocket;
  installFakeTerminal({ strict: true, output: (session) => `sample:${session}` });
  const socket = new WebSocket(terminalFixtureUrl("output", "demo-session"));
  const messages: string[] = [];
  socket.addEventListener("message", (event) => messages.push(new TextDecoder().decode(event.data)));
  await vi.runAllTimersAsync();
  expect(messages).toEqual(["sample:demo-session"]);
  expect(real).not.toHaveBeenCalled();
  socket.close();
});

it("rejects every non-fixture socket in strict mode", () => {
  installFakeTerminal({ strict: true });
  expect(() => new WebSocket("ws://127.0.0.1:1234/ws")).toThrow("cannot open network sockets");
  expect(() => new WebSocket("wss://example.com")).toThrow("cannot open network sockets");
});

it("preserves the gallery's default terminal output", async () => {
  vi.useFakeTimers();
  installFakeTerminal();
  const socket = new WebSocket(terminalFixtureUrl("output", "gallery-session"));
  let output = "";
  socket.addEventListener("message", (event) => { output = new TextDecoder().decode(event.data); });
  await vi.runAllTimersAsync();
  expect(output).toContain("Welcome to Claude Code");
  expect(output).toContain("Fix the summary layout");
  socket.close();
});

/** The sample's rows as `brokenUrlsAt` reads them, after `clear` and the colour codes are stripped.
 * `cols` pads each row out to the terminal's width, which is what a real buffer does with the cells
 * after a piece that stops short. */
function rowsOf(output: string, clear = "", cols?: number): { lines: string[]; rows: TerminalRow[] } {
  // eslint-disable-next-line no-control-regex -- strips the sample's ANSI SGR colour codes.
  const lines = output.slice(clear.length).replace(/\x1b\[[0-9;]*m/g, "").split("\r\n");
  const rows = lines.map((line) => {
    const cells = [...line].flatMap((ch) => (isWide(ch) ? [ch, ""] : [ch]));
    if (cols !== undefined && cells.length < cols) cells.push(...Array<string>(cols - cells.length).fill(" "));
    return { cells, wrapped: false as const };
  });
  return { lines, rows };
}

/** The column after the row's last text. A blank cell, and the empty half of a wide character, is not text. */
function textEnd(cells: string[]): number {
  let end = cells.length;
  while (end > 0 && (cells[end - 1] === " " || cells[end - 1] === "")) end--;
  return end;
}

const CLEAR = "\x1b[H\x1b[2J\x1b[3J";

it("lays the program-broken address out against the reported width", async () => {
  const cols = 56;
  vi.useFakeTimers();
  installFakeTerminal();
  const socket = new WebSocket(terminalFixtureUrl("output", "gallery-session"));
  let output = "";
  socket.addEventListener("open", () => socket.send(JSON.stringify({ type: "resize", cols, rows: 24 })));
  socket.addEventListener("message", (event) => { output = new TextDecoder().decode(event.data); });
  await vi.runAllTimersAsync();
  const { lines, rows } = rowsOf(output);
  const first = lines.findIndex((line) => line.includes("展开状态"));
  expect(rows[first].cells).toHaveLength(cols);
  const [url] = brokenUrlsAt((y) => rows[y - 1], first + 1);
  expect(url.range.start.y).toBe(first + 1);
  expect(url.url).toMatch(/&lang=zh-Hans$/);
  expect(url.url.split("trace=")[0]).toBe("http://localhost:5175/gallery-frame.html?scenario=terminal-broken-url-hover&");
  socket.close();
});

it("joins an address the program broke short of the last column", async () => {
  const tail = "account&theme=light&lang=zh-Hans";
  const whole = `http://localhost:5219/gallery-frame.html?scenario=dialog-new-session-auto-select-${tail}`;
  vi.useFakeTimers();
  for (const cols of [40, 56, 72, 100]) {
    installFakeTerminal();
    const socket = new WebSocket(terminalFixtureUrl("output", "gallery-session"));
    let output = "";
    socket.addEventListener("open", () => socket.send(JSON.stringify({ type: "resize", cols, rows: 24 })));
    socket.addEventListener("message", (event) => { output = new TextDecoder().decode(event.data); });
    await vi.runAllTimersAsync();
    socket.close();
    const { lines, rows } = rowsOf(output, "", cols);
    const first = lines.findIndex((line) => line.startsWith("http://localhost:5219/"));
    const tailRow = lines.findIndex((line) => line.includes("account&theme=light"));
    expect(first).toBeGreaterThanOrEqual(0);
    expect(tailRow).toBeGreaterThan(first);
    // Blank cells stay after the first piece, and no piece runs past the row.
    expect(textEnd(rows[first].cells)).toBeLessThanOrEqual(cols - 12);
    expect(textEnd(rows[first].cells)).toBeGreaterThan("http://".length);
    for (let y = first; y <= tailRow; y++) expect(textEnd(rows[y].cells)).toBeLessThanOrEqual(cols);
    const [url] = brokenUrlsAt((y) => rows[y - 1], first + 1);
    expect(url.url).toBe(whole);
    expect(url.range.start.y).toBe(first + 1);
    expect(url.range.end.y).toBe(tailRow + 1);
    const segments = linkSegmentsIn((y) => rows[y - 1], first + 1, tailRow + 1);
    const top = segments.find((segment) => segment.y === first + 1);
    const marked = segments.find((segment) => segment.y === tailRow + 1);
    expect(top).toEqual({ y: first + 1, col: 0, width: textEnd(rows[first].cells) });
    expect(marked && marked.col + marked.width).toBe(textEnd(rows[tailRow].cells));
    expect(marked && marked.col + marked.width).toBeLessThan(cols);
    // The address that fills its row stays its own link, and is the earlier `lang=zh-Hans`.
    const filled = lines.findIndex((line) => line.includes("展开状态"));
    const [filledUrl] = brokenUrlsAt((y) => rows[y - 1], filled + 1);
    expect(filledUrl.url.endsWith("&lang=zh-Hans")).toBe(true);
    expect(filledUrl.range.end.y).toBeLessThan(first + 1);
    expect(lines.findIndex((line) => line.includes("lang=zh-Hans"))).toBeLessThan(tailRow);
  }
});

it("redraws at the new width when the terminal resizes after the output is on screen", async () => {
  vi.useFakeTimers();
  installFakeTerminal();
  const socket = new WebSocket(terminalFixtureUrl("output", "gallery-session"));
  const messages: string[] = [];
  socket.addEventListener("open", () => socket.send(JSON.stringify({ type: "resize", cols: 56, rows: 24 })));
  socket.addEventListener("message", (event) => messages.push(new TextDecoder().decode(event.data)));
  await vi.runAllTimersAsync();
  socket.send(JSON.stringify({ type: "resize", cols: 40, rows: 24 }));
  socket.send(JSON.stringify({ type: "resize", cols: 40, rows: 30 }));
  expect(messages).toHaveLength(2);
  expect(messages[1].startsWith(CLEAR)).toBe(true);
  const { lines, rows } = rowsOf(messages[1], CLEAR);
  expect(rows[lines.findIndex((line) => line.includes("展开状态"))].cells).toHaveLength(40);
  socket.close();
});

it("writes supplied output once and leaves a resize to the terminal's own reflow", async () => {
  vi.useFakeTimers();
  installFakeTerminal({ output: () => "supplied" });
  const socket = new WebSocket(terminalFixtureUrl("output", "demo-session"));
  const messages: string[] = [];
  socket.addEventListener("open", () => socket.send(JSON.stringify({ type: "resize", cols: 56, rows: 24 })));
  socket.addEventListener("message", (event) => messages.push(new TextDecoder().decode(event.data)));
  await vi.runAllTimersAsync();
  socket.send(JSON.stringify({ type: "resize", cols: 40, rows: 24 }));
  expect(messages).toEqual(["supplied"]);
  socket.close();
});

it("draws an exited shell's last screen and then closes", async () => {
  vi.useFakeTimers();
  installFakeTerminal();
  const socket = new WebSocket(terminalFixtureUrl("exited", "shell-1"));
  let output = "";
  let closed = false;
  socket.addEventListener("message", (event) => { output = new TextDecoder().decode(event.data); });
  socket.addEventListener("close", () => { closed = true; });
  await vi.runAllTimersAsync();
  expect(output).toContain("built in 1.2s");
  expect(closed).toBe(true);
});

it("draws a shell at its prompt and leaves the socket open", async () => {
  vi.useFakeTimers();
  installFakeTerminal();
  const socket = new WebSocket(terminalFixtureUrl("shell", "shell-1"));
  let output = "";
  let closed = false;
  socket.addEventListener("message", (event) => { output = new TextDecoder().decode(event.data); });
  socket.addEventListener("close", () => { closed = true; });
  await vi.runAllTimersAsync();
  expect(output).toContain("git status");
  expect(closed).toBe(false);
  socket.close();
});

it("leaves a session the supplied output declines to the sample of its URL's behaviour", async () => {
  vi.useFakeTimers();
  installFakeTerminal({ output: (session) => (session === "known" ? "supplied" : undefined) });
  const drawn = async (behaviour: "shell" | "output", session: string) => {
    const socket = new WebSocket(terminalFixtureUrl(behaviour, session));
    let output = "";
    socket.addEventListener("message", (event) => { output = new TextDecoder().decode(event.data); });
    await vi.runAllTimersAsync();
    socket.close();
    return output;
  };
  expect(await drawn("output", "known")).toBe("supplied");
  expect(await drawn("shell", "other")).toContain("git status");
});
