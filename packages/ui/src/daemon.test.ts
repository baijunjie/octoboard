// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { daemonWsUrl, resolveDaemonAddress, resolveRecovered } from "./daemon";
import type { PlatformAdapter } from "./platform";

const platform = { kind: "tauri" } as PlatformAdapter;

function openAt(search: string): void {
  window.history.replaceState({}, "", `/${search}`);
}

// The root `.env.local` reaches a test run too (see `vite.config.ts`), so a developer's own constants are blanked.
beforeEach(() => {
  vi.stubEnv("VITE_DAEMON_PORT", "");
  vi.stubEnv("VITE_DAEMON_KEY", "");
});

afterEach(() => {
  vi.unstubAllEnvs();
  openAt("");
});

it.each([
  ["?port=4242&recovered=1", true],
  ["?port=4242", false],
  ["?port=4242&recovered=0", false],
])("reads the shell's recovered marker in %s as %s", (search, expected) => {
  openAt(search);
  expect(resolveRecovered()).toBe(expected);
});

it("takes the port and the key from the page's query", () => {
  openAt("?port=4242&key=abc123");
  expect(resolveDaemonAddress(platform)).toEqual({ origin: "ws://127.0.0.1:4242", key: "abc123" });
});

it("takes the port and the key from the dev-time constants when the query has none", () => {
  vi.stubEnv("VITE_DAEMON_PORT", "4242");
  vi.stubEnv("VITE_DAEMON_KEY", "fromenv");
  expect(resolveDaemonAddress(platform)).toEqual({ origin: "ws://127.0.0.1:4242", key: "fromenv" });
});

it("prefers the key in the query to the constant", () => {
  vi.stubEnv("VITE_DAEMON_KEY", "fromenv");
  openAt("?port=4242&key=fromquery");
  expect(resolveDaemonAddress(platform)?.key).toBe("fromquery");
});

it("has no address when the key is missing, whatever else is known", () => {
  openAt("?port=4242");
  expect(resolveDaemonAddress(platform)).toBeUndefined();
  openAt("?port=4242&key=");
  expect(resolveDaemonAddress(platform)).toBeUndefined();
  vi.stubEnv("VITE_DAEMON_PORT", "4242");
  openAt("");
  expect(resolveDaemonAddress(platform)).toBeUndefined();
});

it("has no address when only the key is known and the page's own origin cannot be the daemon", () => {
  openAt("?key=abc123");
  expect(resolveDaemonAddress(platform)).toBeUndefined();
});

it("puts the percent-encoded key on every WebSocket URL", () => {
  const address = { origin: "ws://127.0.0.1:4242", key: "a b&c" };
  expect(daemonWsUrl(address, "/ws/control")).toBe("ws://127.0.0.1:4242/ws/control?key=a%20b%26c");
  expect(daemonWsUrl(address, "/ws/term/s-1")).toBe("ws://127.0.0.1:4242/ws/term/s-1?key=a%20b%26c");
});
