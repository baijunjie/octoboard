// @vitest-environment jsdom
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

import type { AccountQuota } from "../protocol";
import { AccountUsageBar } from "./AccountUsageBar";

const NOW = Date.UTC(2026, 9, 18, 12, 0, 0);
const HOUR = 60 * 60_000;

const stale: AccountQuota = {
  agent: "claude",
  account: null,
  fetched_at: NOW - HOUR,
  plan: "max",
  windows: [
    { kind: "session", label: null, used_percent: 40, resets_at: NOW + HOUR, window_minutes: 300 },
    { kind: "weekly", label: null, used_percent: 10, resets_at: NOW + 72 * HOUR, window_minutes: 10_080 },
  ],
  unavailable: { code: "quota_timed_out", params: {}, message: "x" }, email: null,
};

function fills(element: React.ReactElement): number {
  const host = document.createElement("div");
  host.innerHTML = renderToStaticMarkup(element);
  return host.querySelectorAll(".quota-bar-track > span").length;
}

it("leaves the track empty for windows kept from before a failed read, unless the surface asks to draw them", () => {
  expect(fills(<AccountUsageBar agent="claude" quota={stale} now={NOW} />)).toBe(0);
  expect(fills(<AccountUsageBar agent="claude" quota={stale} now={NOW} stale />)).toBe(2);
});
