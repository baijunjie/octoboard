import { expect, it } from "vitest";

import { TEST_PROVIDERS } from "../test/testProviders";
import { applyCompatibility, changedCompatibility, compatibilitySettings, withCompatibility } from "./launchCompatibility";

it.each(TEST_PROVIDERS)("offers only $id's declared compatibility settings in descriptor order", (provider) => {
  expect(compatibilitySettings(TEST_PROVIDERS, provider.id)).toEqual(provider.settings.filter((setting) => setting.compatibility_with !== null));
});

it("offers no compatibility settings for a provider not served", () => {
  expect(compatibilitySettings(TEST_PROVIDERS, "unknown")).toEqual([]);
});

it("holds On and Off explicitly, and removes a held value on follow without mutating the input", () => {
  const initial = { first: true, second: false };
  expect(withCompatibility(initial, "first", false)).toEqual({ first: false, second: false });
  expect(withCompatibility(initial, "second", true)).toEqual({ first: true, second: true });
  expect(withCompatibility(initial, "first", null)).toEqual({ second: false });
  expect(initial).toEqual({ first: true, second: false });
});

it("sends only changed switches, including false and null for follow", () => {
  expect(changedCompatibility({ unchanged: false, added: false, changed: false }, { unchanged: false, changed: true, cleared: false }))
    .toEqual({ added: false, changed: false, cleared: null });
  expect(changedCompatibility({ held: true }, { held: true })).toEqual({});
});

it("applies a sparse change without dropping unedited switches or modifying either input", () => {
  const initial = { kept: false, cleared: true, changed: false };
  const changes = { cleared: null, changed: true, added: false };
  expect(applyCompatibility(initial, changes)).toEqual({ kept: false, changed: true, added: false });
  expect(applyCompatibility(initial, undefined)).toEqual(initial);
  expect(initial).toEqual({ kept: false, cleared: true, changed: false });
  expect(changes).toEqual({ cleared: null, changed: true, added: false });
});
