import type { Scenario } from "../scenario";
import { SAMPLE, consoleOf, projectOf, sessionOf, snapshotState } from "./builders";
import { SHELL_TIMEOUT } from "./shellTimeout";

const TRUST = "Trust prompts and toasts";

// Git's own multi-line refusal, as `failed` carries it.
const GIT_REFUSAL = [
  "error: cannot delete branch 'fix/rename-index' used by worktree at '/Users/dev/worktrees/search-api-rename-index'",
  "hint: to remove it, first remove the worktree with git worktree remove, then run git branch -D fix/rename-index again once nothing else holds the branch",
].join("\n");

const { console: console_, web, api } = SAMPLE;
// A session of each of the other two agents, for the prompts and notices that name the agent.
const codexSession = sessionOf("s-api-codex", console_.id, api.id, "Review the retry logic", "working", {
  agent: "codex",
});
const grokProject = projectOf("p-grok-home", console_.id, "Home", { path: "/Users/dev" });
const grokSession = sessionOf("s-web-grok", console_.id, grokProject.id, "Tidy the stylesheet", "working", { agent: "grok" });
const sessions = [...SAMPLE.sessions, codexSession, grokSession];
const trustSessions = sessions.filter((session) => session.id !== "s-api-1").map((session) =>
  ["s-web-1", codexSession.id, grokSession.id].includes(session.id)
    ? { ...session, status: "waiting_user" as const }
    : session,
);
const requestingConsole = consoleOf("c-requester", "Release coordination");
// A linked worktree of the Search API repository, which Octoboard has as a project of its own.
const worktreeProject = projectOf("p-api-experiments", console_.id, "Search API experiments");

export const noticeScenarios: Scenario[] = [
  {
    id: "trust-prompt",
    group: TRUST,
    category: "normal",
    title: "Queued trust prompts for all agents",
    description: "Claude Code and Codex offer their projects' parent folders, while the Grok Build project is the home directory itself, whose parent is too broad to offer. The warning's configuration-file token takes its tone while the project path stays neutral. Each agent has its own caution, and Codex names its repository-root trust. All three asking sessions raise their hands. Not now advances through the queued prompts while the hands stay raised until trust is answered in the terminal.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api, grokProject],
      sessions: trustSessions,
      trustPrompts: [
        { session: "s-web-1", agent: "claude", project: web.id, path: web.path, trustDir: "/Users/dev/code" },
        { session: codexSession.id, agent: "codex", project: api.id, path: api.path, trustDir: "/Users/dev/code" },
        { session: grokSession.id, agent: "grok", project: grokProject.id, path: grokProject.path, trustDir: null },
      ],
    }),
  },
  {
    id: "trust-prompt-dismissed",
    group: TRUST,
    category: "normal",
    title: "Trust still waiting after Not now",
    description:
      "The folder-trust dialog has been deferred. Fix the summary layout keeps its raised hand because the agent is still waiting for trust in the terminal.",
    state: snapshotState({
      consoles: [console_],
      projects: [web],
      sessions: SAMPLE.sessions
        .filter((session) => session.project_id === web.id || session.role === "console")
        .map((session) => session.id === "s-web-1" ? { ...session, status: "waiting_user" } : session),
    }),
  },
  {
    id: "session-request-same-project",
    group: TRUST,
    category: "normal",
    title: "Request to start a session in the requester's own project",
    description: "A session in Website asks for a Claude Code session in Website itself. The requester is named by its terminal title beside its name, and its project and console are not named again, since the question already names them.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api, grokProject],
      sessions: sessions.map((s) => s.id === "s-web-1"
        ? { ...s, status: "waiting_user" as const, terminal_title: "Remove the redundant details" }
        : s),
      sessionRequests: [
        {
          requestId: "r-1",
          session: "s-web-1",
          role: "project",
          console: console_.id,
          project: web.id,
          agent: "claude",
          branches: [],
          requestedAt: 1,
        },
      ],
    }),
  },
  {
    id: "session-request-project",
    group: TRUST,
    category: "normal",
    title: "Request to start a session in another project",
    description: "A session in Search API asks for a Grok Build session in Search API experiments, a linked worktree of the same repository. The requester, named by its terminal title beside its name, belongs to a different console and project, so all requester and destination names appear as chips. The question is followed by the branches that would be handed over, each a code chip joined as the language joins a list. A literal % is shown as written (fix/coverage-at-100%), and the long name wraps inside its chip.",
    state: snapshotState({
      consoles: [console_, requestingConsole],
      projects: [web, { ...api, console_id: requestingConsole.id }, worktreeProject, grokProject],
      sessions: sessions.map((s) => ({
        ...s,
        ...(s.project_id === api.id ? { console_id: requestingConsole.id } : {}),
        ...(s.id === "s-api-1" ? { terminal_title: "Deduplicate retried writes" } : {}),
      })),
      sessionRequests: [
        {
          requestId: "r-1",
          session: "s-api-1",
          role: "project",
          console: console_.id,
          project: worktreeProject.id,
          agent: "grok",
          branches: ["feature/saved-searches", "fix/coverage-at-100%25", "feature/rework-the-suggestion-index-so-that-prefixes-are-matched-before-fuzzy-candidates"],
          requestedAt: 1,
        },
      ],
    }),
  },
  {
    id: "session-request-console",
    group: TRUST,
    category: "normal",
    title: "Request to start a console session",
    description: "A console session asks for a Codex console session: the requester is named by its console and the target is a console, with no project in the sentence.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api, grokProject],
      sessions: sessions.map((s) => (s.id === SAMPLE.consoleSession.id ? { ...s, status: "waiting_user" } : s)),
      sessionRequests: [
        {
          requestId: "r-1",
          session: SAMPLE.consoleSession.id,
          role: "console",
          console: console_.id,
          project: null,
          agent: "codex",
          branches: [],
          requestedAt: 1,
        },
      ],
    }),
  },
  {
    id: "toast-error",
    group: TRUST,
    category: "exceptional",
    title: "Queued error alerts",
    description:
      "Errors require explicit OK, with no close button or automatic dismissal. The first names a missing folder as code. Recovery labels such as Browse, Try again and Settings are chips. OK advances through path, connection and account remedies (the account one puts a Manage accounts… button in the footer, left of OK), two refused reopens (the agent no longer has the session's conversation, and Codex could not restore its archived one), each an expected refusal with its own advice rather than the unexpected-failure alert, a session-specific error, a failed trust confirmation and an unknown code with fallback text.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "error", code: "path_not_found", params: { path: "/Users/dev/code/missing" }, message: "" },
      { kind: "error", code: "connection_closed", params: {}, message: "The daemon connection closed" },
      { kind: "error", code: "unknown_account", params: { account: "removed-account" }, message: "The configured account no longer exists" },
      {
        kind: "error",
        code: "conversation_unresumable",
        params: { agent: "Claude Code", session: "s-web-1" },
        message: "Claude Code no longer has the conversation of this session, so it cannot be resumed",
      },
      {
        kind: "error",
        code: "codex_unarchive_failed",
        params: { detail: "no rollout found for thread id 0199c3a2-7e41-7b52-9d0e-4f6a1c85b2d7" },
        message: "`codex unarchive` failed: no rollout found for thread id 0199c3a2-7e41-7b52-9d0e-4f6a1c85b2d7",
      },
      { kind: "error", code: "session_request_not_waiting", params: { session: "s-web-1" }, message: "", session: "s-web-1" },
      {
        kind: "error",
        code: "trust_answer_failed",
        params: { agent: "Codex", reason_code: "cursor_moved_away" },
        message: "",
        session: codexSession.id,
      },
      { kind: "error", code: "a_code_from_a_newer_daemon", params: {}, message: "The daemon's own English text, shown as is." },
    ],
  },
  {
    id: "toast-message-not-sent",
    group: TRUST,
    category: "exceptional",
    title: "Message written but not sent by the agent",
    description: "An error alert tied to the Claude Code session: it names the agent, says the message waits in the session's input box and that nothing more is written into the session until a prompt is sent, then gives the recovery of opening the session and pressing Enter.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{ kind: "error", code: "message_not_sent", params: { agent: "Claude Code" }, message: "", session: "s-web-1" }],
  },
  {
    id: "error-unexpected",
    group: TRUST,
    category: "exceptional",
    title: "Unexpected failure and issue reporting",
    description: "The danger alert asks for a screenshot and reproduction steps. The body names Report issue in a noninteractive hint chip. Its secondary footer button, with an external-link icon, sits to the left of OK and opens the configured issue form. Diagnostic details remain available below the guidance.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    externalLinks: "available",
    toasts: [{ kind: "error", code: "internal_error", params: { detail: "Unexpected session state while applying an update" }, message: "Unexpected session state while applying an update" }],
  },
  {
    id: "error-issue-link-failed",
    group: TRUST,
    category: "exceptional",
    title: "Issue form could not be opened",
    description: "The opener failure takes precedence over the original unexpected error. Its actionable recovery supplies the manual issue URL and has no recursive Report issue link. OK returns to the original failure.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    externalLinks: { error: "The operating system could not open the default browser" },
    toasts: [{ kind: "error", code: "internal_error", params: { detail: "Unexpected session state while applying an update" }, message: "Unexpected session state while applying an update" }],
    steps: [(ui) => ui.press(ui.t("error.issue.report"))],
  },
  {
    id: "toast-session-branches-not-removed",
    group: TRUST,
    category: "exceptional",
    title: "Branches and worktrees left after an archive",
    description: "Under the header saying these were to be removed and are still there, the danger alert groups the leftovers by reason under short lead lines: a locked worktree, its path shortened with ~, its row ending \"uncommitted changes\", and under \"Removing this failed:\" a branch git failed to delete, whose multi-line git message is a code block under its name. Recovery guidance follows. The archive itself stands.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "error",
      code: "session_branches_not_removed",
      params: {
        count: "2",
        items: JSON.stringify([
          { kind: "worktree", name: "/Users/dev/worktrees/website-changelog-tidy", branch: "changelog-tidy", state: "locked", uncommitted: true, detail: null },
          { kind: "branch", name: "fix/rename-index", branch: "fix/rename-index", state: "failed", uncommitted: false, detail: GIT_REFUSAL },
        ]),
      },
      message: "The worktree is locked; the branch could not be deleted",
    }],
  },
  {
    id: "toast-session-branches-not-removed-locked",
    group: TRUST,
    category: "exceptional",
    title: "Only locked worktrees left after an archive",
    description: "With a single group of leftovers the alert has no header over it: only the lead line of its group (the worktrees are locked), over a worktree with a branch chip and one on a plain detached HEAD, then the recovery guidance.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "error",
      code: "session_branches_not_removed",
      params: {
        count: "2",
        items: JSON.stringify([
          { kind: "worktree", name: "/Users/dev/worktrees/search-api-ranking", branch: "feat/ranking", state: "locked", uncommitted: false, detail: null },
          { kind: "worktree", name: "/Users/dev/worktrees/search-api-spike", branch: null, state: "locked", uncommitted: false, detail: null },
        ]),
      },
      message: "The worktrees are locked",
    }],
  },
  {
    id: "toast-project-branches-not-determined",
    group: TRUST,
    category: "exceptional",
    title: "Project removed, what went with its folder not found out",
    description: "The danger alert names the removed project as a chip and says the worktrees outside its folder, and its branches if the repository stays, were left, with git's reason in a code block below the sentence, then its own recovery guidance. The project itself is gone.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "error",
      code: "project_branches_not_determined",
      params: { name: "Docs Site", detail: "fatal: not a git repository: '/Users/dev/code/docs-site/.git'" },
      message: "",
    }],
  },
  {
    id: "toast-running-session-titles",
    group: TRUST,
    category: "exceptional",
    title: "Archiving refused, running session names",
    description: "Each running session is a separate chip, including names containing commas and quotation marks.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions: sessions.map((session) => session.id === "s-web-1"
        ? { ...session, title: 'Review "draft", then release' }
        : session),
    }),
    toasts: [{
      kind: "error",
      code: "session_has_running_sessions",
      params: {
        count: "2",
        sessions: '"Review "draft", then release", "Update the dependencies"',
        session_ids: JSON.stringify(["s-web-1", "s-web-2"]),
      },
      message: "",
    }],
  },
  {
    id: "toast-launch-path",
    group: TRUST,
    category: "exceptional",
    title: "Launch failed, nested path in its reason",
    description: "The session title and the configuration path inside the failure reason are chips.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "error",
      code: "launch_failed",
      params: {
        title: "Draft the release notes",
        reason_code: "config_dir_unreachable",
        agent: "Claude Code",
        path: "/Users/dev/.claude-work",
        reason: "Permission denied",
      },
      message: "",
    }],
  },
  {
    id: "toast-shell-timeout",
    group: TRUST,
    category: "exceptional",
    title: "Shell environment timeout alert",
    description: "The error alert presents the shell command as selectable code, then the cause, launch consequence and recovery.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{ kind: "error", ...SHELL_TIMEOUT }],
  },
  {
    id: "toast-notice",
    group: TRUST,
    category: "exceptional",
    title: "Warning notices and recovery",
    description: "Deliberate warning notices: an untrusted workspace workaround naming its allow and deny settings as code and a trust entry blocked by a locked store. Session links identify where recovery is needed. The trust file's path is set as code.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "notice", code: "claude_workspace_untrusted", params: {}, message: "", session: "s-web-1" },
      {
        kind: "notice",
        code: "trust_not_carried_over",
        params: {
          agent: "Grok Build",
          path: "/Users/dev/.grok/trusted_folders.toml",
          reason: "the file stayed locked by another program",
          reason_code: "store_locked",
        },
        message: "",
        session: grokSession.id,
      },
    ],
  },
];
