// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { installFakeTerminal, terminalFixtureUrl } from "./fakeTerminal";

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
