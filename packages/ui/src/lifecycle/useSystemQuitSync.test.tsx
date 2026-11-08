// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import type { PlatformAdapter } from "../platform";
import { DEFAULT_SETTINGS } from "../store";
import { BROWSER_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useSystemQuitSync } from "./useSystemQuitSync";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Sync() {
  useSystemQuitSync();
  return null;
}

it("hands the shell the daemon's choice at startup and on every change", async () => {
  const set = vi.fn(() => Promise.resolve());
  const daemon = testDaemon();
  const platform: PlatformAdapter = { kind: "tauri", host: BROWSER_HOST, exit: { register: () => () => {}, heartbeat: async () => {}, confirmQuit: async () => {}, setSystemQuitWithoutAsking: set } };
  mountHost(<Sync />, { platform, daemon });
  await act(async () => {});
  expect(set).toHaveBeenLastCalledWith(false);
  await act(async () => {
    daemon.store.setState({ settings: { ...DEFAULT_SETTINGS, resume_sessions_on_start: true } });
  });
  expect(set).toHaveBeenLastCalledWith(true);
});
