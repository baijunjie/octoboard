import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import type { Scenario } from "../scenario";
import type { PlainMessageKey } from "../../i18n/catalog";
import { accountOf, broadcastErrorToast, consoleOf, failureTitle, projectOf, sessionNoticeToast, sessionOf, snapshotState } from "../../fixtures/builders";
import type { ToastFixture } from "../../fixtures/fixture";

const console_ = consoleOf("c-errors", "الإصدار Release coordination");
const project = projectOf("p-errors", console_.id, "البحث Search API");
const session = sessionOf("s-errors", console_.id, project.id, "مراجعة Review index", "idle");
const account = accountOf("account-errors", "claude", "العمل Work", "/Users/dev/.claude-work");
const state = snapshotState({
  consoles: [console_], projects: [project], sessions: [session],
  settings: { ...DEFAULT_SETTINGS, added_agents: null, default_clone_dir: "/Users/dev/Projects", accounts: [account] },
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
  unknown_session: "dialog.session.failedTitle",
  unknown_account: "settings.accounts.dialog.saveFailedTitle",
  git_failed: "sidebar.project.pullFailedTitle",
  conflicting_fields: "dialog.session.failedTitle",
  shell_environment_timeout: "dialog.session.failedTitle",
};

const alert = (id: string, title: string, description: string, toast: ToastFixture): Scenario => ({
  id, title, description, group: "Trust prompts and toasts", category: "exceptional", state, toasts: [toast],
});

/** A failure as the app shows it: one an action raised, under the title that action gives it, where the code has one
 * (`ACTION_TITLE`), otherwise an error the daemon broadcasts, naming no session. */
const error = (id: string, title: string, code: string, params: Record<string, string>, description: string, message = ""): Scenario =>
  alert(id, title, description, ACTION_TITLE[code]
    ? { kind: "raised", code, params, message, title: failureTitle(ACTION_TITLE[code]) }
    : broadcastErrorToast(code, params, message));

/** A session the daemon reports a failed launch of while the store holds no record of it, as when the record is gone:
 * the alert takes the session's name from the failure itself and has no location to give. */
const GONE_SESSION = "s-launch-gone";

/** A failed launch, which the daemon reports as a notice about its session. */
const launchError = (id: string, title: string, params: Record<string, string>, description: string): Scenario =>
  alert(id, title, description, sessionNoticeToast(GONE_SESSION, "launch_failed", params));

/** An acknowledgement whose raw text is the target: the Error details disclosure it sits behind is open. */
const withDiagnostics = (scenario: Scenario): Scenario => ({ ...scenario, steps: [(ui) => ui.press(ui.t("error.diagnostics"))] });

/** Each acknowledgement replaces the last one, so each distinct layout has its own direct entry. */
export const structuredErrorScenarios: Scenario[] = [
  error("error-unknown-session-known", "Unknown session with a retained name", "unknown_session", { session: session.id }, "The current record name is a prose chip whose Arabic and Latin text keeps its own direction, and the refusal's remedy follows it. The other records (console, project, shell terminal) read alike."),
  error("error-unknown-session-missing", "Unknown session with only its ID", "unknown_session", { session: "missing-session-42" }, "The missing record's technical ID is a left-to-right code chip, without inventing a name."),
  error("error-unknown-account-known", "Unknown account with a retained name", "unknown_account", { account: account.id }, "The remedy for a missing account comes with a Manage accounts button in the footer, left of OK."),
  error("error-schema-conflicting-fields", "Conflicting schema field refusal", "conflicting_fields", { first: "project_id", second: "console_id" }, "A request the window should never send: both conflicting schema identifiers use technical chips, and the unexpected-failure guidance and Report issue button come with them."),
  withDiagnostics(launchError("error-nested-launch-diagnostics", "Nested launch failure with raw diagnostics", { title: session.title, reason_code: "codex_unarchive_failed", detail: "error: failed to restore archived rollout\n    /Users/dev/.codex/archived_sessions/مرحبا/rollout.jsonl: permission denied" }, "The alert is titled \"Could not start the session\", with the session's name a chip on the \"Session:\" line and the nested restore failure worded by the application alone; its raw multiline diagnostics are in the Error details disclosure, open here, and never in the explanation.")),
  withDiagnostics(error("error-shell-timeout-diagnostics", "Shell timeout with only producer diagnostics", "shell_environment_timeout", { detail: "shell environment timed out while reading login profile\n    /Users/dev/.zprofile: waiting for credential helper" }, "This login producer supplies only raw diagnostics, with no shell, command or duration to put in a sentence. The alert says only that the shell environment could not be captured and what to do; the producer's multiline text is in the Error details disclosure, open here, in a block of its own.", "The shell environment could not be captured: shell environment timed out while reading login profile\n    /Users/dev/.zprofile: waiting for credential helper")),
  withDiagnostics(error("error-git-diagnostics", "Multiline Git stderr in an acknowledgement", "git_failed", { detail: GIT_STDERR }, "A short sentence of the application's own is followed by the Git recovery guidance (open the project's terminal, resolve the Git error, retry) and no Report issue, then the Error details disclosure, open here, which holds the raw Git stderr in a block of its own. Its indentation and mixed direction are kept and the long line wraps instead of scrolling sideways; the block is capped at 40vh and, when its content exceeds that cap, scrolls vertically as a region reachable by keyboard and named by the disclosure.")),
  error("error-terminal-input-lost", "Terminal input lost, a session's terminal", "terminal_input_lost", { session: session.id }, "A broadcast alert with no request behind it, titled \"Terminal input not accepted\": the summary says the terminal of the session (or, for a shell terminal, the terminal itself), whose name is a chip, disconnected before it had taken everything typed into it, and the recovery step asks the person to check the terminal's input line. No Error details disclosure, since no write failed.", `The terminal ${session.id} disconnected before it had taken everything typed into it; check its input line before continuing.`),
  withDiagnostics(error("error-terminal-input-lost-diagnostics", "Terminal input lost after a failed write", "terminal_input_lost", { session: session.id, detail: "write failed: broken pipe\n    /dev/ttys042: terminal disconnected" }, "The session-named alert with the Error details disclosure, open here, holding the failed write's raw multiline message in a block of its own and not in the summary.", `The terminal ${session.id} disconnected before it had taken everything typed into it: write failed: broken pipe\n    /dev/ttys042: terminal disconnected; check its input line before continuing.`)),
  withDiagnostics(error("error-nested-trust", "Nested trust refusal with raw diagnostics", "trust_answer_failed", { agent: "Codex", reason_code: "terminal_write_failed", detail: "write failed: broken pipe\n    /dev/ttys042: terminal disconnected" }, "The agent name is a chip while the short nested trust reason remains prose; the raw terminal diagnostics are not in it but in the Error details disclosure, open here, in a block of their own.")),
];
