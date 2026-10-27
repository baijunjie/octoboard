import { DEFAULT_ACCOUNT_NAME, type Account, type AccountQuota, type AccountSharing, type Settings } from "../../protocol";
import type { SessionRequest } from "../../store";
import type { Scenario } from "../scenario";
import { ALL_AGENTS, SAMPLE, consoleOf, failureTitle, projectOf, sessionOf, snapshotState } from "./builders";
import { quotaFailure, quotaOf, quotaWindow } from "./quotas";
import { SHELL_TIMEOUT } from "./shellTimeout";
import { DEFAULT_GIT_WORKFLOW } from "../../gitWorkflow";

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
const LONG_TITLE_WORD = "ProductionDeploymentCompatibilityRollbackReadinessChecklist";
// A project session whose title is far longer than the others', with a word that cannot break.
const longTitled = sessionOf("s-web-long", console_.id, web.id, `Reconcile the summary layout with the ${LONG_TITLE_WORD} export and review the remaining overflow cases`, "idle");
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
  default_clone_dir: "/Users/dev/Projects", git_workflow: DEFAULT_GIT_WORKFLOW,
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

// The states of the scenarios on an account whose last sharing change did not finish: Client, reopening Verify the
// migration on it, and a switch to it.
const reopenOnIncompleteAccount = resumeState("s-api-1", requestOf({
  session: "s-api-1",
  action: "reopen",
  role: "project",
  console: console_.id,
  project: web.id,
  agent: "claude",
  target: incompleteTarget.id,
  reopenedOwners: [],
}), resumeSettings);
const switchToIncompleteAccount = moveState(switchRequestTo("a-client"), { accounts: moveAccounts.map((account) => account.id === "a-client" ? { ...account, share_default_config_incomplete: true } : account) });

export const noticeScenarios: Scenario[] = [
  {
    id: "trust-prompt",
    group: TRUST,
    category: "normal",
    title: "Queued trust prompts for all agents",
    description: "Claude Code and Codex offer their projects' parent folders as an unchecked \"Trust every project under ~/code\" option, whose description reads \"Once checked, the folder is trusted instead of this project alone.\" and with no alert under it, while the Grok Build project is the home directory itself, whose parent is too broad to offer, so its dialog has no such option. The caution is plain body text, with no Alert and no \"Caution:\" prefix: a single sentence for that agent, whose configuration-file token and the project path are neutral chips. Each agent has its own caution, and Codex names its repository-root trust. All three asking sessions raise their hands. Not now advances through the queued prompts while the hands stay raised until trust is answered in the terminal.",
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
    id: "trust-prompt-parent-checked",
    group: TRUST,
    category: "normal",
    title: "Trust prompt with the whole parent folder chosen",
    description: "\"Trust every project under ~/code\" is checked. Its description now reads \"The folder will be trusted instead of this project alone.\" and a warning alert stands directly under the option, titled \"Projects added there later will be trusted too.\": the description says trust covers projects at any depth under the folder, for every agent, and that their own permission rules, hooks and instructions take effect; a separate line beneath says \"Trust only a folder whose contents you control.\" The caution above the option is unchanged.",
    state: snapshotState({
      consoles: [console_],
      projects: [web],
      sessions: trustSessions.filter((session) => session.project_id === web.id || session.role === "console"),
      trustPrompts: [{ session: "s-web-1", agent: "claude", project: web.id, path: web.path, trustDir: "/Users/dev/code" }],
    }),
    steps: [(ui) => ui.press(ui.t("dialog.trust.parent", { directory: "~/code" }))],
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
    id: "toast-account-switched-automatically",
    group: TRUST,
    category: "normal",
    title: "Session switched to another account automatically",
    description: "Two notice toasts, not alerts, stacked (hover the stack to spread it) and tied to the Claude Code session. Each says the account the session ran on reached its usage limit, so the app switched the session, named by its title as a chip, to another account, naming the agent in parentheses after the account it left; both account names are chips. One switches from account Work to Personal; the other leaves the agent's default account, whose chip reads \"Default\" here and 默认 in Simplified Chinese, so check it with the language control too.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "session_notice", code: "account_switched_automatically", params: { agent: "Claude Code", from: "Work", account: "Personal", session: "s-web-1" }, message: "", session: "s-web-1" },
      { kind: "session_notice", code: "account_switched_automatically", params: { agent: "Claude Code", from: DEFAULT_ACCOUNT_NAME, account: "Work", session: "s-web-1" }, message: "", session: "s-web-1" },
    ],
  },
  {
    id: "session-request-reopen-incomplete",
    group: TRUST,
    category: "exceptional",
    title: "Request to reopen a session on an account whose sharing change did not finish",
    description: "Reopening Verify the migration would launch it on Client, whose last sharing change did not finish. A small warning mark sits in the question right after the account's chip, Client; no alert stands between the question and the buttons, which stay Refuse and Reopen session: the dialog itself is the confirmation.",
    state: reopenOnIncompleteAccount,
  },
  {
    id: "session-request-reopen-incomplete-details",
    group: TRUST,
    category: "exceptional",
    title: "Request to reopen a session on an account whose sharing change did not finish, mark opened",
    description: "The warning mark after Client in the question is hovered, and its popover reads \"The last sharing change of Client did not finish.\" with \"Its configuration and conversations may be incomplete.\" under it. The dialog stays open behind it with its buttons unchanged.",
    state: reopenOnIncompleteAccount,
    steps: [(ui) => ui.hoverStatusMark(ui.t("dialog.sessionRequest.incomplete.mark"))],
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
    title: "Request to switch to an account whose usage limit is reached",
    description: "The account asked for, Work, has its session limit reached. The picker still stands on Work and the Switch to field is marked invalid, with its own error under the picker saying the usage limit is reached and when it resets. Confirm is disabled until another account is chosen.",
    state: moveState(switchRequestTo("a-work")),
  },
  {
    id: "session-request-switch-signed-out",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account that is not logged in",
    description: "The account asked for, Work, was found not logged in by its last reading. The picker stands on Work and the Switch to field is marked invalid, with its own error under the picker saying Account Work, the name a chip, is not logged in, so a session cannot use it, and to choose another account or log in to it. Confirm is disabled until another account is chosen.",
    state: moveState(switchRequestTo("a-work"), { quotas: moveQuotas.map((quota) => quota.account === "a-work" ? quotaOf("claude", "a-work", null, [], { unavailable: quotaFailure("quota_not_signed_in", { agent: "Claude Code" }, "Claude Code is not signed in: Error: not logged in") }) : quota) }),
  },
  {
    id: "session-request-switch-sharing-change",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account whose sharing change is running",
    description: "The account asked for, Client, is having its sharing changed. The picker stands on Client and the Switch to field is marked invalid, with its own error under the picker saying it is unavailable while its sharing is being changed. Confirm is disabled until another account is chosen.",
    state: moveState(switchRequestTo("a-client"), { accountSharing: [{ account: "a-client", progress: { share: true, phase: "changing", done: 2, total: 9, item: "skills/" } }] }),
  },
  {
    id: "session-request-switch-disabled",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account that is turned off",
    description: "The account asked for, Personal, was turned off in Settings while the request waited (a request for an account that is already off is refused before it is asked). The picker stands on Personal and the Switch to field is marked invalid, with its own error under the picker saying a session cannot use Personal, the name a chip, and to choose another account or enable it again in Settings; its entry in the open list is unselectable, with the short reason \"Disabled\" under its name. Confirm is disabled until another account is chosen.",
    state: moveState(switchRequestTo("a-personal"), { accounts: moveAccounts.map((account) => account.id === "a-personal" ? { ...account, disabled: true } : account) }),
  },
  {
    id: "session-request-switch-account-gone",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account that no longer exists",
    description: "The account asked for was removed after the request was made. The picker is empty with its placeholder and the Switch to field is marked invalid, with its own error under it saying the account no longer exists and asking to choose another. Confirm is disabled until one is chosen.",
    state: moveState(switchRequestTo("a-removed")),
  },
  {
    id: "session-request-switch-already-on",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch a session to the account it is on already",
    description: "The account asked for, Default, is the one the session already runs on. The picker stands on it and the Switch to field is marked invalid, with its own error under the picker saying the session is on Default already. Confirm is disabled until another account is chosen.",
    state: moveState(switchRequestTo(null)),
  },
  {
    id: "session-request-switch-incomplete",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account whose sharing change did not finish",
    description: "The account asked for, Client, had its last sharing change left unfinished. A small warning mark sits inside the Switch to picker at the end of the field, beside the chevron; no warning line stands between the form and the buttons. Confirm stays Switch account and is enabled: the dialog itself is the confirmation.",
    state: switchToIncompleteAccount,
  },
  {
    id: "session-request-switch-incomplete-details",
    group: TRUST,
    category: "exceptional",
    title: "Request to switch to an account whose sharing change did not finish, mark opened",
    description: "The warning mark inside the Switch to picker is hovered, and its popover reads \"The last sharing change of Client did not finish.\" with \"Its configuration and conversations may be incomplete.\" under it, over the form and the picker.",
    state: switchToIncompleteAccount,
    steps: [(ui) => ui.hoverStatusMark(ui.t("dialog.sessionRequest.incomplete.mark"))],
  },
  {
    id: "session-request-handoff-incomplete",
    group: TRUST,
    category: "exceptional",
    title: "Request to hand off to an account whose sharing change did not finish",
    description: "The account asked for, Team, had its last sharing change left unfinished. A small warning mark sits inside the account part of the joined Hand off to picker, at its end; no warning line stands above the buttons. Confirm stays Hand off session and is enabled: the dialog itself is the confirmation.",
    state: moveState(handoffRequestOn("a-team"), { target: idleOwnedSession, accounts: moveAccounts.map((account) => account.id === "a-team" ? { ...account, share_default_config_incomplete: true } : account) }),
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
    description: "The session to switch was archived while the request waited. The dialog itself turns into a failure modal: a danger icon in the header, the title Cannot switch the account, a sentence saying an archived session cannot be switched, and OK as its only button, which refuses the request.",
    state: moveState(switchRequestTo("a-personal"), { target: archivedOwnedSession }),
  },
  {
    id: "session-request-handoff-used-up",
    group: TRUST,
    category: "exceptional",
    title: "Request to hand off to an account whose usage limit is reached",
    description: "The account asked for, Team, has its session limit reached. The picker still stands on Team and the Hand off to field is marked invalid, with its own error under the agent and account pair saying the usage limit is reached and when it resets. Confirm is disabled until another account is chosen.",
    state: moveState(handoffRequestOn("a-team"), { target: idleOwnedSession, quotas: teamUsedUpQuotas }),
  },
  {
    id: "session-request-handoff-choice-gone",
    group: TRUST,
    category: "exceptional",
    title: "Request to hand off to an account that no longer exists",
    description: "The account asked for was removed after the request was made, so the question names it by its id. The agent and the account pickers are both empty, each with its placeholder, and the Hand off to field is marked invalid, with its own error under the pair saying the agent or account is no longer available and asking to choose again. Confirm is disabled until an agent or account is chosen.",
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
    id: "session-request-handoff-starting",
    group: TRUST,
    category: "exceptional",
    title: "Request to hand off a session that is still starting",
    description: "The session to hand off has not finished starting. An error above the picker, titled \"The session is still starting, so it cannot be handed off yet.\", takes the place of the mid-turn text, and Confirm is disabled, though Team is chosen and available. Refuse is the way out.",
    state: moveState(handoffRequest, { target: startingOwnedSession }),
  },
  {
    id: "toast-error",
    group: TRUST,
    category: "exceptional",
    title: "Queued error alerts",
    description:
      "Errors require explicit OK, with no close button or automatic dismissal, and every sentence is the application's own, under a title naming what failed (the first three \"Could not open in the file manager\", \"Could not save the setting\" and \"Could not open a session\", the two refused reopens \"Could not reopen the session\"): raw text from the daemon or the system appears only in the Error details disclosure (closed here) of the alerts that carry it. OK advances through: a file that is gone, named as code, with the advice to find it in the project's files if it was moved or renamed; the closed connection, whose remedy names Try again as a chip; an unknown account, whose Manage accounts… button sits in the footer, left of OK; two refused reopens (the agent no longer has the session's conversation, and Codex could not restore its archived one, whose system text is under Error details), each an expected refusal with its own advice rather than the unexpected-failure alert; a session-specific error and a failed trust confirmation, each titled for what failed (\"Request not carried out\", \"Could not answer the trust confirmation\") with a Session line above the message naming the session and where it is, the session and its project each a chip; and, last, an unknown code from a newer daemon: the generic sentence \"Something unexpected went wrong.\", with the daemon's English text under Error details and the Report issue guidance.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "error", code: "file_not_found", params: { path: "src/main.ts" }, message: "", title: failureTitle("common.revealFailedTitle"), recovery: "hostPathGone" },
      { kind: "error", code: "connection_closed", params: {}, message: (t) => t("connection.error.connectionClosed"), title: failureTitle("settings.saveFailedTitle") },
      { kind: "error", code: "unknown_account", params: { account: "removed-account" }, message: "The configured account no longer exists", title: failureTitle("dialog.session.failedTitle") },
      {
        kind: "error",
        title: failureTitle("sidebar.session.reopenFailedTitle"),
        code: "conversation_unresumable",
        params: { agent: "Claude Code", session: "s-web-1" },
        message: "Claude Code no longer has the conversation of this session, so it cannot be resumed",
      },
      {
        kind: "error",
        title: failureTitle("sidebar.session.reopenFailedTitle"),
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
      { kind: "error", code: "a_code_from_a_newer_daemon", params: {}, message: "The daemon's own English text for a code this build does not know." },
    ],
  },
  {
    id: "toast-quick-create-owner-gone",
    group: TRUST,
    category: "exceptional",
    title: "Quick create refused, the named owner is no longer available",
    description: "The quick-create race has no durable form. Its acknowledgement, titled \"Cannot open a session\", keeps the former owner's full mixed-script task and session title as a wrapping name chip; a known refusal, it gives the advice for a session that is gone and offers no report guidance or Report issue button. The plain error sentence remains available to system surfaces.",
    state: snapshotState({ consoles: [console_], projects: [web], sessions: [] }),
    toasts: [{
      kind: "error",
      title: failureTitle("dialog.session.blockedTitle"),
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
    description: "An error alert tied to a console session, Hub. It is titled \"Message not sent\", and the first line of the body is the session line, \"Session: Hub — Main's console session\", with the session and the console each a chip, which names the console rather than a project. Then the body names the agent, says the message waits in the session's input box and that nothing more is written into the session until a prompt is sent, then gives the recovery of opening the session and pressing Enter.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{ kind: "error", code: "message_not_sent", params: { agent: "Claude Code" }, message: "", session: SAMPLE.consoleSession.id }],
  },
  {
    id: "toast-message-not-sent-long-title",
    group: TRUST,
    category: "exceptional",
    title: "Message not sent, session with a long title",
    description: "The session line of the alert, when the session's title is long and holds a word with no break opportunity: look that the session's chip wraps inside the dialog, keeps the project chip (Website) readable after it, and leaves the dialog's width, the title above it and the message below it undisturbed.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions: [...sessions, longTitled] }),
    toasts: [{ kind: "error", code: "message_not_sent", params: { agent: "Claude Code" }, message: "", session: longTitled.id }],
  },
  {
    id: "toast-queued-messages-dropped",
    group: TRUST,
    category: "exceptional",
    title: "Queued messages dropped for a project session",
    description: "An error alert titled \"Queued messages dropped\", with the session line above the message, the session and its project each a chip: what was dropped, the advice to check the session's input line before sending anything else, and why that line may hold part of a message.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{ kind: "error", code: "queued_messages_dropped", params: {}, message: "", session: "s-web-1" }],
  },
  {
    id: "toast-message-notices",
    group: TRUST,
    category: "exceptional",
    title: "Message discarded by a dialog or diverted by a view",
    description: "Two notice toasts, not alerts, stacked (hover the stack to spread it) and tied to the Claude Code session; each names the agent. One says a dialog came up while the message was written and discarded it, and that the message is written again once the dialog has been answered. The other says the message went into a view of its background agents rather than the conversation, and asks the person to check the session; it does not say the message will be written again.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "session_notice", code: "message_discarded_by_dialog", params: { agent: "Claude Code" }, message: "", session: "s-web-1" },
      { kind: "session_notice", code: "message_diverted_by_view", params: { agent: "Claude Code" }, message: "", session: "s-web-1" },
    ],
  },
  {
    id: "error-unexpected",
    group: TRUST,
    category: "exceptional",
    title: "Unexpected failure and issue reporting",
    description: "The danger alert says only \"Something unexpected went wrong.\", then that this may be a bug that cannot be resolved here, and to use Report issue (named in a noninteractive hint chip) to file an Issue with the steps that led to it; it never asks for a screenshot. Its secondary footer button, with an external-link icon, sits to the left of OK and opens the configured issue form with the sanitized error details already filled in (a new browser tab here). The daemon's own text for the failure is in the Error details disclosure below the guidance, closed here; it is the daemon's text, not a JavaScript stack.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    toasts: [{ kind: "error", code: "internal_error", params: { detail: "Unexpected session state while applying an update" }, message: "Unexpected session state while applying an update" }],
  },
  {
    id: "error-issue-link-failed",
    group: TRUST,
    category: "exceptional",
    title: "Issue form could not be opened",
    description: "The opener failure takes precedence over the original unexpected error. Its actionable recovery asks to copy the issue address shown above and open it in the browser by hand, and to describe the steps that led to the error when reporting; the address is the plain issue URL, not the prefilled one, alone on its line as plain monospace text without a chip, and there is no recursive Report issue link. Error details is open and holds only the system's reason (\"The operating system could not open the default browser\") on a surface of its own as one line that scrolls sideways, never the application's own sentence or a stack. OK returns to the original failure.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    externalLinkError: "The operating system could not open the default browser",
    toasts: [{ kind: "error", code: "internal_error", params: { detail: "Unexpected session state while applying an update" }, message: "Unexpected session state while applying an update" }],
    steps: [(ui) => ui.press(ui.t("error.issue.report")), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "toast-session-branches-not-removed",
    group: TRUST,
    category: "exceptional",
    title: "Branches and worktrees left after an archive",
    description: "Under the header saying these were to be removed and are still there, the danger alert groups the leftovers by reason under short lead lines: under \"Removing this failed:\" a branch git failed to delete, shown as its name alone, and a locked worktree, its path shortened with ~ as plain monospace text, its row ending with a soft warning \"uncommitted changes\" badge. Recovery guidance follows, then the Error details disclosure, open here: below a heading naming the branch, git's multi-line message sits in a block of its own, so no git text is in the list above. The archive itself stands.",
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
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
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
    description: "The danger alert names the removed project as a chip and says the worktrees outside its folder, and its branches if the repository stays, were left, then its own recovery guidance. Git's reason is not in the sentence: it is in the Error details disclosure, open here, in a block of its own. The project itself is gone.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "error",
      code: "project_branches_not_determined",
      params: { name: "Docs Site", detail: "fatal: not a git repository: '/Users/dev/code/docs-site/.git'" },
      message: "",
    }],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
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
      title: failureTitle("dialog.archiveSession.failedTitle"),
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
    id: "toast-launch-failed-session",
    group: TRUST,
    category: "exceptional",
    title: "Launch failed for a session the app still has",
    description: "An error alert titled \"Could not start the session\", without the session's name. The first line of the body is the session line, \"Session: Review the retry logic — Search API\", with the session and its project each a chip, then the failure itself: the missing path is a chip in the sentence saying it does not exist.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "error",
      code: "launch_failed",
      params: { title: codexSession.title, reason_code: "path_not_found", path: "/Users/dev/code/search-api" },
      message: "",
      session: codexSession.id,
    }],
  },
  {
    id: "toast-launch-path",
    group: TRUST,
    category: "exceptional",
    title: "Launch failed, nested path in its reason",
    description: "The session record is gone, so its name comes from the failure itself: the alert is still titled \"Could not start the session\", with the \"Session:\" line above the message holding the name as a chip and no location; the configuration path inside the failure reason is a chip.",
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
    id: "error-account-switch-automatically-failed",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed, session left interrupted",
    description: "An error alert titled \"Could not switch account\", with the session line above the message. The sentence says the session on Work reached its usage limit, and the app could neither switch it to Personal nor bring it back up on the account it had, so it is interrupted. Beneath it, on a line of its own, comes why it could not be brought back up, in the application's own words: sharing for Work being changed, with Work a chip, and nothing after it. No advice to resume it, none to wait and try again, no Error details disclosure and no Report issue: only the acknowledgement button.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "session_notice",
      code: "account_switch_automatically_failed",
      params: {
        agent: "Claude Code",
        from: "Work",
        from_id: "a-work",
        account: "Personal",
        account_id: "a-personal",
        name: "Work",
        reason: "Sharing for account Work is being changed.",
        reason_code: "account_sharing_in_progress",
      },
      message: "",
      session: "s-web-1",
    }],
  },
  {
    id: "error-account-switch-automatically-failed-kept-aside",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed, an older item kept aside on a full disk",
    description: "The same error alert, \"Could not switch account\", for a session on Work that could not be switched to Personal nor brought back up on Work, because the conversation could not be copied into Work's folder and an older item there could not be put back. After the sentence and the reason line, which says where the older item is kept and until when (the folders are chips), comes the short advice to copy that item out of the folder before then, with the timing not repeated, then, as a paragraph of its own, the remedy for a full disk, which does not say to retry. Each appears once. No Report issue.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "session_notice",
      code: "account_switch_automatically_failed",
      params: {
        agent: "Claude Code",
        from: "Work",
        from_id: "a-work",
        account: "Personal",
        account_id: "a-personal",
        reason: "The conversation could not be copied into /Users/dev/.claude-work: No space left on device (os error 28)",
        reason_code: "relocation_failed",
        path: "/Users/dev/.claude-work",
        stage: "kept_staging",
        cause: "not_restored",
        kept_in: "/Users/dev/.claude-work/.octoboard-switch/9b1d4e7a-5c3f-4a28-b6d0-1e8f2a7c3d59",
        detail: "No space left on device (os error 28)",
      },
      message: "",
      session: "s-web-1",
    }],
  },
  {
    id: "error-account-switch-automatically-failed-disk-full",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed on a full disk reported as an internal error",
    description: "The same error alert, \"Could not switch account\", for a session on the default account (its chip reads \"Default\" here and 默认 in Simplified Chinese, so check it with the language control too) that could not be brought back up, by a failure the daemon classed as internal but whose text says the disk is full: the line beneath the sentence is \"The disk is full.\", followed by the remedy for a full disk, which does not say to retry. There is no Report issue guidance or button, and the daemon's raw text is only in the Error details disclosure, open here.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "session_notice",
      code: "account_switch_automatically_failed",
      params: {
        agent: "Claude Code",
        from: DEFAULT_ACCOUNT_NAME,
        account: "Work",
        account_id: "a-work",
        reason: "No space left on device (os error 28)",
        reason_code: "internal_error",
      },
      message: "",
      session: "s-web-1",
    }],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "error-account-switch-automatically-failed-access-denied",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed on a denied folder access reported as an internal error",
    description: "The same error alert, \"Could not switch account\", for a session on Work that could not be brought back up, by a failure the daemon classed as internal but whose text says access to a folder was denied: the line beneath the sentence is \"Access was denied.\", followed by the remedy for denied access (check the folder's file permissions and that its disk is writable, grant the app access in system privacy settings if needed), which ends there: it does not offer to choose another accessible folder, and does not say to retry. There is no Report issue guidance or button, and the daemon's raw text is only in the Error details disclosure, open here.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "session_notice",
      code: "account_switch_automatically_failed",
      params: {
        agent: "Claude Code",
        from: "Work",
        from_id: "a-work",
        account: "Personal",
        account_id: "a-personal",
        reason: "Permission denied (os error 13)",
        reason_code: "internal_error",
      },
      message: "",
      session: "s-web-1",
    }],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "error-account-switch-automatically-failed-uncatalogued",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed for a reason the app has no words for",
    description: "The same error alert, \"Could not switch account\", for a session on Work that could not be brought back up, but the failure carries a reason code this build has no wording for: the line beneath the sentence is the generic \"Something unexpected went wrong.\" with the Report issue guidance and button, and the daemon's English text (also in Simplified Chinese) is only in the Error details disclosure, open here.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "session_notice",
      code: "account_switch_automatically_failed",
      params: {
        agent: "Claude Code",
        from: "Work",
        from_id: "a-work",
        account: "Personal",
        account_id: "a-personal",
        reason: "The configuration directory could not be prepared.",
        reason_code: "a_code_from_a_newer_daemon",
      },
      message: "",
      session: "s-web-1",
    }],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "toast-shell-timeout",
    group: TRUST,
    category: "exceptional",
    title: "Shell environment timeout alert",
    description: "The error alert presents the shell command as selectable code, then the cause, launch consequence and recovery as separate readable parts.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{ kind: "error", title: failureTitle("dialog.session.failedTitle"), ...SHELL_TIMEOUT }],
  },
  {
    id: "toast-notice",
    group: TRUST,
    category: "exceptional",
    title: "Warning notices and recovery",
    description: "Deliberate warning notices: an untrusted workspace workaround naming its allow and deny settings as code and a trust entry blocked by a locked store. Session links identify where recovery is needed. The trust file's path is set as code.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "session_notice", code: "claude_workspace_untrusted", params: {}, message: "", session: "s-web-1" },
      {
        kind: "session_notice",
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
  {
    id: "error-sharing-resume-failed",
    group: TRUST,
    category: "exceptional",
    title: "Session not resumed after a sharing change",
    description: "A session notice that is a failure is an error alert, not a toast, and has no Report issue. Titled \"Could not resume the session\", with the session line above the message, it says only that a session of account Work could not be resumed after its sharing changed, so it is left interrupted, and to resume it yourself. The system's own text (No such file or directory, os error 2) is not in the sentence but in the Error details disclosure, open here.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "session_notice",
      code: "sharing_resume_failed",
      params: { account: "a-work", name: "Work", detail: "No such file or directory (os error 2)" },
      message: "",
      session: "s-web-1",
    }],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "error-sharing-repair-failed",
    group: TRUST,
    category: "exceptional",
    title: "Session started before its sharing links were repaired",
    description: "An error alert titled \"Could not repair sharing links\", with the session line above the message, says only that the session started with account Work's directory as it stands, because its links to the default configuration could not be brought up to date. Under it come the advice for each cause (permission denied, in use). The entries that could not be repaired are in the Error details disclosure, open here, each with what it was left as and the system's own text in a block of its own under it.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "session_notice",
      code: "account_sharing_repair_failed",
      params: {
        account: "a-work",
        name: "Work",
        items: JSON.stringify([
          { item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied", sessions: [] },
          { item: "plugins/", left: "unchanged", cause: "busy", detail: "resource busy", sessions: [] },
        ]),
      },
      message: "",
      session: "s-web-1",
    }],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "error-trust-not-carried-over-raw",
    group: TRUST,
    category: "exceptional",
    title: "Trust not carried over, with what the system said",
    description: "A trust entry that could not be copied is a toast when the cause is worded (see Warning notices and recovery); with the system's own text it is an error alert titled \"Trust not carried over\", with the session line above the message. The sentence says only that Grok Build will ask again the next time it opens the folder, with the file as code and the reason as the application's own short words; the system's text (Permission denied, os error 13) is only in the Error details disclosure, open here.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{
      kind: "session_notice",
      code: "trust_not_carried_over",
      params: {
        agent: "Grok Build",
        path: "/Users/dev/.grok/trusted_folders.toml",
        reason: "Permission denied (os error 13)",
        reason_code: "store_io_failed",
        detail: "Permission denied (os error 13)",
      },
      message: "",
      session: grokSession.id,
    }],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
];
