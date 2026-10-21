import { expect, it } from "vitest";

import { displayColumns, ellipsize } from "./displayColumns";

it("counts East Asian wide characters and emoji as two columns, and bidi isolates as none", () => {
  expect(displayColumns("abc")).toBe(3);
  expect(displayColumns("重构")).toBe(4);
  expect(displayColumns("👨‍👩‍👧")).toBe(2);
  expect(displayColumns("e\u0301")).toBe(1);
  expect(displayColumns("\u2068ab\u2069")).toBe(2);
});

it("leaves text that fits whole and cuts the rest to the columns with an ellipsis", () => {
  expect(ellipsize("abcdef", 6)).toBe("abcdef");
  expect(ellipsize("abcdefgh", 6)).toBe("abcde…");
  expect(displayColumns(ellipsize("重构登录流程", 7))).toBeLessThanOrEqual(7);
});

it("keeps emoji sequences and combining marks whole when it cuts", () => {
  const family = "👨‍👩‍👧";
  expect(ellipsize(`${family}${family}${family}`, 5)).toBe(`${family}${family}…`);
  expect(ellipsize("e\u0301e\u0301e\u0301e\u0301", 3)).toBe("e\u0301e\u0301…");
});

it("closes an isolate the cut leaves open", () => {
  expect(ellipsize("\u2068abcdefgh\u2069", 4)).toBe("\u2068abc…\u2069");
});
