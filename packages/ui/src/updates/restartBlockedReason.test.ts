// @vitest-environment jsdom
import { expect, it } from "vitest";

import { t } from "../i18n/language";
import { restartBlockedReason } from "./restartBlockedReason";
import type { UpdateState } from "./updateStore";

const INSTALLED = { phase: "installed", installedVersion: "0.2.0" } as UpdateState;

it("gives no reason when nothing blocks the restart", () => {
  expect(restartBlockedReason(t, INSTALLED)).toBeUndefined();
});

it("says an agent's update is running while it blocks the restart", () => {
  expect(restartBlockedReason(t, { ...INSTALLED, restartBlocked: "agentUpdate" })).toBe("An agent is being updated. Restart when the update finishes.");
});
