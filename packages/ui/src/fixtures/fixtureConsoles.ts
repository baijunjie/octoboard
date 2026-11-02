// What the fixture daemon answers for the consoles: creating one, saving its settings and deleting it.
import { isLive, type Console, type RequestBody } from "../protocol";
import { consoleOf } from "./builders";
import { HOME, trimmedOrNull, changedPerAgent, changedLaunch, type FixtureContext, type Handlers } from "./fixtureContext";

/** What a `create_console` or `update_console` request sets of the console, each field only where
 * the request names it, on top of the console it changes (a blank one for a new console); an
 * `update_console` that leaves a field out leaves it alone, and one that names it as `null`
 * clears it, as `update_console` of `coordinator/consoles.rs` does. Account and model references
 * are taken as given, not checked against the settings or an agent's list. */
function consoleFields(current: Console, body: Extract<RequestBody, { type: "create_console" | "update_console" }>): Partial<Console> {
  const fields: Partial<Console> = {};
  if (body.name !== undefined) fields.name = body.name.trim();
  if (body.default_agent !== undefined) fields.default_agent = body.default_agent;
  if (body.project_session_agent !== undefined) fields.project_session_agent = body.project_session_agent;
  fields.accounts = changedPerAgent(current.accounts, body.accounts, (_, account) => account);
  fields.launch = changedPerAgent(current.launch, body.launch, changedLaunch);
  fields.project_session_launch = changedPerAgent(current.project_session_launch, body.project_session_launch, changedLaunch);
  if (body.icon !== undefined) fields.icon = body.icon;
  if (body.console_session_prompt !== undefined) fields.console_session_prompt = trimmedOrNull(body.console_session_prompt);
  return fields;
}

/** The replies about consoles. */
export function createConsoleReplies({ store, fail, refuseLocked }: FixtureContext): Handlers {
  return {
    create_console: (body) => {
      // As the daemon does, the new console reaches the store before the request is answered
      // (`coordinator/consoles.rs` broadcasts first), so a scenario can reach the first
      // console's arrival.
      const id = `console-${Date.now()}`;
      const blank = consoleOf(id, body.name.trim(), { workdir: `${HOME}/.octoboard/consoles/${id}`, created_at: Date.now() });
      store.setState((state) => ({
        consoles: new Map(state.consoles).set(id, {
          ...blank,
          ...consoleFields(blank, body),
          name: body.name.trim(),
          default_agent: body.default_agent,
        }),
      }));
      return { type: "ack", id: null };
    },
    update_console: (body) => {
      // Broadcast before the answer, as the daemon does, so a dialog reopened on the console
      // reads what was saved.
      const current = store.getState().consoles.get(body.console);
      if (current) store.setState((state) => ({ consoles: new Map(state.consoles).set(current.id, { ...current, ...consoleFields(current, body) }) }));
      return { type: "ack", id: null };
    },
    delete_console: (body) => {
      const current = store.getState();
      if (!current.consoles.has(body.console)) return fail({ code: "unknown_console", params: { console: body.console }, message: `unknown console ${body.console}` });
      refuseLocked([...current.sessions.values()].filter((session) => session.console_id === body.console));
      if ([...current.sessions.values()].some((session) => session.console_id === body.console && isLive(session.status))) return fail({ code: "console_has_running_sessions", params: {}, message: "This console still has running sessions" });
      const consoles = new Map(current.consoles);
      consoles.delete(body.console);
      const projects = new Map([...current.projects].filter(([, project]) => project.console_id !== body.console));
      const sessions = new Map([...current.sessions].filter(([, session]) => session.console_id !== body.console));
      const pages = new Map([...current.pages].filter(([id]) => sessions.has(id)));
      const gitStatuses = new Map([...current.gitStatuses].filter(([id]) => projects.has(id)));
      const shellTerminals = new Map([...current.shellTerminals].filter(([, terminal]) => projects.has(terminal.project_id)));
      store.setState({ consoles, projects, sessions, pages, gitStatuses, shellTerminals });
      return { type: "ack", id: null };
    },
  };
}
