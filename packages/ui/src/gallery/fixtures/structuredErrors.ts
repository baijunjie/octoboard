import type { Scenario } from "../scenario";
import { consoleOf, pageOf, projectOf, sessionOf, shellTerminalOf, snapshotState } from "./builders";
import { DEFAULT_WORKTREE_SETTINGS } from "../../worktreeSettings";

const console_ = consoleOf("c-errors", "الإصدار Release coordination");
const project = projectOf("p-errors", console_.id, "البحث Search API");
const session = sessionOf("s-errors", console_.id, project.id, "مراجعة Review index", "idle");
const hub = sessionOf("s-error-hub", console_.id, undefined, "Hub", "idle");
const shell = shellTerminalOf("shell-errors", project, "بناء Build shell", 0);
const account = { id: "account-errors", agent: "claude" as const, name: "العمل Work", config_dir: "/Users/dev/.claude-work" };
const state = snapshotState({
  consoles: [console_], projects: [project], sessions: [session, hub], shellTerminals: [shell],
  pages: { [hub.id]: [pageOf("page-errors", hub.id, "<p>Report</p>", 1)] },
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/Users/dev/Projects", worktrees: DEFAULT_WORKTREE_SETTINGS, accounts: [account] },
});

/** The browser producer forwards Git stderr verbatim, including its leading spaces and newlines. */
export const GIT_STDERR = [
  "error: Your local changes to the following files would be overwritten by checkout:",
  "    src/مرحبا/index.ts",
  `    src/${"long-directory-name/".repeat(18)}index.ts`,
  "Please commit your changes or stash them before you switch branches.",
  "Aborting",
].join("\n");

const error = (id: string, title: string, code: string, params: Record<string, string>, description: string, message = ""): Scenario => ({
  id, title, description, group: "Trust prompts and toasts", category: "exceptional", state,
  toasts: [{ kind: "error", code, params, message }],
});

/** Each acknowledgement replaces the last one, so each semantic route has its own direct entry. */
export const structuredErrorScenarios: Scenario[] = [
  ...[
    ["console", console_.id], ["project", project.id], ["session", session.id],
    ["shell_terminal", shell.id], ["account", account.id],
  ].flatMap(([kind, id]) => [
    error(`error-unknown-${kind.replaceAll("_", "-")}-known`, `Unknown ${kind.replaceAll("_", " ")} with a retained name`, `unknown_${kind}`, { [kind]: id }, "The current record name is a prose chip whose Arabic and Latin text keeps its own direction."),
    error(`error-unknown-${kind.replaceAll("_", "-")}-missing`, `Unknown ${kind.replaceAll("_", " ")} with only its ID`, `unknown_${kind}`, { [kind]: `missing-${kind}-42` }, "The missing record's technical ID is a left-to-right code chip, without inventing a name."),
  ]),
  error("error-not-git-repository-known", "Repository refusal with a project name", "not_a_git_repository", { project: project.id }, "The retained project's name uses a prose chip."),
  error("error-not-git-repository-missing", "Repository refusal with a project ID", "not_a_git_repository", { project: "missing-project-42" }, "Without a project record, the repository refusal keeps its technical ID."),
  error("error-unknown-page", "Unknown report page ID", "unknown_page", { page: "page-errors" }, "A page has no record name; its ID stays a technical chip even when the page is still cached."),
  error("error-schema-field-too-long", "Generic schema field length refusal", "field_too_long", { field: "default_clone_dir", limit: "255" }, "A generic field identifier uses a technical chip; the numeric limit stays ordinary text."),
  error("error-schema-field-required", "Required schema field refusal", "field_required", { field: "console_id" }, "The schema field is a technical literal rather than a record name."),
  error("error-schema-conflicting-fields", "Conflicting schema field refusal", "conflicting_fields", { first: "project_id", second: "console_id" }, "Both conflicting schema identifiers use technical chips."),
  error("error-schema-launch-setting", "Invalid launch setting refusal", "invalid_launch_setting", { agent: "Claude Code", field: "reasoning_effort", value: "unsupported-effort" }, "Both the schema field and its rejected value use technical chips."),
  error("error-nested-launch", "Nested launch failure with technical cause", "launch_failed", { title: session.title, reason_code: "invalid_launch_setting", agent: "Claude Code", field: "reasoning_effort", value: "unsupported-effort" }, "The launch title is a name chip and the nested failure keeps its field and rejected value as separate code chips."),
  error("error-nested-launch-diagnostics", "Nested launch failure with raw diagnostics", "launch_failed", { title: session.title, reason_code: "codex_unarchive_failed", detail: "error: failed to restore archived rollout\n    /Users/dev/.codex/archived_sessions/مرحبا/rollout.jsonl: permission denied" }, "The launch title is a name chip while the nested restore failure keeps raw multiline diagnostics outside the explanation."),
  error("error-shell-timeout-diagnostics", "Shell timeout with only producer diagnostics", "shell_environment_timeout", { detail: "shell environment timed out while reading login profile\n    /Users/dev/.zprofile: waiting for credential helper" }, "This login producer supplies only raw diagnostics. A short shell-capture explanation precedes the external code block without inventing a shell, command or duration.", "The shell environment could not be captured: shell environment timed out while reading login profile\n    /Users/dev/.zprofile: waiting for credential helper"),
  error("error-git-diagnostics", "Multiline Git stderr in an acknowledgement", "git_failed", { detail: GIT_STDERR }, "A short explanation precedes raw Git stderr in an external code block. Its indentation, mixed direction and long line remain selectable and horizontally scrollable by keyboard."),
  error("error-agent-claude", "Referenced Claude Code in a refusal", "message_not_sent", { agent: "Claude Code" }, "The referenced agent uses a prose name chip; the corresponding accessible and notification wording remains plain text."),
  error("error-nested-trust", "Nested trust refusal with raw diagnostics", "trust_answer_failed", { agent: "Codex", reason_code: "terminal_write_failed", detail: "write failed: broken pipe\n    /dev/ttys042: terminal disconnected" }, "The agent name is a chip while the short nested trust reason remains prose; raw terminal diagnostics follow in a separate code block."),
];
