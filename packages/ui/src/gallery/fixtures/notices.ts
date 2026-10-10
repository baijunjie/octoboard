import type { Scenario } from "../scenario";
import { SAMPLE, sessionOf, snapshotState } from "./builders";

const TRUST = "Trust prompts and toasts";

const { console: console_, web, api } = SAMPLE;
// A session of each of the other two agents, for the prompts and notices that name the agent.
const codexSession = sessionOf("s-api-codex", console_.id, api.id, "Review the retry logic", "working", {
  agent: "codex",
});
const grokSession = sessionOf("s-web-grok", console_.id, web.id, "Tidy the stylesheet", "working", { agent: "grok" });
const sessions = [...SAMPLE.sessions, codexSession, grokSession];

export const noticeScenarios: Scenario[] = [
  {
    id: "trust-prompt",
    group: TRUST,
    title: "Trust prompt, a second one queued",
    description:
      "Claude Code asking, with the parent-folder option; the project's folder is shown, the second prompt waits behind it.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      trustPrompts: [
        { session: "s-web-1", agent: "claude", project: web.id, path: web.path, trustDir: "/Users/dev/code" },
        { session: codexSession.id, agent: "codex", project: api.id, path: api.path, trustDir: null },
      ],
    }),
  },
  {
    id: "trust-prompt-codex",
    group: TRUST,
    title: "Trust prompt, Codex asking",
    description: "Codex's caution and the note that it records the trust for the repository root.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      trustPrompts: [
        { session: codexSession.id, agent: "codex", project: api.id, path: api.path, trustDir: "/Users/dev/code" },
      ],
    }),
  },
  {
    id: "trust-prompt-grok",
    group: TRUST,
    title: "Trust prompt, Grok Build asking",
    description: "Grok Build's caution, without the parent-folder option.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      trustPrompts: [{ session: grokSession.id, agent: "grok", project: web.id, path: web.path, trustDir: null }],
    }),
  },
  {
    id: "console-session-request",
    group: TRUST,
    title: "Request for a console session, a second one queued",
    description:
      "A project session asking for a console session; a second request waits behind it.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api],
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
    title: "Error toasts",
    description:
      "Daemon error codes worded from the catalog, two naming a session (one an approval given after the session stopped waiting), one with a code the catalog lacks.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    toasts: [
      { kind: "error", code: "path_not_found", params: { path: "/Users/dev/code/missing" }, message: "" },
      { kind: "error", code: "session_not_running", params: {}, message: "", session: "s-web-2" },
      { kind: "error", code: "console_request_not_waiting", params: { session: "s-web-1" }, message: "", session: "s-web-1" },
      { kind: "error", code: "a_code_from_a_newer_daemon", params: {}, message: "The daemon's own English text, shown as is." },
    ],
  },
  {
    id: "toast-notice",
    group: TRUST,
    title: "Notice toasts",
    description:
      "Session notices: a setting Octoboard had to work around, a failed press of a trust confirmation, a trust entry that could not be carried over, and a long text.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    toasts: [
      { kind: "notice", code: "claude_workspace_untrusted", params: {}, message: "", session: "s-web-1" },
      {
        kind: "notice",
        code: "trust_answer_failed",
        params: { agent: "Codex", reason_code: "cursor_moved_away" },
        message: "",
        session: codexSession.id,
      },
      { kind: "notice", code: "queued_messages_dropped", params: {}, message: "", session: "s-web-2" },
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
