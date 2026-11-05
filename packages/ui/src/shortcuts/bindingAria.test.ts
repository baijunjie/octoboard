import { expect, it } from "vitest";

import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { ariaBinding } from "./binding";

it("writes ARIA's modifier names rather than localized key-cap names", () => {
  expect(ariaBinding({ code: "KeyF", primary: true, shift: true }, MAC_HOST)).toBe("Shift+Meta+F");
  expect(ariaBinding({ code: "KeyF", primary: true, shift: true }, WINDOWS_HOST)).toBe("Shift+Control+F");
  expect(ariaBinding({ code: "KeyG", primary: true, shift: true, other: true, alt: true }, MAC_HOST)).toBe("Control+Alt+Shift+Meta+G");
  expect(ariaBinding({ code: "KeyG", primary: true, shift: true, other: true, alt: true }, WINDOWS_HOST)).toBe("Meta+Alt+Shift+Control+G");
});

it.each([["Space", "Space"], ["Comma", ","], ["Digit1", "1"], ["F5", "F5"], ["ArrowLeft", "ArrowLeft"]])("writes %s by the key name ARIA accepts", (code, key) => {
  expect(ariaBinding({ code, primary: true }, MAC_HOST)).toBe(`Meta+${key}`);
  expect(ariaBinding({ code, primary: true }, WINDOWS_HOST)).toBe(`Control+${key}`);
});
