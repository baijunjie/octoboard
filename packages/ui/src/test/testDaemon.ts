// The daemon stand-in the tests render under. Every member does nothing, so a test names only what it
// reads, and a member `Daemon` gains later is added here once.
import { looseRequestSender, REPLY_TYPES, type ReplyOf, type RequestHandler, type RequestType } from "../requests";
import { CodedError } from "../errors/codedError";
import { t } from "../i18n/language";
import type { ClientRequestBody, Event } from "../protocol";
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
    onContentSearch: () => () => {},
    onMemoryChange: () => () => {},
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
  list_orphaned_memories: { type: "orphaned_memory_list", id: null, places: [] },
};

function emptyDaemonReply(body: ClientRequestBody): ReplyOf<RequestType> | undefined {
  if (body.type === "list_agent_models") return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models: [], unavailable: null };
  if (body.type === "read_session_history") return { type: "session_history", id: null, session: body.session, messages: [] };
  if (body.type === "list_memories") return { type: "memory_list", id: null, scope: body.console !== undefined ? { kind: "console", console_id: body.console } : { kind: "place", host_id: "", path: "" }, memories: [] };
  if (body.type === "list_pages") return { type: "page_list", id: null, pages: [], session_id: body.session };
  return EMPTY_REPLIES[body.type];
}

/** Supplies an empty reply when a dialog-specific handler leaves an ambient read unanswered. */
export function withEmptyReplies(request: RequestHandler): RequestHandler {
  return async (body) => {
    const reply = await request(body);
    const empty = emptyDaemonReply(body);
    if (empty && reply.type !== empty.type) return empty;
    return reply.type === "ack" ? defaultDaemonReply(body) : reply;
  };
}

/** Answers ambient reads with an empty fixture and acknowledges only requests that allow it. */
export function defaultDaemonReply(body: ClientRequestBody): ReplyOf<RequestType> {
  const empty = emptyDaemonReply(body);
  if (empty) return empty;
  const expected: Event["type"] | readonly Event["type"][] = REPLY_TYPES[body.type];
  if (expected === "ack" || (Array.isArray(expected) && expected.includes("ack"))) return { type: "ack", id: null };
  throw new CodedError(t("connection.error.connectionIsClosed"), "connection_closed", {});
}
