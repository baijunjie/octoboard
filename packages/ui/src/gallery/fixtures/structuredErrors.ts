import type { Scenario } from "../scenario";
import type { PlainMessageKey } from "../../i18n/catalog";
import { consoleOf, failureTitle, pageOf, projectOf, sessionOf, shellTerminalOf, snapshotState } from "./builders";
import { DEFAULT_GIT_WORKFLOW } from "../../gitWorkflow";

const console_ = consoleOf("c-errors", "الإصدار Release coordination");
const project = projectOf("p-errors", console_.id, "البحث Search API");
const session = sessionOf("s-errors", console_.id, project.id, "مراجعة Review index", "idle");
const hub = sessionOf("s-error-hub", console_.id, undefined, "Hub", "idle");
const shell = shellTerminalOf("shell-errors", project, "بناء Build shell", 0);
const account = { id: "account-errors", agent: "claude" as const, name: "العمل Work", config_dir: "/Users/dev/.claude-work" };
const state = snapshotState({
  consoles: [console_], projects: [project], sessions: [session, hub], shellTerminals: [shell],
  pages: { [hub.id]: [pageOf("page-errors", hub.id, "<p>Report</p>", 1)] },
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/Users/dev/Projects", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [account] },
});

/** The browser producer forwards Git stderr verbatim, including its leading spaces and newlines. */
export const GIT_STDERR = [
  "error: Your local changes to the following files would be overwritten by checkout:",
  "    src/مرحبا/index.ts",
  `    src/${"long-directory-name/".repeat(18)}index.ts`,
  "Please commit your changes or stash them before you switch branches.",
  "Aborting",
].join("\n");

/** The title the action that fails this way gives its alert in the app; a code with an `alertTitle` of its own (a failed
 * launch, a message not sent, a trust answer) needs none, as its own wins. */
const ACTION_TITLE: Record<string, PlainMessageKey> = {
  unknown_console: "dialog.console.saveFailedTitle",
  unknown_project: "dialog.project.saveFailedTitle",
  unknown_session: "dialog.session.failedTitle",
  unknown_shell_terminal: "shellTerminal.closeFailedTitle",
  unknown_account: "settings.accounts.dialog.saveFailedTitle",
  not_a_git_repository: "sidebar.project.pullFailedTitle",
  git_failed: "sidebar.project.pullFailedTitle",
  unknown_page: "report.submitFailedTitle",
  field_too_long: "settings.saveFailedTitle",
  field_required: "dialog.session.failedTitle",
  conflicting_fields: "dialog.session.failedTitle",
  invalid_launch_setting: "dialog.session.failedTitle",
  shell_environment_timeout: "dialog.session.failedTitle",
};

const error = (id: string, title: string, code: string, params: Record<string, string>, description: string, message = ""): Scenario => ({
  id, title, description, group: "Trust prompts and toasts", category: "exceptional", state,
  toasts: [{ kind: "error", code, params, message, ...(ACTION_TITLE[code] && { title: failureTitle(ACTION_TITLE[code]) }) }],
});

/** An acknowledgement whose raw text is the target: the Error details disclosure it sits behind is open. */
const withDiagnostics = (scenario: Scenario): Scenario => ({ ...scenario, steps: [(ui) => ui.press(ui.t("error.diagnostics"))] });

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
  withDiagnostics(error("error-nested-launch-diagnostics", "Nested launch failure with raw diagnostics", "launch_failed", { title: session.title, reason_code: "codex_unarchive_failed", detail: "error: failed to restore archived rollout\n    /Users/dev/.codex/archived_sessions/مرحبا/rollout.jsonl: permission denied" }, "The alert is titled \"Could not start the session\", with the session's name a chip on the \"Session:\" line and the nested restore failure worded by the application alone; its raw multiline diagnostics are in the Error details disclosure, open here, and never in the explanation.")),
  withDiagnostics(error("error-shell-timeout-diagnostics", "Shell timeout with only producer diagnostics", "shell_environment_timeout", { detail: "shell environment timed out while reading login profile\n    /Users/dev/.zprofile: waiting for credential helper" }, "This login producer supplies only raw diagnostics, with no shell, command or duration to put in a sentence. The alert says only that the shell environment could not be captured and what to do; the producer's multiline text is in the Error details disclosure, open here, in a block of its own.", "The shell environment could not be captured: shell environment timed out while reading login profile\n    /Users/dev/.zprofile: waiting for credential helper")),
  withDiagnostics(error("error-git-diagnostics", "Multiline Git stderr in an acknowledgement", "git_failed", { detail: GIT_STDERR }, "A short sentence of the application's own is followed by the Git recovery guidance (open the project's terminal, resolve the Git error, retry) and no Report issue, then the Error details disclosure, open here, which holds the raw Git stderr in a block of its own. Its indentation and mixed direction are kept and the long line wraps instead of scrolling sideways; the block is capped at 40vh and, when its content exceeds that cap, scrolls vertically as a region reachable by keyboard and named by the disclosure.")),
  error("error-terminal-input-lost", "Terminal input lost, a session's terminal", "terminal_input_lost", { session: session.id }, "A broadcast alert with no request behind it, titled \"Terminal input not accepted\": the summary says the terminal of the session, whose name is a chip, disconnected before it had taken everything typed into it, and the recovery step asks the person to check the terminal's input line. No Error details disclosure, since no write failed.", `The terminal ${session.id} disconnected before it had taken everything typed into it; check its input line before continuing.`),
  error("error-terminal-input-lost-shell", "Terminal input lost, a shell terminal", "terminal_input_lost", { shell_terminal: shell.id }, "The same alert for a shell terminal: the summary leads with the terminal itself, whose name is a chip, instead of a session's terminal; the recovery step is unchanged.", `The terminal ${shell.id} disconnected before it had taken everything typed into it; check its input line before continuing.`),
  withDiagnostics(error("error-terminal-input-lost-diagnostics", "Terminal input lost after a failed write", "terminal_input_lost", { session: session.id, detail: "write failed: broken pipe\n    /dev/ttys042: terminal disconnected" }, "The session-named alert with the Error details disclosure, open here, holding the failed write's raw multiline message in a block of its own and not in the summary.", `The terminal ${session.id} disconnected before it had taken everything typed into it: write failed: broken pipe\n    /dev/ttys042: terminal disconnected; check its input line before continuing.`)),
  error("error-agent-claude", "Referenced Claude Code in a refusal", "message_not_sent", { agent: "Claude Code" }, "The referenced agent uses a prose name chip; the corresponding accessible and notification wording remains plain text."),
  withDiagnostics(error("error-nested-trust", "Nested trust refusal with raw diagnostics", "trust_answer_failed", { agent: "Codex", reason_code: "terminal_write_failed", detail: "write failed: broken pipe\n    /dev/ttys042: terminal disconnected" }, "The agent name is a chip while the short nested trust reason remains prose; the raw terminal diagnostics are not in it but in the Error details disclosure, open here, in a block of their own.")),
];
