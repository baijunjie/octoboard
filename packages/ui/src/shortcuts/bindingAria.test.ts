import { expect, it } from "vitest";

import { IS_MAC } from "../input/keys";
import { ariaBinding } from "./binding";

it("writes ARIA's modifier names rather than localized key-cap names", () => {
  expect(ariaBinding({ code: "KeyF", primary: true, shift: true })).toBe(`Shift+${IS_MAC ? "Meta" : "Control"}+F`);
  expect(ariaBinding({ code: "KeyG", primary: true, shift: true, other: true, alt: true })).toBe(`${IS_MAC ? "Control" : "Meta"}+Alt+Shift+${IS_MAC ? "Meta" : "Control"}+G`);
});

it.each([["Space", "Space"], ["Comma", ","], ["Digit1", "1"], ["F5", "F5"], ["ArrowLeft", "ArrowLeft"]])("writes %s by the key name ARIA accepts", (code, key) => {
  expect(ariaBinding({ code, primary: true })).toBe(`${IS_MAC ? "Meta" : "Control"}+${key}`);
});
