import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

import { LANGUAGES } from "./languages";

// The desktop bundle declares the offered languages so that VoiceOver speaks control roles in the
// system's language; a language added to the list but not to the bundle would be spoken in English.
// The bundle names a language by Apple's localization identifier, which for Chinese is the script tag.
const APPLE_IDENTIFIERS: Partial<Record<(typeof LANGUAGES)[number], string>> = { "zh-CN": "zh-Hans", "zh-TW": "zh-Hant" };

it("declares every offered language as a bundle localization", () => {
  const plist = readFileSync(
    new URL("../../../../apps/desktop/src-tauri/Info.template.plist", import.meta.url),
    "utf8",
  );
  const array =
    /<key>CFBundleLocalizations<\/key>\s*<array>([\s\S]*?)<\/array>/.exec(
      plist,
    )?.[1];
  const declared = [
    ...(array ?? "").matchAll(/<string>([^<]+)<\/string>/g),
  ].map((m) => m[1]);
  expect(declared).toEqual(LANGUAGES.map((language) => APPLE_IDENTIFIERS[language] ?? language));
});
