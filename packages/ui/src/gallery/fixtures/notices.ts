import type { Account, AccountQuota, AccountSharing, Settings } from "../../protocol";
import type { SessionRequest } from "../../store";
import type { Scenario } from "../scenario";
import { ALL_AGENTS, SAMPLE, consoleOf, projectOf, sessionOf, snapshotState } from "./builders";
import { quotaOf, quotaWindow } from "./quotas";
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

/** What `requestOf` takes of one kind of request: all of it but what the daemon always sends and the scenarios rarely
 * vary, with the branches optional. Distributed over the kinds, so each action asks for exactly its own fields. */
type RequestFields<Request> = Request extends unknown
  ? Omit<Request, "requestId" | "branches" | "requestedAt"> & { branches?: string[] }
  : never;

/** A session request as the store holds it for what the daemon sent: the fields every request carries are set, and
 * `fields` gives the rest, which the contract requires per action (a request that starts a session carries no target,
 * owner, conversation or account, and no archived owners). */
const requestOf = (fields: RequestFields<SessionRequest>): SessionRequest => ({
  requestId: "r-1",
  branches: [],
  requestedAt: 1,
  ...fields,
});

// The accounts the requests that resume a conversation name: Work, and Client, whose last sharing change did not finish.
const accounts: Account[] = [
  { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
  { id: "a-client", agent: "claude", name: "Client", config_dir: "/Users/dev/.claude-client", share_default_config_incomplete: true },
];
const resumeSettings: Settings = {
  auto_pull_repositories: false,
  auto_delete_unresumable_archives: true,
  added_agents: ALL_AGENTS,
  default_clone_dir: "/Users/dev/Projects",
  accounts,
};
const onAccount = (account: Account) => ({ account_id: account.id, config_dir: account.config_dir });

// Sessions a request to reopen or resume names: an archived one that reports to an archived console session, an
// interrupted one on Work, an archived one on Client, and a chain whose names have no break opportunity: the target and
// its owner are project sessions of Search API, the owner a lead session that reports to an archived console session.
const archivedOwner = sessionOf("s-owner-archived", console_.id, undefined, "Release coordinator", "archived");
const reopenTarget = sessionOf("s-reopen-target", console_.id, api.id, "Add idempotency keys", "archived", { bound_to: archivedOwner.id });
const resumeTarget = sessionOf("s-resume-target", console_.id, web.id, "Port the importer to the new parser", "interrupted", onAccount(accounts[0]));
const incompleteTarget = sessionOf("s-reopen-client", console_.id, web.id, "Verify the migration", "archived", onAccount(accounts[1]));
const LONG_NAME = "Review-the-release-readiness-checklist-for-the-production-deployment-compatibility-and-rollback-plan";
const longTop = sessionOf("s-owner-top", console_.id, undefined, `${LONG_NAME}-coordinator`, "archived");
const longOwner = sessionOf("s-owner-long", console_.id, api.id, `${LONG_NAME}-owner`, "archived", { bound_to: longTop.id });
const longTarget = sessionOf("s-reopen-long", console_.id, api.id, `${LONG_NAME}-worker`, "archived", { bound_to: longOwner.id });
const resumeSessions = [...sessions, archivedOwner, reopenTarget, resumeTarget, incompleteTarget, longTop, longOwner, longTarget];
const waiting = (id: string) => (session: (typeof sessions)[number]) => session.id === id ? { ...session, status: "waiting_user" as const } : session;
const resumeState = (requesting: string, request: SessionRequest, settings?: Settings) => snapshotState({
  consoles: [console_],
  projects: [web, api, grokProject],
  sessions: resumeSessions.map(waiting(requesting)),
  sessionRequests: [request],
  settings,
});

// Requests to switch or hand off a session the requester owns: the console session asks about Verify the migration, a
// Claude Code session of Website bound to it and running on the default account, mid-turn for a switch and idle for a
// handoff, which the daemon refuses while the session works. Claude Code has Work (its session window used up),
// Personal (quota left) and Client (its sharing change is running in some scenarios, unfinished in another), Codex has
// Team.
const ownedSession = sessionOf("s-owned-session", console_.id, web.id, "Verify the migration", "working", { bound_to: SAMPLE.consoleSession.id });
const idleOwnedSession = { ...ownedSession, status: "idle" as const };
const startingOwnedSession = { ...ownedSession, status: "starting" as const };
const archivedOwnedSession = { ...ownedSession, status: "archived" as const };
const moveAccounts: Account[] = [
  { id: "a-work", agent: "claude", name: "Work", config_dir: "/Users/dev/.claude-work" },
  { id: "a-personal", agent: "claude", name: "Personal", config_dir: "/Users/dev/.claude-personal" },
  { id: "a-client", agent: "claude", name: "Client", config_dir: "/Users/dev/.claude-client" },
  { id: "a-team", agent: "codex", name: "Team", config_dir: "/Users/dev/.codex-team" },
];
const moveQuotas: AccountQuota[] = [
  quotaOf("claude", null, "max", [quotaWindow("session", 22, 190), quotaWindow("weekly", 41, 3 * 24 * 60 + 4 * 60)]),
  quotaOf("claude", "a-work", "pro", [quotaWindow("session", 100, 125), quotaWindow("weekly", 63, 2 * 24 * 60 + 5 * 60)]),
  quotaOf("claude", "a-personal", "pro", [quotaWindow("session", 36, 40), quotaWindow("weekly", 48, 24 * 60 + 90)]),
  quotaOf("codex", null, "plus", [quotaWindow("session", 20, 95), quotaWindow("weekly", 52, 5 * 24 * 60)]),
  quotaOf("codex", "a-team", "pro", [quotaWindow("session", 30, 200), quotaWindow("weekly", 41, 3 * 24 * 60 + 60)]),
];
const moveState = (request: SessionRequest, options: { target?: typeof ownedSession; accountSharing?: AccountSharing[]; accounts?: Account[]; quotas?: AccountQuota[] } = {}) => snapshotState({
  consoles: [console_],
  projects: [web, api, grokProject],
  sessions: [...sessions.map(waiting(SAMPLE.consoleSession.id)), options.target ?? ownedSession],
  sessionRequests: [request],
  settings: { ...resumeSettings, accounts: options.accounts ?? moveAccounts },
  accountQuotas: options.quotas ?? moveQuotas,
  accountSharing: options.accountSharing,
});
// The daemon names the target's own project in a request, whichever session asks.
const switchRequestTo = (account: string | null) => requestOf({
  action: "switch",
  session: SAMPLE.consoleSession.id,
  role: "project",
  console: console_.id,
  project: web.id,
  agent: "claude",
  target: ownedSession.id,
  account,
});
const handoffRequestOn = (account: string) => requestOf({
  action: "handoff",
  session: SAMPLE.consoleSession.id,
  role: "project",
  console: console_.id,
  project: web.id,
  agent: "codex",
  target: ownedSession.id,
  account,
  model: "gpt-5.6-sol",
  effort: "high",
});
const handoffRequest = handoffRequestOn("a-team");
// The quotas with Team's session window used up.
const teamUsedUpQuotas = moveQuotas.map((quota) => quota.account === "a-team" ? quotaOf("codex", "a-team", "pro", [quotaWindow("session", 100, 125), quotaWindow("weekly", 41, 3 * 24 * 60 + 60)]) : quota);

export const noticeScenarios: Scenario[] = [
  {
    id: "trust-prompt",
    group: TRUST,
    category: "normal",
    title: "Queued trust prompts for all agents",
    description: "Claude Code and Codex offer their projects' parent folders, while the Grok Build project is the home directory itself, whose parent is too broad to offer. The warning, titled \"The project's own configuration takes effect\", has its configuration-file token take its tone while the project path stays neutral. Each agent has its own caution, and Codex names its repository-root trust. All three asking sessions raise their hands. Not now advances through the queued prompts while the hands stay raised until trust is answered in the terminal.",
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
        requestOf({ action: "start", session: "s-web-1", role: "project", console: console_.id, project: web.id, agent: "claude" }),
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
        requestOf({
          action: "start",
          session: "s-api-1",
          role: "project",
          console: console_.id,
          project: worktreeProject.id,
          agent: "grok",
          branches: ["feature/saved-searches", "fix/coverage-at-100%25", "feature/rework-the-suggestion-index-so-that-prefixes-are-matched-before-fuzzy-candidates"],
        }),
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
        requestOf({ action: "start", session: SAMPLE.consoleSession.id, role: "console", console: console_.id, project: null, agent: "codex" }),
      ],
    }),
  },
  {
    id: "session-request-reopen",
    group: TRUST,
    category: "normal",
    title: "Request to reopen an archived session with its owner",
    description: "A session in Website asks to reopen Add idempotency keys, an archived Claude Code session of Search API on the default account. The question names the session, its account, project and console as chips, says it reports to its owner (named) and that the owner, archived too, would be reopened with it. The title and the confirm button say Reopen.",
    state: resumeState("s-web-1", requestOf({
      session: "s-web-1",
      action: "reopen",
      role: "project",
      console: console_.id,
      project: api.id,
      agent: "claude",
      target: reopenTarget.id,
      owner: archivedOwner.id,
      reopenedOwners: [archivedOwner.id],
    })),
  },
  {
    id: "session-request-resume",
    group: TRUST,
    category: "normal",
    title: "Request to resume an interrupted session",
    description: "The console session asks to resume Port the importer to the new parser, an interrupted Claude Code session of Website on the account Work. It has no owner, so the question ends in that it reports to you. The title and the confirm button say Resume.",
    state: resumeState(SAMPLE.consoleSession.id, requestOf({
      session: SAMPLE.consoleSession.id,
      action: "resume",
      role: "project",
      console: console_.id,
      project: web.id,
      agent: "claude",
      target: resumeTarget.id,
      reopenedOwners: [],
    }), resumeSettings),
  },
  {
    id: "session-request-resume-record",
    group: TRUST,
    category: "normal",
    title: "Request to continue a conversation only an account's record holds",
    description: "A session in Search API asks to continue a conversation no session holds, kept in the record of the account Work in Website. The question gives the conversation's id as a code chip, then the agent, account, project and console, and says the new session would report to you. The title says Resume a conversation and the confirm button Resume conversation.",
    state: resumeState("s-api-1", requestOf({
      session: "s-api-1",
      action: "resume_record",
      role: "project",
      console: console_.id,
      project: web.id,
      agent: "claude",
      account: "a-work",
      conversation: "3f9d2c1e-6b47-4a85-9e0d-71c2a8b45f16",
    }), resumeSettings),
  },
  {
    id: "session-request-resume-record-default",
    group: TRUST,
    category: "normal",
    title: "Request to continue a conversation the default account's record holds",
    description: "As the previous one, for a Codex conversation kept in the record of Codex's default account, which the question names Default, in Search API.",
    state: resumeState("s-web-1", requestOf({
      session: "s-web-1",
      action: "resume_record",
      role: "project",
      console: console_.id,
      project: api.id,
      agent: "codex",
      account: null,
      conversation: "0199c3a2-7e41-7b52-9d0e-4f6a1c85b2d7",
    })),
  },
  {
    id: "session-request-switch",
    group: TRUST,
    category: "normal",
    title: "Request to switch a session's account",
    description: "The console session asks to switch Verify the migration, its Claude Code session, to the account Personal. The form under the question has Personal chosen in the Switch to picker, with the model and reasoning effort below it and a note that a change to them is kept for the session. Confirm reads Switch account and is enabled.",
    state: moveState(switchRequestTo("a-personal")),
  },
  {
    id: "session-request-handoff",
    group: TRUST,
    category: "normal",
    title: "Request to hand off a session",
    description: "The console session asks to hand off Verify the migration, which is idle, to a new Codex session on the account Team. Under the question the agent and account picker stands on Codex and Team, with the Advanced section open on the model GPT-5.6-Sol and the reasoning effort the call asked for. Nothing the asking session wrote is shown. Confirm reads Hand off session and is enabled.",
    state: moveState(handoffRequest, { target: idleOwnedSession }),
    steps: [(ui) => ui.press(ui.t("dialog.agentSettings.advanced"))],
  },
  {
    id: "session-request-reopen-long-names",
    group: TRUST,
    category: "normal",
    title: "Request to reopen a session whose owners have long names",
    description: "The reopened session and both archived owners it would bring back have names with no break opportunity, which wrap inside their chips and the sentence; the owners are listed in the plural form, topmost first, and the one it reports to is named after \"reports to\".",
    state: resumeState("s-web-1", requestOf({
      session: "s-web-1",
      action: "reopen",
      role: "project",
      console: console_.id,
      project: api.id,
      agent: "claude",
      target: longTarget.id,
      owner: longOwner.id,
      reopenedOwners: [longTop.id, longOwner.id],
    })),
  },
  {
    id: "session-request-reopen-incomplete",
    group: TRUST,
    category: "exceptional",
    title: "Request to reopen a session on an account whose sharing change did not finish",
    description: "Reopening Verify the migration would launch it on Client, whose last sharing change did not finish, so a warning sits between the question and the buttons, titled with the account's name (\"The last sharing change of Client did not finish\") and saying under it that its configuration and conversations may be incomplete. The buttons stay Refuse and Reopen session: the dialog itself is the confirmation.",
    state: resumeState("s-api-1", requestOf({
      session: "s-api-1",
      action: "reopen",
      role: "project",
      console: console_.id,
      project: web.id,
      agent: "claude",
      target: incompleteTarget.id,
      reopenedOwners: [],
    }), resumeSettings),
  },
  {
    id: "session-request-reopen-unknown",
    group: TRUST,
    category: "exceptional",
    title: "Request to reopen a session the app does not know",
    description: "The session to reopen is not among the sessions this app has, so the question gives its id as a code chip where the name would be and a dash chip where its account would be. The owner it reports to is named as usual.",
    state: resumeState("s-web-1", requestOf({
      session: "s-web-1",
      action: "reopen",
      role: "project",
      console: console_.id,
      project: api.id,
      agent: "claude",
      target: "s-reopen-unknown",
      owner: SAMPLE.consoleSession.id,
      reopenedOwners: [],
    })),
  },
  {
    id: "session-request-switch-used-up",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account whose quota is used up",
    description: "The account asked for, Work, has its session quota used up. The picker still stands on Work, and an error above it says the quota is used up and when it resets. Confirm is disabled until another account is chosen.",
    state: moveState(switchRequestTo("a-work")),
  },
  {
    id: "session-request-switch-sharing-change",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account whose sharing change is running",
    description: "The account asked for, Client, is having its sharing changed. The picker stands on Client, and an error above it says its sharing is changing, so the session cannot move to it yet. Confirm is disabled until another account is chosen.",
    state: moveState(switchRequestTo("a-client"), { accountSharing: [{ account: "a-client", progress: { share: true, phase: "changing", done: 2, total: 9, item: "skills/" } }] }),
  },
  {
    id: "session-request-switch-disabled",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account that is turned off",
    description: "The account asked for, Personal, was turned off in Settings while the request waited (a request for an account that is already off is refused before it is asked). An error above the picker, titled Account is disabled, says a session cannot use Personal and to choose another account or turn it back on in Settings. The picker stands on Personal, whose entry in the open list reads Unavailable while this account is disabled. Confirm is disabled until another account is chosen.",
    state: moveState(switchRequestTo("a-personal"), { accounts: moveAccounts.map((account) => account.id === "a-personal" ? { ...account, disabled: true } : account) }),
  },
  {
    id: "session-request-switch-account-gone",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account that no longer exists",
    description: "The account asked for was removed after the request was made. The picker is empty with its placeholder, and an error above it says the account no longer exists and asks to choose another. Confirm is disabled until one is chosen.",
    state: moveState(switchRequestTo("a-removed")),
  },
  {
    id: "session-request-switch-incomplete",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account whose sharing change did not finish",
    description: "The account asked for, Client, had its last sharing change left unfinished, so a warning line sits between the form and the buttons, naming the account and saying its configuration and conversations may be incomplete. Confirm stays Switch account and is enabled: the dialog itself is the confirmation.",
    state: moveState(switchRequestTo("a-client"), { accounts: moveAccounts.map((account) => account.id === "a-client" ? { ...account, share_default_config_incomplete: true } : account) }),
  },
  {
    id: "session-request-switch-starting",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch a session that is still starting",
    description: "The session to switch has not finished starting. An error above the picker says it cannot be switched yet, and Confirm is disabled, though Personal is chosen and available.",
    state: moveState(switchRequestTo("a-personal"), { target: startingOwnedSession }),
  },
  {
    id: "session-request-switch-archived",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch a session that was archived",
    description: "The session to switch was archived while the request waited. An error above the picker says an archived session cannot be switched, and Confirm is disabled, though Personal is chosen and available.",
    state: moveState(switchRequestTo("a-personal"), { target: archivedOwnedSession }),
  },
  {
    id: "session-request-handoff-used-up",
    group: TRUST,
    category: "exceptional",
    title: "Request to hand off to an account whose quota is used up",
    description: "The account asked for, Team, has its session quota used up. The picker still stands on Team, and an error above it says the quota is used up and when it resets. Confirm is disabled until another account is chosen.",
    state: moveState(handoffRequestOn("a-team"), { target: idleOwnedSession, quotas: teamUsedUpQuotas }),
  },
  {
    id: "session-request-handoff-choice-gone",
    group: TRUST,
    category: "exceptional",
    title: "Request to hand off to an account that no longer exists",
    description: "The account asked for was removed after the request was made, so the question names it by its id. The agent and the account pickers are both empty, each with its placeholder, and an error above them asks to choose again. Confirm is disabled until an agent or account is chosen.",
    state: moveState(handoffRequestOn("a-removed"), { target: idleOwnedSession }),
  },
  {
    id: "session-request-handoff-working",
    group: TRUST,
    category: "exceptional",
    title: "Request to hand off a session that has started working",
    description: "The session to hand off started a turn while the request waited. An error above the picker says the session is in the middle of a turn and cannot be handed off yet. Confirm is disabled, and Refuse is the way out.",
    state: moveState(handoffRequest),
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
    id: "toast-quick-create-owner-gone",
    group: TRUST,
    category: "exceptional",
    title: "Quick create refused, the named owner is no longer available",
    description: "The quick-create race has no durable form. Its acknowledgement keeps the former owner's full mixed-script task and session title as a wrapping name chip; the plain error sentence remains available to system surfaces.",
    state: snapshotState({ consoles: [console_], projects: [web], sessions: [] }),
    toasts: [{
      kind: "error",
      code: "quick_create_owner_gone",
      params: { name: "Review release readiness مرحبا 東京 (UnbrokenOwnerNameForProductionDeploymentCompatibilityReview)" },
      message: "The owner is no longer available",
    }],
  },
  {
    id: "toast-message-not-sent",
    group: TRUST,
    category: "exceptional",
    title: "Message written but not sent by the agent",
    description: "An error alert tied to a console session, Hub, so its title is a sentence with the console's name, Main, as a chip in it, not a project name: it names the agent, says the message waits in the session's input box and that nothing more is written into the session until a prompt is sent, then gives the recovery of opening the session and pressing Enter.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{ kind: "error", code: "message_not_sent", params: { agent: "Claude Code" }, message: "", session: SAMPLE.consoleSession.id }],
  },
  {
    id: "toast-message-notices",
    group: TRUST,
    category: "exceptional",
    title: "Message discarded by a dialog or diverted by a view",
    description: "Two notice toasts, not alerts, stacked and tied to the Claude Code session; each names the agent. One says a dialog came up while the message was written and discarded it, and that the message is written again once the dialog has been answered. The other says the message went into a view of its background agents rather than the conversation, and asks the person to check the session; it does not say the message will be written again.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "notice", code: "message_discarded_by_dialog", params: { agent: "Claude Code" }, message: "", session: "s-web-1" },
      { kind: "notice", code: "message_diverted_by_view", params: { agent: "Claude Code" }, message: "", session: "s-web-1" },
    ],
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
    description: "The opener failure takes precedence over the original unexpected error. Its actionable recovery supplies the manual issue URL, alone on its line as plain monospace text without a chip, and has no recursive Report issue link. OK returns to the original failure.",
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
    description: "Under the header saying these were to be removed and are still there, the danger alert groups the leftovers by reason under short lead lines: a locked worktree, its path shortened with ~ as plain monospace text, its row ending with a soft warning \"uncommitted changes\" badge, and under \"Removing this failed:\" a branch git failed to delete, whose multi-line git message is a code block under its name. Recovery guidance follows. The archive itself stands.",
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
    description: "With a single group of leftovers the alert has no header over it: only the lead line of its group (the worktrees are locked), over a worktree with its branch (a branch icon and plain monospace name) and one on a plain detached HEAD, then the recovery guidance.",
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
    description: "The error alert presents the shell command as selectable code, then the cause, launch consequence and recovery as separate readable parts.",
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
