// @vitest-environment jsdom
import { expect, it } from "vitest";

import { format, type Translate } from "../../i18n/catalog";
import { recoveryAction } from "./ErrorGuidance";

const t: Translate = (key, ...args) => format("en", key, args[0]);

it("offers the way to Settings' Agent accounts for a reset credit refused for want of a sign-in", () => {
  expect(recoveryAction(t, "resetCreditLogin")).toEqual({ label: "Manage accounts" });
});
