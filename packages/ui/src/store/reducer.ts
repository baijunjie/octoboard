import type { ConnectionState } from "../daemon-client";
import { accountQuotaOf } from "../accounts/accountQuotaOf";
import { accountKey, loginIsActive } from "../accounts/accountRecords";
import { sameValue } from "../values/sameValue";
import { type AccountLogin, type AccountQuota, type AccountSharing, type Agent, type AgentCliStatus, type Event, type Page, type Project, type ScheduledTask, type ScheduledTaskRun, type ShellTerminal } from "../protocol";
import { CODE } from "../errors/codes";
import type { PageSubmission, State, TrustPrompt } from "./state";
import { sessionRequestOf, withRequest } from "./sessionRequests";

export type Action =
  | { kind: "connection"; state: ConnectionState }
  | { kind: "retry_pending"; pending: boolean }
  | { kind: "event"; event: Event }
  | { kind: "dismiss_trust_prompt"; session: string }
  | { kind: "track_account_login"; key: string; tracked: boolean }
  | { kind: "track_agent_cli_update"; agent: Agent; from: number | null | undefined }
  | { kind: "track_pages_listing"; session: string; listing: boolean }
  | { kind: "dismiss_account_login"; login: AccountLogin }
  | { kind: "queue_account_switches"; ids: readonly string[]; queued: boolean }
  | { kind: "await_restart"; awaited: boolean }
  | { kind: "native_apps"; agents: readonly Agent[]; asked: readonly string[]; seen: readonly string[] };

/** The map of `records` by `idOf`, each one the record `previous` already holds for its id when that
 * is the same data. A snapshot (the first, or a lag recovery) names everything again, and what it
 * repeats unchanged must not look new to the components holding it. */
function keepSame<T>(previous: Map<string, T>, records: T[], idOf: (record: T) => string): Map<string, T> {
  return new Map(
    records.map((record) => {
      const id = idOf(record);
      const held = previous.get(id);
      return [id, held !== undefined && sameValue(held, record) ? held : record];
    }),
  );
}

/** The scheduled tasks without those `gone` matches, and the runs without theirs. */
function tasksWithout(state: State, gone: (task: ScheduledTask) => boolean): Pick<State, "scheduledTasks" | "scheduledTaskRuns"> {
  const scheduledTasks = withoutTasks(state.scheduledTasks, gone);
  return { scheduledTasks, scheduledTaskRuns: runsOfTasks(state.scheduledTaskRuns, scheduledTasks) };
}

/** `tasks` without those `gone` matches (a deleted console's or project's, which the daemon's database takes with it
 * unannounced); `tasks` itself when none does, or while none has been listed. */
function withoutTasks(tasks: Map<string, ScheduledTask> | undefined, gone: (task: ScheduledTask) => boolean): Map<string, ScheduledTask> | undefined {
  if (!tasks || ![...tasks.values()].some(gone)) return tasks;
  return new Map([...tasks].filter(([, task]) => !gone(task)));
}

/** The most runs of a task the daemon keeps (`SCHEDULED_TASK_RUNS_KEPT`), past which an added run pushes out the oldest. */
const RUNS_KEPT = 50;

/** `runs` of only the tasks `tasks` still holds; `runs` itself when none is gone, or while no task has been listed. */
function runsOfTasks(runs: Map<string, ScheduledTaskRun[]>, tasks: Map<string, ScheduledTask> | undefined): Map<string, ScheduledTaskRun[]> {
  if (!tasks || [...runs.keys()].every((id) => tasks.has(id))) return runs;
  return new Map([...runs].filter(([id]) => tasks.has(id)));
}

/** The CLI updates this window asked for (`cliUpdatesStartedHere`) that a snapshot leaves open: one the status shows
 * running, and one it shows ended as an update other than the one the status held when the request went out (`from`), so
 * that its failure still reaches the alert. One with no update in the status, or whose update is still that earlier one,
 * is dropped: the request never reached the daemon, or its answer was lost with the connection. `asked` itself when none
 * is dropped. */
function keepCliUpdatesStillOpen(asked: Map<Agent, number | null>, statuses: AgentCliStatus[]): Map<Agent, number | null> {
  const open = [...asked].filter(([agent, from]) => {
    const update = statuses.find((status) => status.agent === agent)?.update;
    return update !== undefined && update !== null && (update.state === "running" || update.started_at !== from);
  });
  return open.length === asked.size ? asked : new Map(open);
}

/** `quotas` with each entry replaced by the one `previous` already holds for its account when that
 * is the same data: the list arrives whole after every reading, and a component showing one
 * account must not render again for another account's reading. */
function keepSameQuotas(previous: AccountQuota[], quotas: AccountQuota[]): AccountQuota[] {
  return quotas.map((quota) => {
    const held = accountQuotaOf(previous, quota.agent, quota.account);
    return held !== undefined && sameValue(held, quota) ? held : quota;
  });
}

/** `next` with each field the value `previous` already holds when it is the same data, or `previous` itself when every
 * field is: the settings arrive whole on every change of any one of them, and a component reading one field (the
 * accounts every session row reads) must not render again for a change of another. */
function keepSameFields<T extends object>(previous: T, next: T): T {
  const keys = Object.keys(next) as (keyof T)[];
  const kept = Object.fromEntries(keys.map((key) => [key, sameValue(previous[key], next[key]) ? previous[key] : next[key]])) as T;
  const unchanged = keys.length === Object.keys(previous).length && keys.every((key) => kept[key] === previous[key]);
  return unchanged ? previous : kept;
}

/** Whether `sharing` says anything beyond naming the account; one that does not means the account is free. */
function hasSharingState(sharing: AccountSharing): boolean {
  return sharing.in_use !== undefined || sharing.progress !== undefined || sharing.idle_sessions !== undefined || sharing.failure !== undefined;
}

/** `held` with `id` in it or not, as `member` says; `held` itself when that changes nothing. */
function withMember(held: ReadonlySet<string>, id: string, member: boolean): ReadonlySet<string> {
  if (held.has(id) === member) return held;
  const next = new Set(held);
  if (member) next.add(id);
  else next.delete(id);
  return next;
}

/** The shell terminals of the projects still in `projects`. The daemon ends a removed project's
 * terminals and says so with a `shell_terminal_deleted` for each, but a client that kept them until
 * then would offer, for a moment, a terminal of a project that is gone. */
function withoutRemovedProjectTerminals(terminals: Map<string, ShellTerminal>, projects: Map<string, Project>): Map<string, ShellTerminal> {
  if (Array.from(terminals.values()).every((terminal) => projects.has(terminal.project_id))) return terminals;
  return new Map(Array.from(terminals).filter(([, terminal]) => projects.has(terminal.project_id)));
}

/** `page` with the submission of `heard` when that is a later revision than its own, else `page` itself. The daemon raises
 * a page's revision with every mark and clearing, and its announcements of one page (a `page_list` computed at some
 * moment, each `page_submission`) can arrive in any order, so only the revision tells which is the later. */
function withLaterSubmission(page: Page, heard: PageSubmission | undefined): Page {
  if (!heard || heard.submission_revision <= page.submission_revision) return page;
  return { ...page, submission: heard.submission, submission_revision: heard.submission_revision };
}

/** `early` without the entries for the pages in `ids`; `early` itself when it has none of them. */
function withoutEarlySubmissions(early: ReadonlyMap<string, PageSubmission> | undefined, ids: Iterable<string>): ReadonlyMap<string, PageSubmission> | undefined {
  if (!early) return early;
  const left = new Map(early);
  for (const id of ids) left.delete(id);
  return left.size === early.size ? early : left;
}

/** `state.earlySubmissions` with the entry of `session` set to `early`, dropped when that is empty. */
function withEarlySubmissionsOf(state: State, session: string, early: ReadonlyMap<string, PageSubmission> | undefined): Map<string, ReadonlyMap<string, PageSubmission>> {
  if (early === state.earlySubmissions.get(session)) return state.earlySubmissions;
  const earlySubmissions = new Map(state.earlySubmissions);
  if (early && early.size > 0) earlySubmissions.set(session, early);
  else earlySubmissions.delete(session);
  return earlySubmissions;
}

/** `state` without the sessions in `ids` and what is held for them: their pages, the pages they had
 * deleted, and any confirmation, request or account switch of theirs, none of which anyone can act
 * on once the session is gone. The daemon announces a session's deletion through one event for the
 * session itself, and through only the console's or the project's when that takes its sessions
 * with it, so each of those cascades goes through here. */
function withoutSessions(state: State, ids: ReadonlySet<string>): State {
  const kept = ([id]: [string, unknown]) => !ids.has(id);
  return {
    ...state,
    sessions: new Map(Array.from(state.sessions).filter(kept)),
    pages: new Map(Array.from(state.pages).filter(kept)),
    deletedPages: new Map(Array.from(state.deletedPages).filter(kept)),
    earlySubmissions: new Map(Array.from(state.earlySubmissions).filter(kept)),
    pagesListing: new Set(Array.from(state.pagesListing).filter((id) => !ids.has(id))),
    trustPrompts: state.trustPrompts.filter((p) => !ids.has(p.session)),
    sessionRequests: state.sessionRequests.filter((r) => !ids.has(r.session)),
  };
}

export function reducer(state: State, action: Action): State {
  switch (action.kind) {
    case "connection":
      // A dropped connection forgets the daemon its hello described: the next one may meet another daemon, which
      // greets it again, or sends no hello at all.
      return { ...state, connectionState: action.state, daemon: action.state === "reconnecting" || action.state === "closed" ? null : state.daemon };
    case "retry_pending":
      return { ...state, retryPending: action.pending };
    case "dismiss_trust_prompt":
      return { ...state, trustPrompts: state.trustPrompts.filter((p) => p.session !== action.session) };
    case "track_account_login": {
      const held = state.loginsStartedHere.includes(action.key);
      if (held === action.tracked) return state;
      return { ...state, loginsStartedHere: action.tracked ? [...state.loginsStartedHere, action.key] : state.loginsStartedHere.filter((key) => key !== action.key) };
    }
    case "track_pages_listing": {
      const pagesListing = withMember(state.pagesListing, action.session, action.listing);
      return pagesListing === state.pagesListing ? state : { ...state, pagesListing };
    }
    case "track_agent_cli_update": {
      if (state.cliUpdatesStartedHere.get(action.agent) === action.from) return state;
      const cliUpdatesStartedHere = new Map(state.cliUpdatesStartedHere);
      if (action.from === undefined) cliUpdatesStartedHere.delete(action.agent);
      else cliUpdatesStartedHere.set(action.agent, action.from);
      return { ...state, cliUpdatesStartedHere };
    }
    case "dismiss_account_login":
      return state.accountLogins.includes(action.login) ? { ...state, accountLogins: state.accountLogins.filter((held) => held !== action.login) } : state;
    case "queue_account_switches": {
      // Held apart from `sessions`, which stay as the daemon sent them; what a session shows applies it
      // (`holdsAccountSwitch`).
      const queuedAccountSwitches = action.ids.reduce((held, id) => withMember(held, id, action.queued), state.queuedAccountSwitches);
      return queuedAccountSwitches === state.queuedAccountSwitches ? state : { ...state, queuedAccountSwitches };
    }
    case "native_apps": {
      const seen = new Set(action.seen);
      const unseen = new Set(state.nativeAppsUnseen);
      for (const id of action.asked) {
        if (seen.has(id)) unseen.delete(id);
        else unseen.add(id);
      }
      const apps = sameValue(state.nativeApps, action.agents) ? state.nativeApps : action.agents;
      const held = unseen.size === state.nativeAppsUnseen.size && [...unseen].every((id) => state.nativeAppsUnseen.has(id)) ? state.nativeAppsUnseen : unseen;
      return apps === state.nativeApps && held === state.nativeAppsUnseen ? state : { ...state, nativeApps: apps, nativeAppsUnseen: held };
    }
    case "await_restart":
      return state.restartAwaited === action.awaited ? state : { ...state, restartAwaited: action.awaited };
    case "event": {
      const event = action.event;
      switch (event.type) {
        case "hello":
          return { ...state, daemon: { version: event.daemon_version, generation: event.generation }, daemonUpdateStale: true, restartAwaited: false };
        case "snapshot":
          return {
            ...state,
            hosts: new Map(event.hosts.map((h) => [h.id, h])),
            consoles: keepSame(state.consoles, event.consoles, (c) => c.id),
            projects: keepSame(state.projects, event.projects, (p) => p.id),
            sessions: keepSame(state.sessions, event.sessions, (s) => s.id),
            // A snapshot means either first connect or a lag recovery, and carries no pages either
            // way (see `page_list` in apps/daemon/PROTOCOL.md). Cleared rather than kept, because a console session
            // that was deleted while this client was behind would otherwise go on holding its
            // pages forever — nothing else ever removes an entry for a session the snapshot no
            // longer lists.
            pages: new Map(),
            deletedPages: new Map(),
            earlySubmissions: new Map(),
            pagesListing: new Set(),
            snapshotEpoch: state.snapshotEpoch + 1,
            trustPrompts: [],
            trustedDirectories: event.trusted_directories,
            sessionRequests: [],
            gitStatuses: keepSame(state.gitStatuses, event.git_statuses, (s) => s.project),
            settings: keepSameFields(state.settings, event.settings),
            providers: sameValue(state.providers, event.providers) ? state.providers : event.providers,
            agentAvailability: new Map(event.agent_availability.map((a) => [a.agent, a])),
            agentCliStatuses: keepSame(state.agentCliStatuses, event.agent_cli_statuses, (s) => s.agent),
            cliUpdatesStartedHere: keepCliUpdatesStillOpen(state.cliUpdatesStartedHere, event.agent_cli_statuses),
            accountQuotas: keepSameQuotas(state.accountQuotas, event.account_quotas),
            accountLogins: event.account_logins,
            loginsStartedHere: state.loginsStartedHere.filter((key) => event.account_logins.some((login) => accountKey(login.agent, login.account) === key)),
            accountSharing: keepSame(state.accountSharing, event.account_sharing.filter(hasSharingState), (sharing) => sharing.account),
            homeDir: event.home_dir,
            host: sameValue(state.host, event.host) ? state.host : event.host,
            keepAwakeHeld: event.keep_awake_held,
            daemonUpdate: sameValue(state.daemonUpdate, event.daemon_update) ? state.daemonUpdate : event.daemon_update,
            daemonUpdateStale: false,
            remoteAccess: sameValue(state.remoteAccess, event.remote_access) ? state.remoteAccess : event.remote_access,
            shellTerminals: keepSame(state.shellTerminals, event.shell_terminals, (t) => t.id),
          };
        case "trusted_directories_updated":
          // The prompts queued are left alone: whether a project's directory really lies under a
          // trusted one is the daemon's call, and it tells this client through the session, whose
          // trust wait ends (`session_upserted`).
          return { ...state, trustedDirectories: event.trusted_directories };
        case "console_upserted": {
          if (sameValue(state.consoles.get(event.console.id), event.console)) return state;
          const consoles = new Map(state.consoles);
          consoles.set(event.console.id, event.console);
          return { ...state, consoles };
        }
        case "console_deleted": {
          const consoles = new Map(state.consoles);
          consoles.delete(event.console);
          // The daemon's database takes the console's projects and sessions with it, and says so
          // only through this one event — a client that kept them would go on offering a terminal
          // and a Resume for a session that no longer exists anywhere.
          const projects = new Map(
            Array.from(state.projects).filter(([, p]) => p.console_id !== event.console),
          );
          const removed = new Set(
            Array.from(state.sessions.values()).filter((s) => s.console_id === event.console).map((s) => s.id),
          );
          // Same cascade one level further down: a git status for a project the console took
          // with it has nothing left to be about.
          const gitStatuses = new Map(
            Array.from(state.gitStatuses).filter(([projectId]) => projects.has(projectId)),
          );
          return {
            ...withoutSessions(state, removed),
            consoles,
            projects,
            gitStatuses,
            shellTerminals: withoutRemovedProjectTerminals(state.shellTerminals, projects),
            ...tasksWithout(state, (task) => task.console_id === event.console),
          };
        }
        case "session_opened": {
          // A title or status broadcast can arrive before this launch-time reply. Keep the
          // authoritative record if one already exists; otherwise seed the newly opened session.
          if (state.sessions.has(event.session.id)) return state;
          const sessions = new Map(state.sessions);
          sessions.set(event.session.id, event.session);
          return { ...state, sessions };
        }
        case "project_upserted": {
          if (sameValue(state.projects.get(event.project.id), event.project)) return state;
          const projects = new Map(state.projects);
          projects.set(event.project.id, event.project);
          // A project that now has the permission has nothing left to ask: the daemon presses its
          // waiting confirmations itself, whichever agent shows them.
          const trustPrompts = event.project.trust_consent
            ? state.trustPrompts.filter((p) => p.project !== event.project.id)
            : state.trustPrompts;
          return { ...state, projects, trustPrompts };
        }
        case "project_deleted": {
          const projects = new Map(state.projects);
          projects.delete(event.project);
          // Same cascade as a deleted console, one level down.
          const removed = new Set(
            Array.from(state.sessions.values()).filter((s) => s.project_id === event.project).map((s) => s.id),
          );
          // The daemon sends no event for this; see `gitStatuses` above.
          const gitStatuses = new Map(state.gitStatuses);
          gitStatuses.delete(event.project);
          return {
            ...withoutSessions(state, removed),
            projects,
            gitStatuses,
            shellTerminals: withoutRemovedProjectTerminals(state.shellTerminals, projects),
            ...tasksWithout(state, (task) => task.project_id === event.project),
          };
        }
        case "scheduled_tasks_listed": {
          // The whole list, which replaces what is held; records that did not change keep their identity.
          const scheduledTasks = keepSame(state.scheduledTasks ?? new Map(), event.tasks, (task) => task.id);
          return { ...state, scheduledTasks, scheduledTasksZone: event.time_zone, scheduledTaskRuns: runsOfTasks(state.scheduledTaskRuns, scheduledTasks) };
        }
        case "scheduled_task_saved":
        case "scheduled_task_upserted": {
          // A save's reply and its broadcast both carry the task, but the daemon broadcasts before it replies and the two
          // are not ordered on the socket: the reply only seeds a task this window lacks, never overwrites a held one, which
          // may already be newer (another window's edit, a run recorded). Before the first listing nothing is held: the
          // listing that follows on the same ordered socket includes the change, and a map of one task would be taken for
          // the daemon's whole list.
          if (!state.scheduledTasks) return state;
          const held = state.scheduledTasks.get(event.task.id);
          if (event.type === "scheduled_task_saved" ? held !== undefined : sameValue(held, event.task)) return state;
          const scheduledTasks = new Map(state.scheduledTasks);
          scheduledTasks.set(event.task.id, event.task);
          return { ...state, scheduledTasks };
        }
        case "scheduled_task_deleted": {
          if (!state.scheduledTasks?.has(event.task)) return state;
          const scheduledTasks = new Map(state.scheduledTasks);
          scheduledTasks.delete(event.task);
          return { ...state, scheduledTasks, scheduledTaskRuns: runsOfTasks(state.scheduledTaskRuns, scheduledTasks) };
        }
        case "scheduled_task_runs_listed": {
          // The whole list, which replaces what is held; not for a task this window knows to be gone.
          if (state.scheduledTasks && !state.scheduledTasks.has(event.task)) return state;
          if (sameValue(state.scheduledTaskRuns.get(event.task), event.runs)) return state;
          const scheduledTaskRuns = new Map(state.scheduledTaskRuns);
          scheduledTaskRuns.set(event.task, event.runs);
          return { ...state, scheduledTaskRuns };
        }
        case "scheduled_task_run_added": {
          // Only to a task whose runs are listed: the listing of another brings this run with it. A run already held (the
          // listing answered after it was recorded) is not added twice.
          const held = state.scheduledTasks && state.scheduledTaskRuns.get(event.run.task_id);
          if (!held || held.some((run) => run.id === event.run.id)) return state;
          const scheduledTaskRuns = new Map(state.scheduledTaskRuns);
          scheduledTaskRuns.set(event.run.task_id, [event.run, ...held].slice(0, RUNS_KEPT));
          return { ...state, scheduledTaskRuns };
        }
        case "session_upserted": {
          // A session's status broadcasts repeat what the store holds as often as they change it.
          if (sameValue(state.sessions.get(event.session.id), event.session)) return state;
          const sessions = new Map(state.sessions);
          sessions.set(event.session.id, event.session);
          // The daemon shows a session at its trust confirmation as `waiting_user`, and publishes
          // that before it broadcasts the prompt, so a status that is anything else, which a session
          // that no longer runs has too, means the confirmation is no longer waiting for an answer
          // here (answered, covered by a permission or a trusted directory, or gone). A prompt not
          // yet queued is untouched, and one that is is dropped only by its own session's upsert.
          const trustPrompts = event.session.status === "waiting_user"
            ? state.trustPrompts
            : state.trustPrompts.filter((p) => p.session !== event.session.id);
          return { ...state, sessions, trustPrompts };
        }
        case "session_deleted":
          return withoutSessions(state, new Set([event.session]));
        case "trust_prompt": {
          if (state.trustPrompts.some((p) => p.session === event.session)) return state;
          // The daemon sends only a confirmation nobody has the permission for, judging a trusted
          // directory by where the project really is on disk, which this client cannot see.
          if (state.projects.get(event.project)?.trust_consent) return state;
          const prompt: TrustPrompt = {
            session: event.session,
            agent: event.agent,
            project: event.project,
            path: event.path,
            trustDir: event.trust_dir,
          };
          return { ...state, trustPrompts: [...state.trustPrompts, prompt] };
        }
        case "session_notice": {
          // Octoboard could not press the session's confirmation: it is left for the person to answer
          // in the terminal, which the notice says, so a prompt still queued for it would ask about
          // something no go-ahead can do any more.
          if (event.code !== CODE.trust_answer_failed) return state;
          const trustPrompts = state.trustPrompts.filter((p) => p.session !== event.session);
          return trustPrompts.length === state.trustPrompts.length ? state : { ...state, trustPrompts };
        }
        case "session_request": {
          if (state.sessionRequests.some((r) => r.requestId === event.request_id)) return state;
          const request = sessionRequestOf(event);
          if (!request) return state;
          return { ...state, sessionRequests: withRequest(state.sessionRequests, request) };
        }
        case "session_request_closed": {
          const sessionRequests = state.sessionRequests.filter((r) => r.requestId !== event.request_id);
          return sessionRequests.length === state.sessionRequests.length ? state : { ...state, sessionRequests };
        }
        case "page_list": {
          // The reply to a `list_pages` request, but requests and broadcasts are not ordered
          // against each other (see "Client to daemon" and "Daemon to client" in
          // `apps/daemon/PROTOCOL.md`), so a `page_created` for a page `show_page` inserted after this
          // reply was computed can have already landed here. Replacing wholesale would drop that
          // page; instead keep the reply's order first, then append whatever locally held page
          // the reply is missing.
          // A `page_created` is always the newest page, so appending after keeps the merged order
          // correct. Pages can also be deleted, and a reply computed before a deletion can be
          // applied after its `pages_deleted`: the deleted ids are remembered and filtered out.
          // A listed page's submission may be older than the one held, or than a `page_submission`
          // heard before the page was held: of the three, the latest revision is kept.
          const existing = state.pages.get(event.session_id) ?? [];
          const deleted = state.deletedPages.get(event.session_id);
          const early = state.earlySubmissions.get(event.session_id);
          const held = new Map(existing.map((p) => [p.id, p]));
          const listed = (deleted ? event.pages.filter((p) => !deleted.has(p.id)) : event.pages).map((p) =>
            withLaterSubmission(withLaterSubmission(p, held.get(p.id)), early?.get(p.id)),
          );
          const seen = new Set(listed.map((p) => p.id));
          const pages = new Map(state.pages);
          pages.set(event.session_id, [...listed, ...existing.filter((p) => !seen.has(p.id))]);
          const earlySubmissions = withEarlySubmissionsOf(state, event.session_id, withoutEarlySubmissions(early, seen));
          return { ...state, pages, earlySubmissions, pagesListing: withMember(state.pagesListing, event.session_id, false) };
        }
        case "page_created": {
          // Appended even with no baseline yet: that window is exactly the first `list_pages`
          // being in flight, and dropping the event here would mean waiting for a second push to
          // ever see it. The transient "1 / 1" this produces is corrected once that reply's merge
          // (above) lands.
          // A `page_submission` for the page can come before it, from a window that listed the page as soon as it
          // was stored.
          const session = event.page.session_id;
          const existing = state.pages.get(session) ?? [];
          const early = state.earlySubmissions.get(session);
          const pages = new Map(state.pages);
          pages.set(session, [...existing, withLaterSubmission(event.page, early?.get(event.page.id))]);
          const earlySubmissions = withEarlySubmissionsOf(state, session, withoutEarlySubmissions(early, [event.page.id]));
          return { ...state, pages, earlySubmissions };
        }
        case "page_submission": {
          // Applied only when it is a later revision than the page's: a late announcement of an earlier change
          // (a clearing heard after the next mark) must not undo the later one.
          const heard: PageSubmission = { submission: event.submission, submission_revision: event.submission_revision };
          const existing = state.pages.get(event.session_id);
          const index = existing?.findIndex((p) => p.id === event.page) ?? -1;
          if (!existing || index === -1) {
            // A page this window does not hold yet (its list is still in flight, or computed before the page was
            // stored): remembered until the page arrives, as the list may carry an earlier submission than this.
            // Only for a session this window lists or holds the pages of: for any other the entry would never be
            // taken, and a window hears the submissions of every session.
            if (!existing && !state.pagesListing.has(event.session_id)) return state;
            const early = state.earlySubmissions.get(event.session_id);
            const remembered = early?.get(event.page);
            if (remembered && remembered.submission_revision >= heard.submission_revision) return state;
            return { ...state, earlySubmissions: withEarlySubmissionsOf(state, event.session_id, new Map(early).set(event.page, heard)) };
          }
          const page = withLaterSubmission(existing[index], heard);
          if (page === existing[index]) return state;
          const pages = new Map(state.pages);
          pages.set(event.session_id, existing.map((p, i) => (i === index ? page : p)));
          return { ...state, pages };
        }
        case "pages_deleted": {
          // Kept as an empty list rather than dropped when none is left: the session's pages are
          // still known, and there are none.
          // The ids are remembered even with no list yet, as the reply in flight may hold them.
          const gone = new Set([...(state.deletedPages.get(event.session_id) ?? []), ...event.pages]);
          const deletedPages = new Map(state.deletedPages);
          deletedPages.set(event.session_id, gone);
          const earlySubmissions = withEarlySubmissionsOf(state, event.session_id, withoutEarlySubmissions(state.earlySubmissions.get(event.session_id), event.pages));
          const existing = state.pages.get(event.session_id);
          if (!existing) return { ...state, deletedPages, earlySubmissions };
          const pages = new Map(state.pages);
          pages.set(event.session_id, existing.filter((page) => !gone.has(page.id)));
          return { ...state, pages, deletedPages, earlySubmissions };
        }
        case "project_git_status": {
          if (sameValue(state.gitStatuses.get(event.status.project), event.status)) return state;
          const gitStatuses = new Map(state.gitStatuses);
          gitStatuses.set(event.status.project, event.status);
          return { ...state, gitStatuses };
        }
        case "shell_terminal_upserted": {
          const terminal = event.shell_terminal;
          if (sameValue(state.shellTerminals.get(terminal.id), terminal)) return state;
          const shellTerminals = new Map(state.shellTerminals);
          shellTerminals.set(terminal.id, terminal);
          return { ...state, shellTerminals };
        }
        case "shell_terminal_deleted": {
          if (!state.shellTerminals.has(event.shell_terminal)) return state;
          const shellTerminals = new Map(state.shellTerminals);
          shellTerminals.delete(event.shell_terminal);
          return { ...state, shellTerminals };
        }
        case "settings_updated": {
          const settings = keepSameFields(state.settings, event.settings);
          return settings === state.settings ? state : { ...state, settings };
        }
        case "keep_awake_held_updated":
          return state.keepAwakeHeld === event.keep_awake_held ? state : { ...state, keepAwakeHeld: event.keep_awake_held };
        case "daemon_update_status_updated":
          return sameValue(state.daemonUpdate, event.status) && !state.daemonUpdateStale ? state : { ...state, daemonUpdate: event.status, daemonUpdateStale: false };
        case "remote_access_updated":
          return sameValue(state.remoteAccess, event.remote_access) ? state : { ...state, remoteAccess: event.remote_access };
        case "agent_availability_updated":
          return { ...state, agentAvailability: new Map(event.agent_availability.map((a) => [a.agent, a])) };
        case "agent_cli_status_updated": {
          if (sameValue(state.agentCliStatuses.get(event.status.agent), event.status)) return state;
          const agentCliStatuses = new Map(state.agentCliStatuses);
          agentCliStatuses.set(event.status.agent, event.status);
          return { ...state, agentCliStatuses };
        }
        case "account_quotas_updated":
          return { ...state, accountQuotas: keepSameQuotas(state.accountQuotas, event.account_quotas) };
        case "account_sharing": {
          const { type: _type, ...sharing } = event;
          const accountSharing = new Map(state.accountSharing);
          if (hasSharingState(sharing)) accountSharing.set(sharing.account, sharing);
          else accountSharing.delete(sharing.account);
          return { ...state, accountSharing };
        }
        case "account_login_updated": {
          const { login } = event;
          const others = state.accountLogins.filter((held) => held.agent !== login.agent || held.account !== login.account);
          const key = accountKey(login.agent, login.account);
          const running = loginIsActive(login);
          return {
            ...state,
            accountLogins: [...others, login],
            loginsStartedHere: running ? state.loginsStartedHere : state.loginsStartedHere.filter((held) => held !== key),
          };
        }
        default:
          return state;
      }
    }
    default:
      return state;
  }
}
