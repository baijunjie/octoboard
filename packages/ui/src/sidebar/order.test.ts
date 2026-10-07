import { describe, expect, it } from "vitest";

import type { Project, Session, SessionStatus } from "../protocol";
import { liveSessions, sortProjects } from "./order";

const session = (id: string, status: SessionStatus, started_at: number, pinned = false): Session => ({
  id,
  agent: "claude",
  console_id: "c",
  project_id: "p",
  host_id: "h",
  role: "project",
  origin: "user",
  title: id,
  status,
  has_conversation: false,
  include_in_hub: false,
  started_at,
  pinned,
});

const project = (id: string, name: string, pinned = false): Project => ({
  id,
  console_id: "c",
  host_id: "h",
  name,
  path: `/${name}`,
  source: "local",
  claude_trust_consent: false,
  pinned,
  tags: [],
});

describe("liveSessions", () => {
  it("puts pinned first, then a raised hand, work, idle and interrupted, newest first within a status", () => {
    const ordered = liveSessions([
      session("old-idle", "idle", 1),
      session("interrupted", "interrupted", 9),
      session("new-idle", "idle", 5),
      session("working", "working", 2),
      session("archived", "archived", 10),
      session("waiting", "waiting_user", 3),
      session("pinned", "interrupted", 0, true),
    ]);
    expect(ordered.map((s) => s.id)).toEqual(["pinned", "waiting", "working", "new-idle", "old-idle", "interrupted"]);
  });
});

describe("sortProjects", () => {
  it("puts pinned first, then a raised hand, work, a running session, then the inactive ones by name", () => {
    const sessions: Record<string, Session[]> = {
      b: [session("b1", "working", 1)],
      c: [session("c1", "waiting_user", 1)],
      d: [session("d1", "idle", 1)],
    };
    const ordered = sortProjects(
      [project("d", "delta"), project("a10", "item 10"), project("b", "beta"), project("a2", "item 2"), project("c", "charlie"), project("z", "zulu", true)],
      (p) => sessions[p.id] ?? [],
    );
    expect(ordered.map((p) => p.id)).toEqual(["z", "c", "b", "d", "a2", "a10"]);
  });
});
