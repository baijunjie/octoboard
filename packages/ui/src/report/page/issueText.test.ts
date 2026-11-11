import { expect, it } from "vitest";

import type { Translate } from "../../i18n/catalog";
import { issueText } from "./issueText";

/** A translation that shows the key and the parameters it was given. */
const t = ((key: string, params?: Record<string, unknown>) => `${key} ${JSON.stringify(params ?? {})}`) as unknown as Translate;
const plain = (text: string) => text.replace(/[⁨⁩]/g, "");

it("writes a numeric limit and a step in the language's own digits and grouping, isolated", () => {
  expect(issueText(t, "en", { kind: "min", limit: 1234.5 })).toContain("⁨1,234.5⁩");
  expect(plain(issueText(t, "de", { kind: "max", limit: 1234.5 }))).toContain("1.234,5");
  expect(plain(issueText(t, "de", { kind: "step", step: 0.25 }))).toContain("0,25");
});

it("writes a date, a time and a date with a time limit the way the language writes them, isolated", () => {
  const date = plain(issueText(t, "en", { kind: "notBefore", limit: "2026-11-20" }));
  expect(date).toContain("Nov 20, 2026");
  expect(plain(issueText(t, "en", { kind: "notAfter", limit: "09:30" }))).toContain("9:30");
  const both = plain(issueText(t, "en", { kind: "notAfter", limit: "2026-11-27T10:00" }));
  expect(both).toContain("Nov 27, 2026");
  expect(both).toContain("10:00");
  expect(issueText(t, "ja", { kind: "notBefore", limit: "2026-11-20" })).toContain("⁨");
});

it("words a pattern that could not be tested apart from one the value does not match", () => {
  expect(issueText(t, "en", { kind: "patternUnchecked" })).toContain("report.invalid.patternUnchecked");
  expect(issueText(t, "en", { kind: "pattern" })).toContain("report.invalid.pattern ");
});
