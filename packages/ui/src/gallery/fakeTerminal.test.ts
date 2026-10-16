// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { brokenUrlsAt, type TerminalRow } from "../terminal/terminalLinks";
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

/** The sample's rows as `brokenUrlsAt` reads them, after `clear` and the colour codes are stripped. */
function rowsOf(output: string, clear = ""): { lines: string[]; rows: TerminalRow[] } {
  // eslint-disable-next-line no-control-regex -- strips the sample's ANSI SGR colour codes.
  const lines = output.slice(clear.length).replace(/\x1b\[[0-9;]*m/g, "").split("\r\n");
  const rows = lines.map((line) => ({ cells: [...line].flatMap((ch) => (isWide(ch) ? [ch, ""] : [ch])), wrapped: false }));
  return { lines, rows };
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
