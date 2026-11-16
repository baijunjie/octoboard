// @vitest-environment jsdom
import { vi, expect, it, beforeEach } from "vitest";

import { setLanguage } from "../i18n/language";
import { ThemeProvider } from "../preferences/theme";
import { DEFAULT_SETTINGS } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useClientSettingsReview } from "./useClientSettingsReview";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type LanguageLine = { value: string; current?: string } | undefined;

let languageLine: LanguageLine;
const keepLine = (line: LanguageLine) => {
  languageLine = line;
};

function Host({ document, onLine }: { document: unknown; onLine: (line: LanguageLine) => void }) {
  const { values } = useClientSettingsReview(document);
  onLine(values.find((value) => value.field === "interface_language"));
  return null;
}

/** Reviews a document carrying German while the window shows Chinese and the daemon holds `held`. */
function review(held: string | null) {
  // The theme provider asks the window for the system's colour scheme.
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  const daemon = testDaemon({ state: { settings: { ...DEFAULT_SETTINGS, interface_language: held } } });
  mountHost(<ThemeProvider><Host document={{ settings: { interface_language: "de" } }} onLine={keepLine} /></ThemeProvider>, { daemon });
}

beforeEach(() => {
  languageLine = undefined;
  setLanguage("zh-Hans");
});

it("compares the document's language with the one the daemon holds", () => {
  review("ja");
  expect(languageLine).toMatchObject({ value: "Deutsch", current: "日本語" });
});

it("compares with the window's language when the daemon holds none this build offers", () => {
  review("tlh");
  expect(languageLine).toMatchObject({ value: "Deutsch", current: "简体中文" });
});
