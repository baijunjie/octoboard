// @vitest-environment jsdom
import { act } from "react";
import { afterEach, expect, it } from "vitest";

import { queuedMessage, sessionOf, snapshotState } from "../fixtures/builders";
import { t } from "../i18n/language";
import type { ClientRequestBody } from "../protocol";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { editQueuedMessage } from "./composerActions";
import { composerDraft } from "./composerDrafts";
import { sessionComposerEnabled } from "./composerSetting";
import { useEndEditsWhenOff } from "./useEndEditsWhenOff";

afterEach(() => sessionComposerEnabled.set(false));

function Probe(): null {
  useEndEditsWhenOff();
  return null;
}

function setUp() {
  sessionComposerEnabled.set(true);
  const sent: ClientRequestBody[] = [];
  const daemon = testDaemon({
    state: snapshotState({ sessions: [sessionOf("s", "c", "p", "Work", "idle")], messageQueues: {} }),
    request: async (body) => {
      sent.push(body);
      return { type: "ack", id: null };
    },
  });
  mountHost(<Probe />, { daemon });
  act(() => editQueuedMessage({ store: daemon.store, request: daemon.request, t }, "s", queuedMessage({ id: "m1", text: "waits", mode: "when_idle" }), "unix"));
  return { sent, daemon };
}

it("ends this window's edit as the composer goes off, however it went off, giving the message its mode back", () => {
  const { sent, daemon } = setUp();
  // The switch pressed here, or the preference taken up from another window's storage: either way its value turns off.
  act(() => sessionComposerEnabled.set(false));
  expect(composerDraft(daemon.store, "s").editing).toBeUndefined();
  expect(sent).toEqual([
    { type: "set_queued_message_mode", session: "s", message: "m1", mode: "manual" },
    { type: "set_queued_message_mode", session: "s", message: "m1", mode: "when_idle" },
  ]);
});

it("leaves the edit alone while the composer stays on", () => {
  const { sent, daemon } = setUp();
  act(() => sessionComposerEnabled.set(true));
  expect(composerDraft(daemon.store, "s").editing?.message).toBe("m1");
  expect(sent).toHaveLength(1);
});
