// @vitest-environment jsdom
import { expect, it, onTestFinished } from "vitest";

import { t } from "../../i18n/language";
import { acknowledgeAllAlerts } from "../../test/alertQueue";
import { errorAlerts } from "../alerts/errorAlerts";
import { raiseQuoteRefused } from "./raiseQuoteRefused";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it.each([
  ["input", false, "No session is ready for input now, so nothing was written to the input box.", "quoteInput"],
  ["input", true, "This session is not taking input now, so nothing was written to the input box.", "quoteInput"],
  ["program", false, "The program is not accepting pasted lines now, so nothing was written to the input box.", "quoteProgram"],
  ["program", true, "The program is not accepting pasted lines now, so nothing was written to the input box.", "quoteProgram"],
] as const)("tells a Quote refused for the %s (in the session terminal's own menu: %s) with its words and its remedy", (block, inTerminal, message, recovery) => {
  onTestFinished(acknowledgeAllAlerts);
  raiseQuoteRefused(block, t, { inTerminal });
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(errorAlerts.getSnapshot()[0]).toMatchObject({
    title: "Could not quote the selection",
    message,
    classification: { kind: "actionable", recovery },
  });
});
