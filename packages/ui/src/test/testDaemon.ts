// The daemon stand-in the tests render under. Every member does nothing, so a test names only what it
// reads, and a member `Daemon` gains later is added here once.
import { looseRequestSender, type RequestHandler } from "../requests";
import { createStateStore, type Daemon, type State } from "../store";

/** A `Daemon` over a store that starts at `state`, whose requests are rejected unless the test gives
 * its own `request` (a plain handler, see `looseRequestSender`), and whose other members do nothing. `overrides`
 * replace members as they are. */
export function testDaemon({ state, request = () => Promise.reject(new Error("no daemon")), ...overrides }: { state?: Partial<State>; request?: RequestHandler } & Partial<Omit<Daemon, "request">> = {}): Daemon {
  return {
    store: createStateStore(state),
    request: looseRequestSender(request),
    raiseError: () => {},
    showNotice: () => {},
    onNotification: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
    ...overrides,
  };
}
