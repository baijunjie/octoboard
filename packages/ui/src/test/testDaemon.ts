// The daemon stand-in the tests render under. Every member does nothing, so a test names only what it
// reads, and a member `Daemon` gains later is added here once.
import { looseRequestSender, type ReplyOf, type RequestHandler, type RequestType } from "../requests";
import { createStateStore, type Daemon, type State } from "../store";
import { availabilityOf } from "../fixtures/fixtureProviders";
import { TEST_PROVIDERS } from "./testProviders";

/** A `Daemon` over a store that starts at `state` (with the providers a daemon serves, unless it names its own), whose requests are rejected unless the test gives
 * its own `request` (a plain handler, see `looseRequestSender`), and whose other members do nothing. `overrides`
 * replace members as they are. */
export function testDaemon({ state, request = () => Promise.reject(new Error("no daemon")), ...overrides }: { state?: Partial<State>; request?: RequestHandler } & Partial<Omit<Daemon, "request">> = {}): Daemon {
  return {
    store: createStateStore({ providers: TEST_PROVIDERS, agentAvailability: availabilityOf(state?.providers ?? TEST_PROVIDERS), ...state }),
    request: looseRequestSender(request),
    showNotice: () => {},
    onNotification: () => () => {},
    onImportProgress: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
    ...overrides,
  };
}

/** The replies the daemon gives requests that have nothing to report, where the reply carries what a component reads. */
const EMPTY_REPLIES: { [K in RequestType]?: ReplyOf<K> } = {
  list_processes: { type: "processes_listed", id: null, trees: [] },
  inspect_session_branches: { type: "session_branches_inspected", id: null, items: [] },
  inspect_session_records: { type: "session_records_inspected", id: null, presence: "unknown" },
  inspect_project_removal: { type: "project_removal_inspected", id: null, items: [], processes: [] },
  list_agent_models: { type: "agent_models", id: null, agent: "claude", account: null, models: [], unavailable: null },
  read_session_history: { type: "session_history", id: null, session: "", messages: [] },
};

/** `request` answering each request in `EMPTY_REPLIES` that it gives another reply for (a stand-in that answers every
 * other request with `ack`) as one with nothing to report: the daemon answers each with nothing else, and a component
 * reads the fields of the reply it is typed to get. */
export function withEmptyReplies(request: RequestHandler): RequestHandler {
  return async (body) => {
    const reply = await request(body);
    const empty = EMPTY_REPLIES[body.type];
    return empty && reply.type !== empty.type ? empty : reply;
  };
}
