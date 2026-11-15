// @vitest-environment jsdom
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { connectPageEntry, daemonIdentity, remoteDaemonName, windowKind } from "./windowKind";

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

it("knows the connect page and the entry it was opened for, in the shell only", () => {
  openAt("?connect=1&entry=staging");
  expect(windowKind()).not.toBe("connect");
  expect(connectPageEntry()).toBeUndefined();
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  expect(windowKind()).toBe("connect");
  expect(connectPageEntry()).toBe("staging");
  openAt("?connect=1");
  expect(windowKind()).toBe("connect");
  expect(connectPageEntry()).toBeUndefined();
  openAt("?port=4242&key=aa");
  expect(windowKind()).not.toBe("connect");
});

it("knows the notification-area flyout's page, in the shell only", () => {
  openAt("?flyout=1");
  expect(windowKind()).not.toBe("flyout");
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  expect(windowKind()).toBe("flyout");
  openAt("?flyout=0");
  expect(windowKind()).not.toBe("flyout");
  openAt("?connect=1");
  expect(windowKind()).not.toBe("flyout");
  expect(windowKind()).toBe("connect");
});

it("has no connect page entry on a page that is not the connect page, or one that names none", () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  openAt("?entry=staging");
  expect(connectPageEntry()).toBeUndefined();
  openAt("?connect=1&entry=");
  expect(connectPageEntry()).toBeUndefined();
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
