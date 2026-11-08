// @vitest-environment jsdom
import { expect, it } from "vitest";

import { LanguageProvider } from "../i18n/react";
import type { ExitCapability, PlatformAdapter } from "../platform";
import { DEFAULT_SETTINGS } from "../store";
import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const exit: ExitCapability = { register: () => () => {}, heartbeat: async () => {}, setSystemQuitWithoutAsking: async () => {}, confirmQuit: async () => {} };
const SYSTEM_QUIT = "quits it without asking";

function mountResume(platform: PlatformAdapter) {
  const daemon = testDaemon({ state: { settings: { ...DEFAULT_SETTINGS, resume_sessions_on_start: true } } });
  return mountHost(
    <LanguageProvider>
      <DaemonSwitchSetting field="resume_sessions_on_start" />
    </LanguageProvider>,
    { daemon, platform },
  );
}

it("says a logout, restart or shutdown quits without asking only in the macOS desktop app", () => {
  expect(mountResume({ kind: "tauri", host: MAC_HOST, exit }).textContent).toContain(SYSTEM_QUIT);
});

it.each([
  ["the Windows desktop app", { kind: "tauri", host: WINDOWS_HOST, exit }],
  ["a browser, even on a Mac", { kind: "browser", host: { ...BROWSER_HOST, os: "macos" } }],
] as const)("leaves that out in %s, which quits as for any quit", (_name, platform) => {
  const container = mountResume(platform);
  expect(container.textContent).toContain("resumes every interrupted session");
  expect(container.textContent).not.toContain(SYSTEM_QUIT);
});
