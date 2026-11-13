// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import { BROWSER_HOST } from "../test/hosts";
import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import { useServedByDaemon } from "./useServedByDaemon";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  vi.unstubAllEnvs();
  window.history.replaceState({}, "", "/");
});

function Probe(): React.ReactElement {
  return <p>{String(useServedByDaemon())}</p>;
}

function shown(platform: PlatformAdapter | undefined): string | null {
  const container = document.createElement("div");
  const root = createRoot(container);
  act(() => root.render(platform ? <PlatformProvider value={platform}><Probe /></PlatformProvider> : <Probe />));
  const text = container.textContent;
  act(() => root.unmount());
  return text;
}

const browser = { kind: "browser", host: BROWSER_HOST } as PlatformAdapter;
const tauri = { kind: "tauri", host: BROWSER_HOST } as PlatformAdapter;

it("is true only for the production browser build opened with the key of the daemon that served it", () => {
  vi.stubEnv("DEV", false);
  window.history.replaceState({}, "", "/?key=abc123");
  expect(shown(browser)).toBe("true");
  expect(shown(tauri)).toBe("false");
  expect(shown(undefined)).toBe("false");
  window.history.replaceState({}, "", "/?port=4242&key=abc123");
  expect(shown(browser)).toBe("false");
});

it("is false on a dev server", () => {
  vi.stubEnv("DEV", true);
  vi.stubGlobal("__OCTOBOARD_DEV_PROXY__", true);
  window.history.replaceState({}, "", "/?key=abc123");
  expect(shown(browser)).toBe("false");
  vi.unstubAllGlobals();
});
