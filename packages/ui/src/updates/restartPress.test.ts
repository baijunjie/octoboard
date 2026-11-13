// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { t } from "../i18n/language";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { restartPress } from "./restartPress";
import type { UpdateState, UpdateStore } from "./updateStore";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(acknowledgeAllAlerts);

const INSTALLED = { phase: "installed", installedVersion: "0.2.0" } as UpdateState;

function storeOf(): UpdateStore {
  return { restart: vi.fn(async () => {}) } as unknown as UpdateStore;
}

it("is available and asks the store to restart when nothing blocks the restart", () => {
  const store = storeOf();
  const { unavailable, onPress } = restartPress(t, INSTALLED, store);
  expect(unavailable).toBeUndefined();
  onPress();
  expect(store.restart).toHaveBeenCalledOnce();
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
});

it("is unavailable with the reason while an agent's update runs, and explains with the daemon's refusal instead of restarting", () => {
  const store = storeOf();
  const { unavailable, onPress } = restartPress(t, { ...INSTALLED, restartBlocked: "agentUpdate" }, store);
  expect(unavailable).toBe("An agent is being updated. Restart when the update finishes.");
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  onPress();
  expect(store.restart).not.toHaveBeenCalled();
  const alerts = errorAlerts.getSnapshot();
  expect(alerts).toHaveLength(1);
  expect(alerts[0]?.title).toBe("Cannot restart");
  expect(alerts[0]?.error).toBeInstanceOf(CodedError);
  expect((alerts[0]?.error as CodedError).code).toBe("agent_update_running");
  expect(alerts[0]?.message).toContain("An agent is being updated");
});
