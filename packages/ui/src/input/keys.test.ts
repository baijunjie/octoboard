import { expect, it } from "vitest";

import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { isPrimaryModifier, isSecondaryPress } from "./keys";

it.each([
  ["the secondary button", { button: 2, ctrlKey: false }, true, true],
  ["a plain primary press", { button: 0, ctrlKey: false }, false, false],
  ["Control with the primary button", { button: 0, ctrlKey: true }, true, false],
])("reads %s as a context menu press on macOS (%s) and on Windows (%s)", (_name, press, mac, windows) => {
  expect(isSecondaryPress(press, MAC_HOST)).toBe(mac);
  expect(isSecondaryPress(press, WINDOWS_HOST)).toBe(windows);
});

it("counts Control with the primary button on every platform for a target that asks for it", () => {
  expect(isSecondaryPress({ button: 0, ctrlKey: true }, WINDOWS_HOST, true)).toBe(true);
});

it("takes the host's own shortcut modifier, and not both", () => {
  expect(isPrimaryModifier({ metaKey: true, ctrlKey: false }, MAC_HOST)).toBe(true);
  expect(isPrimaryModifier({ metaKey: false, ctrlKey: true }, MAC_HOST)).toBe(false);
  expect(isPrimaryModifier({ metaKey: false, ctrlKey: true }, WINDOWS_HOST)).toBe(true);
  expect(isPrimaryModifier({ metaKey: true, ctrlKey: true }, WINDOWS_HOST)).toBe(false);
});
