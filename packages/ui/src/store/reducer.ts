import type { ConnectionState } from "../daemon-client";
import { accountQuotaOf } from "../accountQuotaOf";
import { sameValue } from "../sameValue";
import type { AccountLogin, AccountQuota, AccountSharing, Event, Project, Session, ShellTerminal } from "../protocol";
import { accountLoginKey, type State, type TrustPrompt } from "./state";
import { sessionRequestOf, withRequest } from "./sessionRequests";

export type Action =
  | { kind: "connection"; state: ConnectionState }
  | { kind: "event"; event: Event }
  | { kind: "dismiss_trust_prompt"; session: string }
  | { kind: "track_account_login"; key: string; tracked: boolean }
  | { kind: "dismiss_account_login"; login: AccountLogin }
  | { kind: "queue_account_switches"; ids: readonly string[]; queued: boolean };

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

/** `quotas` with each entry replaced by the one `previous` already holds for its account when that
 * is the same data: the list arrives whole after every reading, and a component showing one
 * account must not render again for another account's reading. */
function keepSameQuotas(previous: AccountQuota[], quotas: AccountQuota[]): AccountQuota[] {
  return quotas.map((quota) => {
    const held = accountQuotaOf(previous, quota.agent, quota.account);
    return held !== undefined && sameValue(held, quota) ? held : quota;
  });
}

/** Whether `sharing` says anything beyond naming the account; one that does not means the account is free. */
function hasSharingState(sharing: AccountSharing): boolean {
  return sharing.in_use !== undefined || sharing.progress !== undefined || sharing.idle_sessions !== undefined || sharing.failure !== undefined;
}

/** `session` as the window shows it, with `Session.switching_account` set or absent as `switching`
 * says. */
function withSwitching(session: Session, switching: boolean): Session {
  if ((session.switching_account === true) === switching) return session;
  if (switching) return { ...session, switching_account: true };
  const { switching_account: _, ...rest } = session;
  return rest;
}

/** `session` as the window shows it: switching its account when the daemon says so or a batch of
 * this window still holds it. */
function shownSwitching(session: Session, queued: ReadonlySet<string>): Session {
  return withSwitching(session, session.switching_account === true || queued.has(session.id));
}

/** The sessions among `sessions` the daemon says are switching their account. */
function daemonSwitchingOf(sessions: Session[]): Set<string> {
  return new Set(sessions.filter((session) => session.switching_account).map((session) => session.id));
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
    trustPrompts: state.trustPrompts.filter((p) => !ids.has(p.session)),
    sessionRequests: state.sessionRequests.filter((r) => !ids.has(r.session)),
    daemonSwitching: new Set(Array.from(state.daemonSwitching).filter((id) => !ids.has(id))),
  };
}

export function reducer(state: State, action: Action): State {
  switch (action.kind) {
    case "connection":
      return { ...state, connectionState: action.state };
    case "dismiss_trust_prompt":
      return { ...state, trustPrompts: state.trustPrompts.filter((p) => p.session !== action.session) };
    case "track_account_login": {
      const held = state.loginsStartedHere.includes(action.key);
      if (held === action.tracked) return state;
      return { ...state, loginsStartedHere: action.tracked ? [...state.loginsStartedHere, action.key] : state.loginsStartedHere.filter((key) => key !== action.key) };
    }
    case "dismiss_account_login":
      return state.accountLogins.includes(action.login) ? { ...state, accountLogins: state.accountLogins.filter((held) => held !== action.login) } : state;
    case "queue_account_switches": {
      const queuedAccountSwitches = action.ids.reduce((held, id) => withMember(held, id, action.queued), state.queuedAccountSwitches);
      if (queuedAccountSwitches === state.queuedAccountSwitches) return state;
      let sessions = state.sessions;
      for (const id of action.ids) {
        const held = sessions.get(id);
        if (!held) continue;
        const shown = withSwitching(held, state.daemonSwitching.has(id) || queuedAccountSwitches.has(id));
        if (shown === held) continue;
        if (sessions === state.sessions) sessions = new Map(sessions);
        sessions.set(id, shown);
      }
      return { ...state, queuedAccountSwitches, sessions };
    }
    case "event": {
      const event = action.event;
      switch (event.type) {
        case "snapshot":
          return {
            ...state,
            daemonSwitching: daemonSwitchingOf(event.sessions),
            hosts: new Map(event.hosts.map((h) => [h.id, h])),
            consoles: keepSame(state.consoles, event.consoles, (c) => c.id),
            projects: keepSame(state.projects, event.projects, (p) => p.id),
            sessions: keepSame(state.sessions, event.sessions.map((s) => shownSwitching(s, state.queuedAccountSwitches)), (s) => s.id),
            // A snapshot means either first connect or a lag recovery, and carries no pages either
            // way (see `Page` in protocol.ts). Cleared rather than kept, because a console session
            // that was deleted while this client was behind would otherwise go on holding its
            // pages forever — nothing else ever removes an entry for a session the snapshot no
            // longer lists.
            pages: new Map(),
            deletedPages: new Map(),
            snapshotEpoch: state.snapshotEpoch + 1,
            trustPrompts: [],
            trustedDirectories: event.trusted_directories,
            sessionRequests: [],
            gitStatuses: keepSame(state.gitStatuses, event.git_statuses, (s) => s.project),
            settings: event.settings,
            agentAvailability: new Map(event.agent_availability.map((a) => [a.agent, a])),
            accountQuotas: keepSameQuotas(state.accountQuotas, event.account_quotas),
            accountLogins: event.account_logins,
            loginsStartedHere: state.loginsStartedHere.filter((key) => event.account_logins.some((login) => accountLoginKey(login.agent, login.account) === key)),
            accountSharing: keepSame(state.accountSharing, event.account_sharing.filter(hasSharingState), (sharing) => sharing.account),
            homeDir: event.home_dir,
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
          return { ...withoutSessions(state, removed), consoles, projects, gitStatuses, shellTerminals: withoutRemovedProjectTerminals(state.shellTerminals, projects) };
        }
        case "session_opened": {
          // A title or status broadcast can arrive before this launch-time reply. Keep the
          // authoritative record if one already exists; otherwise seed the newly opened session.
          if (state.sessions.has(event.session.id)) return state;
          const sessions = new Map(state.sessions);
          sessions.set(event.session.id, shownSwitching(event.session, state.queuedAccountSwitches));
          return { ...state, sessions, daemonSwitching: withMember(state.daemonSwitching, event.session.id, event.session.switching_account === true) };
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
          return { ...withoutSessions(state, removed), projects, gitStatuses, shellTerminals: withoutRemovedProjectTerminals(state.shellTerminals, projects) };
        }
        case "session_upserted": {
          // A session's status broadcasts repeat what the store holds as often as they change it.
          const shown = shownSwitching(event.session, state.queuedAccountSwitches);
          const daemonSwitching = withMember(state.daemonSwitching, event.session.id, event.session.switching_account === true);
          // The queue can hide a daemon switch starting or ending without changing the shown session.
          if (sameValue(state.sessions.get(event.session.id), shown)) {
            return daemonSwitching === state.daemonSwitching ? state : { ...state, daemonSwitching };
          }
          const sessions = new Map(state.sessions);
          sessions.set(event.session.id, shown);
          // The daemon shows a session at its trust confirmation as `waiting_user`, and publishes
          // that before it broadcasts the prompt, so a status that is anything else, which a session
          // that no longer runs has too, means the confirmation is no longer waiting for an answer
          // here (answered, covered by a permission or a trusted directory, or gone). A prompt not
          // yet queued is untouched, and one that is is dropped only by its own session's upsert.
          const trustPrompts = event.session.status === "waiting_user"
            ? state.trustPrompts
            : state.trustPrompts.filter((p) => p.session !== event.session.id);
          return { ...state, sessions, trustPrompts, daemonSwitching };
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
          if (event.code !== "trust_answer_failed") return state;
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
          const existing = state.pages.get(event.session_id) ?? [];
          const deleted = state.deletedPages.get(event.session_id);
          const listed = deleted ? event.pages.filter((p) => !deleted.has(p.id)) : event.pages;
          const seen = new Set(listed.map((p) => p.id));
          const pages = new Map(state.pages);
          pages.set(event.session_id, [...listed, ...existing.filter((p) => !seen.has(p.id))]);
          return { ...state, pages };
        }
        case "page_created": {
          // Appended even with no baseline yet: that window is exactly the first `list_pages`
          // being in flight, and dropping the event here would mean waiting for a second push to
          // ever see it. The transient "1 / 1" this produces is corrected once that reply's merge
          // (above) lands.
          const existing = state.pages.get(event.page.session_id) ?? [];
          const pages = new Map(state.pages);
          pages.set(event.page.session_id, [...existing, event.page]);
          return { ...state, pages };
        }
        case "pages_deleted": {
          // Kept as an empty list rather than dropped when none is left: the session's pages are
          // still known, and there are none.
          // The ids are remembered even with no list yet, as the reply in flight may hold them.
          const gone = new Set([...(state.deletedPages.get(event.session_id) ?? []), ...event.pages]);
          const deletedPages = new Map(state.deletedPages);
          deletedPages.set(event.session_id, gone);
          const existing = state.pages.get(event.session_id);
          if (!existing) return { ...state, deletedPages };
          const pages = new Map(state.pages);
          pages.set(event.session_id, existing.filter((page) => !gone.has(page.id)));
          return { ...state, pages, deletedPages };
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
        case "settings_updated":
          return { ...state, settings: event.settings };
        case "agent_availability_updated":
          return { ...state, agentAvailability: new Map(event.agent_availability.map((a) => [a.agent, a])) };
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
          const key = accountLoginKey(login.agent, login.account);
          const running = login.status === "starting" || login.status === "waiting";
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
