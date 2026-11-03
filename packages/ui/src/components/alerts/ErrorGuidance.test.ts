// @vitest-environment jsdom
import { expect, it } from "vitest";

import { translatorFor, type Translate } from "../../i18n/catalog";
import { recoveryAction } from "./ErrorGuidance";

const t: Translate = translatorFor("en");

it("offers the way to Settings' Agent accounts for a reset credit refused for want of a sign-in", () => {
  expect(recoveryAction(t, "resetCreditLogin")).toEqual({ label: "Manage accounts" });
});
