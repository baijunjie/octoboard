// What the fixture daemon answers for the agent accounts: their creation and removal, sharing, sign-in, the default
// account's name and the use of a Codex reset credit, and the refusals a session gets on an account.
import { consoleAccount, inheritedAccount } from "../accounts/accountInheritance";
import { FIXTURE_AGENTS, FIXTURE_PROVIDERS, servedLabel } from "./fixtureProviders";
import { type Account, type AccountLogin, type AccountQuota, type AccountSharing, type Agent, type PerAgent, type ResetCreditOutcome, type ProjectAccount, type Console, type Project, type QuotaWindow, type ClientRequestBody, type McpSignInState, type SharingCause, type Session } from "../protocol";
import { DEFAULT_ACCOUNT_NAME } from "../accounts/defaultAccount";
import { accountQuotaOf } from "../accounts/accountQuotaOf";
import { accountDisabled } from "../accounts/choiceEntries";
import { isLive } from "../sessions/sessionStatus";
import { exhaustedQuotaWindow, isNotLoggedIn } from "../accounts/quotaFormat";
import { applyDaemonEvent } from "../store";
import type { FixtureError } from "./sampleFiles";
import { minutesAgo } from "./builders";
import type { AccountMcpAnswer, McpServerAuthAnswer } from "./fixture";
import { neverReplied, normalizedPath, type Ack, type FixtureContext, type Handlers } from "./fixtureContext";

/** The readings among `quotas` that the daemon would not hold: those of an account turned off with no session that has a
 * process running on it, as it reads no quota for one. The fixture daemon drops them when an account is turned off, and
 * the gallery's test keeps scenario states from holding any, so the rule is written once. */
export function readingsOfAccountsLeftOff(
  settings: { accounts: Account[]; disabled_default_accounts?: readonly Agent[] },
  quotas: readonly AccountQuota[],
  sessions: Iterable<Session>,
): AccountQuota[] {
  const running = [...sessions].filter((session) => isLive(session.status));
  return quotas.filter((quota) =>
    accountDisabled(settings.accounts, settings.disabled_default_accounts, quota.agent, quota.account)
    && !running.some((session) => session.agent === quota.agent && session.account_id === quota.account));
}

/** Whether a held reading has a credit a use can take: the first the list names that has not expired (the list is in
 * the order they are used), or any, where the reading holds only the count. */
function usableResetCredit(reading: AccountQuota, now: number): boolean {
  const credits = reading.reset_credits;
  if (!credits) return false;
  return credits.credits === null || credits.credits.some((credit) => credit.expires_at === null || credit.expires_at > now);
}

/** A held reading without the credit a use that came back `reset` took, as the daemon edits it before it answers: one
 * fewer, the credit named off the list (the first that has not expired, and only from a reading that was read, else
 * none is named and the list is not known any more), and no `reset_credits` at all when none is left. Its windows and
 * time are as they were. */
function quotaWithoutUsedCredit(reading: AccountQuota, now: number): AccountQuota {
  const { reset_credits: credits, ...rest } = reading;
  if (!credits) return reading;
  const list = credits.credits;
  const taken = reading.unavailable === null && list ? list.findIndex((credit) => credit.expires_at === null || credit.expires_at > now) : -1;
  const available_count = credits.available_count - 1;
  if (available_count <= 0) return rest;
  return { ...rest, reset_credits: { available_count, credits: list && taken >= 0 ? list.filter((_, index) => index !== taken) : null } };
}

/** An account's reading as the daemon reads it again after a run of Codex: a `reset` leaves every window full and not
 * started, with no reset time, as the daemon holds a Codex window with nothing used and a reset a whole window away
 * (the credits as `quotaWithoutUsedCredit` left them); `no_credit` drops the credits; anything else (`nothing_to_reset`,
 * `already_redeemed`, a failed run) changes only the time. */
function quotaAfterResetCredit(reading: AccountQuota, outcome: ResetCreditOutcome | "failed"): AccountQuota {
  const now = Date.now();
  if (outcome === "no_credit") {
    const { reset_credits: _, ...rest } = reading;
    return { ...rest, fetched_at: now };
  }
  if (outcome !== "reset") return { ...reading, fetched_at: now };
  return {
    ...reading,
    fetched_at: now,
    windows: reading.windows.map((window) => ({ ...window, used_percent: 0, resets_at: null })),
  };
}

/** The staged refusals that come before Codex runs (or when it cannot be run), after which the daemon reads nothing
 * again: it reads again after every run of Codex, a failed one included. */
const NO_REREAD_REFUSALS = new Set([
  "binary_not_found",
  "reset_credit_in_progress",
  "daemon_shutting_down",
  "unknown_account",
  "agent_not_available",
  "shell_environment_timeout",
]);

/** How long after a sign-in starts the CLI has handed back its page to authorize on. */
export const LOGIN_WAITING_MS = 50;
/** How long after that a scenario's staged outcome ends the sign-in. */
export const LOGIN_ENDS_MS = 300;
/** How long after answering a use of a reset credit the fixture daemon reads the account's quota again, unless the scenario
 * says otherwise (`resetCreditReading`): after the reply has reached the window, as the real daemon's read does. */
export const RESET_CREDIT_REREAD_MS = 100;

/** What the CLI said when a login the scenario ends as `failed` did not go through. */
const LOGIN_FAILURE_DETAIL = [
  "Login failed: Request failed with status code 400",
  "    authorization endpoint rejected the device code",
  "    /Users/dev/.claude-work/مرحبا/credentials.json: permission denied",
].join("\n");

/** The failure the daemon keeps of a change that failed part-way, from the `account_sharing_failed` error a scenario stages. */
function failedSharing(error: FixtureError, share: boolean): NonNullable<AccountSharing["failure"]> {
  return {
    share,
    cause: (error.params?.cause as SharingCause | undefined) ?? "other",
    items: JSON.parse(error.params?.items ?? "[]") as NonNullable<AccountSharing["failure"]>["items"],
    detail: error.params?.detail ?? error.message,
  };
}

/** One per-agent group with `map` applied to each agent's value. */
function mappedPerAgent<T>(group: PerAgent<T>, map: (value: T) => T): PerAgent<T> {
  return Object.fromEntries(FIXTURE_AGENTS.map((agent) => [agent, map(group[agent])])) as PerAgent<T>;
}

/** The window's name in a quota refusal; mirrors the daemon's English `window_name`. */
function quotaWindowName(window: QuotaWindow): string {
  if (window.label) return window.label;
  if (window.kind === "session") return "5-hour";
  if (window.kind !== "credit_period") return "Weekly";
  const periodDays = (window.window_minutes ?? 0) / (24 * 60);
  if (periodDays >= 6 && periodDays <= 8) return "Weekly";
  return periodDays >= 28 && periodDays <= 31 ? "Monthly" : "Credits";
}

/** The refusals a session or a switch gets on an account, which the sessions' requests and the accounts' own share. */
export function createAccountGuards({ store, fail }: Pick<FixtureContext, "store" | "fail">) {
  /** The account a new session of `agent` launches under: the one named, or, when none is, the one
   * the project (else the console) resolves to. */
  const launchAccount = (owner: Console, project: Project | undefined, agent: Agent, account: string | null | undefined) => {
    const current = store.getState();
    const accountId = account === undefined
      ? project ? inheritedAccount(owner, project, agent, current.settings.accounts) : consoleAccount(owner, agent, current.settings.accounts)
      : account;
    return current.settings.accounts.find((candidate) => candidate.id === accountId && candidate.agent === agent);
  };
  /** Refuses a session on, or a switch to, an account whose last reading in the store has a session, weekly or
   * credit-period window used up (100% or more) that has not reset, as the daemon does with the reading it holds:
   * a model's weekly window never counts, an account not read yet is never refused, and of several used-up
   * windows the one that resets last (one with no reset time after all) is named, in English, with its kind, length
   * and whether the reading also has a model's weekly window, from which the app words it. */
  const refuseIfQuotaExhausted = (agent: Agent, account: Account | undefined) => {
    const current = store.getState();
    const reading = accountQuotaOf(current.accountQuotas, agent, account?.id ?? null);
    const blocking = exhaustedQuotaWindow(reading, Date.now());
    if (!blocking) return;
    const windowName = quotaWindowName(blocking);
    const modelWeekly = typeof reading === "object" && reading.windows.some((window) => window.kind === "weekly_model");
    const name = account?.name ?? current.settings.default_account_names?.[agent] ?? DEFAULT_ACCOUNT_NAME;
    return fail({
      code: "account_quota_exhausted",
      params: {
        agent: servedLabel(agent),
        account: name,
        window: windowName,
        window_kind: blocking.kind,
        ...(blocking.window_minutes != null && { window_minutes: String(blocking.window_minutes) }),
        ...(modelWeekly && { model_weekly: "true" }),
        ...(blocking.resets_at != null && { resets_at: String(blocking.resets_at) }),
      },
      message: `${name} has reached its ${servedLabel(agent)} usage limit (${windowName})`,
    });
  };
  /** Refuses a session on, a handoff to, or a switch to an account the user turned off, as the daemon does before it
   * looks at the quota: `account` is the account the launch resolves to, `undefined` the agent's default account. The
   * name in the error is the account's, the default account's being the one the user gave it (else the reserved word). */
  const refuseIfDisabled = (agent: Agent, account: Account | undefined) => {
    const current = store.getState();
    const disabled = account ? account.disabled === true : current.settings.disabled_default_accounts?.includes(agent) === true;
    if (!disabled) return;
    const name = account?.name ?? current.settings.default_account_names?.[agent] ?? DEFAULT_ACCOUNT_NAME;
    return fail({
      code: "account_disabled",
      params: { agent: servedLabel(agent), account: name },
      message: `${name} is disabled for ${servedLabel(agent)}. Choose another account, or turn it back on in Settings → Agent accounts.`,
    });
  };
  /** Refuses a session on, a handoff to, or a switch to an account whose last reading in the store says it is not logged
   * in (`quota_not_signed_in`, or `quota_config_dir_missing` for a directory that does not exist yet), as the daemon does
   * with the reading it holds, after the quota check. A reading that failed for another reason, and an account not read
   * yet, never refuse. The name is the account's, as for `refuseIfDisabled`. */
  const refuseIfNotSignedIn = (agent: Agent, account: Account | undefined) => {
    const current = store.getState();
    const reading = accountQuotaOf(current.accountQuotas, agent, account?.id ?? null);
    if (typeof reading !== "object" || !isNotLoggedIn(reading.unavailable)) return;
    const name = account?.name ?? current.settings.default_account_names?.[agent] ?? DEFAULT_ACCOUNT_NAME;
    return fail({
      code: "account_not_signed_in",
      params: { agent: servedLabel(agent), account: name },
      message: `${name} (${servedLabel(agent)}) is not logged in. Choose another account, or log in to it in Settings → Agent accounts.`,
    });
  };
  /** Refuses what the daemon turns away while a change of the sharing of `accountId` runs: a session opened on it, one
   * of its sessions resumed, a switch onto or away from it, a handoff from it. The default account (`null`) has no sharing. */
  const refuseIfSharingChanges = (accountId: string | null | undefined) => {
    if (!accountId) return;
    const current = store.getState();
    if (!current.accountSharing.get(accountId)?.progress) return;
    const name = current.settings.accounts.find((candidate) => candidate.id === accountId)?.name ?? accountId;
    return fail({ code: "account_sharing_in_progress", params: { account: accountId, name }, message: `a change of the sharing of account \`${name}\` is running` });
  };
  return { launchAccount, refuseIfQuotaExhausted, refuseIfDisabled, refuseIfNotSignedIn, refuseIfSharingChanges };
}

/** The refusals of `createAccountGuards`. */
export type AccountGuards = ReturnType<typeof createAccountGuards>;

/** The replies about agent accounts. */
export function createAccountReplies({ scenario, store, fail }: FixtureContext): Handlers {
  /** The `inspect_account_trash` requests answered so far, which a list of answers is read by. */
  let trashInspections = 0;
  /** The `read_account_mcp_inventory` requests answered so far for each account, which a list of answers is read by. */
  const mcpReads = new Map<string | null, number>();
  /** The sign-in state a succeeded sign-in or sign-out left each server in, by account and server, which the readings
   * show in place of the staged one. */
  const mcpSignIns = new Map<string, McpSignInState>();
  /** The sign-ins staged as pending, by account and server, each with its answer, which a second sign-in to the server
   * joins, and what ends it as cancelled. A pending sign-out is not held here: it cannot be cancelled. */
  const pendingMcpSignIns = new Map<string, { answer: Promise<McpServerAuthEnded>; cancel: () => void }>();
  const mcpAuthKey = (body: { account?: string | null; server: string }) => `${body.account ?? ""}\0${body.server}`;
  const ended = (outcome: Extract<McpServerAuthAnswer, { status: string }>) => ({ type: "account_mcp_server_auth_ended", id: null, outcome }) as const;
  type McpServerAuthEnded = ReturnType<typeof ended>;
  const mcpServerAuth = (body: Extract<ClientRequestBody, { type: "sign_in_account_mcp_server" | "sign_out_account_mcp_server" }>) => {
    const staged = scenario.accountMcpServerAuth;
    const answer: McpServerAuthAnswer = (staged instanceof Map ? staged.get(body.server) : staged) ?? { status: "succeeded", detail: null };
    if (answer === "pending") {
      if (body.type === "sign_out_account_mcp_server") return new Promise<McpServerAuthEnded>(() => {});
      const key = mcpAuthKey(body);
      const held = pendingMcpSignIns.get(key);
      if (held) return held.answer;
      let cancel!: () => void;
      const pending = new Promise<McpServerAuthEnded>((resolve) => {
        cancel = () => resolve(ended({ status: "cancelled", detail: null }));
      });
      pendingMcpSignIns.set(key, { answer: pending, cancel });
      return pending;
    }
    if ("code" in answer) return fail(answer);
    if (answer.status === "succeeded") mcpSignIns.set(mcpAuthKey(body), body.type === "sign_in_account_mcp_server" ? "signed_in" : "needs_sign_in");
    return ended(answer);
  };

  const changeAccount = (body: Extract<ClientRequestBody, { type: "create_account" | "update_account" }>): Ack => {
    const current = store.getState();
    const editing = body.type === "update_account" ? current.settings.accounts.find((account) => account.id === body.account) : undefined;
    if (body.type === "update_account" && !editing) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
    const agent = body.type === "create_account" ? body.agent : editing!.agent;
    const customDefault = current.settings.default_account_names?.[agent];
    if (body.name !== undefined && customDefault && body.name.trim().toLowerCase() === customDefault.toLowerCase()) {
      return fail({ code: "account_name_taken", params: { agent: servedLabel(agent), name: customDefault }, message: `${servedLabel(agent)} already has an account named ${customDefault}` });
    }
    if (body.config_dir !== undefined) {
      const dir = normalizedPath(body.config_dir.trim());
      // The daemon refuses nothing for an update that leaves the account's own directory as it is.
      const defaultDir = current.agentAvailability.get(agent)?.default_account_dir;
      const owner = dir === editing?.config_dir
        ? undefined
        : current.settings.accounts.find((account) => account.agent === agent && account.id !== editing?.id && normalizedPath(account.config_dir) === dir)?.name
          ?? (defaultDir != null && normalizedPath(defaultDir) === dir ? DEFAULT_ACCOUNT_NAME : undefined);
      if (owner !== undefined) return fail({ code: "config_dir_taken", params: { agent: servedLabel(agent), name: owner }, message: `${servedLabel(agent)} account ${owner} already uses this config directory` });
    }
    return { type: "ack", id: null };
  };

  /** The daemon reads no quota for an account that is turned off unless a session is running on it, so turning one off
   * drops the reading it held and announces the list. Turning one back on reads it again, which the fixtures do not
   * model: it waits for its first reading. The reading of an account whose last session ends is dropped by the daemon
   * a little later, which the fixtures do not model either. */
  const dropReadingsOfAccountsLeftOff = () => {
    const current = store.getState();
    const dropped = readingsOfAccountsLeftOff(current.settings, current.accountQuotas, current.sessions.values());
    if (dropped.length > 0) applyDaemonEvent(store, { type: "account_quotas_updated", account_quotas: current.accountQuotas.filter((quota) => !dropped.includes(quota)) });
  };

  return {
    set_account_disabled: (body) => {
      // `null` is the agent's default account, kept by agent in `disabled_default_accounts`; an owned account carries
      // the flag itself. Running sessions are not touched, and a request that changes nothing records nothing.
      const current = store.getState();
      if (body.account === null) {
        const held = current.settings.disabled_default_accounts ?? [];
        if (held.includes(body.agent) === body.disabled) return { type: "ack", id: null };
        const disabled_default_accounts = body.disabled ? [...held, body.agent] : held.filter((agent) => agent !== body.agent);
        store.setState({ settings: { ...current.settings, disabled_default_accounts } });
        dropReadingsOfAccountsLeftOff();
        return { type: "ack", id: null };
      }
      const account = current.settings.accounts.find((item) => item.id === body.account && item.agent === body.agent);
      if (!account) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
      if ((account.disabled === true) === body.disabled) return { type: "ack", id: null };
      store.setState({ settings: { ...current.settings, accounts: current.settings.accounts.map((item) => item.id === account.id ? { ...item, disabled: body.disabled } : item) } });
      dropReadingsOfAccountsLeftOff();
      return { type: "ack", id: null };
    },
    set_account_sharing: (body) => {
      // The default account is the directory the others share, so it has no choice. Turning
      // sharing on for an owned account whose directory is that one, under another spelling, is
      // refused the same way, and nothing is recorded. The spellings are compared lexically:
      // the fixture has no filesystem, so it does not resolve a symlink the way the daemon does.
      const current = store.getState();
      if (body.account === null) {
        return fail({ code: "default_account_not_shareable", params: {}, message: "the default account is the configuration the other accounts share" });
      }
      const account = current.settings.accounts.find((item) => item.id === body.account);
      if (!account) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
      const defaultDir = current.agentAvailability.get(account.agent)?.default_account_dir;
      const shares = account.share_default_config === true;
      if (body.share && !shares && defaultDir != null && normalizedPath(account.config_dir) === normalizedPath(defaultDir)) {
        return fail({ code: "default_account_not_shareable", params: { account: account.id, name: account.name }, message: `account \`${account.name}\`'s directory is the default account's own` });
      }
      // A change runs at once or is refused: the scenario's `accountSharing` marks an account whose change runs
      // or whose sessions or sign-in are in the way, and nothing is recorded for either.
      const state = current.accountSharing.get(account.id);
      if (state?.progress) return fail({ code: "account_sharing_in_progress", params: { account: account.id, name: account.name }, message: `a change of the sharing of account \`${account.name}\` is running` });
      if (state?.in_use && (state.in_use.sessions > 0 || state.in_use.signing_in)) {
        const { sessions, signing_in } = state.in_use;
        return fail({ code: "account_in_use", params: { account: account.id, name: account.name, sessions: String(sessions), signing_in: String(signing_in) }, message: `account \`${account.name}\` is in use` });
      }
      // Asking again for the recorded value does nothing, unless the last change did not finish: then it retries.
      if (shares === body.share && account.share_default_config_incomplete !== true) return { type: "ack", id: null };
      // A refusal other than a failed change leaves nothing recorded.
      const error = scenario.accountSharingError;
      if (error && error.code !== "account_sharing_failed") return fail(error);
      // Otherwise the choice is recorded, and the account is complete unless the scenario fails the change; a
      // failure is not rolled back, and the daemon keeps what it left until the next change starts.
      store.setState((now) => {
        const { failure: _, ...rest } = now.accountSharing.get(account.id) ?? { account: account.id };
        const accountSharing = new Map(now.accountSharing);
        const failure = error && failedSharing(error, body.share);
        const next: AccountSharing = { ...rest, ...(failure && { failure }) };
        if (next.in_use !== undefined || next.progress !== undefined || next.failure !== undefined) accountSharing.set(account.id, next);
        else accountSharing.delete(account.id);
        return {
          accountSharing,
          settings: { ...now.settings, accounts: now.settings.accounts.map((item) => item.id === account.id ? { ...item, share_default_config: body.share, share_default_config_incomplete: error !== undefined } : item) },
        };
      });
      if (error) return fail(error);
      return { type: "ack", id: null };
    },
    set_default_account_name: (body) => {
      const current = store.getState();
      const name = body.name.trim();
      const taken = name && current.settings.accounts.find((account) => account.agent === body.agent && account.name.trim().toLowerCase() === name.toLowerCase());
      if (taken) return fail({ code: "account_name_taken", params: { agent: servedLabel(body.agent), name: taken.name }, message: `${servedLabel(body.agent)} already has an account named ${taken.name}` });
      const { [body.agent]: _, ...others } = current.settings.default_account_names ?? {};
      // A blank name, or the reserved word in any letter case, clears the custom name.
      const clears = !name || name.toLowerCase() === DEFAULT_ACCOUNT_NAME.toLowerCase();
      store.setState({ settings: { ...current.settings, default_account_names: clears ? others : { ...others, [body.agent]: name } } });
      return { type: "ack", id: null };
    },
    start_account_login: (body) => {
      const current = store.getState();
      if (scenario.loginStartError) return fail(scenario.loginStartError);
      if (current.accountLogins.some((login) => login.agent === body.agent && login.account === (body.account ?? null) && (login.status === "starting" || login.status === "waiting"))) {
        return fail({ code: "login_already_running", params: { agent: servedLabel(body.agent) }, message: `${servedLabel(body.agent)} is already being signed in for this account` });
      }
      const account = body.account ?? null;
      const publish = (login: Partial<AccountLogin>) => store.setState((state) => ({
        accountLogins: [
          ...state.accountLogins.filter((held) => held.agent !== body.agent || held.account !== account),
          { agent: body.agent, account, status: "starting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null, ...login },
        ],
      }));
      publish({});
      // What the CLIs hand back: Claude Code and Codex a page to authorize on, Grok Build its device page and code.
      const grok = body.agent === "grok";
      const page = grok ? "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234" : "https://auth.example.com/authorize";
      const waiting = { status: "waiting" as const, authorization_url: page, device_code: grok ? "ABCD-1234" : null, opened_by_agent: grok };
      setTimeout(() => {
        publish(waiting);
        if (!scenario.loginOutcome) return;
        setTimeout(() => {
          const outcome = scenario.loginOutcome!;
          publish(outcome === "failed"
            ? { status: "failed", failure: { code: "login_failed", params: { agent: servedLabel(body.agent), detail: LOGIN_FAILURE_DETAIL }, message: `${servedLabel(body.agent)} reported: ${LOGIN_FAILURE_DETAIL}` } }
            : { status: outcome });
        }, LOGIN_ENDS_MS);
      }, LOGIN_WAITING_MS);
      return { type: "ack", id: null };
    },
    cancel_account_login: (body) => {
      const account = body.account ?? null;
      const running = store.getState().accountLogins.find((login) => login.agent === body.agent && login.account === account && (login.status === "starting" || login.status === "waiting"));
      if (!running) return fail({ code: "login_not_running", params: { agent: servedLabel(body.agent) }, message: `${servedLabel(body.agent)} is not being signed in for this account` });
      store.setState((state) => ({ accountLogins: state.accountLogins.map((login) => login === running ? { ...login, status: "cancelled", authorization_url: null, device_code: null } : login) }));
      return { type: "ack", id: null };
    },
    use_reset_credit: (body) => {
      const current = store.getState();
      const account = body.account ?? null;
      if (!FIXTURE_PROVIDERS.find(({ id }) => id === body.agent)?.capabilities.reset_credits) {
        return fail({ code: "provider_unsupported", params: { agent: body.agent, operation: "reset_credit" }, message: `${servedLabel(body.agent)} does not offer using a rate limit reset credit` });
      }
      if (account !== null && !current.settings.accounts.some((held) => held.id === account && held.agent === body.agent)) {
        return fail({ code: "unknown_account", params: { account }, message: `unknown account ${account}` });
      }
      const staged = scenario.resetCreditUse;
      if (staged === "pending") return neverReplied();
      const reading = accountQuotaOf(current.accountQuotas, body.agent, account);
      const held = typeof reading === "object" ? reading : undefined;
      // The daemon reads the account again after every run of Codex, a failed one included, a moment after it
      // answers: as the scenario says, or never. A refusal that comes before Codex runs reads nothing again.
      const rereading = scenario.resetCreditReading;
      const reread = (outcome: ResetCreditOutcome | "failed") => {
        if (!held || rereading === "never") return;
        setTimeout(() => {
          const latest = accountQuotaOf(store.getState().accountQuotas, body.agent, account);
          if (typeof latest !== "object") return;
          // A failed read keeps the windows and the credits as they were and carries the failure.
          const next: AccountQuota = rereading?.unavailable ? { ...latest, unavailable: rereading.unavailable } : quotaAfterResetCredit(latest, outcome);
          applyDaemonEvent(store, { type: "account_quotas_updated", account_quotas: store.getState().accountQuotas.map((quota) => quota === latest ? next : quota) });
        }, rereading?.after ?? RESET_CREDIT_REREAD_MS);
      };
      if (typeof staged === "object") {
        if (!NO_REREAD_REFUSALS.has(staged.code)) reread("failed");
        return fail(staged);
      }
      // Without a staged outcome Codex uses a credit when the reading holds one it can (not expired) and says none
      // is left otherwise.
      const outcome = staged ?? (held && usableResetCredit(held, Date.now()) ? "reset" : "no_credit");
      // Only a use that came back `reset` spends a credit: it takes it off the held reading, windows as they were,
      // before the daemon answers; the reading that follows brings the new windows.
      if (held && outcome === "reset") {
        applyDaemonEvent(store, { type: "account_quotas_updated", account_quotas: current.accountQuotas.map((quota) => quota === held ? quotaWithoutUsedCredit(held, Date.now()) : quota) });
      }
      reread(outcome);
      return { type: "reset_credit_used", id: null, outcome };
    },
    read_account_mcp_inventory: (body) => {
      const account = body.account ?? null;
      const staged = scenario.accountMcpInventory;
      const stagedFor = staged instanceof Map ? staged.get(account) : staged;
      const reads = mcpReads.get(account) ?? 0;
      mcpReads.set(account, reads + 1);
      const answers = (Array.isArray(stagedFor) ? stagedFor : [stagedFor]) as (AccountMcpAnswer | undefined)[];
      const answer = answers[Math.min(reads, answers.length - 1)] ?? {};
      if (answer === "pending") return neverReplied();
      if ("code" in answer) return fail(answer);
      const { ageMinutes, connectors, servers, unavailable } = answer;
      return {
        type: "account_mcp_inventory_read",
        id: null,
        inventory: {
          agent: body.agent,
          account,
          read_at: ageMinutes ? minutesAgo(ageMinutes) : Date.now(),
          connectors: connectors ?? [],
          servers: (servers ?? []).map((server) => ({ ...server, sign_in: mcpSignIns.get(mcpAuthKey({ account, server: server.name })) ?? server.sign_in })),
          unavailable: unavailable ?? null,
        },
      };
    },
    sign_in_account_mcp_server: mcpServerAuth,
    sign_out_account_mcp_server: mcpServerAuth,
    cancel_account_mcp_sign_in: (body) => {
      const held = pendingMcpSignIns.get(mcpAuthKey(body));
      if (!held) return fail({ code: "mcp_sign_in_not_running", params: { agent: body.agent, server: body.server }, message: `no sign-in to ${body.server} is running` });
      pendingMcpSignIns.delete(mcpAuthKey(body));
      held.cancel();
      return { type: "ack", id: null };
    },
    create_account: changeAccount,
    update_account: changeAccount,
    inspect_account_trash: (body) => {
      if (!store.getState().settings.accounts.some((account) => account.id === body.account)) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
      const answers = scenario.accountTrashInspection;
      const inspection = (Array.isArray(answers) ? answers[Math.min(trashInspections, answers.length - 1)] : answers) ?? { status: "ok" } as const;
      trashInspections += 1;
      if (inspection === "pending") return neverReplied();
      if ("code" in inspection) return fail(inspection);
      return { type: "account_trash_inspected", id: null, outcome: inspection };
    },
    delete_account: (body) => {
      const current = store.getState();
      if (!current.settings.accounts.some((account) => account.id === body.account)) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
      if (body.trash_directory && scenario.accountDeleteError) return fail(scenario.accountDeleteError);
      const consoles = new Map([...current.consoles].map(([id, console]) => [id, { ...console, accounts: mappedPerAgent(console.accounts, (account) => account === body.account ? null : account) }]));
      const projects = new Map([...current.projects].map(([id, project]) => [id, { ...project, accounts: mappedPerAgent(project.accounts, (account): ProjectAccount => account.kind === "account" && account.id === body.account ? { kind: "inherit" } : account) }]));
      // An account that shares has its sharing turned off first, at once; when that does not finish, the account stays,
      // incomplete, with the failure kept, as `set_account_sharing` leaves it.
      const target = current.settings.accounts.find((account) => account.id === body.account)!;
      const error = scenario.accountSharingError;
      if (error?.code === "account_sharing_failed" && (target.share_default_config === true || target.share_default_config_incomplete === true)) {
        store.setState((now) => ({
          accountSharing: new Map(now.accountSharing).set(target.id, { account: target.id, failure: failedSharing(error, false) }),
          settings: { ...now.settings, accounts: now.settings.accounts.map((item) => item.id === target.id ? { ...item, share_default_config: false, share_default_config_incomplete: true } : item) },
        }));
        return fail(error);
      }
      store.setState({ consoles, projects, settings: { ...current.settings, accounts: current.settings.accounts.filter((account) => account.id !== body.account) } });
      return { type: "ack", id: null };
    },
    reorder_accounts: (body) => {
      const current = store.getState();
      const expected: (string | null)[] = [null, ...current.settings.accounts.filter((account) => account.agent === body.agent).map((account) => account.id)];
      if (body.accounts.length !== expected.length || !expected.every((id) => body.accounts.filter((given) => given === id).length === 1)) {
        return fail({ code: "account_order_invalid", params: {}, message: "the new order must name each account of the agent and its default exactly once" });
      }
      const held = current.settings.account_order?.[body.agent];
      if (!held || held.length !== body.accounts.length || held.some((id, index) => body.accounts[index] !== id)) {
        applyDaemonEvent(store, { type: "settings_updated", settings: { ...current.settings, account_order: { ...current.settings.account_order, [body.agent]: body.accounts } } });
      }
      return { type: "ack", id: null };
    },
  };
}
