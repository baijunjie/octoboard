// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { daemonRefusedKey, daemonWsUrl, isKeylessDaemonPage, isLocalDaemon, KEY_CHECK_TIMEOUT_MS, resolveDaemonAddress, resolveRecovered, resolveStartupError } from "./daemon";
import type { PlatformAdapter } from "./platform";
import { BROWSER_HOST } from "./test/hosts";

const platform = { kind: "tauri", host: BROWSER_HOST } as PlatformAdapter;

it.each([
  ["ws://127.0.0.1:4242", true],
  ["ws://localhost:4242", true],
  ["wss://[::1]:4242", true],
  ["ws://192.168.1.2:4242", false],
  ["wss://remote.example", false],
  ["ws://localhost.remote.example", false],
  ["not an address", false],
])("identifies whether %s shares the local filesystem", (origin, local) => {
  expect(isLocalDaemon({ origin, key: "test" })).toBe(local);
});

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

it("reads the shell's startup failure from ?error=, percent-decoded, only under the desktop shell", () => {
  openAt("?error=Another%20instance%20is%20running");
  expect(resolveStartupError(platform)).toBe("Another instance is running");
  openAt("?port=4242");
  expect(resolveStartupError(platform)).toBeUndefined();
});

it("ignores ?error= in a browser, where anyone can put text there", () => {
  openAt("?error=Daemon%20failed");
  expect(resolveStartupError({ kind: "browser", host: BROWSER_HOST } as PlatformAdapter)).toBeUndefined();
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

const browser = { kind: "browser", host: BROWSER_HOST } as PlatformAdapter;

it("marks the address of the page's own origin as served by the daemon, and no other", () => {
  vi.stubEnv("DEV", false);
  openAt("?key=abc123");
  expect(resolveDaemonAddress(browser)).toEqual({ origin: `ws://${window.location.host}`, key: "abc123", servedByDaemon: true });
  openAt("?port=4242&key=abc123");
  expect(resolveDaemonAddress(browser)).toEqual({ origin: "ws://127.0.0.1:4242", key: "abc123" });
});

it("does not mark the address of a dev server's proxy as served by the daemon", () => {
  vi.stubEnv("DEV", true);
  vi.stubGlobal("__OCTOBOARD_DEV_PROXY__", true);
  onTestFinished(() => void vi.unstubAllGlobals());
  openAt("?key=abc123");
  expect(resolveDaemonAddress(browser)).toEqual({ origin: `ws://${window.location.host}`, key: "abc123" });
});

it("takes only the production browser build opened with no key and no port as a keyless daemon page", () => {
  vi.stubEnv("DEV", false);
  expect(isKeylessDaemonPage(browser)).toBe(true);
  openAt("?key=abc123");
  expect(isKeylessDaemonPage(browser)).toBe(false);
  openAt("?port=4242");
  expect(isKeylessDaemonPage(browser)).toBe(false);
  openAt("");
  expect(isKeylessDaemonPage(platform)).toBe(false);
  vi.stubEnv("DEV", true);
  expect(isKeylessDaemonPage(browser)).toBe(false);
});

it.each([
  [401, true],
  [204, false],
  [404, false],
  [403, false],
  [500, false],
])("takes a key check answered %i as refused: %s", async (status, refused) => {
  const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status })));
  vi.stubGlobal("fetch", fetchMock);
  onTestFinished(() => void vi.unstubAllGlobals());
  expect(await daemonRefusedKey({ origin: "ws://localhost", key: "a b" })).toBe(refused);
  expect(fetchMock).toHaveBeenCalledWith("/key-check?key=a%20b", expect.objectContaining({ cache: "no-store" }));
});

it("takes a key check that cannot reach the daemon as no refusal", async () => {
  vi.stubGlobal("fetch", () => Promise.reject(new TypeError("Failed to fetch")));
  onTestFinished(() => void vi.unstubAllGlobals());
  expect(await daemonRefusedKey({ origin: "ws://localhost", key: "k" })).toBe(false);
});

it("gives the key check its own time limit and takes an aborted check as no refusal", async () => {
  const limit = new AbortController();
  const timeout = vi.spyOn(AbortSignal, "timeout").mockReturnValue(limit.signal);
  const fetchMock = vi.fn(
    (_url: string, init: RequestInit) =>
      new Promise<Response>((_resolve, reject) => init.signal!.addEventListener("abort", () => reject(new DOMException("aborted", "TimeoutError")))),
  );
  vi.stubGlobal("fetch", fetchMock);
  onTestFinished(() => {
    vi.unstubAllGlobals();
    timeout.mockRestore();
  });
  const refused = daemonRefusedKey({ origin: "ws://localhost", key: "k" });
  expect(timeout).toHaveBeenCalledWith(KEY_CHECK_TIMEOUT_MS);
  limit.abort();
  expect(await refused).toBe(false);
});

it("takes a page given a key or a port by the build's constants as not keyless", () => {
  vi.stubEnv("DEV", false);
  vi.stubEnv("VITE_DAEMON_KEY", "from-env");
  expect(isKeylessDaemonPage(browser)).toBe(false);
  vi.stubEnv("VITE_DAEMON_KEY", "");
  vi.stubEnv("VITE_DAEMON_PORT", "4242");
  expect(isKeylessDaemonPage(browser)).toBe(false);
});
