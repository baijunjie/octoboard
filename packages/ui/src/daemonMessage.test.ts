import { expect, it } from "vitest";

import { appConfig } from "./appConfig";
import { daemonMessage } from "./daemonMessage";
import type { Console, Project, Session } from "./protocol";

const records = {
  consoles: new Map([["c1", { id: "c1", name: "Work" } as Console]]),
  projects: new Map<string, Project>(),
  sessions: new Map([["s1", { id: "s1", title: "Hub" } as Session]]),
};

it("words a known code from the catalog, showing a record by its name and an unknown record by its id", () => {
  expect(daemonMessage("en", "unknown_session", { session: "s1" }, "x", records)).toBe("Unknown session Hub.");
  expect(daemonMessage("en", "unknown_project", { project: "p9" }, "x", records)).toBe("Unknown project p9.");
  expect(daemonMessage("en", "path_not_found", { path: "/a/b" }, "x", records)).toBe("“/a/b” does not exist.");
});

it("words a trust answer's failure from its reason code, keeping the English reason for an unknown one", () => {
  const reason = "the screen did not go away after Enter";
  expect(daemonMessage("en", "claude_trust_answer_failed", { reason, reason_code: "screen_not_dismissed" }, "x", records)).toBe(
    `${appConfig.name} could not answer Claude Code's trust screen (${reason}). Answer it in the terminal.`,
  );
  expect(daemonMessage("en", "claude_trust_answer_failed", { reason: "why", reason_code: "newer" }, "x", records)).toBe(
    `${appConfig.name} could not answer Claude Code's trust screen (why). Answer it in the terminal.`,
  );
});

it("shows the daemon's own text for a code the catalog does not have", () => {
  expect(daemonMessage("en", "a_newer_code", {}, "the English text", records)).toBe("the English text");
});

it("picks the singular or plural wording from a numeric count param", () => {
  const params = (count: string, sessions: string) => ({ count, sessions });
  const one = daemonMessage("en", "console_session_has_running_sessions", params("1", "“A”"), "x", records);
  const many = daemonMessage("en", "console_session_has_running_sessions", params("2", "“A”, “B”"), "x", records);
  expect(one).toContain("1 session bound to it is running: “A”.");
  expect(many).toContain("2 sessions bound to it are running: “A”, “B”.");
});
