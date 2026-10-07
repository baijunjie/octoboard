import type { Scenario } from "../scenario";
import { SAMPLE, snapshotState } from "./builders";

const TRUST = "Trust and toasts";

const { console: console_, web, api, sessions } = SAMPLE;

export const noticeScenarios: Scenario[] = [
  {
    id: "trust-prompt",
    group: TRUST,
    title: "Trust prompt, a second one queued",
    description: "The dialog with the parent-folder option; the project's folder is shown, the second prompt waits behind it.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api],
      sessions,
      trustPrompts: [
        { session: "s-web-1", project: web.id, path: web.path, trustDir: "/Users/dev/code" },
        { session: "s-api-1", project: api.id, path: api.path, trustDir: null },
      ],
    }),
  },
  {
    id: "toast-error",
    group: TRUST,
    title: "Error toasts",
    description: "Daemon error codes worded from the catalog, one naming a session, one with a code the catalog lacks.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    toasts: [
      { kind: "error", code: "path_not_found", params: { path: "/Users/dev/code/missing" }, message: "" },
      { kind: "error", code: "session_not_running", params: {}, message: "", session: "s-web-2" },
      { kind: "error", code: "a_code_from_a_newer_daemon", params: {}, message: "The daemon's own English text, shown as is." },
    ],
  },
  {
    id: "toast-notice",
    group: TRUST,
    title: "Notice toasts",
    description: "Session notices: a setting Octoboard had to work around, and a long text.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    toasts: [
      { kind: "notice", code: "claude_workspace_untrusted", params: {}, message: "", session: "s-web-1" },
      {
        kind: "notice",
        code: "claude_trust_answer_failed",
        params: { reason_code: "cursor_moved_away" },
        message: "",
        session: "s-api-1",
      },
      { kind: "notice", code: "queued_messages_dropped", params: {}, message: "", session: "s-web-2" },
    ],
  },
];
