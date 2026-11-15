// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import type { PlatformAdapter, RemoteDaemonsCapability } from "../platform";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
import { remoteDaemonsOffered } from "./remoteDaemonsOffered";

const capability = {} as RemoteDaemonsCapability;
const DESKTOP: PlatformAdapter = { kind: "tauri", host: MAC_HOST, remoteDaemons: capability };

afterEach(() => {
  window.history.replaceState({}, "", "/");
  vi.unstubAllGlobals();
});

it("is offered in the application's own window, which has the capability", () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  expect(remoteDaemonsOffered(DESKTOP)).toBe(true);
});

it("is not offered in a browser, whose adapter has none of the shell's capabilities", () => {
  expect(remoteDaemonsOffered({ kind: "browser", host: BROWSER_HOST })).toBe(false);
});

it("is not offered in the window of a remote daemon, which has the capability too", () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  window.history.replaceState({}, "", "/?remote=server&daemon=me%40server");
  expect(remoteDaemonsOffered(DESKTOP)).toBe(false);
});

it("is not offered on the connect page, which is no daemon's window", () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  window.history.replaceState({}, "", "/?connect=1");
  expect(remoteDaemonsOffered(DESKTOP)).toBe(false);
});
