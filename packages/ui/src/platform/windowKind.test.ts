// @vitest-environment jsdom
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { daemonIdentity, remoteDaemonName, windowKind } from "./windowKind";

function openAt(search: string): void {
  window.history.replaceState({}, "", `/${search}`);
}

afterEach(() => {
  openAt("");
});

it("takes the daemon for the local one unless the shell names it remote, whatever its address", () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  openAt("?port=4242&key=aa");
  expect(windowKind()).toBe("local");
  expect(remoteDaemonName()).toBeUndefined();
  expect(daemonIdentity()).toBeUndefined();
  openAt("?port=4242&key=aa&daemon=me%40host%3A22&remote=host%20one");
  expect(windowKind()).toBe("remote");
  expect(remoteDaemonName()).toBe("host one");
  expect(daemonIdentity()).toBe("me@host:22");
});

it("reads no remote daemon outside the shell, where anyone can put text in the query", () => {
  openAt("?port=4242&key=aa&daemon=x&remote=host");
  expect(windowKind()).toBe("local");
  expect(remoteDaemonName()).toBeUndefined();
  expect(daemonIdentity()).toBeUndefined();
});

it("knows the notification-area flyout's page, in the shell only", () => {
  openAt("?flyout=1");
  expect(windowKind()).not.toBe("flyout");
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  expect(windowKind()).toBe("flyout");
  openAt("?flyout=0");
  expect(windowKind()).not.toBe("flyout");
  openAt("?port=4242&key=aa");
  expect(windowKind()).toBe("local");
});

it("keeps what a remote window remembers under no identity when the shell gave none, and a remote name is empty for no remote daemon", () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  openAt("?remote=host");
  expect(remoteDaemonName()).toBe("host");
  expect(daemonIdentity()).toBeUndefined();
  openAt("?remote=&daemon=me%40host");
  expect(remoteDaemonName()).toBeUndefined();
  expect(daemonIdentity()).toBeUndefined();
});
