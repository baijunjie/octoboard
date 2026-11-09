// What the fixture daemon answers for the sessions: opening, resuming, archiving and deleting them, handing one off,
// switching its account or model, and the small changes to one (pin, lock, colour, title).
import { applyCompatibility } from "../agents/launchCompatibility";
import { sameValue } from "../values/sameValue";
import { isAgentAdded, isAgentUnavailable, projectSessionAgent } from "../agents/agents";
import { servedLabel } from "./fixtureProviders";
import { type Agent, type ConsoleSessionColour, type Session, type SessionRecordPresence } from "../protocol";
import { CONSOLE_SESSION_COLOURS } from "../sessions/sessionColours";
import { applyDaemonEvent } from "../store";
import type { AccountGuards } from "./fixtureAccounts";
import type { ProcessLedger } from "./fixtureProcesses";
import { neverReplied, type FixtureContext, type Handlers } from "./fixtureContext";

/** How long a tree's top process must have run before an archive's inspection lists it: what started a moment ago is
 * a command still finishing (the daemon's `ASK_AFTER`, repeated here as the fixture stands on the daemon's side). */
const PROCESS_ASK_AFTER_MS = 30_000;

/** The refusal to open a session, or hand one off, on an agent the user has not added. */
const agentNotAdded = (agent: Agent) => ({
  code: "agent_not_added",
  params: { agent: servedLabel(agent) },
  message: `${servedLabel(agent)} is not added. Add it in Settings → Agent accounts to open a session with it.`,
});

/** The replies about sessions. */
export function createSessionReplies({ scenario, store, fail, refuseLocked }: FixtureContext, guards: AccountGuards, ledger: ProcessLedger): Handlers {
  const { launchAccount, refuseIfSharingChanges, refuseIfDisabled, refuseIfQuotaExhausted, refuseIfNotSignedIn } = guards;
  const { treesOfSession, oldestFirst, keepTrees, refuseUnkeepable, endSessions } = ledger;
  let openedSessionCount = 0;
  // The daemon remembers its ordinal counter independently of session history. Seed it from
  // fixture ordinals once, so renaming or deleting a session never reuses an ordinal.
  const initial = store.getState();
  const nextConsoleOrdinals = new Map([...initial.consoles.keys()].map((id) => {
    const sessions = [...initial.sessions.values()].filter((session) => session.console_id === id && session.kind === "console");
    return [id, Math.max(0, ...sessions.map((session, index) => session.ordinal ?? index + 1)) + 1];
  }));

  /** A console session's colour as the daemon assigns one: the first of the palette that no session of
   * the console that is not archived wears (an interrupted one still holds its colour), else the first. */
  const freshColour = (consoleId: string): ConsoleSessionColour => {
    const used = new Set([...store.getState().sessions.values()].filter((session) => session.console_id === consoleId && session.kind === "console" && session.status !== "archived").map((session) => session.colour));
    return CONSOLE_SESSION_COLOURS.find((colour) => !used.has(colour)) ?? CONSOLE_SESSION_COLOURS[0];
  };
  const recordsPresence = (ids: string[]): SessionRecordPresence => {
    let presence: SessionRecordPresence = "absent";
    for (const id of ids) {
      const item = store.getState().sessions.has(id) ? scenario.sessionRecords?.[id] ?? "unknown" : "unknown";
      if (item === "present") return "present";
      if (item === "unknown") presence = "unknown";
    }
    return presence;
  };

  return {
    archive_session: (body) => {
      const current = store.getState();
      const session = current.sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: "Unknown session" });
      // A tree that cannot be kept refuses the archive before anything is kept or ended.
      refuseUnkeepable(body.keep_process_ids ?? []);
      // The trees the user chose to keep are theirs before anything else happens, so a refusal leaves them kept.
      keepTrees(body.keep_process_ids ?? []);
      if (scenario.sessionArchiveError) return fail(scenario.sessionArchiveError);
      // Match coordinator::bindings::team over the valid fixture hierarchy: owners reach two
      // levels.
      const childrenOf = (owner: Session) => [...current.sessions.values()].filter((child) => child.bound_to === owner.id);
      const direct = childrenOf(session);
      const team = [...direct.flatMap(childrenOf), ...direct];
      refuseLocked([session, ...team]);
      const descendants = team.filter((child) => child.status !== "archived");
      const sessions = new Map(current.sessions);
      const ended_at = Date.now();
      for (const record of [...descendants, session]) sessions.set(record.id, { ...record, status: "archived", ended_at });
      store.setState({ sessions });
      // The processes of the sessions archived end with them.
      const ended = new Set([...descendants, session].map((record) => record.id));
      endSessions(ended);
      return { type: "ack", id: null };
    },
    resume_session: (body) => {
      if (scenario.sessionResumes?.[body.session] === "pending") return neverReplied();
      const current = store.getState();
      const session = current.sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      if (session.status !== "archived" && session.status !== "interrupted") return fail({ code: "session_already_running", params: { session: body.session }, message: "this session is not interrupted or archived" });
      refuseIfSharingChanges(session.account_id);
      // Reopening a child brings back archived owners, while siblings stay archived.
      const sessions = new Map(current.sessions);
      const reopen = (record: Session) => sessions.set(record.id, { ...record, status: "idle", ended_at: null });
      reopen(session);
      let owner = session.bound_to;
      while (owner) {
        const record = sessions.get(owner);
        if (!record || record.status !== "archived") break;
        reopen(record);
        owner = record.bound_to;
      }
      store.setState({ sessions });
      return { type: "ack", id: null };
    },
    switch_session_account: (body) => {
      const current = store.getState();
      const session = current.sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      if (session.status === "archived") return fail({ code: "session_archived", params: { session: body.session }, message: "An archived session cannot switch accounts" });
      const account = body.account === null ? undefined : current.settings.accounts.find((account) => account.id === body.account && account.agent === session.agent);
      if (body.account !== null && !account) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
      if ((session.account_id ?? null) === body.account) return fail({ code: "session_already_on_account", params: { session: body.session }, message: "this session is on that account already" });
      refuseIfSharingChanges(session.account_id);
      refuseIfSharingChanges(account?.id);
      refuseIfDisabled(session.agent, account);
      refuseIfQuotaExhausted(session.agent, account);
      refuseIfNotSignedIn(session.agent, account);
      // The daemon broadcasts the session as switching while the switch holds its claim, and the
      // session as it is once the switch ends.
      const { switching_account: _, ...resting } = session;
      applyDaemonEvent(store, { type: "session_upserted", session: { ...resting, switching_account: true } });
      const reply = scenario.sessionAccountSwitches?.[body.session];
      if (reply === "pending") return neverReplied();
      if (reply) {
        // A relaunch under the new account that ended at once leaves the session interrupted on the account it had;
        // any other refusal leaves it as it was.
        const interrupted = reply.code === "switch_did_not_come_up";
        applyDaemonEvent(store, { type: "session_upserted", session: interrupted ? { ...resting, status: "interrupted" } : resting });
        return fail(reply);
      }
      applyDaemonEvent(store, { type: "session_upserted", session: { ...resting, account_id: account?.id ?? null, config_dir: account?.config_dir ?? null } });
      return { type: "ack", id: null };
    },
    set_session_model: (body) => {
      const current = store.getState();
      const session = current.sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      if (body.model === undefined && body.effort === undefined && body.compatibility === undefined) return fail({ code: "field_required", params: { field: "model" }, message: "model is required" });
      if (session.status === "archived") return fail({ code: "session_archived", params: { session: body.session }, message: "An archived session cannot change its model" });
      if (session.status === "starting" || session.switching_account) return fail({ code: "session_already_starting", params: { session: body.session }, message: "this session is already being started" });
      refuseIfSharingChanges(session.account_id);
      // A value left out keeps what the session has; a blank one leaves the choice to the agent.
      const chosen = (value: string | null | undefined, kept: string | null | undefined) => (value === undefined ? kept : value?.trim() || null);
      const [model, effort] = [chosen(body.model, session.model), chosen(body.effort, session.effort)];
      const compatibility = applyCompatibility(session.compatibility, body.compatibility);
      if (model === session.model && effort === session.effort && sameValue(compatibility, session.compatibility)) return { type: "session_model_set", id: null, session: session.id, outcome: "unchanged" };
      if (session.status === "interrupted") {
        applyDaemonEvent(store, { type: "session_upserted", session: { ...session, model, effort, compatibility } });
        return { type: "session_model_set", id: null, session: session.id, outcome: "recorded" };
      }
      // The daemon shows the session starting from the moment the restart is decided; once the process is down the
      // model and effort its records showed are forgotten.
      const { observed_model: _model, observed_effort: _effort, ...forgotten } = session;
      const reply = scenario.sessionModelChanges?.[body.session];
      if (reply === "pending") {
        applyDaemonEvent(store, { type: "session_upserted", session: { ...forgotten, status: "starting" } });
        return neverReplied();
      }
      if (reply) {
        // Only a relaunch that ended at once leaves the process down; a process that did not stop is as it was.
        const interrupted = reply.code === "model_change_did_not_come_up";
        applyDaemonEvent(store, { type: "session_upserted", session: interrupted ? { ...forgotten, status: "interrupted" } : session });
        return fail(reply);
      }
      applyDaemonEvent(store, { type: "session_upserted", session: { ...forgotten, status: "idle", model, effort, compatibility } });
      return { type: "session_model_set", id: null, session: session.id, outcome: "restarted", continued: true };
    },
    handoff_session: (body) => {
      const current = store.getState();
      const session = current.sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      // A refusal the scenario stages: one the window could not have seen coming.
      const staged = scenario.sessionHandoffs?.[body.session];
      if (staged === "pending") return neverReplied();
      if (staged) return fail(staged);
      if (session.status === "archived") return fail({ code: "session_archived", params: { session: session.id }, message: "An archived session cannot be handed off" });
      if (session.status === "working" || session.status === "starting") return fail({ code: "session_working", params: { session: session.id }, message: "The session is in the middle of a turn" });
      refuseLocked([session]);
      refuseIfSharingChanges(session.account_id);
      if (body.agent === session.agent) return fail({ code: "handoff_to_same_agent", params: { session: session.id, agent: servedLabel(session.agent) }, message: "The session already runs on that agent" });
      if (isAgentUnavailable(current.agentAvailability, body.agent)) return fail({ code: "agent_not_available", params: { agent: servedLabel(body.agent) }, message: `${servedLabel(body.agent)} is not available` });
      if (!isAgentAdded(current.settings.added_agents, body.agent)) return fail(agentNotAdded(body.agent));
      if (body.account != null && !current.settings.accounts.some((account) => account.id === body.account && account.agent === body.agent)) return fail({ code: "unknown_account", params: { account: body.account }, message: `unknown account ${body.account}` });
      const owner = current.consoles.get(session.console_id);
      if (!owner) return fail({ code: "unknown_console", params: { console: session.console_id }, message: `unknown console ${session.console_id}` });
      const project = session.project_id ? current.projects.get(session.project_id) : undefined;
      const account = launchAccount(owner, project, body.agent, body.account);
      refuseIfDisabled(body.agent, account);
      refuseIfQuotaExhausted(body.agent, account);
      refuseIfNotSignedIn(body.agent, account);
      const ordinal = nextConsoleOrdinals.get(owner.id) ?? 1;
      if (!project) nextConsoleOrdinals.set(owner.id, ordinal + 1);
      // The receiving session takes the original's place; the original is archived and what it
      // owned is bound to the new one.
      const received: Session = {
        id: `s-gallery-opened-${++openedSessionCount}`,
        agent: body.agent,
        console_id: session.console_id,
        project_id: session.project_id,
        host_id: session.host_id,
        kind: session.kind,
        origin: session.origin,
        title: session.title,
        status: "working",
        has_conversation: false,
        role: session.role,
        pinned: session.pinned,
        locked: false,
        started_at: Date.now(),
        agent_session_id: null,
        bound_to: project ? session.bound_to : null,
        ordinal: project ? null : ordinal,
        colour: project ? null : freshColour(owner.id),
        account_id: account?.id ?? null,
        config_dir: account?.config_dir ?? null,
        ended_at: null,
        ...(body.model !== undefined && { model: body.model }),
        ...(body.effort !== undefined && { effort: body.effort }),
        compatibility: applyCompatibility({}, body.compatibility),
      };
      const sessions = new Map(current.sessions);
      for (const owned of current.sessions.values()) if (owned.bound_to === session.id) sessions.set(owned.id, { ...owned, bound_to: received.id });
      sessions.set(session.id, { ...session, status: "archived", ended_at: Date.now() });
      sessions.set(received.id, received);
      store.setState({ sessions });
      return { type: "session_opened", id: null, session: received };
    },
    release_session: (body) => {
      const session = store.getState().sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      if (scenario.sessionReleaseError) {
        const movedTo = scenario.sessionReleaseMovedTo;
        if (movedTo) {
          // As the daemon's transfer: a coordinator stays one under a console session, and is a worker under any other owner.
          const stays = session.role === "coordinator" && store.getState().sessions.get(movedTo)?.kind === "console";
          store.setState((state) => ({ sessions: new Map(state.sessions).set(session.id, { ...session, bound_to: movedTo, role: stays ? "coordinator" : "worker" }) }));
        }
        return fail(scenario.sessionReleaseError);
      }
      // Refused with the daemon's codes and English messages.
      if (session.kind === "console") return fail({ code: "console_session_never_bound", params: { session: session.id }, message: `\`${session.id}\` is a console session, which is never bound, so it has no owner to be released from` });
      if (!session.bound_to) return fail({ code: "session_not_bound", params: { session: session.id }, message: `\`${session.id}\` is not bound to any owner, so there is nothing to release` });
      // Whatever the session owns stays bound to it, and so do its branches; only its own binding goes.
      store.setState((state) => ({ sessions: new Map(state.sessions).set(session.id, { ...session, bound_to: null, role: "coordinator" }) }));
      return { type: "ack", id: null };
    },
    set_session_pinned: (body) => {
      const session = store.getState().sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      const updated = { ...session, pinned: body.pinned };
      store.setState((state) => ({ sessions: new Map(state.sessions).set(body.session, updated) }));
      return { type: "ack", id: null };
    },
    set_session_locked: (body) => {
      const session = store.getState().sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      if (body.locked && session.status === "archived") return fail({ code: "session_archived", params: { session: body.session }, message: "An archived session cannot be locked" });
      store.setState((state) => ({ sessions: new Map(state.sessions).set(body.session, { ...session, locked: body.locked }) }));
      return { type: "ack", id: null };
    },
    set_session_colour: (body) => {
      const session = store.getState().sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      const updated = { ...session, colour: body.colour };
      store.setState((state) => ({ sessions: new Map(state.sessions).set(body.session, updated) }));
      return { type: "ack", id: null };
    },
    locate_conversation_to_resume: () => {
      const located = scenario.conversationLocation ?? { kind: "not_found" as const };
      if (located === "pending") return neverReplied();
      if ("code" in located) return fail(located);
      return { type: "conversation_located", id: null, location: located };
    },
    open_session: (body) => {
      if (scenario.sessionOpenError) return fail(scenario.sessionOpenError);
      const current = store.getState();
      const owner = current.consoles.get(body.console_id);
      if (!owner) return fail({ code: "unknown_console", params: { console: body.console_id }, message: `unknown console ${body.console_id}` });
      const project = body.project_id ? current.projects.get(body.project_id) : undefined;
      if (body.project_id && !project) return fail({ code: "unknown_project", params: { project: body.project_id }, message: `unknown project ${body.project_id}` });
      const agent = body.agent ?? (project ? project.default_agent ?? projectSessionAgent(owner) : owner.default_agent);
      if (!isAgentAdded(current.settings.added_agents, agent)) return fail(agentNotAdded(agent));
      const account = launchAccount(owner, project, agent, body.account);
      refuseIfSharingChanges(account?.id);
      refuseIfDisabled(agent, account);
      refuseIfQuotaExhausted(agent, account);
      refuseIfNotSignedIn(agent, account);
      const ordinal = nextConsoleOrdinals.get(owner.id) ?? 1;
      if (!project) nextConsoleOrdinals.set(owner.id, ordinal + 1);
      const session: Session = {
        id: `s-gallery-opened-${++openedSessionCount}`,
        agent,
        console_id: owner.id,
        project_id: project?.id ?? null,
        host_id: project?.host_id ?? "local",
        kind: project ? "project" : "console",
        role: project && body.bound_to ? "worker" : "coordinator",
        origin: "user",
        title: (body.title?.trim() || undefined) ?? project?.name ?? owner.name,
        status: body.task == null ? "idle" : "working",
        has_conversation: false,
        pinned: false,
        locked: false,
        started_at: Date.now(),
        agent_session_id: null,
        bound_to: project ? body.bound_to ?? null : null,
        ordinal: project ? null : ordinal,
        colour: project ? null : freshColour(owner.id),
        account_id: account?.id ?? null,
        config_dir: account?.config_dir ?? null,
        ended_at: null,
        ...(body.model !== undefined && { model: body.model }),
        ...(body.effort !== undefined && { effort: body.effort }),
        compatibility: applyCompatibility({}, body.compatibility),
      };
      store.setState({ sessions: new Map(current.sessions).set(session.id, session) });
      return { type: "session_opened", id: null, session };
    },
    rename_session: (body) => {
      const current = store.getState().sessions.get(body.session);
      if (!current) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      const title = body.title.trim();
      if (!title) return fail({ code: "field_required", params: { field: "title" }, message: "title is required" });
      store.setState((state) => ({ sessions: new Map(state.sessions).set(current.id, { ...current, title }) }));
      return { type: "ack", id: null };
    },
    read_session_history: (body) => {
      if (!store.getState().sessions.has(body.session)) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      const history = scenario.sessionHistory?.[body.session] ?? [];
      if (history === "pending") return neverReplied();
      if (history === "unavailable") return { type: "session_history", id: null, session: body.session, messages: [], unavailable: true };
      if (!Array.isArray(history)) return fail(history);
      return { type: "session_history", id: null, session: body.session, messages: history };
    },
    inspect_session_records: (body) => {
      return { type: "session_records_inspected", id: null, presence: recordsPresence(body.sessions) };
    },
    inspect_session_branches: (body) => {
      if (scenario.sessionBranches === "pending") return neverReplied();
      if (scenario.sessionBranches && !Array.isArray(scenario.sessionBranches)) return fail(scenario.sessionBranches);
      const sessions = store.getState().sessions;
      const asked = new Set(body.sessions.filter((id) => sessions.has(id)));
      const items = (scenario.sessionBranches ?? []).filter((item) => item.session !== null && asked.has(item.session));
      if (!body.processes) return { type: "session_branches_inspected", id: null, items };
      const oldest = Date.now() - PROCESS_ASK_AFTER_MS;
      const processes = oldestFirst([...asked].flatMap(treesOfSession)).filter((tree) => tree.started_at <= oldest);
      return { type: "session_branches_inspected", id: null, items, ...(processes.length > 0 ? { processes } : {}) };
    },
    delete_session: (body) => {
      const all = [...store.getState().sessions.values()];
      const session = store.getState().sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: "Unknown session" });
      refuseLocked([session]);
      // As `delete_session_if_unused` of `store/sessions.rs`, repeated here and not taken from the sidebar's
      // `isUnusedSession`: no conversation, nothing bound to it, and either no owner or opened by
      // the user (one an agent started was dispatched by its owner, which expects it to exist,
      // so it is archived instead).
      const unused = !session.has_conversation && !all.some((s) => s.bound_to === session.id)
        && (!session.bound_to || session.origin === "user");
      if (session.status === "starting" || (body.only_if_unused && !unused)
        || (session.status !== "archived" && !unused)) {
        return fail({ code: "session_not_deletable", params: { session: session.id }, message: "Session is no longer unused or is still starting" });
      }
      const doomed = new Set([session.id]);
      const visit = (owner: string) => {
        for (const child of all.filter((s) => s.bound_to === owner && s.status === "archived")) {
          if (doomed.has(child.id)) continue;
          doomed.add(child.id);
          visit(child.id);
        }
      };
      if (session.status === "archived") visit(session.id);
      refuseLocked(all.filter((record) => doomed.has(record.id)));
      store.setState((state) => ({
        sessions: new Map([...state.sessions].filter(([id]) => !doomed.has(id))),
        pages: new Map([...state.pages].filter(([id]) => !doomed.has(id))),
      }));
      return { type: "ack", id: null };
    },
    delete_archived_sessions: (body) => {
      // What the sessions' deletion comes to, written to the store directly (the reducer that
      // answers `session_deleted` by dropping the session and its pages is not reachable from
      // here) by the time the request is answered, as the daemon's deletes are, so the archive
      // view a scenario has open empties as it would.
      const all = Array.from(store.getState().sessions.values());
      const doomed = new Set<string>();
      // As `delete_archived_group` of `coordinator/lifecycle.rs`: an archived session goes, and
      // with it the archived sessions bound to it, each with its own; one that is not archived
      // stays and keeps what is bound to it. The daemon's own rule is repeated here and not
      // taken from the sidebar's `boundArchivedSessions`, so that a change to what the sidebar
      // lists does not change what a deletion removes.
      const deleteGroup = (session: Session) => {
        if (doomed.has(session.id) || session.status !== "archived") return;
        doomed.add(session.id);
        all.filter((s) => s.bound_to === session.id).forEach(deleteGroup);
      };
      // The scope is the project's, the console session's, or with neither the console's
      // console sessions.
      all
        .filter(
          (s) =>
            s.console_id === body.console &&
            (body.project !== undefined
              ? s.project_id === body.project
              : body.console_session !== undefined
                ? s.bound_to === body.console_session
                : s.kind === "console"),
        )
        .forEach(deleteGroup);
      refuseLocked(all.filter((record) => doomed.has(record.id)));
      store.setState((state) => ({
        sessions: new Map(Array.from(state.sessions).filter(([id]) => !doomed.has(id))),
        pages: new Map(Array.from(state.pages).filter(([id]) => !doomed.has(id))),
      }));
      return { type: "ack", id: null };
    },
    list_pages: (body) => {
      const refused = scenario.pageListErrors?.[body.session];
      if (refused) return fail(refused);
      // As the real client's reply reaches the reducer, so a console session with no fixture
      // pages ends up listed as empty rather than not listed yet.
      const pages = store.getState().pages.get(body.session) ?? [];
      store.setState((state) =>
        state.pages.has(body.session)
          ? state
          : { pages: new Map(state.pages).set(body.session, pages) },
      );
      return { type: "page_list", id: null, session_id: body.session, pages };
    },
    answer_session_request: () => {
      if (scenario.sessionRequestAnswerError) return fail(scenario.sessionRequestAnswerError);
      return { type: "ack", id: null };
    },
    confirm_trust: (body) => {
      if (body.trust_parent_dir === true && scenario.trustParentDirError) return fail(scenario.trustParentDirError);
      return { type: "ack", id: null };
    },
  };
}
