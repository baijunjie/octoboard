import { DEFAULT_SETTINGS } from "../../store/defaultSettings";
import { type Account, type AccountQuota, type AccountSharing, type Settings } from "../../protocol";
import { DEFAULT_ACCOUNT_NAME } from "../../accounts/defaultAccount";
import { AGENTS } from "../../agents/agents";
import type { SessionRequest } from "../../store";
import type { Scenario } from "../scenario";
import { accountOf, SAMPLE, consoleOf, failureTitle, projectOf, sessionOf, snapshotState, broadcastErrorToast, sessionNoticeToast } from "../../fixtures/builders";
import { quotaOf, quotaWindow } from "../../fixtures/quotas";
import { SHELL_TIMEOUT } from "./shellTimeout";

const TRUST = "Trust prompts and toasts";

// Git's own multi-line refusal, as `failed` carries it.
const GIT_REFUSAL = [
  "error: cannot delete branch 'fix/rename-index' used by worktree at '/Users/dev/worktrees/search-api-rename-index'",
  "hint: to remove it, first remove the worktree with git worktree remove, then run git branch -D fix/rename-index again once nothing else holds the branch",
].join("\n");

// What the daemon says in English of a worktree it kept for want of knowing whether it is or holds another project's
// folder; the UI words that itself from `cause` and shows none of this.
const FOLDER_UNKNOWN_DETAIL = "whether it is or holds another project's folder could not be told";

// The same for a worktree kept because it is the home folder, the root of a disk or holds the app's own data; the UI
// words that from `cause` as well and shows none of this.
const PROTECTED_FOLDER_DETAIL = "it is the home folder, the root of a disk or a folder holding Octoboard's data";

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
  accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
  accountOf("a-client", "claude", "Client", "/Users/dev/.claude-client", { share_default_config_incomplete: true }),
];
const resumeSettings: Settings = {
  ...DEFAULT_SETTINGS,
  added_agents: [...AGENTS],
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
  accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
  accountOf("a-personal", "claude", "Personal", "/Users/dev/.claude-personal"),
  accountOf("a-client", "claude", "Client", "/Users/dev/.claude-client"),
  accountOf("a-team", "codex", "Team", "/Users/dev/.codex-team"),
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
    id: "session-request-project",
    group: TRUST,
    category: "normal",
    title: "Request to start a session in another project",
    description: "A session in Search API asks for a Grok Build session in Search API experiments, a linked worktree of the same repository. The requester, named by its terminal title beside its name, belongs to a different console and project, so all requester and destination names appear as chips. The question is followed by the branches that would be handed over, each a code chip joined as the language joins a list. A literal % is shown as written (fix/coverage-at-100%), and the long name wraps inside its chip. A request in the requester's own project, for a console session, or to resume an interrupted session or a conversation only an account's record holds, asks the same question in its own words.",
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
      sessionNoticeToast("s-web-1", "account_switched_automatically", { agent: "Claude Code", from: "Work", account: "Personal", session: "s-web-1" }),
      sessionNoticeToast("s-web-1", "account_switched_automatically", { agent: "Claude Code", from: DEFAULT_ACCOUNT_NAME, account: "Work", session: "s-web-1" }),
    ],
  },
  {
    id: "trust-prompt-directory-unreachable",
    group: TRUST,
    category: "exceptional",
    title: "Trust prompt refused for a folder that could not be looked at",
    description: "The go-ahead with \"Trust every project under ~/code\" checked was refused. An error alert titled \"Could not answer the trust confirmation\" stands over the trust dialog, which is still open behind it with the option still checked and its buttons still there. Its body says the project's path, as a chip, or the folder above it did not respond in time, so that folder could not be checked; the remedy line says to uncheck the folder option to trust this project on its own, or to try again once the volume responds again.",
    state: snapshotState({
      consoles: [console_],
      projects: [web],
      sessions: trustSessions.filter((session) => session.project_id === web.id || session.role === "console"),
      trustPrompts: [{ session: "s-web-1", agent: "claude", project: web.id, path: web.path, trustDir: "/Users/dev/code" }],
    }),
    trustParentDirError: { code: "trust_directory_unreachable", params: { path: web.path }, message: `\`${web.path}\` or the folder above it did not respond in time, so that folder could not be checked` },
    steps: [
      (ui) => ui.press(ui.t("dialog.trust.parent", { directory: "~/code" })),
      (ui) => ui.confirm(),
    ],
  },
  {
    id: "session-request-approve-failed",
    group: TRUST,
    category: "exceptional",
    title: "Approving a session request failed",
    description: "Start session was pressed and the daemon refused the approval because the asking session had stopped waiting. The error alert is titled \"Could not approve the request\", the dialog's own failure title, and not the refusal's own \"Request not carried out\". It names the session and its project as chips, says the session is no longer waiting for an answer, so its request was not carried out, and gives the remedy line.",
    state: snapshotState({
      consoles: [console_],
      projects: [web, api, grokProject],
      sessions: sessions.map((s) => s.id === "s-web-1" ? { ...s, status: "waiting_user" as const } : s),
      sessionRequests: [
        requestOf({ action: "start", session: "s-web-1", role: "project", console: console_.id, project: web.id, agent: "claude" }),
      ],
    }),
    sessionRequestAnswerError: { code: "session_request_not_waiting", params: { session: "s-web-1" }, message: "This session is no longer waiting for an answer, so its request was not carried out." },
    steps: [(ui) => ui.confirm()],
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
    description: "The account asked for, Work, has its session limit reached. The picker still stands on Work and the Switch to field is marked invalid, with its own error under the picker saying the usage limit is reached and when it resets. Confirm is disabled until another account is chosen. An account that is not logged in, turned off, having its sharing changed, or the one the session is on already is refused in the same place with its own sentence; one removed since leaves the picker empty with its placeholder and the sentence asks to choose another.",
    state: moveState(switchRequestTo("a-work")),
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
    description: "The account asked for, Team, has its session limit reached. The picker still stands on Team and the Hand off to field is marked invalid, with its own error under the agent and account pair saying the usage limit is reached and when it resets. Confirm is disabled until another account is chosen. An agent or account removed since leaves both pickers empty with their placeholders, and the sentence asks to choose again.",
    state: moveState(handoffRequestOn("a-team"), { target: idleOwnedSession, quotas: teamUsedUpQuotas }),
  },
  {
    id: "session-request-handoff-working",
    group: TRUST,
    category: "exceptional",
    title: "Request to hand off a session that has started working",
    description: "The session to hand off started a turn while the request waited. An error above the picker says the session is in the middle of a turn and cannot be handed off yet. Confirm is disabled, and Refuse is the way out. A session that is still starting is refused in the same place.",
    state: moveState(handoffRequest),
  },
  {
    id: "toast-error",
    group: TRUST,
    category: "exceptional",
    title: "Queued error alerts",
    description:
      "Errors require explicit OK, with no close button or automatic dismissal, and every sentence is the application's own, under a title naming what failed (the first three \"Could not open in the file manager\", \"Could not save the setting\" and \"Could not open a session\", the two refused reopens \"Could not reopen the session\"): raw text from the daemon or the system appears only in the Error details disclosure (closed here) of the alerts that carry it. OK advances through: a file that is gone, named as code, with the advice to find it in the project's files if it was moved or renamed; the closed connection, whose remedy names Try again as a chip; an unknown account, whose Manage accounts button sits in the footer, left of OK; two refused reopens (the agent no longer has the session's conversation, and Codex could not restore its archived one, whose system text is under Error details), each an expected refusal with its own advice rather than the unexpected-failure alert; a failed trust confirmation, titled \"Could not answer the trust confirmation\", with a Session line above the message naming the session and where it is, the session and its project each a chip; and, last, an unknown code from a newer daemon: the generic sentence \"Something unexpected went wrong.\", with the daemon's English text under Error details and the Report issue guidance.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      { kind: "raised", code: "file_not_found", params: { path: "src/main.ts" }, message: "", title: failureTitle("common.revealFailedTitle"), recovery: "hostPathGone" },
      { kind: "raised", code: "connection_closed", params: {}, message: (t) => t("connection.error.connectionClosed"), title: failureTitle("settings.saveFailedTitle") },
      { kind: "raised", code: "unknown_account", params: { account: "removed-account" }, message: "The configured account no longer exists", title: failureTitle("dialog.session.failedTitle") },
      {
        kind: "raised",
        title: failureTitle("sidebar.session.reopenFailedTitle"),
        code: "conversation_unresumable",
        params: { agent: "Claude Code", session: "s-web-1" },
        message: "Claude Code no longer has the conversation of this session, so it cannot be resumed",
      },
      {
        kind: "raised",
        title: failureTitle("sidebar.session.reopenFailedTitle"),
        code: "codex_unarchive_failed",
        params: { detail: "no rollout found for thread id 0199c3a2-7e41-7b52-9d0e-4f6a1c85b2d7" },
        message: "`codex unarchive` failed: no rollout found for thread id 0199c3a2-7e41-7b52-9d0e-4f6a1c85b2d7",
      },
      sessionNoticeToast(codexSession.id, "trust_answer_failed", { agent: "Codex", reason_code: "cursor_moved_away" }),
      broadcastErrorToast("a_code_from_a_newer_daemon", {}, "The daemon's own English text for a code this build does not know."),
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
      kind: "raised",
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
    description: "An error alert tied to a console session, Hub. It is titled \"Message not sent\", and the body starts with separate Session and Console lines, with Hub and Main each a name chip. Then the body names the agent and explains the single retry of the current input without pasting the message again. Pending dialogs must be answered first. The recovery asks the person to check the automatic retry before pressing Enter if the message still waits in the input box, and not to paste it again. Queued messages dropped, and messages dropped in a handoff or a sharing change, use the same alert with their own wording.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast(SAMPLE.consoleSession.id, "message_not_sent", { agent: "Claude Code", submit_retry: "once" })],
  },
  {
    id: "toast-message-not-sent-long-title",
    group: TRUST,
    category: "exceptional",
    title: "Message not sent, session with a long title",
    description: "The session line of the alert, when the session's title is long and holds a word with no break opportunity: look that the session's chip wraps inside the dialog, keeps the project chip (Website) readable after it, and leaves the dialog's width, the title above it and the message below it undisturbed.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions: [...sessions, longTitled] }),
    toasts: [sessionNoticeToast(longTitled.id, "message_not_sent", { agent: "Claude Code", submit_retry: "once" })],
  },
  {
    id: "toast-message-acceptance-unconfirmed",
    group: TRUST,
    category: "exceptional",
    title: "Message not confirmed by the agent",
    description: "An error alert titled \"Message not confirmed\", with the session line above the message, the session and its project each a chip: the summary says the agent (Codex) did not confirm that it accepted a message the app wrote into the session, with the agent and app names as chips; the recovery step asks the person to check the conversation before deciding whether to send the message again; the details say the message may already be queued, is not sent again by the app, and that later messages are held for at most three seconds or until another prompt is sent.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast("s-web-1", "message_acceptance_unconfirmed", { agent: "Codex" })],
  },
  {
    id: "toast-submit-retry-failed",
    group: TRUST,
    category: "exceptional",
    title: "Submit not confirmed, the retry of Enter failed",
    description: "An error alert titled \"Submit not confirmed\", with the session line above the message (the session and its project each a chip) and no Report issue button: a notice about this session, not an unexpected error. The summary says the agent (Codex) did not confirm sending a message the app wrote into the session and that the app could not press Enter again; the recovery line below it asks the person to check the input box and press Enter themselves, with Enter as a key chip; and the Error details disclosure (closed here) holds the system's own text for the failed retry, which is the only place raw text appears.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast(codexSession.id, "submit_retry_failed", { agent: "Codex", detail: "Broken pipe (os error 32) (0 bytes written)" })],
  },
  {
    id: "toast-typing-blocked",
    group: TRUST,
    category: "exceptional",
    title: "Message partly written, the longest input recovery notice",
    description: "An error alert titled \"Message partly written\", with the session line above the message (the session and its project each a chip) and no Report issue button: a notice about this session, not an unexpected error. It has all three parts in their longest wording, laid out as in the other input recovery notices (Draft save not confirmed, Draft restore not confirmed, Typing held back; the first has no details): look that the summary, the recovery line (check the input box and remove what is left of the message) and the details below it each wrap inside the dialog, with the agent and app names as chips. Choose zh-Hans and a narrow width with the page's controls to see the longest copy.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast("s-web-1", "typing_blocked", { agent: "Claude Code" })],
  },
  {
    id: "toast-typing-lost",
    group: TRUST,
    category: "exceptional",
    title: "Typing lost, an input recovery notice without a recovery line",
    description: "An error alert titled \"Typing lost\", with the session line above the message and no Report issue button. The body is a single sentence, that the session ended before the agent (Codex, a chip) confirmed putting its draft back and what was typed into it meanwhile is lost and not typed in when the session is resumed: look that it is laid out without a details block or a recovery line, and that nothing is left as a gap where they would be.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast(codexSession.id, "typing_lost", { agent: "Codex", bytes: "37" })],
  },
  {
    id: "toast-message-notices",
    group: TRUST,
    category: "exceptional",
    title: "Message discarded by a dialog or diverted by a view",
    description: "Two notice toasts, not alerts, stacked (hover the stack to spread it) and tied to the Claude Code session; each names the agent. One says a dialog came up while the message was written and discarded it, and that the message is written again once the dialog has been answered. The other says the message went into a view of its background agents rather than the conversation, and asks the person to check the session; it does not say the message will be written again.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      sessionNoticeToast("s-web-1", "message_discarded_by_dialog", { agent: "Claude Code" }),
      sessionNoticeToast("s-web-1", "message_diverted_by_view", { agent: "Claude Code" }),
    ],
  },
  {
    id: "error-unexpected",
    group: TRUST,
    category: "exceptional",
    title: "Unexpected failure and issue reporting",
    description: "The danger alert says only \"Something unexpected went wrong.\", then that this may be a bug that cannot be resolved here, and to use Report issue (named in a noninteractive hint chip) to file an Issue with the steps that led to it; it never asks for a screenshot. Its secondary footer button, with an external-link icon, sits to the left of OK and opens the configured issue form with the sanitized error details already filled in (a new browser tab here). The daemon's own text for the failure is in the Error details disclosure below the guidance, closed here; it is the daemon's text, not a JavaScript stack.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    toasts: [broadcastErrorToast("internal_error", { detail: "Unexpected session state while applying an update" }, "Unexpected session state while applying an update")],
  },
  {
    id: "error-issue-link-failed",
    group: TRUST,
    category: "exceptional",
    title: "Issue form could not be opened",
    description: "The opener failure takes precedence over the original unexpected error. Its actionable recovery asks to copy the issue address shown above and open it in the browser by hand, and to describe the steps that led to the error when reporting; the address is the plain issue URL, not the prefilled one, alone on its line as plain monospace text without a chip, and there is no recursive Report issue link. Error details is open and holds only the system's reason (\"The operating system could not open the default browser\") on a surface of its own as one line that scrolls sideways, never the application's own sentence or a stack. OK returns to the original failure.",
    state: snapshotState({ consoles: [console_], projects: [web, api], sessions }),
    externalLinkError: "The operating system could not open the default browser",
    toasts: [broadcastErrorToast("internal_error", { detail: "Unexpected session state while applying an update" }, "Unexpected session state while applying an update")],
    steps: [(ui) => ui.press(ui.t("error.issue.report")), (ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "toast-session-branches-not-removed",
    group: TRUST,
    category: "exceptional",
    title: "Branches and worktrees left after an archive",
    description: "Under the header saying these were to be removed and are still there, the danger alert groups the leftovers by reason under short lead lines: under \"Removing this failed:\" a branch git failed to delete, shown as its name alone; under the lead for a locked worktree, that worktree, its folder icon before the path shortened with ~ and its branch icon on the next line aligned with the folder icon, its row ending with a soft warning \"uncommitted changes\" badge; then, under the singular lead saying the worktree is your home folder, the root of a disk or holds the app's own data so it was kept, the ~ worktree on branch scratch-home with no raw text of its own; and, last, under the plural lead \"Whether these worktrees are or hold another project's folder could not be checked:\", two worktrees with no raw text of their own, followed by the plural note that they may hold another project's files and to check what is in them before removing them by hand. Recovery guidance follows, then the Error details disclosure, open here: below a heading naming the branch, git's multi-line message sits in a block of its own, so no git text is in the list above and none for the three worktrees. The archive itself stands.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [broadcastErrorToast("session_branches_not_removed", {
      count: "5",
      items: JSON.stringify([
        { kind: "worktree", name: "/Users/dev/worktrees/website-changelog-tidy", branch: "changelog-tidy", state: "locked", uncommitted: true, detail: null },
        { kind: "branch", name: "fix/rename-index", branch: "fix/rename-index", state: "failed", uncommitted: false, detail: GIT_REFUSAL },
        { kind: "worktree", name: "/Users/dev/worktrees/website-mounted-share", branch: "mounted-share", state: "failed", uncommitted: false, detail: FOLDER_UNKNOWN_DETAIL, cause: "project_folder_unknown" },
        { kind: "worktree", name: "/Users/dev/worktrees/website-mounted-assets", branch: null, state: "failed", uncommitted: false, detail: FOLDER_UNKNOWN_DETAIL, cause: "project_folder_unknown" },
        { kind: "worktree", name: "/Users/dev", branch: "scratch-home", state: "failed", uncommitted: false, detail: PROTECTED_FOLDER_DETAIL, cause: "protected_folder" },
      ]),
    }, "The worktree is locked; the branch could not be deleted")],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "toast-session-branches-not-removed-folder-unknown",
    group: TRUST,
    category: "exceptional",
    title: "Worktree kept, whether it holds another project's folder unknown",
    description: "With a single worktree left, the alert has no header: the singular lead \"Whether this worktree is or holds another project's folder could not be checked:\" is its summary, over the worktree (folder icon, path shortened with ~, branch on the next line) with no raw text under it, then the singular note that it may hold another project's files and to check what is in it before removing it by hand. There is no recovery guidance and no Error details disclosure, as there is no raw text to show. Worktrees kept as the home folder or the app's data put their own lead in the same alert, without the note.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [broadcastErrorToast("session_branches_not_removed", {
      count: "1",
      items: JSON.stringify([
        { kind: "worktree", name: "/Users/dev/worktrees/website-mounted-share", branch: "mounted-share", state: "failed", uncommitted: false, detail: FOLDER_UNKNOWN_DETAIL, cause: "project_folder_unknown" },
      ]),
    }, "The worktree could not be removed")],
  },
  {
    id: "toast-session-branches-not-removed-locked",
    group: TRUST,
    category: "exceptional",
    title: "Only locked worktrees left after an archive",
    description: "With a single group of leftovers the alert has no header over it: only the lead line of its group (the worktrees are locked), over two worktrees. Each path has a folder icon; the first has its branch icon on a separate line aligned with the folder icon, while the second has a detached HEAD note indented to the path text. Recovery guidance follows.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [broadcastErrorToast("session_branches_not_removed", {
      count: "2",
      items: JSON.stringify([
        { kind: "worktree", name: "/Users/dev/worktrees/search-api-ranking", branch: "feat/ranking", state: "locked", uncommitted: false, detail: null },
        { kind: "worktree", name: "/Users/dev/worktrees/search-api-spike", branch: null, state: "locked", uncommitted: false, detail: null },
      ]),
    }, "The worktrees are locked")],
  },
  {
    id: "toast-project-branches-not-determined",
    group: TRUST,
    category: "exceptional",
    title: "Project removed, what went with its folder not found out",
    description: "The danger alert names the removed project as a chip and says the worktrees outside its folder, and its branches if the repository stays, were left, then its own recovery guidance. Git's reason is not in the sentence: it is in the Error details disclosure, open here, in a block of its own. The project itself is gone.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [broadcastErrorToast("project_branches_not_determined", { name: "Docs Site", detail: "fatal: not a git repository: '/Users/dev/code/docs-site/.git'" })],
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
      kind: "raised",
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
    description: "An error alert titled \"Could not start the session\", without the session's name. The first line of the body is the session line, \"Session: Review the retry logic — Search API\", with the session and its project each a chip, then the failure itself: the missing path is a chip in the sentence saying it does not exist. A session the app no longer has is named by a chip alone on that line.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast(codexSession.id, "launch_failed", { title: codexSession.title, reason_code: "path_not_found", path: "/Users/dev/code/search-api" })],
  },
  {
    id: "error-account-switch-automatically-failed",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed, session left interrupted",
    description: "An error alert titled \"Could not switch account\", with the session line above the message. The sentence says the session on Work reached its usage limit, and the app could neither switch it to Personal nor bring it back up on the account it had, so it is interrupted. Beneath it, on a line of its own, comes why it could not be brought back up, in the application's own words: sharing for Work being changed, with Work a chip, and nothing after it. No advice to resume it, none to wait and try again, no Error details disclosure and no Report issue: only the acknowledgement button. Reasons that carry a remedy of their own are in the scenarios below.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast("s-web-1", "account_switch_automatically_failed", {
      agent: "Claude Code",
      from: "Work",
      from_id: "a-work",
      account: "Personal",
      account_id: "a-personal",
      name: "Work",
      reason: "Sharing for account Work is being changed.",
      reason_code: "account_sharing_in_progress",
    })],
  },
  {
    id: "error-account-switch-automatically-failed-kept-aside",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed, an older item kept aside on a full disk",
    description: "The same error alert, \"Could not switch account\", for a session on Work that could not be switched to Personal nor brought back up on Work, because the conversation could not be copied into Work's folder and an older item there could not be put back. After the sentence and the reason line, which says where the older item is kept and until when (the folders are chips), comes the short advice to copy that item out of the folder before then, with the timing not repeated, then, as a paragraph of its own, the remedy for a full disk, which does not say to retry. Each appears once. No Report issue.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast("s-web-1", "account_switch_automatically_failed", {
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
    })],
  },
  {
    id: "error-account-switch-automatically-failed-disk-full",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed on a full disk reported as an internal error",
    description: "The same error alert, \"Could not switch account\", for a session that could not be brought back up, by a failure the daemon classed as internal but whose text says the disk is full: the line beneath the sentence is \"The disk is full.\", followed by the remedy for a full disk, which does not say to retry. There is no Report issue guidance or button, and the daemon's raw text is only in the Error details disclosure, open here. A text that says access to a folder was denied reads alike, with \"Access was denied.\" and the remedy for denied access in their place, and so does a failure to read or write the application's own data, with \"Octoboard could not read or write its own data.\" and the remedy for its data folder, which does not say to retry.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast("s-web-1", "account_switch_automatically_failed", {
      agent: "Claude Code",
      from: DEFAULT_ACCOUNT_NAME,
      account: "Work",
      account_id: "a-work",
      reason: "No space left on device (os error 28)",
      reason_code: "internal_error",
    })],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "error-account-switch-automatically-failed-uncatalogued",
    group: TRUST,
    category: "exceptional",
    title: "Automatic account switch failed for a reason the app has no words for",
    description: "The same error alert, \"Could not switch account\", for a session on Work that could not be brought back up, but the failure carries a reason code this build has no wording for: the line beneath the sentence is the generic \"Something unexpected went wrong.\" with the Report issue guidance and button, and the daemon's English text (also in Simplified Chinese) is only in the Error details disclosure, open here.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast("s-web-1", "account_switch_automatically_failed", {
      agent: "Claude Code",
      from: "Work",
      from_id: "a-work",
      account: "Personal",
      account_id: "a-personal",
      reason: "The configuration directory could not be prepared.",
      reason_code: "a_code_from_a_newer_daemon",
    })],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "toast-shell-timeout",
    group: TRUST,
    category: "exceptional",
    title: "Shell environment timeout alert",
    description: "The error alert presents the shell command as selectable code, then the cause, launch consequence and recovery as separate readable parts.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [{ kind: "raised", title: failureTitle("dialog.session.failedTitle"), ...SHELL_TIMEOUT }],
  },
  {
    id: "toast-notice",
    group: TRUST,
    category: "exceptional",
    title: "Warning notices and recovery",
    description: "Deliberate warning notices, not alerts: an untrusted workspace workaround naming its allow and deny settings as code, a trust entry blocked by a locked store, and a request that was not carried out because the session was no longer waiting for an answer. Session links identify where recovery is needed. The trust file's path is set as code.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [
      sessionNoticeToast("s-web-1", "claude_workspace_untrusted"),
      sessionNoticeToast("s-web-1", "session_request_not_waiting", { session: "s-web-1" }),
      sessionNoticeToast(grokSession.id, "trust_not_carried_over", {
        agent: "Grok Build",
        path: "/Users/dev/.grok/trusted_folders.toml",
        reason: "the file stayed locked by another program",
        reason_code: "store_locked",
      }),
    ],
  },
  {
    id: "error-sharing-resume-failed",
    group: TRUST,
    category: "exceptional",
    title: "Session not resumed after a sharing change",
    description: "A session notice that is a failure is an error alert, not a toast, and has no Report issue. Titled \"Could not resume the session\", with the session line above the message, it says only that a session of account Work could not be resumed after its sharing changed, so it is left interrupted, and to resume it yourself. The system's own text (No such file or directory, os error 2) is not in the sentence but in the Error details disclosure, open here. A trust entry that could not be carried over, with the system's text, reads alike.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast("s-web-1", "sharing_resume_failed", { account: "a-work", name: "Work", detail: "No such file or directory (os error 2)" })],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "error-sharing-repair-failed",
    group: TRUST,
    category: "exceptional",
    title: "Session started before its sharing links were repaired",
    description: "An error alert titled \"Could not repair sharing links\", with the session line above the message, says only that the session started with account Work's directory as it stands, because its links to the default configuration could not be brought up to date. Under it come the advice for each cause (permission denied, in use). The entries that could not be repaired are in the Error details disclosure, open here, each with what it was left as and the system's own text in a block of its own under it.",
    state: snapshotState({ consoles: [console_], projects: [web, api, grokProject], sessions }),
    toasts: [sessionNoticeToast("s-web-1", "account_sharing_repair_failed", {
      account: "a-work",
      name: "Work",
      items: JSON.stringify([
        { item: "skills/", left: "unchanged", cause: "permission_denied", detail: "permission denied", sessions: [] },
        { item: "plugins/", left: "unchanged", cause: "busy", detail: "resource busy", sessions: [] },
      ]),
    })],
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
];
