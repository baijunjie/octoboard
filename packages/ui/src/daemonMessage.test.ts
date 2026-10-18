import { expect, it } from "vitest";

import { appConfig } from "./appConfig";
import { EXAMPLE_CONVERSATION_ID } from "./conversationId";
import { daemonMessage } from "./daemonMessage";
import { CATALOGS } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import type { Language } from "./i18n/languages";
import type { Console, Project, Session } from "./protocol";

const records = {
  consoles: new Map([["c1", { id: "c1", name: "Work" } as Console]]),
  projects: new Map<string, Project>(),
  sessions: new Map([["s1", { id: "s1", title: "Hub" } as Session]]),
};

it("words a known code from the catalog, showing a record by its name and an unknown record by its id", () => {
  expect(daemonMessage("en", "unknown_session", { session: "s1" }, "x", records)).toBe(`Unknown session ${isolate("Hub")}.`);
  expect(daemonMessage("en", "unknown_project", { project: "p9" }, "x", records)).toBe(`Unknown project ${isolate("p9")}.`);
  expect(daemonMessage("en", "path_not_found", { path: "/a/b" }, "x", records)).toBe(`${isolate("/a/b")} does not exist.`);
});

it("words an invalid conversation ID with the example the UI supplies", () => {
  expect(daemonMessage("en", "resume_id_invalid", { id: "x" }, "y", records)).toContain(isolate(EXAMPLE_CONVERSATION_ID));
});

it("words a trust answer's failure from its reason code, keeping the English reason for an unknown one", () => {
  const reason = "the confirmation did not go away after the key was sent";
  const agent = "Codex";
  const params = { agent, reason, reason_code: "screen_not_dismissed" };
  expect(daemonMessage("en", "trust_answer_failed", params, "x", records)).toBe(
    `${appConfig.name} could not press ${isolate("Codex")}'s trust confirmation (${reason}). Answer it in the terminal.`,
  );
  expect(daemonMessage("en", "trust_answer_failed", { agent, reason: "why", reason_code: "newer" }, "x", records)).toBe(
    `${appConfig.name} could not press ${isolate("Codex")}'s trust confirmation (${isolate("why")}). Answer it in the terminal.`,
  );
});

it("words a failed launch's reason from the failure's own code and params, keeping the English reason for an unknown code", () => {
  const params = { title: "Hub", reason: "the English account", reason_code: "binary_not_found", binary: "codex" };
  expect(daemonMessage("en", "launch_failed", params, "x", records)).toBe(
    `Could not start ${isolate("Hub")}: ${isolate("codex")} was not found on ${isolate("PATH")} in the snapshotted shell environment.`,
  );
  expect(daemonMessage("en", "launch_failed", { ...params, reason_code: "newer" }, "x", records)).toBe(
    `Could not start ${isolate("Hub")}: ${isolate("the English account")}`,
  );
});

it("shows the daemon's own text for a code the catalog does not have", () => {
  expect(daemonMessage("en", "a_newer_code", {}, "the English text", records)).toBe("the English text");
});

it("picks the singular or plural wording from a numeric count param", () => {
  const params = (count: string, sessions: string) => ({ count, sessions });
  const one = daemonMessage("en", "session_has_running_sessions", params("1", "“A”"), "x", records);
  const many = daemonMessage("en", "session_has_running_sessions", params("2", "“A”, “B”"), "x", records);
  expect(one).toContain(`1 session under it is running: ${isolate("“A”")}.`);
  expect(many).toContain(`2 sessions under it are running: ${isolate("“A”, “B”")}.`);
});

// Each code a configuration import can be refused with, or report on a project, is worded by the
// catalog of every language that has one; a miss would show English, or the daemon's own text.
it.each([
  ["configuration_not_recognised", {}],
  ["configuration_version_unsupported", { version: "3", supported: "1" }],
  ["configuration_unreadable", { detail: "bad" }],
  ["import_resolution_invalid", { console: "0", project: "1" }],
  ["import_choice_unavailable", { choice: "clone" }],
  ["import_settings_missing", {}],
] as const)("words the configuration code %s in every catalog, each in its own language", (code, params) => {
  const english = daemonMessage("en", code, params, "the daemon's text", records);
  expect(english).not.toBe("the daemon's text");
  for (const language of Object.keys(CATALOGS).filter((candidate) => candidate !== "en") as Language[]) {
    // Worded in its own language, not the English a missing message falls back to.
    const worded = daemonMessage(language, code, params, "the daemon's text", records);
    expect(worded, language).not.toBe(english);
    expect(worded, language).not.toBe("the daemon's text");
  }
});

it("localizes structured running-session lists using live titles without parsing punctuation", () => {
  const params = { count: "2", sessions: "legacy fallback", session_ids: JSON.stringify(["s1", "gone"]) };
  const message = daemonMessage("en", "session_has_running_sessions", params, "fallback", records);
  expect(message).toContain(`${isolate("Hub")}, ${isolate("gone")}`);
  expect(message).not.toContain("legacy fallback");
  expect(daemonMessage("en", "session_has_running_sessions", { ...params, session_ids: "bad" }, "fallback", records)).toContain("legacy fallback");
});

it.each([
  ["account_name_taken", "Claude Code 已有名为 ⁨默认⁩ 的账户。"],
  ["config_dir_taken", "Claude Code 的账户 ⁨默认⁩ 已在使用此配置目录。"],
] as const)("words the default account in a %s message by its name in the current language", (code, zh) => {
  const params = { agent: "Claude Code", name: "Default" };
  expect(daemonMessage("zh-Hans", code, params, "x", records)).toBe(zh.replace("Claude Code", isolate("Claude Code")));
  expect(daemonMessage("en", code, params, "x", records)).toContain(isolate("Default"));
  expect(daemonMessage("zh-Hans", code, { ...params, name: "Work" }, "x", records)).toContain(isolate("Work"));
});

it("words each branch or worktree still there with its reason in plain text, one apart from the next", () => {
  const items = JSON.stringify([
    { kind: "branch", name: "fix/a,b", branch: "fix/a,b", state: "unmerged", uncommitted: false, detail: null },
    { kind: "worktree", name: "/home/dev/wt", branch: null, state: "failed", uncommitted: false, detail: "error: x, y" },
  ]);
  const message = daemonMessage("en", "session_branches_not_removed", { count: "2", items }, "fallback", { ...records, homeDir: "/home/dev" });
  expect(message).toBe(`These were to be removed and are still there: Removing this failed: ${isolate("~/wt")} (detached HEAD): ${isolate("error: x, y")}. This branch is not merged: ${isolate("fix/a,b")}.`);
  const zh = daemonMessage("zh-Hans", "session_branches_not_removed", { count: "2", items }, "fallback", { ...records, homeDir: "/home/dev" });
  expect(zh).toBe(`以下内容本应删除，但仍然保留：以下内容删除失败：${isolate("~/wt")}（分离的 HEAD）：${isolate("error: x, y")}。以下分支尚未合并：${isolate("fix/a,b")}。`);
});

it("puts the reason what goes with a removed project's folder is unknown on a line of its own", () => {
  const message = daemonMessage("en", "project_branches_not_determined", { name: "Docs", detail: "fatal: not a git repository" }, "fallback", records);
  expect(message.endsWith(".\nfatal: not a git repository")).toBe(true);
});

it("says a single group of leftovers by its own lead alone", () => {
  const items = JSON.stringify([{ kind: "branch", name: "fix/a", branch: "fix/a", state: "unmerged", uncommitted: false, detail: null }]);
  expect(daemonMessage("en", "session_branches_not_removed", { count: "1", items }, "fallback", records)).toBe(`This branch is not merged: ${isolate("fix/a")}.`);
});
