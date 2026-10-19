import { expect, it } from "vitest";

import type { Console, Project, Session } from "../protocol";
import { agentUsage } from "./agentUsage";

const console_ = (id: string, patch: Partial<Console>) => [id, { id, name: id, default_agent: "claude", ...patch } as Console] as const;

it("finds the consoles, projects and unarchived sessions that use an agent", () => {
  const usage = agentUsage(
    "codex",
    new Map([console_("a", { default_agent: "codex" }), console_("b", { project_session_agent: "codex" }), console_("c", {})]),
    new Map([["p", { id: "p", name: "P", default_agent: "codex" } as Project], ["q", { id: "q", name: "Q", default_agent: null } as Project]]),
    new Map([
      ["s1", { id: "s1", title: "One", agent: "codex", status: "idle" } as Session],
      ["s2", { id: "s2", agent: "codex", status: "archived" } as Session],
      ["s3", { id: "s3", agent: "claude", status: "idle" } as Session],
    ]),
    (session) => session.title,
  );
  expect(usage).toEqual({ consoles: ["a", "b"], projects: ["P"], sessions: ["One"] });
});

it("finds nothing for an agent nothing uses", () => {
  expect(agentUsage("grok", new Map(), new Map(), new Map(), (session) => session.title)).toEqual({ consoles: [], projects: [], sessions: [] });
});
