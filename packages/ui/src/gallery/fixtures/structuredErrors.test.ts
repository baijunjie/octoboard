import { describe, expect, it } from "vitest";
import { daemonMessage } from "../../errors/daemonMessage";
import { shellTerminalOf } from "../../fixtures/builders";
import { structuredErrorScenarios } from "./structuredErrors";

const sample = structuredErrorScenarios[0].state!;
const [consoleId] = sample.consoles!.keys();
const [project] = sample.projects!.values();
const shellTerminal = shellTerminalOf("shell-errors", project, "بناء Build shell", 0);

/** The names a text is worded from: the records the scenarios' shared state holds, and a shell terminal. */
const records = {
  consoles: sample.consoles!, projects: sample.projects!, sessions: sample.sessions!,
  shellTerminals: new Map([[shellTerminal.id, shellTerminal]]), accounts: sample.settings!.accounts,
};

/** Codes and variants these scenarios do not show — ones they no longer show, for a record that is held and for one that
 * is gone, and session notices and request failures shown elsewhere or nowhere in the gallery — so their placeholders
 * stay filled in every language without a scenario here. */
const UNSHOWN: Array<{ code: string; params: Record<string, string> }> = [
  { code: "unknown_console", params: { console: consoleId } },
  { code: "unknown_console", params: { console: "missing-console-42" } },
  { code: "unknown_project", params: { project: project.id } },
  { code: "unknown_project", params: { project: "missing-project-42" } },
  { code: "unknown_shell_terminal", params: { shell_terminal: shellTerminal.id } },
  { code: "unknown_shell_terminal", params: { shell_terminal: "missing-shell_terminal-42" } },
  { code: "not_a_git_repository", params: { project: project.id } },
  { code: "not_a_git_repository", params: { project: "missing-project-42" } },
  { code: "unknown_page", params: { page: "page-errors" } },
  { code: "field_too_long", params: { field: "default_clone_dir", limit: "255" } },
  { code: "field_required", params: { field: "console_id" } },
  { code: "invalid_launch_setting", params: { agent: "Claude Code", field: "reasoning_effort", value: "unsupported-effort" } },
  { code: "terminal_input_lost", params: { shell_terminal: shellTerminal.id } },
  { code: "submit_retry_failed", params: { agent: "Codex", detail: "Broken pipe (os error 32) (0 bytes written)" } },
  { code: "draft_save_unconfirmed", params: { agent: "Codex" } },
  { code: "draft_restore_unconfirmed", params: { agent: "Codex" } },
  { code: "typing_held", params: { agent: "Codex" } },
  { code: "typing_blocked", params: { agent: "Codex" } },
  { code: "typing_lost", params: { agent: "Codex", bytes: "37" } },
  { code: "queued_messages_lost", params: { fragment: "held" } },
  { code: "launch_failed", params: { title: "Review index", reason_code: "invalid_launch_setting", agent: "Claude Code", field: "reasoning_effort", value: "unsupported-effort" } },
];

// A successful render alone cannot reveal a missing fixture parameter: interpolation leaves it visible.
describe("structured error fixture contracts", () => {
  it.each(["en", "zh-Hans", "ar"] as const)("fills every catalog placeholder in %s", (language) => {
    for (const scenario of structuredErrorScenarios) {
      const state = scenario.state!;
      for (const toast of scenario.toasts ?? []) {
        const error = toast.kind === "broadcast" ? toast.event : toast.kind === "raised" ? toast : undefined;
        if (!error?.code) continue;
        const text = daemonMessage(language, error.code, error.params ?? {}, typeof error.message === "string" ? error.message : "", {
          consoles: state.consoles!, projects: state.projects!, sessions: state.sessions!,
          shellTerminals: state.shellTerminals, accounts: state.settings!.accounts,
        });
        expect(text, scenario.id).not.toMatch(/\{[a-zA-Z_]+\}/);
        expect(text.trim(), scenario.id).not.toBe("");
      }
    }
  });

  it.each(["en", "zh-Hans", "ar"] as const)("fills every placeholder of the codes no scenario shows in %s", (language) => {
    for (const { code, params } of UNSHOWN) {
      const text = daemonMessage(language, code, params, "", records);
      expect(text, code).not.toMatch(/\{[a-zA-Z_]+\}/);
      expect(text.trim(), code).not.toBe("");
    }
  });
});
