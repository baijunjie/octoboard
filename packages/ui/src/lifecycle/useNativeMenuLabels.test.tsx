// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { setLanguage } from "../i18n/language";
import { LanguageProvider } from "../i18n/react";
import type { PlatformAdapter } from "../platform";
import { flushPromises } from "../test/clock";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { useNativeMenuLabels } from "./useNativeMenuLabels";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const setMenuLabels = vi.fn(async (_labels: Record<string, string>, _accelerator?: string) => {});
const setJumpListLabels = vi.fn(async (_labels: Record<string, string>) => {});
beforeEach(() => {
  setMenuLabels.mockClear();
  setJumpListLabels.mockClear();
  setLanguage("en");
});

function Host() {
  useNativeMenuLabels();
  return null;
}

const MAC: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  appMenu: { onSettingsRequested: () => () => {}, setLabels: (labels, accelerator) => setMenuLabels(labels, accelerator) },
};
const WINDOWS: PlatformAdapter = {
  kind: "tauri",
  host: WINDOWS_HOST,
  jumpList: { setLabels: (labels) => setJumpListLabels(labels) },
};

async function mount(platform: PlatformAdapter) {
  await act(async () => void mountHost(<LanguageProvider><Host /></LanguageProvider>, { platform }));
  await flushPromises();
}

it("gives the menu bar the Remote menu's labels with the others, in the language the UI shows", async () => {
  await mount(MAC);
  const labels = setMenuLabels.mock.lastCall![0];
  expect(labels).toMatchObject({ "menu.remote": "Remote", "menu.newRemoteWindow": "New Remote Window", "menu.openMainWindow": "Open This Mac’s Window" });

  await act(async () => setLanguage("zh-Hans"));
  await flushPromises();
  expect(setMenuLabels.mock.lastCall![0]).toMatchObject({ "menu.remote": "远程连接", "menu.newRemoteWindow": "新建远程窗口" });
});

it("gives the Windows jump list the Remote menu's labels under its keys, in Windows' sentence case and with the window line worded for the PC", async () => {
  await mount(WINDOWS);
  expect(setJumpListLabels).toHaveBeenCalledExactlyOnceWith({
    "menu.remote": "Remote",
    "menu.newRemoteWindow": "New remote window",
    "menu.noRemoteDaemons": "No remote daemons",
    "menu.openMainWindow": "Open this PC’s window",
  });
});

it("hands the jump list the labels again in the new language", async () => {
  await mount(WINDOWS);
  await act(async () => setLanguage("zh-Hans"));
  await flushPromises();
  expect(setJumpListLabels).toHaveBeenCalledTimes(2);
  expect(setJumpListLabels.mock.lastCall![0]["menu.remote"]).toBe("远程连接");
  expect(setJumpListLabels.mock.lastCall![0]["menu.noRemoteDaemons"]).toBe("没有远程守护进程");
});

it("hands the menu nothing where the platform has no menu bar, and the jump list nothing where it has none", async () => {
  await mount(MAC);
  await mount(WINDOWS);
  expect(setMenuLabels).toHaveBeenCalledTimes(1);
  expect(setJumpListLabels).toHaveBeenCalledTimes(1);
});

it("survives a shell that refuses the labels", async () => {
  setJumpListLabels.mockRejectedValueOnce(new Error("no jump list"));
  await mount(WINDOWS);
  expect(setJumpListLabels).toHaveBeenCalledOnce();
});
