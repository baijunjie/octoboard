import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

import { LANGUAGES } from "./languages";

// The desktop bundle declares the offered languages so that VoiceOver speaks control roles in the
// system's language; a language added to the list but not to the bundle would be spoken in English.
it("declares every offered language as a bundle localization", () => {
  const plist = readFileSync(
    new URL("../../../../apps/desktop/src-tauri/Info.plist", import.meta.url),
    "utf8",
  );
  const array =
    /<key>CFBundleLocalizations<\/key>\s*<array>([\s\S]*?)<\/array>/.exec(
      plist,
    )?.[1];
  const declared = [
    ...(array ?? "").matchAll(/<string>([^<]+)<\/string>/g),
  ].map((m) => m[1]);
  expect(declared).toEqual([...LANGUAGES]);
});
