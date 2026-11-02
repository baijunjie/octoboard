// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import entry from "./main.tsx?raw";

const originals = Object.fromEntries(
  ["localStorage", "sessionStorage"].map((key) => [key, Object.getOwnPropertyDescriptor(window, key)!]),
);
afterEach(() => {
  for (const [key, descriptor] of Object.entries(originals)) Object.defineProperty(window, key, descriptor);
  localStorage.clear();
  vi.restoreAllMocks();
  vi.resetModules();
});

/** The modules the preview's entry imports statically, which all evaluate before its body runs. */
const staticImports = [...entry.matchAll(/^import\s+(?!type\b)[^;]*?from\s+"([^"]+)";/gms)].map((match) => match[1]);

it("starts in the requested language, whatever the system's, without touching the host origin", async () => {
  expect(staticImports).toContain("./scenarios");
  vi.spyOn(navigator, "languages", "get").mockReturnValue(["en-US"]);
  const host = window.localStorage;
  for (const specifier of staticImports) await import(/* @vite-ignore */ specifier);

  const { installPreviewStorage } = await import("./storage");
  const { writeStartingPreferences } = await import("../fixtures/preferences");
  installPreviewStorage();
  writeStartingPreferences(localStorage, { language: "zh-Hans", theme: "dark" });

  const { currentLanguage } = await import("../i18n/language");
  expect(currentLanguage()).toBe("zh-Hans");
  expect(host.length).toBe(0);
});
