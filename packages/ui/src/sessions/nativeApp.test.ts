import { describe, expect, it } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import type { Session, SessionStatus } from "../protocol";
import { openInAppBlock, openInAppNote } from "./nativeApp";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

const session = (status: SessionStatus, extra: Partial<Session> = {}) => sessionOf("s", "c", "p", "Title", status, { agent_session_id: "0b8f1c52-3d1e-4a7b-9c41-5e6d7f8091a2", has_conversation: true, ...extra });

describe("openInAppBlock", () => {
  it.each<SessionStatus>(["interrupted", "archived"])("lets a %s session with a conversation its latest record can be seen for be opened", (status) => {
    expect(openInAppBlock(t, session(status))).toBeUndefined();
  });

  it.each<SessionStatus>(["starting", "working", "waiting_user", "dormant", "idle"])("keeps a %s session, which has a process, from being opened, and tells the user to stop it", (status) => {
    expect(openInAppBlock(t, session(status))).toEqual({
      detail: "Stop the session first: the agent's app and Octoboard would both write to this conversation.",
      state: "Session running",
    });
  });

  it("keeps a session with no conversation yet from being opened", () => {
    const block = { detail: "The session has no conversation yet, so there is nothing to open.", state: "No conversation yet" };
    expect(openInAppBlock(t, session("interrupted", { has_conversation: false }))).toEqual(block);
    expect(openInAppBlock(t, session("archived", { agent_session_id: null }))).toEqual(block);
  });

  it("tells, without keeping it from being opened, that the app may not see a conversation whose latest record the default directory does not hold", () => {
    expect(openInAppBlock(t, session("interrupted"))).toBeUndefined();
    expect(openInAppNote(t, false)).toEqual({
      detail: "The agent's app may not see this conversation: its latest record is kept in this account's own directory, not the default one.",
      note: "The agent's app may not see this conversation",
    });
    expect(openInAppNote(t, true)).toBeUndefined();
  });

  it("names a running session first, over a missing conversation", () => {
    expect(openInAppBlock(t, session("working", { has_conversation: false }))?.state).toBe("Session running");
    expect(openInAppBlock(t, session("interrupted", { has_conversation: false }))?.state).toBe("No conversation yet");
  });
});
