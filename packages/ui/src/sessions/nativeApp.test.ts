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

  it("lets a session at rest, with a process, be opened", () => {
    expect(openInAppBlock(t, session("idle"))).toBeUndefined();
    expect(openInAppBlock(t, session("waiting_user", { at_usage_limit: true }))).toBeUndefined();
  });

  it.each<SessionStatus>(["working", "waiting_user", "dormant"])("keeps a %s session, which is at work, from being opened, and tells the user when it can be", (status) => {
    expect(openInAppBlock(t, session(status))).toEqual({
      detail: "The session is still working or waiting for you. It can be opened once it is waiting for instructions or interrupted.",
      state: "Session busy",
    });
  });

  it("keeps a starting session, and one an account switch holds, from being opened, with no short reason", () => {
    expect(openInAppBlock(t, session("starting"))).toEqual({ detail: "The session is still starting, so it cannot be opened yet." });
    expect(openInAppBlock(t, session("interrupted"), { switching: true })).toEqual({ detail: "Unavailable while this session is switching its account" });
  });

  it("names a locked session before one at work", () => {
    expect(openInAppBlock(t, session("working", { locked: true }))?.state).toBe("Session locked");
  });

  it("keeps an idle session with work running in the background from being opened, and a usage-limit wait only while it has none", () => {
    const working = { detail: "The session is still working or waiting for you. It can be opened once it is waiting for instructions or interrupted.", state: "Session busy" };
    const task = { id: "dev-server", type: "shell" };
    expect(openInAppBlock(t, session("idle", { background_tasks: [task] }))).toEqual(working);
    expect(openInAppBlock(t, session("idle", { background_tasks: [] }))).toBeUndefined();
    expect(openInAppBlock(t, session("waiting_user", { at_usage_limit: true, background_tasks: [task] }))).toEqual(working);
    expect(openInAppBlock(t, session("waiting_user", { at_usage_limit: true, background_tasks: [] }))).toBeUndefined();
  });

  it("keeps a locked session that is not archived from being opened, as archiving refuses it", () => {
    expect(openInAppBlock(t, session("interrupted", { locked: true }))).toEqual({ detail: "The session is locked, so it cannot be archived and opened in the agent's app.", state: "Session locked" });
    expect(openInAppBlock(t, session("archived", { locked: true }))).toBeUndefined();
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

  it("names the first block in the order locked, at work, switching, starting, no conversation", () => {
    const gone = { has_conversation: false };
    expect(openInAppBlock(t, session("working", { locked: true, ...gone }), { switching: true })?.state).toBe("Session locked");
    expect(openInAppBlock(t, session("working", gone), { switching: true })?.state).toBe("Session busy");
    expect(openInAppBlock(t, session("starting", gone), { switching: true })?.detail).toBe("Unavailable while this session is switching its account");
    expect(openInAppBlock(t, session("starting", gone))?.detail).toBe("The session is still starting, so it cannot be opened yet.");
    expect(openInAppBlock(t, session("interrupted", gone))?.state).toBe("No conversation yet");
  });

  it("lets an archived session be opened whatever else its record says, but for a missing conversation", () => {
    expect(openInAppBlock(t, session("archived", { locked: true }), { switching: false })).toBeUndefined();
  });

  it("names a session at work first, over a missing conversation", () => {
    expect(openInAppBlock(t, session("working", { has_conversation: false }))?.state).toBe("Session busy");
    expect(openInAppBlock(t, session("interrupted", { has_conversation: false }))?.state).toBe("No conversation yet");
  });
});
