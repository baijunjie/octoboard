// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { sessionComposerEnabled } from "../composer/composerSetting";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import type { ClientRequestBody } from "../protocol";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { SessionComposerSetting } from "./SessionComposerSetting";

function mount() {
  const sent: ClientRequestBody[] = [];
  const daemon = testDaemon({ request: async (body) => (sent.push(body), { type: "ack", id: null }) });
  const container = mountHost(<SessionComposerSetting />, { daemon });
  return { sent, container, toggle: () => container.querySelector<HTMLInputElement>("input[type=checkbox]")! };
}

beforeEach(() => {
  localStorage.clear();
  sessionComposerEnabled.set(false);
});
afterEach(() => {
  sessionComposerEnabled.set(false);
  localStorage.clear();
});

describe("the message composer setting", () => {
  it("is off by default", () => {
    const { toggle, container } = mount();
    expect(toggle().checked).toBe(false);
    expect(container.textContent).toContain("Message composer");
    expect(sessionComposerEnabled.get()).toBe(false);
  });

  it("turns the composer on and off for this client alone, kept in its own storage and never sent to the daemon", async () => {
    const { toggle, sent } = mount();
    await act(async () => toggle().click());
    expect(toggle().checked).toBe(true);
    expect(sessionComposerEnabled.get()).toBe(true);
    expect(localStorage.getItem(PREFERENCE_KEYS.sessionComposer)).toBe("true");
    await act(async () => toggle().click());
    expect(toggle().checked).toBe(false);
    expect(sessionComposerEnabled.get()).toBe(false);
    expect(localStorage.getItem(PREFERENCE_KEYS.sessionComposer)).toBeNull();
    expect(sent).toEqual([]);
  });

  it("shows the switch on while the preference is on", () => {
    sessionComposerEnabled.set(true);
    expect(mount().toggle().checked).toBe(true);
  });
});
