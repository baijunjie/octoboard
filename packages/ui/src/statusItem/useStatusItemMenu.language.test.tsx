// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { StatusItemCapability } from "../platform";
import type { PlatformAdapter } from "../platform";
import { MAC_HOST } from "../test/hosts";
import type { StatusItemMenuDraft } from "./statusItemMenu";

vi.mock("./statusItemIcons", () => ({ withIcons: async (draft: StatusItemMenuDraft) => ({ ...draft, icons: {} }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The menu a remote daemon's window (identity `me@host`) sends while its own language is `own` and the local
 * windows' cached language is `local`. */
async function menuOf(own: string, local: string) {
  localStorage.setItem("octoboard.language", local);
  localStorage.setItem("octoboard.language@me@host", own);
  vi.resetModules();
  const { setPreferenceDaemonIdentity } = await import("../preferences/persistedPreference");
  setPreferenceDaemonIdentity("me@host");
  const { useStatusItemMenu } = await import("./useStatusItemMenu");
  const { TestHost } = await import("../test/TestHost");
  const { testDaemon } = await import("../test/testDaemon");
  const language = await import("../i18n/language");
  const { translatorFor } = await import("../i18n/catalog");
  const setMenu = vi.fn<StatusItemCapability["setMenu"]>(() => Promise.resolve());
  const statusItem: StatusItemCapability = { setMenu, setVisible: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} };
  function Host() {
    useStatusItemMenu({ ready: true, onSessionChosen: () => {} });
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    localStorage.clear();
  });
  await act(async () => {
    root.render(
      <TestHost daemon={testDaemon({ state: {}, request: () => Promise.reject(new Error("offline")) })} platform={{ statusItem, host: MAC_HOST } as PlatformAdapter}>
        <Host />
      </TestHost>,
    );
  });
  return { menu: setMenu.mock.calls.at(-1)![0], language, translatorFor, setMenu };
}

it("words the menu in the local windows' language, not the language of the remote daemon the window shows", async () => {
  const { menu, language, translatorFor } = await menuOf("de", "ja");
  expect(language.currentLanguage()).toBe("de");
  expect(menu.open).toBe(translatorFor("ja")("statusItem.open"));
  expect(menu.open).not.toBe(translatorFor("de")("statusItem.open"));
});

it("rewords the menu when a local window changes its language", async () => {
  const { menu, setMenu, translatorFor } = await menuOf("de", "ja");
  expect(menu.quit).toBe(translatorFor("ja")("statusItem.quit"));
  await act(async () => {
    window.dispatchEvent(new StorageEvent("storage", { key: "octoboard.language", newValue: "fr", storageArea: localStorage }));
  });
  expect(setMenu.mock.calls.at(-1)![0].quit).toBe(translatorFor("fr")("statusItem.quit"));
});
