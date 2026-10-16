import type { Scenario } from "../scenario";
import { SAMPLE, projectOf, sessionOf, snapshotState } from "./builders";
import { SHELL_TIMEOUT } from "./shellTimeout";

const TRUST = "Trust prompts and toasts";

const { console: console_, web, api } = SAMPLE;
// A session of each of the other two agents, for the prompts and notices that name the agent.
const codexSession = sessionOf("s-api-codex", console_.id, api.id, "Review the retry logic", "working", {
  agent: "codex",
});
const grokProject = projectOf("p-grok-home", console_.id, "Home", { path: "/Users/dev" });
const grokSession = sessionOf("s-web-grok", console_.id, grokProject.id, "Tidy the stylesheet", "working", { agent: "grok" });
const sessions = [...SAMPLE.sessions, codexSession, grokSession];

export const noticeScenarios: Scenario[] = [
  {
    id: "trust-prompt",
    group: TRUST,
    category: "normal",
    title: "Queued trust prompts for all agents",
    description:
      "Claude Code and Codex offer their projects' parent folders; the Grok Build project is the home directory itself, whose parent is too broad to offer. The warning's configuration-file token takes its tone while the project path stays neutral. Each agent has its own caution and Codex names its repository-root trust. Not now advances through the three queued prompts.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api, grokProject],
      sessions,
      trustPrompts: [
        { session: "s-web-1", agent: "claude", project: web.id, path: web.path, trustDir: "/Users/dev/code" },
        { session: codexSession.id, agent: "codex", project: api.id, path: api.path, trustDir: "/Users/dev/code" },
        { session: grokSession.id, agent: "grok", project: grokProject.id, path: grokProject.path, trustDir: null },
      ],
    }),
  },
  {
    id: "console-session-request",
    group: TRUST,
    category: "normal",
    title: "Request for a console session, a second one queued",
    description:
      "A project session asking for a console session; a second request waits behind it.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api, grokProject],
      sessions,
      consoleRequests: [
        {
          requestId: "r-1",
          session: "s-web-1",
          console: console_.id,
          project: web.id,
          requestedAt: 1,
        },
        {
          requestId: "r-2",
          session: codexSession.id,
          console: console_.id,
          project: api.id,
          requestedAt: 2,
        },
      ],
    }),
  },
  {
    id: "toast-error",
    group: TRUST,
    category: "exceptional",
    title: "Error toasts",
    description:
      "Daemon error codes worded from the catalog: the missing folder's path is set as code in the toast's title, one toast names a session (an approval given after the session stopped waiting), and one has a code the catalog lacks.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "error", code: "path_not_found", params: { path: "/Users/dev/code/missing" }, message: "" },
      { kind: "error", code: "console_request_not_waiting", params: { session: "s-web-1" }, message: "", session: "s-web-1" },
      { kind: "error", code: "a_code_from_a_newer_daemon", params: {}, message: "The daemon's own English text, shown as is." },
    ],
  },
  {
    id: "toast-shell-timeout",
    group: TRUST,
    category: "exceptional",
    title: "Shell environment timeout toast",
    description: "One error toast presents the shell command as selectable code, then the cause, launch consequence and recovery.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{ kind: "error", ...SHELL_TIMEOUT }],
  },
  {
    id: "toast-notice",
    group: TRUST,
    category: "exceptional",
    title: "Warning notices and recovery",
    description:
      "Deliberate warning notices: an untrusted workspace workaround naming its allow and deny settings as code, a failed trust confirmation, a trust entry blocked by a locked store and a session whose launch failed after it appeared (named by its title, with no session link, since it is gone). Session links identify where recovery is needed, and the trust file's path is set as code.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "notice", code: "claude_workspace_untrusted", params: {}, message: "", session: "s-web-1" },
      {
        kind: "notice",
        code: "trust_answer_failed",
        params: { agent: "Codex", reason_code: "cursor_moved_away" },
        message: "",
        session: codexSession.id,
      },
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
      {
        kind: "notice",
        code: "launch_failed",
        params: {
          title: "Draft the release notes",
          reason: "`codex` was not found on PATH in the snapshotted shell environment",
          reason_code: "binary_not_found",
          binary: "codex",
        },
        message: "",
      },
      {
        kind: "notice",
        code: "launch_failed",
        params: {
          title: "Draft the release notes",
          reason: "the shell environment did not finish within 10s",
          reason_code: "shell_environment_timeout",
          shell: "/bin/zsh",
          command: "env -0 && printf octoboard-env-end",
          timeout: "10s",
        },
        message: "",
      },
    ],
  },
];
